use crate::board::*;
use crate::error::CatanError;
use crate::player::Player;
use crate::types::*;
use rand::Rng;
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TurnPhase {
    Roll,
    ResolveSeven,
    Main,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    Setup {
        round: u8,
        reverse: bool,
        awaiting_road_for: Option<VertexId>,
    },
    Turn(TurnPhase),
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    PlaceSettlement(VertexId),
    PlaceRoad(EdgeId),
    RollDice,
    MoveRobber {
        tile: TileId,
        target: Option<PlayerId>,
    },
    BuildRoad(EdgeId),
    BuildSettlement(VertexId),
    BuildCity(VertexId),
    BuyDevelopmentCard,
    PlayKnight {
        tile: TileId,
        target: Option<PlayerId>,
    },
    PlayRoadBuilding(EdgeId, EdgeId),
    PlayInvention(Resource, Option<Resource>),
    PlayMonopoly(Resource),
    OfferTrade {
        to: PlayerId,
        give: Resources,
        receive: Resources,
    },
    RespondTrade {
        accept: bool,
    },
    BankTrade {
        give: Resource,
        receive: Resource,
    },
    EndTurn,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub board: Board,
    pub players: Vec<Player>,
    pub player_order: Vec<PlayerId>,
    pub current_player_idx: usize,
    pub phase: GamePhase,
    pub dice: Option<(u8, u8)>,
    pub longest_road: Option<PlayerId>,
    pub largest_army: Option<PlayerId>,
    pub pending_trade: Option<TradeOffer>,
    pub development_deck: Vec<DevelopmentCard>,
    pub winner: Option<PlayerId>,
    pub played_dev_card_this_turn: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerView {
    pub id: PlayerId,
    pub name: String,
    pub color: PlayerColor,
    pub resource_count: u8,
    pub dev_card_count: u8,
    pub victory_points: u8,
    pub played_knights: u8,
    pub remaining_roads: u8,
    pub remaining_settlements: u8,
    pub remaining_cities: u8,
    pub is_me: bool,
    pub my_resources: Option<Resources>,
    pub my_hand: Option<Vec<DevelopmentCard>>,
    pub my_new_cards: Option<Vec<DevelopmentCard>>,
    pub my_victory_point_cards: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameView {
    pub board: Board,
    pub players: Vec<PlayerView>,
    pub my_id: PlayerId,
    pub current_player: PlayerId,
    pub phase: GamePhase,
    pub dice: Option<(u8, u8)>,
    pub longest_road: Option<PlayerId>,
    pub largest_army: Option<PlayerId>,
    pub pending_trade: Option<TradeOffer>,
    pub winner: Option<PlayerId>,
}

impl Game {
    /// Build a player's private view of the game, hiding opponents' hidden cards.
    pub fn view_for(&self, player_id: PlayerId) -> GameView {
        let current_player = self.current_player();
        let players = self
            .players
            .iter()
            .map(|p| {
                let is_me = p.id == player_id;
                let public_vps = self.settlement_city_points(p.id)
                    + self.longest_road_points(p.id)
                    + self.largest_army_points(p.id);
                let vps = if is_me {
                    public_vps + p.victory_point_cards
                } else {
                    public_vps
                };
                PlayerView {
                    id: p.id,
                    name: p.name.clone(),
                    color: p.color,
                    resource_count: p.resources.total(),
                    dev_card_count: p.total_cards(),
                    victory_points: vps,
                    played_knights: p.played_knights,
                    remaining_roads: p.remaining_roads,
                    remaining_settlements: p.remaining_settlements,
                    remaining_cities: p.remaining_cities,
                    is_me,
                    my_resources: if is_me { Some(p.resources) } else { None },
                    my_hand: if is_me { Some(p.hand.clone()) } else { None },
                    my_new_cards: if is_me {
                        Some(p.new_cards.clone())
                    } else {
                        None
                    },
                    my_victory_point_cards: if is_me {
                        Some(p.victory_point_cards)
                    } else {
                        None
                    },
                }
            })
            .collect();

        GameView {
            board: self.board.clone(),
            players,
            my_id: player_id,
            current_player,
            phase: self.phase,
            dice: self.dice,
            longest_road: self.longest_road,
            largest_army: self.largest_army,
            pending_trade: self.pending_trade.clone(),
            winner: self.winner,
        }
    }

    /// Create a new game with a randomized board, turn order, and shuffled dev-card deck.
    pub fn new(players: Vec<Player>) -> Result<Self, CatanError> {
        if players.len() < 2 || players.len() > 4 {
            return Err(CatanError::InvalidPlayer);
        }

        let board = Board::new_standard();

        let mut player_order: Vec<PlayerId> = players.iter().map(|p| p.id).collect();
        player_order.shuffle(&mut thread_rng());

        let mut deck = development_deck();
        deck.shuffle(&mut thread_rng());

        Ok(Self {
            board,
            players,
            player_order,
            current_player_idx: 0,
            phase: GamePhase::Setup {
                round: 0,
                reverse: false,
                awaiting_road_for: None,
            },
            dice: None,
            longest_road: None,
            largest_army: None,
            pending_trade: None,
            development_deck: deck,
            winner: None,
            played_dev_card_this_turn: false,
        })
    }

    pub fn current_player(&self) -> PlayerId {
        self.player_order[self.current_player_idx]
    }

    /// Apply a player action after validating turn order and game state.
    /// Returns an error if the action is illegal or the game is over.
    pub fn apply_action(&mut self, player: PlayerId, action: Action) -> Result<(), CatanError> {
        if self.winner.is_some() {
            return Err(CatanError::GameFinished);
        }

        if player >= self.players.len() {
            return Err(CatanError::InvalidPlayer);
        }

        let result = match self.phase {
            GamePhase::Finished => Err(CatanError::GameFinished),
            GamePhase::Setup { .. } => self.apply_setup_action(player, action),
            GamePhase::Turn(turn_phase) => self.apply_turn_action(player, turn_phase, action),
        };

        if result.is_ok() {
            self.check_winner();
        }
        result
    }

    /// Handle placement of settlements and roads during the two setup rounds.
    fn apply_setup_action(&mut self, player: PlayerId, action: Action) -> Result<(), CatanError> {
        if player != self.current_player() {
            return Err(CatanError::NotYourTurn);
        }

        let (round, reverse, awaiting_road) = match self.phase {
            GamePhase::Setup {
                round,
                reverse,
                awaiting_road_for,
            } => (round, reverse, awaiting_road_for),
            _ => unreachable!(),
        };

        match action {
            Action::PlaceSettlement(vertex_id) if awaiting_road.is_none() => {
                self.validate_setup_settlement(player, vertex_id)?;
                self.settle_building(vertex_id, Building::Settlement(player))?;
                self.phase = GamePhase::Setup {
                    round,
                    reverse,
                    awaiting_road_for: Some(vertex_id),
                };
                Ok(())
            }
            Action::PlaceRoad(edge_id) if awaiting_road.is_some() => {
                let settlement = awaiting_road.unwrap();
                self.validate_setup_road(player, edge_id, settlement)?;
                self.build_road(edge_id, player)?;

                // If this is the second settlement, collect starting resources.
                if round == 1 {
                    self.collect_starting_resources(settlement)?;
                }

                self.advance_setup(round, reverse)
            }
            _ => Err(CatanError::InvalidPhase),
        }
    }

    /// Ensure a setup settlement is on a free vertex and respects the distance rule.
    fn validate_setup_settlement(
        &mut self,
        player: PlayerId,
        vertex_id: VertexId,
    ) -> Result<(), CatanError> {
        let vertex = &self.board.vertices[vertex_id];
        if vertex.building.is_some() {
            return Err(CatanError::InvalidBuildLocation);
        }

        // Distance rule: no adjacent building.
        for &adj in &self.board.adjacent_vertices(vertex_id) {
            if self.board.vertices[adj].building.is_some() {
                return Err(CatanError::InvalidBuildLocation);
            }
        }

        let player = &self.players[player];
        if player.remaining_settlements == 0 {
            return Err(CatanError::NoPiecesRemaining);
        }

        Ok(())
    }

    /// Ensure a setup road connects to the settlement just placed.
    fn validate_setup_road(
        &self,
        player: PlayerId,
        edge_id: EdgeId,
        settlement: VertexId,
    ) -> Result<(), CatanError> {
        let edge = &self.board.edges[edge_id];
        if edge.road.is_some() {
            return Err(CatanError::InvalidBuildLocation);
        }
        if !edge.endpoints.contains(&settlement) {
            return Err(CatanError::InvalidBuildLocation);
        }
        let player_obj = &self.players[player];
        if player_obj.remaining_roads == 0 {
            return Err(CatanError::NoPiecesRemaining);
        }
        Ok(())
    }

    /// Move to the next setup placement or into the first turn.
    fn advance_setup(&mut self, round: u8, reverse: bool) -> Result<(), CatanError> {
        let count = self.player_order.len();
        let direction: isize = if reverse { -1 } else { 1 };

        if (self.current_player_idx == count - 1 && !reverse)
            || (self.current_player_idx == 0 && reverse)
        {
            if round == 0 {
                self.current_player_idx = count - 1;
                self.phase = GamePhase::Setup {
                    round: 1,
                    reverse: true,
                    awaiting_road_for: None,
                };
                return Ok(());
            } else {
                self.current_player_idx = 0;
                self.phase = GamePhase::Turn(TurnPhase::Roll);
                self.dice = None;
                return Ok(());
            }
        }

        self.current_player_idx = ((self.current_player_idx as isize + direction + count as isize)
            % count as isize) as usize;

        self.phase = GamePhase::Setup {
            round,
            reverse,
            awaiting_road_for: None,
        };
        Ok(())
    }

    /// Dispatch a turn-phase action, allowing trade responses out of turn.
    fn apply_turn_action(
        &mut self,
        player: PlayerId,
        turn_phase: TurnPhase,
        action: Action,
    ) -> Result<(), CatanError> {
        if player != self.current_player() {
            // Only trade responses are allowed out of turn.
            if let Action::RespondTrade { accept } = action {
                return self.handle_trade_response(player, accept);
            }
            return Err(CatanError::NotYourTurn);
        }

        match (turn_phase, action) {
            (TurnPhase::Roll, Action::RollDice) => self.roll_dice(),
            (TurnPhase::Roll, Action::PlayKnight { tile, target }) => {
                self.play_knight(player, tile, target)
            }
            (TurnPhase::ResolveSeven, Action::MoveRobber { tile, target }) => {
                self.move_robber(player, tile, target)?;
                self.phase = GamePhase::Turn(TurnPhase::Main);
                Ok(())
            }
            (TurnPhase::Main, Action::BuildRoad(edge_id)) => {
                self.build_road_action(player, edge_id)
            }
            (TurnPhase::Main, Action::BuildSettlement(vertex_id)) => {
                self.build_settlement_action(player, vertex_id)
            }
            (TurnPhase::Main, Action::BuildCity(vertex_id)) => {
                self.build_city_action(player, vertex_id)
            }
            (TurnPhase::Main, Action::BuyDevelopmentCard) => self.buy_development_card(player),
            (TurnPhase::Main, Action::PlayKnight { tile, target }) => {
                self.play_knight(player, tile, target)
            }
            (TurnPhase::Main, Action::PlayRoadBuilding(e1, e2)) => {
                self.play_road_building(player, e1, e2)
            }
            (TurnPhase::Main, Action::PlayInvention(r1, r2)) => self.play_invention(player, r1, r2),
            (TurnPhase::Main, Action::PlayMonopoly(resource)) => {
                self.play_monopoly(player, resource)
            }
            (TurnPhase::Main, Action::OfferTrade { to, give, receive }) => {
                self.offer_trade(player, to, give, receive)
            }
            (TurnPhase::Main, Action::BankTrade { give, receive }) => {
                self.bank_trade(player, give, receive)
            }
            (TurnPhase::Main, Action::EndTurn) => self.end_turn(),
            _ => Err(CatanError::InvalidPhase),
        }
    }

    /// Roll two dice; on a 7 enter robber resolution, otherwise produce resources.
    fn roll_dice(&mut self) -> Result<(), CatanError> {
        let d1 = thread_rng().gen_range(1..=6);
        let d2 = thread_rng().gen_range(1..=6);
        self.dice = Some((d1, d2));
        let sum = d1 + d2;

        if sum == 7 {
            self.discard_over_7();
            self.phase = GamePhase::Turn(TurnPhase::ResolveSeven);
            return Ok(());
        }

        self.produce_resources(sum);
        self.phase = GamePhase::Turn(TurnPhase::Main);
        Ok(())
    }

    /// Discard half of each player's hand (rounded down) when a 7 is rolled.
    fn discard_over_7(&mut self) {
        // In a real game each player over 7 cards discards half. In this synchronous
        // implementation we discard half automatically (rounded down).
        let to_discard: Vec<(PlayerId, u8)> = self
            .players
            .iter()
            .map(|p| (p.id, p.resources.total()))
            .filter(|(_, total)| *total > 7)
            .map(|(id, total)| (id, total / 2))
            .collect();
        for (id, amount) in to_discard {
            self.discard_random(id, amount);
        }
    }

    /// Deterministically discard one resource at a time across available types.
    fn discard_random(&mut self, player: PlayerId, mut amount: u8) {
        let p = &mut self.players[player];
        let all_resources: Vec<Resource> = Resource::all()
            .iter()
            .filter(|&&r| p.resources.get(r) > 0)
            .copied()
            .collect();

        // Naive deterministic discard: remove one from each resource in turn.
        let mut idx = 0;
        while amount > 0 {
            let resource = all_resources[idx % all_resources.len()];
            if p.resources.get(resource) > 0 {
                p.resources.sub(resource, 1);
                amount -= 1;
            }
            idx += 1;
        }
    }

    /// Distribute resources to settlements and cities touching the rolled number.
    fn produce_resources(&mut self, roll: u8) {
        for tile in &self.board.tiles {
            if tile.has_robber || tile.number != Some(roll) {
                continue;
            }
            if let Some(resource) = tile.terrain.resource() {
                for &vertex_id in &tile.vertices {
                    if let Some(Building::City(pid)) = self.board.vertices[vertex_id].building {
                        self.players[pid].resources.add(resource, 2);
                    } else if let Some(Building::Settlement(pid)) =
                        self.board.vertices[vertex_id].building
                    {
                        self.players[pid].resources.add(resource, 1);
                    }
                }
            }
        }
    }

    /// Move the robber to a new tile and optionally steal a card from an adjacent player.
    fn move_robber(
        &mut self,
        player: PlayerId,
        tile_id: TileId,
        target: Option<PlayerId>,
    ) -> Result<(), CatanError> {
        if tile_id >= self.board.tiles.len() {
            return Err(CatanError::InvalidRobberLocation);
        }

        let current_robber = self.board.tiles.iter().find(|t| t.has_robber).map(|t| t.id);
        if Some(tile_id) == current_robber {
            return Err(CatanError::InvalidRobberLocation);
        }

        let mut stolen: Option<(PlayerId, Resource)> = None;
        if let Some(target_id) = target {
            let victims: Vec<PlayerId> = self.board.tiles[tile_id]
                .vertices
                .iter()
                .filter_map(|&v| match self.board.vertices[v].building {
                    Some(Building::Settlement(pid) | Building::City(pid)) => Some(pid),
                    _ => None,
                })
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();

            if victims.contains(&target_id) && target_id != player {
                if let Some(resource) = self.steal_random_card(target_id) {
                    stolen = Some((target_id, resource));
                }
            }
        }

        if let Some(t) = self.board.tiles.iter_mut().find(|t| t.has_robber) {
            t.has_robber = false;
        }
        self.board.tiles[tile_id].has_robber = true;

        if let Some((target_id, resource)) = stolen {
            self.players[target_id].resources.sub(resource, 1);
            self.players[player].resources.add(resource, 1);
        }

        Ok(())
    }

    /// Pick a random resource card from a player, if they have any.
    fn steal_random_card(&self, player_id: PlayerId) -> Option<Resource> {
        let p = &self.players[player_id];
        let available: Vec<Resource> = Resource::all()
            .into_iter()
            .filter(|r| p.resources.get(*r) > 0)
            .collect();
        available.choose(&mut thread_rng()).copied()
    }

    /// Pay for and build a road, then recalculate longest road.
    fn build_road_action(&mut self, player: PlayerId, edge_id: EdgeId) -> Result<(), CatanError> {
        if !self.can_build_road(player, edge_id) {
            return Err(CatanError::InvalidBuildLocation);
        }

        let cost = road_cost();
        if !self.players[player].resources.can_afford(&cost) {
            return Err(CatanError::NotEnoughResources);
        }

        self.pay_cost(player, &cost);
        self.build_road(edge_id, player)?;
        self.recalculate_longest_road();
        Ok(())
    }

    /// A road is buildable if the edge is free, endpoints are not blocked by opponents,
    /// and it reaches the player's road network.
    fn can_build_road(&self, player: PlayerId, edge_id: EdgeId) -> bool {
        let edge = &self.board.edges[edge_id];
        if edge.road.is_some() {
            return false;
        }

        let a = edge.endpoints[0];
        let b = edge.endpoints[1];
        if self.is_blocked_vertex(a, player) || self.is_blocked_vertex(b, player) {
            return false;
        }

        let reachable = self.reachable_vertices(player);
        reachable.contains(&a) || reachable.contains(&b)
    }

    /// Flood-fill the player's road network, stopping at opponent settlements/cities.
    fn reachable_vertices(&self, player: PlayerId) -> HashSet<VertexId> {
        let mut visited = HashSet::new();
        let mut queue: Vec<VertexId> = Vec::new();

        for (id, vertex) in self.board.vertices.iter().enumerate() {
            if matches!(
                vertex.building,
                Some(Building::Settlement(pid) | Building::City(pid)) if pid == player
            ) {
                visited.insert(id);
                queue.push(id);
            }
        }

        while let Some(v) = queue.pop() {
            for &edge_id in &self.board.vertices[v].incident_edges {
                if self.board.edges[edge_id].road == Some(player) {
                    if let Some(other) = self.board.edges[edge_id].other_vertex(v) {
                        if !self.is_blocked_vertex(other, player) && visited.insert(other) {
                            queue.push(other);
                        }
                    }
                }
            }
        }

        visited
    }

    /// Pay for and build a settlement, then recalculate longest road.
    fn build_settlement_action(
        &mut self,
        player: PlayerId,
        vertex_id: VertexId,
    ) -> Result<(), CatanError> {
        if self.board.vertices[vertex_id].building.is_some() {
            return Err(CatanError::InvalidBuildLocation);
        }

        for &adj in &self.board.adjacent_vertices(vertex_id) {
            if self.board.vertices[adj].building.is_some() {
                return Err(CatanError::InvalidBuildLocation);
            }
        }

        let connected = self.board.vertices[vertex_id]
            .incident_edges
            .iter()
            .any(|&e| self.board.edges[e].road == Some(player));
        if !connected {
            return Err(CatanError::InvalidBuildLocation);
        }

        let cost = settlement_cost();
        if !self.players[player].resources.can_afford(&cost) {
            return Err(CatanError::NotEnoughResources);
        }
        if self.players[player].remaining_settlements == 0 {
            return Err(CatanError::NoPiecesRemaining);
        }

        self.pay_cost(player, &cost);
        self.settle_building(vertex_id, Building::Settlement(player))?;
        self.recalculate_longest_road();
        Ok(())
    }

    /// Upgrade an owned settlement to a city, refunding the settlement piece.
    fn build_city_action(
        &mut self,
        player: PlayerId,
        vertex_id: VertexId,
    ) -> Result<(), CatanError> {
        match self.board.vertices[vertex_id].building {
            Some(Building::Settlement(pid)) if pid == player => {}
            _ => return Err(CatanError::InvalidBuildLocation),
        }

        let cost = city_cost();
        if !self.players[player].resources.can_afford(&cost) {
            return Err(CatanError::NotEnoughResources);
        }
        if self.players[player].remaining_cities == 0 {
            return Err(CatanError::NoPiecesRemaining);
        }

        self.pay_cost(player, &cost);
        self.players[player].remaining_settlements += 1;
        self.players[player].remaining_cities -= 1;
        self.board.vertices[vertex_id].building = Some(Building::City(player));
        self.recalculate_longest_road();
        Ok(())
    }

    /// Buy a development card and place it in the player's unplayable new-card pile.
    fn buy_development_card(&mut self, player: PlayerId) -> Result<(), CatanError> {
        let cost = Resources {
            ore: 1,
            grain: 1,
            wool: 1,
            ..Default::default()
        };
        if !self.players[player].resources.can_afford(&cost) {
            return Err(CatanError::NotEnoughResources);
        }
        if self.development_deck.is_empty() {
            return Err(CatanError::NoDevCards);
        }

        self.pay_cost(player, &cost);
        let card = self.development_deck.pop().unwrap();
        self.players[player].new_cards.push(card);
        Ok(())
    }

    /// Play a Knight card, counting toward largest army and moving the robber.
    fn play_knight(
        &mut self,
        player: PlayerId,
        tile_id: TileId,
        target: Option<PlayerId>,
    ) -> Result<(), CatanError> {
        self.validate_play_dev_card(player, DevelopmentCard::Knight)?;
        self.players[player].remove_card(DevelopmentCard::Knight);
        self.players[player].played_knights += 1;
        self.played_dev_card_this_turn = true;
        self.recalculate_largest_army();

        // Move the robber. Treat the play as a robber resolution.
        self.move_robber(player, tile_id, target)
    }

    /// Play Road Building to place two free roads, rolling back the first if the second fails.
    fn play_road_building(
        &mut self,
        player: PlayerId,
        e1: EdgeId,
        e2: EdgeId,
    ) -> Result<(), CatanError> {
        self.validate_play_dev_card(player, DevelopmentCard::RoadBuilding)?;
        if self.players[player].remaining_roads < 2 {
            return Err(CatanError::NoPiecesRemaining);
        }
        // Place both roads for free, validating sequentially so the second may
        // connect to the first.
        if !self.can_build_road(player, e1) {
            return Err(CatanError::InvalidBuildLocation);
        }
        self.build_road(e1, player)?;
        if !self.can_build_road(player, e2) {
            // Roll back the first road on failure.
            self.board.edges[e1].road = None;
            self.players[player].remaining_roads += 1;
            return Err(CatanError::InvalidBuildLocation);
        }
        self.build_road(e2, player)?;
        self.players[player].remove_card(DevelopmentCard::RoadBuilding);
        self.played_dev_card_this_turn = true;
        self.recalculate_longest_road();
        Ok(())
    }

    /// Play an Invention (Year of Plenty) card to gain one or two resources.
    fn play_invention(
        &mut self,
        player: PlayerId,
        r1: Resource,
        r2: Option<Resource>,
    ) -> Result<(), CatanError> {
        self.validate_play_dev_card(player, DevelopmentCard::Invention)?;
        self.players[player].resources.add(r1, 1);
        if let Some(r) = r2 {
            if r != r1 {
                self.players[player].resources.add(r, 1);
            } else {
                self.players[player].resources.add(r1, 1);
            }
        }
        self.players[player].remove_card(DevelopmentCard::Invention);
        self.played_dev_card_this_turn = true;
        Ok(())
    }

    /// Play a Monopoly card to take all of one resource from opponents.
    fn play_monopoly(&mut self, player: PlayerId, resource: Resource) -> Result<(), CatanError> {
        self.validate_play_dev_card(player, DevelopmentCard::Monopoly)?;
        let mut total = 0u8;
        for pid in 0..self.players.len() {
            if pid == player {
                continue;
            }
            let amount = self.players[pid].resources.get(resource);
            self.players[pid].resources.sub(resource, amount);
            total += amount;
        }
        self.players[player].resources.add(resource, total);
        self.players[player].remove_card(DevelopmentCard::Monopoly);
        self.played_dev_card_this_turn = true;
        Ok(())
    }

    /// Only one non-victory-point dev card may be played per turn.
    fn validate_play_dev_card(
        &self,
        player: PlayerId,
        card: DevelopmentCard,
    ) -> Result<(), CatanError> {
        if self.played_dev_card_this_turn && card != DevelopmentCard::VictoryPoint {
            return Err(CatanError::CannotPlayDevCard);
        }
        if !self.players[player].has_card(card) {
            return Err(CatanError::CannotPlayDevCard);
        }
        Ok(())
    }

    /// Offer a trade to another player after verifying both sides can pay.
    fn offer_trade(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        give: Resources,
        receive: Resources,
    ) -> Result<(), CatanError> {
        if from == to || to >= self.players.len() {
            return Err(CatanError::InvalidTrade);
        }
        if !self.players[from].resources.can_afford(&give) {
            return Err(CatanError::NotEnoughResources);
        }
        if !self.players[to].resources.can_afford(&receive) {
            return Err(CatanError::InvalidTrade);
        }

        self.pending_trade = Some(TradeOffer {
            from,
            to,
            give,
            receive,
        });
        Ok(())
    }

    /// Accept or decline a pending trade, restoring the offer if it was for someone else.
    fn handle_trade_response(&mut self, player: PlayerId, accept: bool) -> Result<(), CatanError> {
        let trade = self
            .pending_trade
            .take()
            .ok_or(CatanError::NoPendingTrade)?;
        if trade.to != player {
            // Restore the offer if it wasn't for this player.
            self.pending_trade = Some(trade);
            return Err(CatanError::TradeNotForYou);
        }

        if !accept {
            return Ok(());
        }

        if !self.players[trade.from].resources.can_afford(&trade.give)
            || !self.players[trade.to].resources.can_afford(&trade.receive)
        {
            return Err(CatanError::InvalidTrade);
        }

        self.transfer_resources(trade.from, trade.to, &trade.give)?;
        self.transfer_resources(trade.to, trade.from, &trade.receive)?;
        Ok(())
    }

    /// Exchange resources with the bank at the player's best port rate.
    fn bank_trade(
        &mut self,
        player: PlayerId,
        give: Resource,
        receive: Resource,
    ) -> Result<(), CatanError> {
        if give == receive {
            return Err(CatanError::InvalidTrade);
        }

        let rate = self.best_trade_rate(player, give);
        let p = &mut self.players[player];
        if p.resources.get(give) < rate {
            return Err(CatanError::NotEnoughResources);
        }

        p.resources.sub(give, rate);
        p.resources.add(receive, 1);
        Ok(())
    }

    /// Find the cheapest exchange rate for a resource based on ports the player touches.
    fn best_trade_rate(&self, player: PlayerId, resource: Resource) -> u8 {
        let mut best = 4u8;
        for vertex_id in self.settlements_and_cities(player) {
            if let Some(port) = self.board.vertices[vertex_id].port {
                match port {
                    Port::Generic3To1 => best = best.min(3),
                    Port::Specific2To1(r) if r == resource => best = best.min(2),
                    _ => {}
                }
            }
        }
        best
    }

    /// Return the vertices occupied by this player's settlements and cities.
    fn settlements_and_cities(&self, player: PlayerId) -> Vec<VertexId> {
        self.board
            .vertices
            .iter()
            .filter(|v| {
                matches!(
                    v.building,
                    Some(Building::Settlement(pid) | Building::City(pid)) if pid == player
                )
            })
            .map(|v| v.id)
            .collect()
    }

    /// Move unplayed dev cards to hand, clear turn state, and advance to the next player.
    fn end_turn(&mut self) -> Result<(), CatanError> {
        // Move new cards to hand.
        let current = self.current_player();
        let p = &mut self.players[current];
        p.hand.append(&mut p.new_cards);
        p.new_cards.clear();

        self.played_dev_card_this_turn = false;
        self.pending_trade = None;
        self.dice = None;

        self.current_player_idx = (self.current_player_idx + 1) % self.player_order.len();
        self.phase = GamePhase::Turn(TurnPhase::Roll);
        Ok(())
    }

    /// Grant resources from the tiles touching the second setup settlement.
    fn collect_starting_resources(&mut self, settlement: VertexId) -> Result<(), CatanError> {
        let current = self.current_player();
        let incident_tiles = self.board.vertices[settlement].incident_tiles.clone();
        for &tile_id in &incident_tiles {
            let tile = &self.board.tiles[tile_id];
            if let Some(resource) = tile.terrain.resource() {
                self.players[current].resources.add(resource, 1);
            }
        }
        Ok(())
    }

    /// Place a building on the board and deduct a settlement piece when needed.
    fn settle_building(
        &mut self,
        vertex_id: VertexId,
        building: Building,
    ) -> Result<(), CatanError> {
        self.board.vertices[vertex_id].building = Some(building);
        match building {
            Building::Settlement(pid) => {
                self.players[pid].remaining_settlements -= 1;
            }
            Building::City(_pid) => {}
        }
        Ok(())
    }

    /// Place a road on an edge and deduct a road piece.
    fn build_road(&mut self, edge_id: EdgeId, player: PlayerId) -> Result<(), CatanError> {
        self.board.edges[edge_id].road = Some(player);
        self.players[player].remaining_roads -= 1;
        Ok(())
    }

    /// Subtract a cost from a player's resources.
    fn pay_cost(&mut self, player: PlayerId, cost: &Resources) {
        let p = &mut self.players[player];
        p.resources.brick -= cost.brick;
        p.resources.lumber -= cost.lumber;
        p.resources.ore -= cost.ore;
        p.resources.grain -= cost.grain;
        p.resources.wool -= cost.wool;
    }

    /// Move a bundle of resources from one player to another.
    fn transfer_resources(
        &mut self,
        from: PlayerId,
        to: PlayerId,
        amount: &Resources,
    ) -> Result<(), CatanError> {
        if !self.players[from].resources.can_afford(amount) {
            return Err(CatanError::NotEnoughResources);
        }
        let p_from = &mut self.players[from];
        p_from.resources.brick -= amount.brick;
        p_from.resources.lumber -= amount.lumber;
        p_from.resources.ore -= amount.ore;
        p_from.resources.grain -= amount.grain;
        p_from.resources.wool -= amount.wool;

        let p_to = &mut self.players[to];
        p_to.resources.brick += amount.brick;
        p_to.resources.lumber += amount.lumber;
        p_to.resources.ore += amount.ore;
        p_to.resources.grain += amount.grain;
        p_to.resources.wool += amount.wool;
        Ok(())
    }

    /// Update the winner and finish the game if any player reaches 10 VP.
    pub fn check_winner(&mut self) {
        for &player_id in &self.player_order {
            let p = &self.players[player_id];
            let mut vps = p.public_victory_points();
            vps += self.settlement_city_points(player_id);
            vps += self.longest_road_points(player_id);
            vps += self.largest_army_points(player_id);
            if vps >= 10 {
                self.winner = Some(player_id);
                self.phase = GamePhase::Finished;
                return;
            }
        }
    }

    /// Count public victory points from this player's settlements and cities.
    fn settlement_city_points(&self, player: PlayerId) -> u8 {
        self.board
            .vertices
            .iter()
            .filter(|v| {
                matches!(
                    v.building,
                    Some(Building::Settlement(pid)) if pid == player
                )
            })
            .count() as u8
            + self
                .board
                .vertices
                .iter()
                .filter(|v| {
                    matches!(
                        v.building,
                        Some(Building::City(pid)) if pid == player
                    )
                })
                .count() as u8
                * 2
    }

    /// Award 2 VP if this player holds the longest road.
    fn longest_road_points(&self, player: PlayerId) -> u8 {
        if self.longest_road == Some(player) {
            2
        } else {
            0
        }
    }

    /// Award 2 VP if this player holds the largest army.
    fn largest_army_points(&self, player: PlayerId) -> u8 {
        if self.largest_army == Some(player) {
            2
        } else {
            0
        }
    }

    /// Award largest army to the player with the most played knights (at least 3).
    fn recalculate_largest_army(&mut self) {
        let mut best: Option<(PlayerId, u8)> = None;
        for &pid in &self.player_order {
            let knights = self.players[pid].played_knights;
            if knights >= 3 {
                if best.map_or(true, |(_, k)| knights > k) {
                    best = Some((pid, knights));
                }
            }
        }
        if let Some((pid, _)) = best {
            self.largest_army = Some(pid);
        }
    }

    /// Award longest road to the player with the longest road (at least 5).
    fn recalculate_longest_road(&mut self) {
        let mut best: Option<(PlayerId, usize)> = None;
        for &pid in &self.player_order {
            let length = self.longest_road_for_player(pid);
            if length >= 5 {
                if best.map_or(true, |(_, l)| length > l) {
                    best = Some((pid, length));
                }
            }
        }
        if let Some((pid, _)) = best {
            self.longest_road = Some(pid);
        }
    }

    /// Compute the longest contiguous road for a player, treating opponent settlements as dead ends.
    fn longest_road_for_player(&self, player: PlayerId) -> usize {
        let player_edges: Vec<EdgeId> = self
            .board
            .edges
            .iter()
            .filter(|e| e.road == Some(player))
            .map(|e| e.id)
            .collect();

        if player_edges.is_empty() {
            return 0;
        }

        // Build a graph of the player's roads. Opponent buildings block the path.
        // Road edges that end at an opponent building are modelled as leaves.
        let mut adj: HashMap<VertexId, Vec<VertexId>> = HashMap::new();
        let mut next_virtual = self.board.vertices.len();

        for &edge_id in &player_edges {
            let edge = &self.board.edges[edge_id];
            let a = edge.endpoints[0];
            let b = edge.endpoints[1];
            let a_blocked = self.is_blocked_vertex(a, player);
            let b_blocked = self.is_blocked_vertex(b, player);

            if a_blocked && b_blocked {
                continue;
            }

            if !a_blocked && !b_blocked {
                adj.entry(a).or_default().push(b);
                adj.entry(b).or_default().push(a);
            } else if a_blocked {
                let leaf = next_virtual;
                next_virtual += 1;
                adj.entry(b).or_default().push(leaf);
                adj.entry(leaf).or_default().push(b);
            } else {
                let leaf = next_virtual;
                next_virtual += 1;
                adj.entry(a).or_default().push(leaf);
                adj.entry(leaf).or_default().push(a);
            }
        }

        let mut best = 0usize;
        let starts: Vec<VertexId> = adj.keys().copied().collect();
        for start in starts {
            let mut visited = HashSet::new();
            best = best.max(self.dfs_longest(start, &adj, &mut visited));
        }
        best
    }

    /// An opponent settlement or city blocks a vertex for this player.
    fn is_blocked_vertex(&self, vertex: VertexId, player: PlayerId) -> bool {
        match self.board.vertices[vertex].building {
            Some(Building::Settlement(pid) | Building::City(pid)) if pid != player => true,
            _ => false,
        }
    }

    /// Brute-force search for the longest simple path in the player's road graph.
    fn dfs_longest(
        &self,
        current: VertexId,
        adj: &HashMap<VertexId, Vec<VertexId>>,
        visited: &mut HashSet<VertexId>,
    ) -> usize {
        visited.insert(current);
        let mut max = 0usize;
        if let Some(neighbors) = adj.get(&current) {
            for &next in neighbors {
                if !visited.contains(&next) {
                    max = max.max(1 + self.dfs_longest(next, adj, visited));
                }
            }
        }
        visited.remove(&current);
        max
    }
}

pub fn road_cost() -> Resources {
    Resources {
        brick: 1,
        lumber: 1,
        ..Default::default()
    }
}

pub fn settlement_cost() -> Resources {
    Resources {
        brick: 1,
        lumber: 1,
        grain: 1,
        wool: 1,
        ..Default::default()
    }
}

pub fn city_cost() -> Resources {
    Resources {
        ore: 3,
        grain: 2,
        ..Default::default()
    }
}

fn development_deck() -> Vec<DevelopmentCard> {
    let mut deck = Vec::with_capacity(25);
    for _ in 0..14 {
        deck.push(DevelopmentCard::Knight);
    }
    for _ in 0..5 {
        deck.push(DevelopmentCard::VictoryPoint);
    }
    for _ in 0..2 {
        deck.push(DevelopmentCard::RoadBuilding);
        deck.push(DevelopmentCard::Invention);
        deck.push(DevelopmentCard::Monopoly);
    }
    deck
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_players() -> Vec<Player> {
        vec![
            Player::new(0, "A".to_string(), PlayerColor::Red),
            Player::new(1, "B".to_string(), PlayerColor::Blue),
        ]
    }

    fn valid_settlement_vertex(game: &Game) -> Option<VertexId> {
        for (id, vertex) in game.board.vertices.iter().enumerate() {
            if vertex.building.is_some() {
                continue;
            }
            let mut blocked = false;
            for &edge_id in &vertex.incident_edges {
                let edge = &game.board.edges[edge_id];
                let other = edge.endpoints[0] + edge.endpoints[1] - id;
                if game.board.vertices[other].building.is_some() {
                    blocked = true;
                    break;
                }
            }
            if !blocked {
                return Some(id);
            }
        }
        None
    }

    #[test]
    fn full_setup_and_turn() {
        let mut game = Game::new(make_players()).unwrap();
        assert!(matches!(
            game.phase,
            GamePhase::Setup {
                round: 0,
                reverse: false,
                awaiting_road_for: None
            }
        ));

        // 4 setup placements: settlement + road each
        for _ in 0..4 {
            let player = game.current_player();
            let vertex = valid_settlement_vertex(&game).expect("valid vertex");
            game.apply_action(player, Action::PlaceSettlement(vertex))
                .expect("place settlement");

            let road = game.board.vertices[vertex]
                .incident_edges
                .iter()
                .copied()
                .find(|&e| game.board.edges[e].road.is_none())
                .expect("empty incident edge");
            game.apply_action(player, Action::PlaceRoad(road))
                .expect("place road");
        }

        // After setup, phase should be a turn
        assert!(matches!(game.phase, GamePhase::Turn(_)));

        // Roll dice once
        let player = game.current_player();
        game.apply_action(player, Action::RollDice).expect("roll");

        // If a 7 was rolled, resolve robber
        if let GamePhase::Turn(TurnPhase::ResolveSeven) = game.phase {
            let tile = game
                .board
                .tiles
                .iter()
                .find(|t| !t.has_robber && t.terrain != Terrain::Desert)
                .map(|t| t.id)
                .expect("non-robber tile");
            game.apply_action(player, Action::MoveRobber { tile, target: None })
                .expect("move robber");
        }

        assert!(matches!(game.phase, GamePhase::Turn(_)));

        // End turn
        game.apply_action(player, Action::EndTurn)
            .expect("end turn");
        assert!(matches!(game.phase, GamePhase::Turn(_)));
        assert!(game.current_player() != player);
    }

    fn valid_edge_for_settlement(game: &Game, vertex: VertexId) -> EdgeId {
        game.board.vertices[vertex]
            .incident_edges
            .iter()
            .copied()
            .find(|&e| game.board.edges[e].road.is_none())
            .expect("free incident edge")
    }

    fn run_setup(game: &mut Game) {
        for _ in 0..4 {
            let player = game.current_player();
            let vertex = valid_settlement_vertex(game).expect("valid vertex");
            game.apply_action(player, Action::PlaceSettlement(vertex))
                .expect("place settlement");
            let road = valid_edge_for_settlement(&game, vertex);
            game.apply_action(player, Action::PlaceRoad(road))
                .expect("place road");
        }
    }

    fn other_player(game: &Game, current: PlayerId) -> PlayerId {
        game.players
            .iter()
            .find(|p| p.id != current)
            .map(|p| p.id)
            .expect("other player")
    }

    fn enter_main_phase(game: &mut Game) {
        let player = game.current_player();
        game.apply_action(player, Action::RollDice).expect("roll");
        if let GamePhase::Turn(TurnPhase::ResolveSeven) = game.phase {
            let tile = game
                .board
                .tiles
                .iter()
                .find(|t| !t.has_robber)
                .map(|t| t.id)
                .expect("non-robber tile");
            game.apply_action(player, Action::MoveRobber { tile, target: None })
                .expect("move robber");
        }
    }

    #[test]
    fn invalid_player_count_rejected() {
        let one = vec![Player::new(0, "A".to_string(), PlayerColor::Red)];
        assert_eq!(Game::new(one).unwrap_err(), CatanError::InvalidPlayer);

        let five: Vec<Player> = (0..5)
            .map(|i| Player::new(i, format!("P{i}"), PlayerColor::Red))
            .collect();
        assert_eq!(Game::new(five).unwrap_err(), CatanError::InvalidPlayer);
    }

    #[test]
    fn invalid_player_id_rejected() {
        let mut game = Game::new(make_players()).unwrap();
        let result = game.apply_action(99, Action::RollDice);
        assert_eq!(result.unwrap_err(), CatanError::InvalidPlayer);
    }

    #[test]
    fn not_your_turn_during_setup() {
        let mut game = Game::new(make_players()).unwrap();
        let current = game.current_player();
        let other = other_player(&game, current);
        let vertex = valid_settlement_vertex(&game).unwrap();
        let result = game.apply_action(other, Action::PlaceSettlement(vertex));
        assert_eq!(result.unwrap_err(), CatanError::NotYourTurn);
    }

    #[test]
    fn setup_enforces_settlement_then_road_order() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        let vertex = valid_settlement_vertex(&game).unwrap();
        game.apply_action(player, Action::PlaceSettlement(vertex))
            .unwrap();

        // Still awaiting a road, so another settlement is invalid.
        let next_vertex = valid_settlement_vertex(&game).unwrap();
        assert_eq!(
            game.apply_action(player, Action::PlaceSettlement(next_vertex))
                .unwrap_err(),
            CatanError::InvalidPhase
        );

        let road = valid_edge_for_settlement(&game, vertex);
        game.apply_action(player, Action::PlaceRoad(road)).unwrap();

        // Now awaiting the next settlement, so a road is invalid.
        assert_eq!(
            game.apply_action(game.current_player(), Action::PlaceRoad(road))
                .unwrap_err(),
            CatanError::InvalidPhase
        );
    }

    #[test]
    fn setup_road_must_touch_settlement() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        let vertex = valid_settlement_vertex(&game).unwrap();
        game.apply_action(player, Action::PlaceSettlement(vertex))
            .unwrap();

        let bad_edge = game
            .board
            .edges
            .iter()
            .find(|e| !e.endpoints.contains(&vertex) && e.road.is_none())
            .map(|e| e.id)
            .expect("unconnected edge");
        assert_eq!(
            game.apply_action(player, Action::PlaceRoad(bad_edge))
                .unwrap_err(),
            CatanError::InvalidBuildLocation
        );
    }

    #[test]
    fn setup_respects_distance_rule() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        let vertex = valid_settlement_vertex(&game).unwrap();
        game.apply_action(player, Action::PlaceSettlement(vertex))
            .unwrap();

        let adjacent = game
            .board
            .adjacent_vertices(vertex)
            .into_iter()
            .find(|&a| game.board.vertices[a].building.is_none())
            .expect("free adjacent vertex");

        // Reset phase so we can attempt another settlement placement.
        game.phase = GamePhase::Setup {
            round: 0,
            reverse: false,
            awaiting_road_for: None,
        };
        assert_eq!(
            game.validate_setup_settlement(player, adjacent)
                .unwrap_err(),
            CatanError::InvalidBuildLocation
        );
    }

    #[test]
    fn produce_resources_gives_one_per_settlement() {
        let mut game = Game::new(make_players()).unwrap();

        let mut target = None;
        for tile in &game.board.tiles {
            if tile.has_robber || tile.terrain == Terrain::Desert {
                continue;
            }
            let Some(number) = tile.number else { continue };
            let Some(resource) = tile.terrain.resource() else {
                continue;
            };
            let Some(vertex) = tile.vertices.iter().copied().find(|&v| {
                game.board.vertices[v].building.is_none()
                    && game
                        .board
                        .adjacent_vertices(v)
                        .iter()
                        .all(|&a| game.board.vertices[a].building.is_none())
            }) else {
                continue;
            };
            target = Some((number, resource, vertex));
            break;
        }
        let (number, resource, vertex) = target.expect("productive tile with free vertex");

        let player = game.current_player();
        game.board.vertices[vertex].building = Some(Building::Settlement(player));
        let before = game.players[player].resources.get(resource);
        game.produce_resources(number);
        assert_eq!(game.players[player].resources.get(resource), before + 1);
    }

    #[test]
    fn produce_resources_city_gives_two() {
        let mut game = Game::new(make_players()).unwrap();

        let mut target = None;
        for tile in &game.board.tiles {
            if tile.has_robber || tile.terrain == Terrain::Desert {
                continue;
            }
            let Some(number) = tile.number else { continue };
            let Some(resource) = tile.terrain.resource() else {
                continue;
            };
            let Some(vertex) = tile.vertices.iter().copied().find(|&v| {
                game.board.vertices[v].building.is_none()
                    && game
                        .board
                        .adjacent_vertices(v)
                        .iter()
                        .all(|&a| game.board.vertices[a].building.is_none())
            }) else {
                continue;
            };
            target = Some((number, resource, vertex));
            break;
        }
        let (number, resource, vertex) = target.expect("productive tile with free vertex");

        let player = game.current_player();
        game.board.vertices[vertex].building = Some(Building::City(player));
        let before = game.players[player].resources.get(resource);
        game.produce_resources(number);
        assert_eq!(game.players[player].resources.get(resource), before + 2);
    }

    #[test]
    fn build_road_needs_resources_and_connection() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        let settlement = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == player))
            .map(|v| v.id)
            .expect("player settlement");
        let e1 = valid_edge_for_settlement(&game, settlement);

        game.players[player].resources = Resources {
            brick: 1,
            lumber: 1,
            ..Default::default()
        };
        game.apply_action(player, Action::BuildRoad(e1)).unwrap();
        assert_eq!(game.board.edges[e1].road, Some(player));
        assert_eq!(game.players[player].resources, Resources::zero());

        // A second road is unaffordable now.
        let e2 = game.board.vertices[settlement]
            .incident_edges
            .iter()
            .copied()
            .find(|&e| e != e1 && game.board.edges[e].road.is_none())
            .expect("second incident edge");
        assert_eq!(
            game.apply_action(player, Action::BuildRoad(e2))
                .unwrap_err(),
            CatanError::NotEnoughResources
        );

        // Replenish resources but the first edge is now occupied.
        game.players[player].resources = Resources {
            brick: 1,
            lumber: 1,
            ..Default::default()
        };
        assert_eq!(
            game.apply_action(player, Action::BuildRoad(e1))
                .unwrap_err(),
            CatanError::InvalidBuildLocation
        );
    }

    #[test]
    fn build_settlement_needs_resources_and_rules() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        let settlement = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == player))
            .map(|v| v.id)
            .unwrap();

        // Adjacent vertex violates the distance rule.
        let adjacent = game
            .board
            .adjacent_vertices(settlement)
            .into_iter()
            .find(|&a| game.board.vertices[a].building.is_none())
            .unwrap();
        game.players[player].resources = Resources {
            brick: 1,
            lumber: 1,
            grain: 1,
            wool: 1,
            ..Default::default()
        };
        assert_eq!(
            game.apply_action(player, Action::BuildSettlement(adjacent))
                .unwrap_err(),
            CatanError::InvalidBuildLocation
        );

        // A free, isolated vertex cannot be built without a connecting road.
        let target = game
            .board
            .vertices
            .iter()
            .find(|v| {
                v.building.is_none()
                    && game
                        .board
                        .adjacent_vertices(v.id)
                        .iter()
                        .all(|&a| game.board.vertices[a].building.is_none())
            })
            .map(|v| v.id)
            .unwrap();
        assert_eq!(
            game.apply_action(player, Action::BuildSettlement(target))
                .unwrap_err(),
            CatanError::InvalidBuildLocation
        );

        // Connect it with a road, then build successfully.
        let edge = game.board.vertices[target]
            .incident_edges
            .iter()
            .copied()
            .find(|&e| game.board.edges[e].road.is_none())
            .unwrap();
        game.board.edges[edge].road = Some(player);
        game.players[player].resources = Resources {
            brick: 1,
            lumber: 1,
            grain: 1,
            wool: 1,
            ..Default::default()
        };
        game.apply_action(player, Action::BuildSettlement(target))
            .unwrap();
        assert_eq!(
            game.board.vertices[target].building,
            Some(Building::Settlement(player))
        );

        // The same vertex is now occupied.
        assert_eq!(
            game.apply_action(player, Action::BuildSettlement(target))
                .unwrap_err(),
            CatanError::InvalidBuildLocation
        );
    }

    #[test]
    fn build_city_upgrades_settlement() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        let settlement = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == player))
            .map(|v| v.id)
            .unwrap();
        game.players[player].resources = Resources {
            ore: 3,
            grain: 2,
            ..Default::default()
        };
        game.apply_action(player, Action::BuildCity(settlement))
            .unwrap();
        assert_eq!(
            game.board.vertices[settlement].building,
            Some(Building::City(player))
        );
        assert_eq!(game.players[player].remaining_settlements, 4);
        assert_eq!(game.players[player].remaining_cities, 3);

        // Cannot build a city on an empty vertex.
        let empty_vertex = game
            .board
            .vertices
            .iter()
            .find(|v| v.building.is_none())
            .map(|v| v.id)
            .unwrap();
        game.players[player].resources = Resources {
            ore: 3,
            grain: 2,
            ..Default::default()
        };
        assert_eq!(
            game.apply_action(player, Action::BuildCity(empty_vertex))
                .unwrap_err(),
            CatanError::InvalidBuildLocation
        );
    }

    #[test]
    fn buy_development_card_adds_to_new_cards() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        let before = game.development_deck.len();

        game.players[player].resources = Resources {
            ore: 1,
            grain: 1,
            wool: 1,
            ..Default::default()
        };
        game.apply_action(player, Action::BuyDevelopmentCard)
            .unwrap();
        assert_eq!(game.development_deck.len(), before - 1);
        assert_eq!(game.players[player].new_cards.len(), 1);
        assert_eq!(game.players[player].resources, Resources::zero());

        assert_eq!(
            game.apply_action(player, Action::BuyDevelopmentCard)
                .unwrap_err(),
            CatanError::NotEnoughResources
        );
    }

    #[test]
    fn play_knight_moves_robber_and_increments_knights() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        let player = game.current_player();
        game.players[player].hand.push(DevelopmentCard::Knight);

        let tile = game
            .board
            .tiles
            .iter()
            .find(|t| !t.has_robber)
            .map(|t| t.id)
            .expect("non-robber tile");
        game.apply_action(player, Action::PlayKnight { tile, target: None })
            .unwrap();
        assert!(game.board.tiles[tile].has_robber);
        assert_eq!(game.players[player].played_knights, 1);
        assert!(game.players[player].hand.is_empty());
    }

    #[test]
    fn only_one_dev_card_per_turn() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        let settlement = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == player))
            .map(|v| v.id)
            .unwrap();
        let v2 = game
            .board
            .adjacent_vertices(settlement)
            .into_iter()
            .find(|&a| game.board.vertices[a].building.is_none())
            .unwrap();
        let e1 = game.board.edge_between(settlement, v2).unwrap();
        let e2 = game.board.vertices[v2]
            .incident_edges
            .iter()
            .copied()
            .find(|&e| e != e1 && game.board.edges[e].road.is_none())
            .unwrap();

        game.players[player].hand.push(DevelopmentCard::Knight);
        game.players[player]
            .hand
            .push(DevelopmentCard::RoadBuilding);

        let tile = game
            .board
            .tiles
            .iter()
            .find(|t| !t.has_robber)
            .map(|t| t.id)
            .unwrap();
        game.apply_action(player, Action::PlayKnight { tile, target: None })
            .unwrap();

        assert_eq!(
            game.apply_action(player, Action::PlayRoadBuilding(e1, e2))
                .unwrap_err(),
            CatanError::CannotPlayDevCard
        );
    }

    #[test]
    fn road_building_rolls_back_on_invalid_second_road() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        let settlement = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == player))
            .map(|v| v.id)
            .unwrap();
        let e1 = valid_edge_for_settlement(&game, settlement);

        game.players[player]
            .hand
            .push(DevelopmentCard::RoadBuilding);
        assert_eq!(
            game.apply_action(player, Action::PlayRoadBuilding(e1, e1))
                .unwrap_err(),
            CatanError::InvalidBuildLocation
        );
        assert_eq!(game.board.edges[e1].road, None);
        assert_eq!(game.players[player].remaining_roads, 13);
        assert!(
            game.players[player]
                .hand
                .contains(&DevelopmentCard::RoadBuilding)
        );
    }

    #[test]
    fn play_invention_with_two_different_resources() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        game.players[player].resources = Resources::zero();
        game.players[player].hand.push(DevelopmentCard::Invention);
        game.apply_action(
            player,
            Action::PlayInvention(Resource::Brick, Some(Resource::Lumber)),
        )
        .unwrap();
        assert_eq!(game.players[player].resources.brick, 1);
        assert_eq!(game.players[player].resources.lumber, 1);
    }

    #[test]
    fn play_invention_with_same_resource_gains_two() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        game.players[player].resources = Resources::zero();
        game.players[player].hand.push(DevelopmentCard::Invention);
        game.apply_action(
            player,
            Action::PlayInvention(Resource::Ore, Some(Resource::Ore)),
        )
        .unwrap();
        assert_eq!(game.players[player].resources.ore, 2);
    }

    #[test]
    fn play_monopoly_transfers_from_opponents() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        let other = other_player(&game, player);
        game.players[player].resources = Resources::zero();
        game.players[other].resources = Resources {
            grain: 3,
            brick: 2,
            ..Default::default()
        };
        game.players[player].hand.push(DevelopmentCard::Monopoly);
        game.apply_action(player, Action::PlayMonopoly(Resource::Grain))
            .unwrap();
        assert_eq!(game.players[other].resources.grain, 0);
        assert_eq!(game.players[player].resources.grain, 3);
        assert_eq!(game.players[other].resources.brick, 2);
    }

    #[test]
    fn trade_offer_accept_transfers_resources() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let from = game.current_player();
        let to = other_player(&game, from);
        game.players[from].resources = Resources {
            brick: 2,
            ..Default::default()
        };
        game.players[to].resources = Resources {
            lumber: 1,
            ..Default::default()
        };

        game.apply_action(
            from,
            Action::OfferTrade {
                to,
                give: Resources {
                    brick: 1,
                    ..Default::default()
                },
                receive: Resources {
                    lumber: 1,
                    ..Default::default()
                },
            },
        )
        .unwrap();
        game.apply_action(to, Action::RespondTrade { accept: true })
            .unwrap();

        assert_eq!(game.players[from].resources.brick, 1);
        assert_eq!(game.players[from].resources.lumber, 1);
        assert_eq!(game.players[to].resources.brick, 1);
        assert_eq!(game.players[to].resources.lumber, 0);
    }

    #[test]
    fn trade_offer_reject_leaves_resources_unchanged() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let from = game.current_player();
        let to = other_player(&game, from);
        game.players[from].resources = Resources {
            brick: 2,
            ..Default::default()
        };
        game.players[to].resources = Resources {
            lumber: 1,
            ..Default::default()
        };

        game.apply_action(
            from,
            Action::OfferTrade {
                to,
                give: Resources {
                    brick: 1,
                    ..Default::default()
                },
                receive: Resources {
                    lumber: 1,
                    ..Default::default()
                },
            },
        )
        .unwrap();
        game.apply_action(to, Action::RespondTrade { accept: false })
            .unwrap();

        assert_eq!(game.players[from].resources.brick, 2);
        assert_eq!(game.players[from].resources.lumber, 0);
        assert_eq!(game.players[to].resources.brick, 0);
        assert_eq!(game.players[to].resources.lumber, 1);
    }

    #[test]
    fn cannot_trade_with_self() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        assert_eq!(
            game.apply_action(
                player,
                Action::OfferTrade {
                    to: player,
                    give: Resources::zero(),
                    receive: Resources::zero(),
                }
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );
    }

    #[test]
    fn bank_trade_uses_best_port_rate() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        // 4:1 with no port.
        game.players[player].resources = Resources {
            brick: 4,
            ..Default::default()
        };
        game.apply_action(
            player,
            Action::BankTrade {
                give: Resource::Brick,
                receive: Resource::Lumber,
            },
        )
        .unwrap();
        assert_eq!(game.players[player].resources.brick, 0);
        assert_eq!(game.players[player].resources.lumber, 1);

        // 3:1 with a generic-port settlement.
        let generic_port_vertex = game
            .board
            .vertices
            .iter()
            .find(|v| v.is_coastal() && v.port == Some(Port::Generic3To1))
            .map(|v| v.id)
            .unwrap();
        game.board.vertices[generic_port_vertex].building = Some(Building::Settlement(player));
        game.players[player].resources = Resources {
            lumber: 3,
            ..Default::default()
        };
        game.apply_action(
            player,
            Action::BankTrade {
                give: Resource::Lumber,
                receive: Resource::Ore,
            },
        )
        .unwrap();
        assert_eq!(game.players[player].resources.lumber, 0);
        assert_eq!(game.players[player].resources.ore, 1);

        // 2:1 with a specific-port settlement.
        let ore_port_vertex = game
            .board
            .vertices
            .iter()
            .find(|v| v.is_coastal() && v.port == Some(Port::Specific2To1(Resource::Ore)))
            .map(|v| v.id)
            .unwrap();
        game.board.vertices[ore_port_vertex].building = Some(Building::Settlement(player));
        game.players[player].resources = Resources {
            ore: 2,
            ..Default::default()
        };
        game.apply_action(
            player,
            Action::BankTrade {
                give: Resource::Ore,
                receive: Resource::Grain,
            },
        )
        .unwrap();
        assert_eq!(game.players[player].resources.ore, 0);
        assert_eq!(game.players[player].resources.grain, 1);
    }

    #[test]
    fn bank_trade_same_resource_is_invalid() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        game.players[player].resources = Resources {
            brick: 4,
            ..Default::default()
        };
        assert_eq!(
            game.apply_action(
                player,
                Action::BankTrade {
                    give: Resource::Brick,
                    receive: Resource::Brick,
                }
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );
    }

    #[test]
    fn seven_roll_discards_half() {
        let mut game = Game::new(make_players()).unwrap();
        game.players[0].resources = Resources {
            brick: 4,
            lumber: 3,
            grain: 2,
            ..Default::default()
        };
        let before = game.players[0].resources.total();
        game.discard_over_7();
        assert_eq!(game.players[0].resources.total(), before - before / 2);
    }

    #[test]
    fn move_robber_must_go_to_new_tile() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        let player = game.current_player();
        game.phase = GamePhase::Turn(TurnPhase::ResolveSeven);

        let current_robber_tile = game
            .board
            .tiles
            .iter()
            .find(|t| t.has_robber)
            .map(|t| t.id)
            .unwrap();
        assert_eq!(
            game.apply_action(
                player,
                Action::MoveRobber {
                    tile: current_robber_tile,
                    target: None,
                }
            )
            .unwrap_err(),
            CatanError::InvalidRobberLocation
        );

        assert_eq!(
            game.apply_action(
                player,
                Action::MoveRobber {
                    tile: 9999,
                    target: None,
                }
            )
            .unwrap_err(),
            CatanError::InvalidRobberLocation
        );
    }

    #[test]
    fn end_turn_resets_turn_state() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let first = game.current_player();

        game.players[first]
            .new_cards
            .push(DevelopmentCard::VictoryPoint);
        game.played_dev_card_this_turn = true;
        game.pending_trade = Some(TradeOffer {
            from: first,
            to: other_player(&game, first),
            give: Resources::zero(),
            receive: Resources::zero(),
        });

        game.apply_action(first, Action::EndTurn).unwrap();
        let second = game.current_player();
        assert_ne!(second, first);
        assert!(game.players[first].new_cards.is_empty());
        assert_eq!(game.players[first].hand.len(), 1);
        assert!(!game.played_dev_card_this_turn);
        assert!(game.pending_trade.is_none());
        assert!(matches!(game.phase, GamePhase::Turn(TurnPhase::Roll)));
    }

    #[test]
    fn check_winner_detects_ten_points() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        game.players[player].victory_point_cards = 5;

        let mut placed = 0;
        for v in 0..game.board.vertices.len() {
            if placed == 5 {
                break;
            }
            if game.board.vertices[v].building.is_some() {
                continue;
            }
            if game
                .board
                .adjacent_vertices(v)
                .iter()
                .any(|&a| game.board.vertices[a].building.is_some())
            {
                continue;
            }
            game.board.vertices[v].building = Some(Building::Settlement(player));
            placed += 1;
        }
        assert_eq!(placed, 5);

        game.check_winner();
        assert_eq!(game.winner, Some(player));
        assert!(matches!(game.phase, GamePhase::Finished));
    }

    #[test]
    fn actions_rejected_after_game_finished() {
        let mut game = Game::new(make_players()).unwrap();
        game.winner = Some(0);
        game.phase = GamePhase::Finished;
        assert_eq!(
            game.apply_action(0, Action::RollDice).unwrap_err(),
            CatanError::GameFinished
        );
    }

    #[test]
    fn player_view_hides_opponent_details() {
        let mut game = Game::new(make_players()).unwrap();
        game.players[0].resources = Resources {
            brick: 3,
            ..Default::default()
        };
        game.players[0].hand.push(DevelopmentCard::Knight);
        game.players[1].resources = Resources {
            lumber: 5,
            ..Default::default()
        };

        let view = game.view_for(0);
        let me = view.players.iter().find(|p| p.is_me).unwrap();
        let other = view.players.iter().find(|p| !p.is_me).unwrap();
        assert_eq!(me.my_resources.unwrap().brick, 3);
        assert_eq!(me.dev_card_count, 1);
        assert!(other.my_resources.is_none());
        assert!(other.my_hand.is_none());
        assert_eq!(other.resource_count, 5);
    }

    #[test]
    fn longest_road_awarded_at_five() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        let start = valid_settlement_vertex(&game).unwrap();
        game.board.vertices[start].building = Some(Building::Settlement(player));

        let mut current = start;
        let mut used_edges = HashSet::new();
        for _ in 0..5 {
            let next_edge = game.board.vertices[current]
                .incident_edges
                .iter()
                .copied()
                .find(|&e| !used_edges.contains(&e) && game.board.edges[e].road.is_none())
                .expect("free edge");
            game.board.edges[next_edge].road = Some(player);
            used_edges.insert(next_edge);
            current = game.board.edges[next_edge].other_vertex(current).unwrap();
        }
        game.recalculate_longest_road();
        assert_eq!(game.longest_road, Some(player));
    }

    #[test]
    fn largest_army_awarded_at_three_knights() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        game.players[player].played_knights = 3;
        game.recalculate_largest_army();
        assert_eq!(game.largest_army, Some(player));
    }

    fn force_main_phase(game: &mut Game) {
        game.phase = GamePhase::Turn(TurnPhase::Main);
        game.dice = Some((3, 4));
    }

    /// Find a 2-road path from a settlement to a vertex where a new settlement can be built.
    fn find_buildable_site(
        game: &Game,
        player: PlayerId,
        from: VertexId,
    ) -> Option<(EdgeId, VertexId, EdgeId, VertexId)> {
        for &e1 in &game.board.vertices[from].incident_edges {
            if game.board.edges[e1].road.is_some() {
                continue;
            }
            let mid = game.board.edges[e1].other_vertex(from)?;
            if game.board.vertices[mid].building.is_some() || game.is_blocked_vertex(mid, player) {
                continue;
            }
            for &e2 in &game.board.vertices[mid].incident_edges {
                if e2 == e1 || game.board.edges[e2].road.is_some() {
                    continue;
                }
                let target = game.board.edges[e2].other_vertex(mid)?;
                if target == from
                    || game.board.vertices[target].building.is_some()
                    || game.is_blocked_vertex(target, player)
                {
                    continue;
                }
                if game
                    .board
                    .adjacent_vertices(target)
                    .iter()
                    .any(|&a| game.board.vertices[a].building.is_some())
                {
                    continue;
                }
                return Some((e1, mid, e2, target));
            }
        }
        None
    }

    fn place_extra_settlements(game: &mut Game, player: PlayerId, count: usize) {
        let mut placed = 0;
        for v in 0..game.board.vertices.len() {
            if placed == count {
                break;
            }
            if game.board.vertices[v].building.is_some() {
                continue;
            }
            if game
                .board
                .adjacent_vertices(v)
                .iter()
                .any(|&a| game.board.vertices[a].building.is_some())
            {
                continue;
            }
            game.board.vertices[v].building = Some(Building::Settlement(player));
            game.players[player].remaining_settlements =
                game.players[player].remaining_settlements.saturating_sub(1);
            placed += 1;
        }
        assert_eq!(placed, count, "could not place enough settlements");
    }

    #[test]
    fn multi_turn_game_flow() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);

        let p1 = game.current_player();
        let p2 = other_player(&game, p1);

        // --- Player 1 turn: build road and settlement ---
        force_main_phase(&mut game);
        let settlement1 = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == p1))
            .map(|v| v.id)
            .unwrap();
        let (road1, _mid, road2, new_vertex) = find_buildable_site(&game, p1, settlement1).unwrap();

        // Two roads plus a settlement: 2×(brick+lumber) + (brick+lumber+grain+wool).
        game.players[p1].resources = Resources {
            brick: 3,
            lumber: 3,
            grain: 1,
            wool: 1,
            ..Default::default()
        };
        game.apply_action(p1, Action::BuildRoad(road1)).unwrap();
        game.apply_action(p1, Action::BuildRoad(road2)).unwrap();
        game.apply_action(p1, Action::BuildSettlement(new_vertex))
            .unwrap();
        assert!(matches!(
            game.board.vertices[new_vertex].building,
            Some(Building::Settlement(pid)) if pid == p1
        ));

        game.apply_action(p1, Action::EndTurn).unwrap();
        assert_eq!(game.current_player(), p2);
        assert!(matches!(game.phase, GamePhase::Turn(TurnPhase::Roll)));

        // --- Player 2 turn: build city, buy dev card, trade, play knight ---
        force_main_phase(&mut game);
        let settlement2 = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == p2))
            .map(|v| v.id)
            .unwrap();
        game.players[p2].resources = Resources {
            ore: 3,
            grain: 2,
            ..Default::default()
        };
        game.apply_action(p2, Action::BuildCity(settlement2))
            .unwrap();
        assert_eq!(
            game.board.vertices[settlement2].building,
            Some(Building::City(p2))
        );

        game.players[p2].resources = Resources {
            ore: 1,
            grain: 1,
            wool: 1,
            ..Default::default()
        };
        let deck_before = game.development_deck.len();
        game.apply_action(p2, Action::BuyDevelopmentCard).unwrap();
        assert_eq!(game.development_deck.len(), deck_before - 1);
        assert_eq!(game.players[p2].new_cards.len(), 1);

        game.players[p2].resources = Resources {
            ore: 1,
            ..Default::default()
        };
        game.players[p1].resources = Resources {
            grain: 1,
            ..Default::default()
        };
        game.apply_action(
            p2,
            Action::OfferTrade {
                to: p1,
                give: Resources {
                    ore: 1,
                    ..Default::default()
                },
                receive: Resources {
                    grain: 1,
                    ..Default::default()
                },
            },
        )
        .unwrap();
        game.apply_action(p1, Action::RespondTrade { accept: true })
            .unwrap();
        assert_eq!(game.players[p2].resources.ore, 0);
        assert_eq!(game.players[p2].resources.grain, 1);
        assert_eq!(game.players[p1].resources.grain, 0);
        assert_eq!(game.players[p1].resources.ore, 1);

        game.players[p2].hand.push(DevelopmentCard::Knight);
        let tile = game
            .board
            .tiles
            .iter()
            .find(|t| !t.has_robber)
            .map(|t| t.id)
            .unwrap();
        game.apply_action(p2, Action::PlayKnight { tile, target: None })
            .unwrap();
        assert_eq!(game.players[p2].played_knights, 1);
        assert!(game.board.tiles[tile].has_robber);

        game.apply_action(p2, Action::EndTurn).unwrap();
        assert_eq!(game.current_player(), p1);

        // --- End game: push p1 to 10 VP and verify winner ---
        game.players[p1].victory_point_cards = 5;
        place_extra_settlements(&mut game, p1, 2);
        game.check_winner();
        assert_eq!(game.winner, Some(p1));
        assert!(matches!(game.phase, GamePhase::Finished));

        // Actions are rejected once the game is finished.
        assert_eq!(
            game.apply_action(p1, Action::RollDice).unwrap_err(),
            CatanError::GameFinished
        );
    }
}
