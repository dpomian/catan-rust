use crate::board::*;
use crate::error::CatanError;
use crate::log::{
    AwardChange, DiscardDemand, EventOutcome, EventSubscriber, GameAward, GameEvent, GameLog,
    ResourceDelta, ResourcePayout,
};
use crate::player::Player;
use crate::types::*;
use rand::Rng;
use rand::RngCore;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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
    Discard(Resources),
    BuildRoad(EdgeId),
    BuildSettlement(VertexId),
    BuildCity(VertexId),
    BuyDevelopmentCard,
    PlayKnight {
        tile: TileId,
        target: Option<PlayerId>,
    },
    PlayRoadBuilding(EdgeId, EdgeId),
    PlayInvention(Resource, Resource),
    PlayMonopoly(Resource),
    OfferTrade {
        give: Resources,
        receive: Resources,
    },
    RespondTrade {
        accept: bool,
    },
    CancelTrade {
        trade_id: u64,
    },
    BankTrade {
        give: Resources,
        receive: Resources,
    },
    EndTurn,
}

#[derive(Clone)]
pub struct Game {
    pub board: Board,
    pub players: Vec<Player>,
    pub initial_players: Vec<Player>,
    pub player_order: Vec<PlayerId>,
    pub current_player_idx: usize,
    pub phase: GamePhase,
    pub dice: Option<(u8, u8)>,
    pub longest_road: Option<PlayerId>,
    pub largest_army: Option<PlayerId>,
    pub pending_trade: Option<TradeOffer>,
    pub pending_discards: HashMap<PlayerId, u16>,
    pub development_deck: Vec<DevelopmentCard>,
    pub winner: Option<PlayerId>,
    pub played_dev_card_this_turn: bool,
    pub trade_counter: u64,
    pub seed: u64,
    pub event_log: Vec<GameEvent>,
    event_subscribers: Vec<Arc<dyn EventSubscriber>>,
    rng: StdRng,
    last_outcome: Option<EventOutcome>,
}

impl std::fmt::Debug for Game {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Game")
            .field("board", &self.board)
            .field("players", &self.players)
            .field("initial_players", &self.initial_players)
            .field("player_order", &self.player_order)
            .field("current_player_idx", &self.current_player_idx)
            .field("phase", &self.phase)
            .field("dice", &self.dice)
            .field("longest_road", &self.longest_road)
            .field("largest_army", &self.largest_army)
            .field("pending_trade", &self.pending_trade)
            .field("pending_discards", &self.pending_discards)
            .field("development_deck", &self.development_deck)
            .field("winner", &self.winner)
            .field("played_dev_card_this_turn", &self.played_dev_card_this_turn)
            .field("trade_counter", &self.trade_counter)
            .field("seed", &self.seed)
            .field("event_log", &self.event_log)
            .field("event_subscribers", &self.event_subscribers.len())
            .finish_non_exhaustive()
    }
}

impl PartialEq for Game {
    fn eq(&self, other: &Self) -> bool {
        self.board == other.board
            && self.players == other.players
            && self.initial_players == other.initial_players
            && self.player_order == other.player_order
            && self.current_player_idx == other.current_player_idx
            && self.phase == other.phase
            && self.dice == other.dice
            && self.longest_road == other.longest_road
            && self.largest_army == other.largest_army
            && self.pending_trade == other.pending_trade
            && self.pending_discards == other.pending_discards
            && self.development_deck == other.development_deck
            && self.winner == other.winner
            && self.played_dev_card_this_turn == other.played_dev_card_this_turn
            && self.trade_counter == other.trade_counter
            && self.seed == other.seed
            && self.event_log.len() == other.event_log.len()
            && self
                .event_log
                .iter()
                .zip(&other.event_log)
                .all(|(left, right)| left.same_gameplay(right))
        // event_subscribers are runtime wiring and intentionally excluded.
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerView {
    pub id: PlayerId,
    pub name: String,
    pub color: PlayerColor,
    pub is_bot: bool,
    pub resource_count: u16,
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
    pub pending_discards: HashMap<PlayerId, u16>,
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
                    + self.largest_army_points(p.id)
                    + p.victory_point_cards;
                let vps = if is_me {
                    public_vps + p.hidden_victory_points()
                } else {
                    public_vps
                };
                PlayerView {
                    id: p.id,
                    name: p.name.clone(),
                    color: p.color,
                    is_bot: p.is_bot,
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
            pending_discards: self.pending_discards.clone(),
            winner: self.winner,
        }
    }

    /// Create a new game with a randomized board, turn order, and shuffled dev-card deck.
    pub fn new(players: Vec<Player>) -> Result<Self, CatanError> {
        let seed = thread_rng().next_u64();
        Self::new_with_seed(players, seed)
    }

    /// Create a new game using the provided random seed.
    ///
    /// The same seed, players, and sequence of actions always reproduces the
    /// same game state, which makes event-log replay deterministic.
    pub fn new_with_seed(players: Vec<Player>, seed: u64) -> Result<Self, CatanError> {
        if players.len() < 2
            || players.len() > 4
            || players
                .iter()
                .enumerate()
                .any(|(index, player)| player.id != index)
        {
            return Err(CatanError::InvalidPlayer);
        }

        let mut rng = StdRng::seed_from_u64(seed);
        let mut board = Board::generate_geometry();
        board.assign_terrain_and_numbers(&mut rng);

        let initial_players = players.clone();
        let mut player_order: Vec<PlayerId> = players.iter().map(|p| p.id).collect();
        player_order.shuffle(&mut rng);

        let mut deck = development_deck();
        deck.shuffle(&mut rng);

        Ok(Self {
            board,
            players,
            initial_players,
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
            pending_discards: HashMap::new(),
            development_deck: deck,
            winner: None,
            played_dev_card_this_turn: false,
            trade_counter: 0,
            seed,
            event_log: Vec::new(),
            event_subscribers: Vec::new(),
            rng,
            last_outcome: None,
        })
    }

    pub fn current_player(&self) -> PlayerId {
        self.player_order[self.current_player_idx]
    }

    /// Check an action against the complete game rules without changing this game.
    ///
    /// Runtime event subscribers are removed from the temporary clone, so probing
    /// an action cannot publish events or otherwise produce external side effects.
    pub fn is_action_legal(&self, player: PlayerId, action: Action) -> bool {
        let mut probe = self.clone();
        probe.event_subscribers.clear();
        probe.apply_action(player, action).is_ok()
    }

    /// Return a replayable log for this game.
    pub fn log(&self) -> GameLog {
        GameLog {
            seed: self.seed,
            initial_players: self.initial_players.clone(),
            initial_player_order: self.player_order.clone(),
            events: self.event_log.clone(),
        }
    }

    /// Reconstruct a game by replaying the events in `log`.
    ///
    /// This uses the stored seed to recreate the initial board, turn order, and
    /// development deck, then applies every recorded action in order.
    pub fn replay(log: &GameLog) -> Result<Self, CatanError> {
        let mut game = Self::new_with_seed(log.initial_players.clone(), log.seed)?;
        for event in &log.events {
            game.apply_action(event.player, event.action)?;
        }
        Ok(game)
    }

    /// Register an event subscriber that receives every successfully applied
    /// action as it happens.
    pub fn subscribe(&mut self, subscriber: Arc<dyn EventSubscriber>) {
        self.event_subscribers.push(subscriber);
    }

    fn publish_event(&mut self, event: GameEvent) {
        for subscriber in &self.event_subscribers {
            subscriber.on_event(&event);
        }
        self.event_log.push(event);
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

        self.last_outcome = None;
        let resources_before: Vec<Resources> = self.players.iter().map(|p| p.resources).collect();
        let longest_road_before = self.longest_road;
        let largest_army_before = self.largest_army;
        let winner_before = self.winner;
        let result = match self.phase {
            GamePhase::Finished => Err(CatanError::GameFinished),
            GamePhase::Setup { .. } => self.apply_setup_action(player, action),
            GamePhase::Turn(turn_phase) => self.apply_turn_action(player, turn_phase, action),
        };

        if result.is_ok() {
            self.check_winner();
            let event = GameEvent {
                sequence: self.event_log.len(),
                player,
                action,
                outcome: self.last_outcome.take().unwrap_or(EventOutcome::None),
                resource_changes: self.resource_changes_from(&resources_before),
                award_changes: Self::award_changes(
                    longest_road_before,
                    self.longest_road,
                    largest_army_before,
                    self.largest_army,
                ),
                winner: self.winner.filter(|_| self.winner != winner_before),
                timestamp_ms: Self::now_ms(),
            };
            self.publish_event(event);
        }
        result
    }

    fn resource_changes_from(&self, before: &[Resources]) -> Vec<ResourceDelta> {
        let mut changes = Vec::new();
        for (player, before) in self.players.iter().zip(before) {
            let mut gained = Resources::zero();
            let mut lost = Resources::zero();
            for resource in Resource::all() {
                let old = before.get(resource);
                let new = player.resources.get(resource);
                if new > old {
                    gained.add(resource, new - old);
                } else if old > new {
                    lost.add(resource, old - new);
                }
            }
            if gained.total() > 0 || lost.total() > 0 {
                changes.push(ResourceDelta {
                    player: player.id,
                    gained,
                    lost,
                });
            }
        }
        changes.sort_by_key(|change| change.player);
        changes
    }

    fn award_changes(
        longest_before: Option<PlayerId>,
        longest_after: Option<PlayerId>,
        army_before: Option<PlayerId>,
        army_after: Option<PlayerId>,
    ) -> Vec<AwardChange> {
        let mut changes = Vec::new();
        if longest_before != longest_after {
            changes.push(AwardChange {
                award: GameAward::LongestRoad,
                previous: longest_before,
                holder: longest_after,
            });
        }
        if army_before != army_after {
            changes.push(AwardChange {
                award: GameAward::LargestArmy,
                previous: army_before,
                holder: army_after,
            });
        }
        changes
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0)
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
                // A settlement can break an opponent's road network.
                self.recalculate_longest_road();
                // The second settlement grants one resource from each adjacent hex.
                if round == 1 {
                    let resources = self.collect_starting_resources(vertex_id);
                    self.last_outcome = Some(EventOutcome::ResourcesGained { resources });
                }
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
        let vertex = self
            .board
            .vertices
            .get(vertex_id)
            .ok_or(CatanError::InvalidBuildLocation)?;
        if vertex.building.is_some() {
            return Err(CatanError::InvalidBuildLocation);
        }

        // Distance rule: no adjacent building.
        for adj in self
            .board
            .adjacent_vertices(vertex_id)
            .ok_or(CatanError::InvalidBuildLocation)?
        {
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
        let edge = self
            .board
            .edges
            .get(edge_id)
            .ok_or(CatanError::InvalidBuildLocation)?;
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
        if turn_phase == TurnPhase::ResolveSeven
            && let Action::Discard(resources) = action
        {
            return self.discard_resources(player, resources);
        }

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
                if !self.pending_discards.is_empty() {
                    return Err(CatanError::MustDiscard);
                }
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
            (TurnPhase::Main, Action::OfferTrade { give, receive }) => {
                self.offer_trade(player, give, receive)
            }
            (TurnPhase::Main, Action::CancelTrade { trade_id }) => {
                self.cancel_trade(player, trade_id)
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
        let d1 = self.rng.gen_range(1..=6);
        let d2 = self.rng.gen_range(1..=6);
        self.dice = Some((d1, d2));
        let sum = d1 + d2;

        let production = if sum == 7 {
            self.discard_over_7();
            self.phase = GamePhase::Turn(TurnPhase::ResolveSeven);
            Vec::new()
        } else {
            let production = self.produce_resources(sum);
            self.phase = GamePhase::Turn(TurnPhase::Main);
            production
        };
        let mut discards: Vec<DiscardDemand> = self
            .pending_discards
            .iter()
            .map(|(&player, &count)| DiscardDemand { player, count })
            .collect();
        discards.sort_by_key(|demand| demand.player);
        self.last_outcome = Some(EventOutcome::DiceRoll {
            d1,
            d2,
            production,
            discards,
        });
        Ok(())
    }

    /// Record how many resources each player must discard after a 7.
    fn discard_over_7(&mut self) {
        self.pending_discards = self
            .players
            .iter()
            .filter_map(|p| {
                let total = p.resources.total();
                (total > 7).then_some((p.id, total / 2))
            })
            .collect();
    }

    fn discard_resources(
        &mut self,
        player: PlayerId,
        resources: Resources,
    ) -> Result<(), CatanError> {
        let required = self
            .pending_discards
            .get(&player)
            .copied()
            .ok_or(CatanError::InvalidPhase)?;
        if resources.total() != required || !self.players[player].resources.can_afford(&resources) {
            return Err(CatanError::MustDiscard);
        }

        self.pay_cost(player, &resources);
        self.pending_discards.remove(&player);
        Ok(())
    }

    /// Distribute resources to settlements and cities touching the rolled number.
    fn produce_resources(&mut self, roll: u8) -> Vec<ResourcePayout> {
        let mut produced: HashMap<PlayerId, Resources> = HashMap::new();
        for tile in &self.board.tiles {
            if tile.has_robber || tile.number != Some(roll) {
                continue;
            }
            if let Some(resource) = tile.terrain.resource() {
                for &vertex_id in &tile.vertices {
                    let amount = match self.board.vertices[vertex_id].building {
                        Some(Building::City(pid)) => Some((pid, 2)),
                        Some(Building::Settlement(pid)) => Some((pid, 1)),
                        _ => None,
                    };
                    if let Some((pid, amount)) = amount {
                        produced.entry(pid).or_default().add(resource, amount);
                        self.players[pid].resources.add(resource, amount);
                    }
                }
            }
        }
        let mut payouts: Vec<ResourcePayout> = produced
            .into_iter()
            .map(|(player, resources)| ResourcePayout { player, resources })
            .collect();
        payouts.sort_by_key(|p| p.player);
        payouts
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

        let victims: HashSet<PlayerId> = self.board.tiles[tile_id]
            .vertices
            .iter()
            .filter_map(|&v| match self.board.vertices[v].building {
                Some(Building::Settlement(pid) | Building::City(pid))
                    if pid != player && self.players[pid].resources.total() > 0 =>
                {
                    Some(pid)
                }
                _ => None,
            })
            .collect();
        if target.map_or(!victims.is_empty(), |target_id| {
            !victims.contains(&target_id)
        }) {
            return Err(CatanError::InvalidRobberTarget);
        }

        let stolen = target.and_then(|target_id| {
            self.steal_random_card(target_id)
                .map(|resource| (target_id, resource))
        });

        if let Some(t) = self.board.tiles.iter_mut().find(|t| t.has_robber) {
            t.has_robber = false;
        }
        self.board.tiles[tile_id].has_robber = true;

        if let Some((target_id, resource)) = stolen {
            self.players[target_id].resources.sub(resource, 1);
            self.players[player].resources.add(resource, 1);
            self.last_outcome = Some(EventOutcome::ResourceStolen { resource });
        }

        Ok(())
    }

    /// Pick a random resource card from a player, if they have any.
    fn steal_random_card(&mut self, player_id: PlayerId) -> Option<Resource> {
        let p = &self.players[player_id];
        let available: Vec<Resource> = Resource::all()
            .into_iter()
            .filter(|r| p.resources.get(*r) > 0)
            .collect();
        available.choose(&mut self.rng).copied()
    }

    /// Pay for and build a road, then recalculate longest road.
    fn build_road_action(&mut self, player: PlayerId, edge_id: EdgeId) -> Result<(), CatanError> {
        if !self.can_build_road(player, edge_id) {
            return Err(CatanError::InvalidBuildLocation);
        }
        if self.players[player].remaining_roads == 0 {
            return Err(CatanError::NoPiecesRemaining);
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
        let Some(edge) = self.board.edges.get(edge_id) else {
            return false;
        };
        if edge.road.is_some() {
            return false;
        }

        let a = edge.endpoints[0];
        let b = edge.endpoints[1];
        let reachable = self.reachable_vertices(player);
        (!self.is_blocked_vertex(a, player) && reachable.contains(&a))
            || (!self.is_blocked_vertex(b, player) && reachable.contains(&b))
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
                if self.board.edges[edge_id].road == Some(player)
                    && let Some(other) = self.board.edges[edge_id].other_vertex(v)
                    && !self.is_blocked_vertex(other, player)
                    && visited.insert(other)
                {
                    queue.push(other);
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
        if self
            .board
            .vertices
            .get(vertex_id)
            .ok_or(CatanError::InvalidBuildLocation)?
            .building
            .is_some()
        {
            return Err(CatanError::InvalidBuildLocation);
        }

        for adj in self
            .board
            .adjacent_vertices(vertex_id)
            .ok_or(CatanError::InvalidBuildLocation)?
        {
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
        match self
            .board
            .vertices
            .get(vertex_id)
            .and_then(|vertex| vertex.building)
        {
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
        self.move_robber(player, tile_id, target)?;
        self.players[player].remove_card(DevelopmentCard::Knight);
        self.players[player].played_knights += 1;
        self.played_dev_card_this_turn = true;
        self.recalculate_largest_army();
        Ok(())
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

    /// Play an Invention (Year of Plenty) card to gain two resources.
    fn play_invention(
        &mut self,
        player: PlayerId,
        r1: Resource,
        r2: Resource,
    ) -> Result<(), CatanError> {
        self.validate_play_dev_card(player, DevelopmentCard::Invention)?;
        self.players[player].resources.add(r1, 1);
        self.players[player].resources.add(r2, 1);
        let mut resources = Resources::zero();
        resources.add(r1, 1);
        resources.add(r2, 1);
        self.last_outcome = Some(EventOutcome::ResourcesGained { resources });
        self.players[player].remove_card(DevelopmentCard::Invention);
        self.played_dev_card_this_turn = true;
        Ok(())
    }

    /// Play a Monopoly card to take all of one resource from opponents.
    fn play_monopoly(&mut self, player: PlayerId, resource: Resource) -> Result<(), CatanError> {
        self.validate_play_dev_card(player, DevelopmentCard::Monopoly)?;
        let mut total = 0u16;
        for pid in 0..self.players.len() {
            if pid == player {
                continue;
            }
            let amount = self.players[pid].resources.get(resource);
            self.players[pid].resources.sub(resource, amount);
            total += u16::from(amount);
        }
        self.players[player]
            .resources
            .add(resource, total.min(u16::from(u8::MAX)) as u8);
        self.last_outcome = Some(EventOutcome::Monopoly {
            resource,
            amount: total,
        });
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

    /// Offer a trade to all other players after verifying the offer is non-empty and the initiator can pay.
    fn offer_trade(
        &mut self,
        from: PlayerId,
        give: Resources,
        receive: Resources,
    ) -> Result<(), CatanError> {
        if give.total() == 0 || receive.total() == 0 {
            return Err(CatanError::InvalidTrade);
        }
        if !self.players[from].resources.can_afford(&give) {
            return Err(CatanError::NotEnoughResources);
        }

        let trade_id = self.trade_counter;
        self.trade_counter = self.trade_counter.wrapping_add(1);
        self.pending_trade = Some(TradeOffer {
            trade_id,
            from,
            give,
            receive,
            rejected_by: Vec::new(),
        });
        Ok(())
    }

    /// Cancel a pending trade. Only the initiator can cancel, and only if the trade id matches.
    fn cancel_trade(&mut self, player: PlayerId, trade_id: u64) -> Result<(), CatanError> {
        let trade = self
            .pending_trade
            .as_ref()
            .ok_or(CatanError::NoPendingTrade)?;
        if trade.from != player || trade.trade_id != trade_id {
            return Err(CatanError::TradeNotForYou);
        }
        self.pending_trade = None;
        Ok(())
    }

    /// Accept or decline a pending broadcast trade. Anyone except the initiator may respond;
    /// accepting executes the trade with the responding player.
    fn handle_trade_response(&mut self, player: PlayerId, accept: bool) -> Result<(), CatanError> {
        let trade = self
            .pending_trade
            .as_ref()
            .ok_or(CatanError::NoPendingTrade)?
            .clone();
        if trade.from == player {
            return Err(CatanError::TradeNotForYou);
        }

        if !accept {
            if let Some(offer) = self.pending_trade.as_mut()
                && !offer.rejected_by.contains(&player)
            {
                offer.rejected_by.push(player);
            }
            return Ok(());
        }

        if !self.players[trade.from].resources.can_afford(&trade.give)
            || !self.players[player].resources.can_afford(&trade.receive)
        {
            return Err(CatanError::InvalidTrade);
        }

        self.transfer_resources(trade.from, player, &trade.give)?;
        self.transfer_resources(player, trade.from, &trade.receive)?;
        self.pending_trade = None;
        Ok(())
    }

    /// Exchange resources with the bank at a fixed 4:1 rate. The player must give
    /// exactly four of a single resource and receive exactly one of a different
    /// resource; anything else is rejected (there are no harbors in this game).
    fn bank_trade(
        &mut self,
        player: PlayerId,
        give: Resources,
        receive: Resources,
    ) -> Result<(), CatanError> {
        let given: Vec<Resource> = Resource::all()
            .into_iter()
            .filter(|&r| give.get(r) > 0)
            .collect();
        let received: Vec<Resource> = Resource::all()
            .into_iter()
            .filter(|&r| receive.get(r) > 0)
            .collect();

        if give.total() != 4
            || receive.total() != 1
            || given.len() != 1
            || received.len() != 1
            || given[0] == received[0]
        {
            return Err(CatanError::InvalidTrade);
        }

        let p = &mut self.players[player];
        if !p.resources.can_afford(&give) {
            return Err(CatanError::NotEnoughResources);
        }

        p.resources.sub_resources(&give);
        p.resources.add_resources(&receive);
        Ok(())
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
    fn collect_starting_resources(&mut self, settlement: VertexId) -> Resources {
        let current = self.current_player();
        let incident_tiles = self.board.vertices[settlement].incident_tiles.clone();
        let mut gained = Resources::zero();
        for &tile_id in &incident_tiles {
            let tile = &self.board.tiles[tile_id];
            if let Some(resource) = tile.terrain.resource() {
                gained.add(resource, 1);
                self.players[current].resources.add(resource, 1);
            }
        }
        gained
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
            let mut vps = p.public_victory_points() + p.hidden_victory_points();
            vps += self.settlement_city_points(player_id);
            vps += self.longest_road_points(player_id);
            vps += self.largest_army_points(player_id);
            if vps >= 10 {
                self.reveal_victory_point_cards(player_id);
                self.winner = Some(player_id);
                self.phase = GamePhase::Finished;
                return;
            }
        }
    }

    fn reveal_victory_point_cards(&mut self, player: PlayerId) {
        let p = &mut self.players[player];
        let hidden = p
            .hand
            .iter()
            .chain(&p.new_cards)
            .filter(|&&card| card == DevelopmentCard::VictoryPoint)
            .count() as u8;
        p.hand.retain(|&card| card != DevelopmentCard::VictoryPoint);
        p.new_cards
            .retain(|&card| card != DevelopmentCard::VictoryPoint);
        p.victory_point_cards += hidden;
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
        let max_knights = self
            .players
            .iter()
            .map(|player| player.played_knights)
            .max()
            .unwrap_or(0);
        if max_knights < 3 {
            self.largest_army = None;
            return;
        }
        if self
            .largest_army
            .is_some_and(|holder| self.players[holder].played_knights == max_knights)
        {
            return;
        }

        let leaders: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|player| player.played_knights == max_knights)
            .map(|player| player.id)
            .collect();
        self.largest_army = (leaders.len() == 1).then_some(leaders[0]);
    }

    /// Award longest road to the player with the longest road (at least 5).
    fn recalculate_longest_road(&mut self) {
        let lengths: Vec<(PlayerId, usize)> = self
            .players
            .iter()
            .map(|player| (player.id, self.longest_road_for_player(player.id)))
            .collect();
        let max_length = lengths.iter().map(|(_, length)| *length).max().unwrap_or(0);
        if max_length < 5 {
            self.longest_road = None;
            return;
        }
        if self.longest_road.is_some_and(|holder| {
            lengths
                .iter()
                .any(|&(player, length)| player == holder && length == max_length)
        }) {
            return;
        }

        let leaders: Vec<PlayerId> = lengths
            .into_iter()
            .filter(|&(_, length)| length == max_length)
            .map(|(player, _)| player)
            .collect();
        self.longest_road = (leaders.len() == 1).then_some(leaders[0]);
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
        let mut adj: HashMap<VertexId, Vec<(VertexId, EdgeId)>> = HashMap::new();
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
                adj.entry(a).or_default().push((b, edge_id));
                adj.entry(b).or_default().push((a, edge_id));
            } else if a_blocked {
                let leaf = next_virtual;
                next_virtual += 1;
                adj.entry(b).or_default().push((leaf, edge_id));
                adj.entry(leaf).or_default().push((b, edge_id));
            } else {
                let leaf = next_virtual;
                next_virtual += 1;
                adj.entry(a).or_default().push((leaf, edge_id));
                adj.entry(leaf).or_default().push((a, edge_id));
            }
        }

        let mut best = 0usize;
        let starts: Vec<VertexId> = adj.keys().copied().collect();
        for start in starts {
            let mut used_edges = HashSet::new();
            best = best.max(self.dfs_longest(start, &adj, &mut used_edges));
        }
        best
    }

    /// An opponent settlement or city blocks a vertex for this player.
    fn is_blocked_vertex(&self, vertex: VertexId, player: PlayerId) -> bool {
        matches!(
            self.board.vertices[vertex].building,
            Some(Building::Settlement(pid) | Building::City(pid)) if pid != player
        )
    }

    /// Brute-force search for the longest trail in the player's road graph.
    fn dfs_longest(
        &self,
        current: VertexId,
        adj: &HashMap<VertexId, Vec<(VertexId, EdgeId)>>,
        used_edges: &mut HashSet<EdgeId>,
    ) -> usize {
        let mut max = 0usize;
        if let Some(neighbors) = adj.get(&current) {
            for &(next, edge_id) in neighbors {
                if used_edges.insert(edge_id) {
                    max = max.max(1 + self.dfs_longest(next, adj, used_edges));
                    used_edges.remove(&edge_id);
                }
            }
        }
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
            let target = robber_target(&game, player, tile);
            game.apply_action(player, Action::MoveRobber { tile, target })
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
            let road = valid_edge_for_settlement(game, vertex);
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

    fn robber_target(game: &Game, player: PlayerId, tile: TileId) -> Option<PlayerId> {
        game.board.tiles[tile]
            .vertices
            .iter()
            .filter_map(|&vertex| match game.board.vertices[vertex].building {
                Some(Building::Settlement(pid) | Building::City(pid))
                    if pid != player && game.players[pid].resources.total() > 0 =>
                {
                    Some(pid)
                }
                _ => None,
            })
            .next()
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
            let target = robber_target(game, player, tile);
            game.apply_action(player, Action::MoveRobber { tile, target })
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
            .unwrap()
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
                        .unwrap()
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
                        .unwrap()
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
            .unwrap()
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
                        .unwrap()
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
        let target = robber_target(&game, player, tile);
        game.apply_action(player, Action::PlayKnight { tile, target })
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
            .unwrap()
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
        let target = robber_target(&game, player, tile);
        game.apply_action(player, Action::PlayKnight { tile, target })
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
            Action::PlayInvention(Resource::Brick, Resource::Lumber),
        )
        .unwrap();
        assert_eq!(game.players[player].resources.brick, 1);
        assert_eq!(game.players[player].resources.lumber, 1);
        assert_eq!(
            game.event_log.last().unwrap().outcome,
            EventOutcome::ResourcesGained {
                resources: Resources {
                    brick: 1,
                    lumber: 1,
                    ..Default::default()
                },
            }
        );
    }

    #[test]
    fn play_invention_with_same_resource_gains_two() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        game.players[player].resources = Resources::zero();
        game.players[player].hand.push(DevelopmentCard::Invention);
        game.apply_action(player, Action::PlayInvention(Resource::Ore, Resource::Ore))
            .unwrap();
        assert_eq!(game.players[player].resources.ore, 2);
        assert_eq!(
            game.event_log.last().unwrap().outcome,
            EventOutcome::ResourcesGained {
                resources: Resources {
                    ore: 2,
                    ..Default::default()
                },
            }
        );
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
        assert_eq!(
            game.event_log.last().unwrap().outcome,
            EventOutcome::Monopoly {
                resource: Resource::Grain,
                amount: 3,
            }
        );
        let mut expected = vec![
            ResourceDelta {
                player,
                gained: Resources {
                    grain: 3,
                    ..Default::default()
                },
                lost: Resources::zero(),
            },
            ResourceDelta {
                player: other,
                gained: Resources::zero(),
                lost: Resources {
                    grain: 3,
                    ..Default::default()
                },
            },
        ];
        expected.sort_by_key(|delta| delta.player);
        assert_eq!(game.event_log.last().unwrap().resource_changes, expected);
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
        let mut expected = vec![
            ResourceDelta {
                player: from,
                gained: Resources {
                    lumber: 1,
                    ..Default::default()
                },
                lost: Resources {
                    brick: 1,
                    ..Default::default()
                },
            },
            ResourceDelta {
                player: to,
                gained: Resources {
                    brick: 1,
                    ..Default::default()
                },
                lost: Resources {
                    lumber: 1,
                    ..Default::default()
                },
            },
        ];
        expected.sort_by_key(|delta| delta.player);
        assert_eq!(game.event_log.last().unwrap().resource_changes, expected);
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

        assert!(game.pending_trade.is_some());
        assert_eq!(game.players[from].resources.brick, 2);
        assert_eq!(game.players[from].resources.lumber, 0);
        assert_eq!(game.players[to].resources.brick, 0);
        assert_eq!(game.players[to].resources.lumber, 1);
    }

    #[test]
    fn cancel_trade_withdraws_offer() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let from = game.current_player();
        game.players[from].resources = Resources {
            brick: 2,
            ..Default::default()
        };

        game.apply_action(
            from,
            Action::OfferTrade {
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
        let trade_id = game.pending_trade.as_ref().unwrap().trade_id;
        assert!(game.pending_trade.is_some());

        game.apply_action(from, Action::CancelTrade { trade_id })
            .unwrap();
        assert!(game.pending_trade.is_none());
    }

    #[test]
    fn empty_trade_offer_is_invalid() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        assert_eq!(
            game.apply_action(
                player,
                Action::OfferTrade {
                    give: Resources::zero(),
                    receive: Resources::zero(),
                }
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );
    }

    #[test]
    fn bank_trade_exchanges_resource_bundles() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        game.players[player].resources = Resources {
            brick: 2,
            lumber: 1,
            ..Default::default()
        };
        // A mixed 2-for-2 bundle violates the fixed 4:1 bank rate.
        assert_eq!(
            game.apply_action(
                player,
                Action::BankTrade {
                    give: Resources {
                        brick: 1,
                        lumber: 1,
                        ..Default::default()
                    },
                    receive: Resources {
                        ore: 1,
                        grain: 1,
                        ..Default::default()
                    },
                },
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );
    }

    #[test]
    fn bank_trade_four_for_one_works() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        game.players[player].resources = Resources {
            brick: 4,
            ..Default::default()
        };
        game.apply_action(
            player,
            Action::BankTrade {
                give: Resources {
                    brick: 4,
                    ..Default::default()
                },
                receive: Resources {
                    ore: 1,
                    ..Default::default()
                },
            },
        )
        .unwrap();
        assert_eq!(game.players[player].resources.brick, 0);
        assert_eq!(game.players[player].resources.ore, 1);
        assert_eq!(
            game.event_log.last().unwrap().resource_changes,
            vec![ResourceDelta {
                player,
                gained: Resources {
                    ore: 1,
                    ..Default::default()
                },
                lost: Resources {
                    brick: 4,
                    ..Default::default()
                },
            }]
        );
    }

    #[test]
    fn bank_trade_rejects_non_four_to_one_rates() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        game.players[player].resources = Resources {
            brick: 4,
            lumber: 4,
            ..Default::default()
        };

        // 3:1 is not a valid bank rate.
        assert_eq!(
            game.apply_action(
                player,
                Action::BankTrade {
                    give: Resources {
                        brick: 3,
                        ..Default::default()
                    },
                    receive: Resources {
                        ore: 1,
                        ..Default::default()
                    },
                },
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );

        // Four given for two received is not a valid bank rate.
        assert_eq!(
            game.apply_action(
                player,
                Action::BankTrade {
                    give: Resources {
                        brick: 4,
                        ..Default::default()
                    },
                    receive: Resources {
                        ore: 2,
                        ..Default::default()
                    },
                },
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );

        // The given resources must be a single type, not a mixed bundle.
        assert_eq!(
            game.apply_action(
                player,
                Action::BankTrade {
                    give: Resources {
                        brick: 2,
                        lumber: 2,
                        ..Default::default()
                    },
                    receive: Resources {
                        ore: 1,
                        ..Default::default()
                    },
                },
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );

        // Giving and receiving the same resource is pointless and invalid.
        assert_eq!(
            game.apply_action(
                player,
                Action::BankTrade {
                    give: Resources {
                        brick: 4,
                        ..Default::default()
                    },
                    receive: Resources {
                        brick: 1,
                        ..Default::default()
                    },
                },
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );

        // Nothing was traded away by the rejected offers.
        assert_eq!(game.players[player].resources.brick, 4);
        assert_eq!(game.players[player].resources.lumber, 4);
        assert_eq!(game.players[player].resources.ore, 0);
    }

    #[test]
    fn bank_trade_requires_enough_resources() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();

        game.players[player].resources = Resources {
            brick: 3,
            ..Default::default()
        };
        assert_eq!(
            game.apply_action(
                player,
                Action::BankTrade {
                    give: Resources {
                        brick: 4,
                        ..Default::default()
                    },
                    receive: Resources {
                        ore: 1,
                        ..Default::default()
                    },
                },
            )
            .unwrap_err(),
            CatanError::NotEnoughResources
        );
    }

    #[test]
    fn empty_bank_trade_is_invalid() {
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
                    give: Resources::zero(),
                    receive: Resources::zero(),
                }
            )
            .unwrap_err(),
            CatanError::InvalidTrade
        );
    }

    #[test]
    fn seven_roll_requires_player_to_discard_half() {
        let mut game = Game::new(make_players()).unwrap();
        game.players[0].resources = Resources {
            brick: 4,
            lumber: 3,
            grain: 2,
            ..Default::default()
        };
        game.phase = GamePhase::Turn(TurnPhase::ResolveSeven);
        game.discard_over_7();
        assert_eq!(game.pending_discards.get(&0), Some(&4));

        game.apply_action(
            0,
            Action::Discard(Resources {
                brick: 2,
                lumber: 1,
                grain: 1,
                ..Default::default()
            }),
        )
        .unwrap();
        assert_eq!(game.players[0].resources.total(), 5);
        assert!(!game.pending_discards.contains_key(&0));
        assert_eq!(
            game.event_log.last().unwrap().resource_changes,
            vec![ResourceDelta {
                player: 0,
                gained: Resources::zero(),
                lost: Resources {
                    brick: 2,
                    lumber: 1,
                    grain: 1,
                    ..Default::default()
                },
            }]
        );
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
            trade_id: 0,
            from: first,
            give: Resources::zero(),
            receive: Resources::zero(),
            rejected_by: Vec::new(),
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
                .unwrap()
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
        game.players[1].is_bot = true;

        let view = game.view_for(0);
        let me = view.players.iter().find(|p| p.is_me).unwrap();
        let other = view.players.iter().find(|p| !p.is_me).unwrap();
        assert_eq!(me.my_resources.unwrap().brick, 3);
        assert_eq!(me.dev_card_count, 1);
        assert!(other.my_resources.is_none());
        assert!(other.my_hand.is_none());
        assert_eq!(other.resource_count, 5);
        assert!(other.is_bot);
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

    /// Find a chain of `length` free edges starting from `start`, never entering an
    /// occupied vertex and never revisiting a vertex.
    fn road_chain_from(
        game: &Game,
        current: VertexId,
        length: usize,
        visited: &mut HashSet<VertexId>,
    ) -> Option<Vec<EdgeId>> {
        if length == 0 {
            return Some(Vec::new());
        }
        for &edge_id in &game.board.vertices[current].incident_edges {
            let edge = &game.board.edges[edge_id];
            if edge.road.is_some() {
                continue;
            }
            let next = edge.other_vertex(current).unwrap();
            if game.board.vertices[next].building.is_some() || visited.contains(&next) {
                continue;
            }
            visited.insert(next);
            if let Some(mut rest) = road_chain_from(game, next, length - 1, visited) {
                let mut chain = vec![edge_id];
                chain.append(&mut rest);
                return Some(chain);
            }
            visited.remove(&next);
        }
        None
    }

    /// Remove any roads already owned by the player so road-length assertions
    /// are not inflated by roads placed during setup.
    fn clear_player_roads(game: &mut Game, player: PlayerId) {
        for edge in game.board.edges.iter_mut() {
            if edge.road == Some(player) {
                edge.road = None;
            }
        }
    }

    #[test]
    fn longest_road_awarded_via_build_actions() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        clear_player_roads(&mut game, player);

        // Pay for and build five connected roads during the main phase.
        game.players[player].resources = Resources {
            brick: 10,
            lumber: 10,
            ..Default::default()
        };
        let start = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == player))
            .map(|v| v.id)
            .unwrap();
        let mut visited = HashSet::from([start]);
        let chain = road_chain_from(&game, start, 5, &mut visited).expect("five-edge road chain");
        for &edge_id in &chain {
            game.apply_action(player, Action::BuildRoad(edge_id))
                .unwrap();
        }

        assert_eq!(game.longest_road_for_player(player), 5);
        assert_eq!(game.longest_road, Some(player));
        let award = game
            .event_log
            .iter()
            .rev()
            .find(|event| !event.award_changes.is_empty())
            .expect("longest road award logged");
        assert_eq!(
            award.award_changes,
            vec![AwardChange {
                award: GameAward::LongestRoad,
                previous: None,
                holder: Some(player),
            }]
        );
        let view = game.view_for(player);
        let mine = view.players.iter().find(|p| p.id == player).unwrap();
        // Two setup settlements plus the longest-road bonus.
        assert_eq!(mine.victory_points, 4);
    }

    #[test]
    fn opponent_settlement_breaks_longest_road() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let owner = game.current_player();
        clear_player_roads(&mut game, owner);

        // Owner builds a five-road chain and takes the longest road.
        game.players[owner].resources = Resources {
            brick: 10,
            lumber: 10,
            ..Default::default()
        };
        let start = game
            .board
            .vertices
            .iter()
            .find(|v| matches!(v.building, Some(Building::Settlement(pid)) if pid == owner))
            .map(|v| v.id)
            .unwrap();
        let mut visited = HashSet::from([start]);
        let chain = road_chain_from(&game, start, 5, &mut visited).expect("five-edge road chain");
        for &edge_id in &chain {
            game.apply_action(owner, Action::BuildRoad(edge_id))
                .unwrap();
        }
        assert_eq!(game.longest_road, Some(owner));

        game.apply_action(owner, Action::EndTurn).unwrap();
        let breaker = game.current_player();
        assert_ne!(breaker, owner);
        enter_main_phase(&mut game);

        // Vertices along the chain: start, v1, .., v5. Cutting at v2 or v3
        // leaves both segments shorter than five roads.
        let mut chain_vertices = vec![start];
        let mut current = start;
        for &edge_id in &chain {
            current = game.board.edges[edge_id].other_vertex(current).unwrap();
            chain_vertices.push(current);
        }
        let (cut_vertex, breaker_edge) = chain_vertices[2..=3]
            .iter()
            .copied()
            .find_map(|vertex| {
                let legal = game.board.vertices[vertex].building.is_none()
                    && game
                        .board
                        .adjacent_vertices(vertex)
                        .unwrap()
                        .iter()
                        .all(|&a| game.board.vertices[a].building.is_none());
                if !legal {
                    return None;
                }
                let edge = game.board.vertices[vertex]
                    .incident_edges
                    .iter()
                    .copied()
                    .find(|&e| game.board.edges[e].road.is_none())?;
                Some((vertex, edge))
            })
            .expect("cut vertex with a free edge for the breaker's road");

        // Give the breaker a road reaching the middle of the owner's chain,
        // then legally build the settlement that breaks the chain.
        game.board.edges[breaker_edge].road = Some(breaker);
        game.players[breaker].resources = settlement_cost();
        game.apply_action(breaker, Action::BuildSettlement(cut_vertex))
            .unwrap();

        // No segment is long enough anymore, so the award disappears.
        assert_eq!(game.longest_road, None);
        let view = game.view_for(owner);
        let owner_view = view.players.iter().find(|p| p.id == owner).unwrap();
        // Just the two setup settlements, no longest-road bonus.
        assert_eq!(owner_view.victory_points, 2);
    }

    /// Find a chain of `length` free edges starting from an unoccupied vertex,
    /// mirroring `longest_road_awarded_at_five`'s direct board manipulation.
    fn find_chain(game: &Game, length: usize) -> Option<(VertexId, Vec<EdgeId>)> {
        game.board
            .vertices
            .iter()
            .filter(|v| v.building.is_none())
            .find_map(|v| {
                let mut visited = HashSet::from([v.id]);
                road_chain_from(game, v.id, length, &mut visited).map(|chain| (v.id, chain))
            })
    }

    /// Like `find_chain`, but the start vertex must not touch any of `player`'s
    /// roads, so settling there cannot cut their road network in two.
    fn find_chain_off_network(
        game: &Game,
        length: usize,
        player: PlayerId,
    ) -> Option<(VertexId, Vec<EdgeId>)> {
        game.board
            .vertices
            .iter()
            .filter(|v| {
                v.building.is_none()
                    && v.incident_edges
                        .iter()
                        .all(|&e| game.board.edges[e].road != Some(player))
            })
            .find_map(|v| {
                let mut visited = HashSet::from([v.id]);
                road_chain_from(game, v.id, length, &mut visited).map(|chain| (v.id, chain))
            })
    }

    #[test]
    fn longest_road_holder_keeps_tie_but_can_be_overtaken() {
        let mut game = Game::new(make_players()).unwrap();
        let (first, second) = (0, 1);

        // First player takes the longest road at length five.
        let (start, chain) = find_chain(&game, 5).expect("five-edge road chain");
        game.board.vertices[start].building = Some(Building::Settlement(first));
        for &edge_id in &chain {
            game.board.edges[edge_id].road = Some(first);
        }
        game.recalculate_longest_road();
        assert_eq!(game.longest_road, Some(first));

        // Tying the holder at five roads is not enough to take the award.
        let (start2, chain2) =
            find_chain_off_network(&game, 6, first).expect("six-edge road chain");
        game.board.vertices[start2].building = Some(Building::Settlement(second));
        for &edge_id in &chain2[..5] {
            game.board.edges[edge_id].road = Some(second);
        }
        game.recalculate_longest_road();
        assert_eq!(game.longest_road, Some(first));

        // The sixth road strictly exceeds the holder's and takes the award.
        game.board.edges[chain2[5]].road = Some(second);
        game.recalculate_longest_road();
        assert_eq!(game.longest_road, Some(second));

        let view = game.view_for(second);
        let second_view = view.players.iter().find(|p| p.id == second).unwrap();
        // Settlement plus the longest-road bonus.
        assert_eq!(second_view.victory_points, 3);
        let first_view = view.players.iter().find(|p| p.id == first).unwrap();
        assert_eq!(first_view.victory_points, 1);
    }

    #[test]
    fn largest_army_awarded_at_three_knights() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        game.players[player].played_knights = 3;
        game.recalculate_largest_army();
        assert_eq!(game.largest_army, Some(player));
    }

    #[test]
    fn largest_army_awarded_via_knight_plays() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        let player = game.current_player();
        let rival = other_player(&game, player);

        // Play three knights across three turns (one dev card per turn).
        for _ in 0..3 {
            game.players[player].resources = Resources::zero();
            game.players[rival].resources = Resources::zero();
            game.players[player].hand.push(DevelopmentCard::Knight);
            let tile = game
                .board
                .tiles
                .iter()
                .find(|t| !t.has_robber)
                .map(|t| t.id)
                .unwrap();
            let target = robber_target(&game, player, tile);
            game.apply_action(player, Action::PlayKnight { tile, target })
                .unwrap();
            enter_main_phase(&mut game);
            game.apply_action(player, Action::EndTurn).unwrap();

            game.players[player].resources = Resources::zero();
            game.players[rival].resources = Resources::zero();
            enter_main_phase(&mut game);
            game.apply_action(rival, Action::EndTurn).unwrap();
        }

        assert_eq!(game.players[player].played_knights, 3);
        assert_eq!(game.largest_army, Some(player));
        assert!(
            game.event_log.iter().any(|event| {
                event.award_changes.iter().any(|change| {
                    change.award == GameAward::LargestArmy && change.holder == Some(player)
                })
            }),
            "largest army award should be logged"
        );
        let view = game.view_for(player);
        let mine = view.players.iter().find(|p| p.id == player).unwrap();
        // Two setup settlements plus the largest-army bonus.
        assert_eq!(mine.victory_points, 4);

        // A second player with four knights takes the award.
        game.players[rival].played_knights = 4;
        game.recalculate_largest_army();
        assert_eq!(game.largest_army, Some(rival));
        let view = game.view_for(player);
        let mine = view.players.iter().find(|p| p.id == player).unwrap();
        assert_eq!(mine.victory_points, 2);
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
                    .unwrap()
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
                .unwrap()
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
        let target = robber_target(&game, p2, tile);
        game.apply_action(p2, Action::PlayKnight { tile, target })
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

    #[test]
    fn game_rejects_non_contiguous_or_duplicate_player_ids() {
        let non_contiguous = vec![
            Player::new(0, "A", PlayerColor::Red),
            Player::new(2, "B", PlayerColor::Blue),
        ];
        assert_eq!(
            Game::new(non_contiguous).unwrap_err(),
            CatanError::InvalidPlayer
        );

        let duplicate = vec![
            Player::new(0, "A", PlayerColor::Red),
            Player::new(0, "B", PlayerColor::Blue),
        ];
        assert_eq!(Game::new(duplicate).unwrap_err(), CatanError::InvalidPlayer);
    }

    #[test]
    fn invalid_client_build_ids_return_errors() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        assert_eq!(
            game.apply_action(player, Action::PlaceSettlement(usize::MAX)),
            Err(CatanError::InvalidBuildLocation)
        );
    }

    #[test]
    fn paid_road_requires_an_available_piece() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        enter_main_phase(&mut game);
        let player = game.current_player();
        let edge = game
            .board
            .edges
            .iter()
            .find(|edge| game.can_build_road(player, edge.id))
            .unwrap()
            .id;
        game.players[player].resources = road_cost();
        game.players[player].remaining_roads = 0;
        assert_eq!(
            game.apply_action(player, Action::BuildRoad(edge)),
            Err(CatanError::NoPiecesRemaining)
        );
    }

    #[test]
    fn invalid_knight_move_does_not_consume_card() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);
        let player = game.current_player();
        game.players[player].hand.push(DevelopmentCard::Knight);
        game.phase = GamePhase::Turn(TurnPhase::Main);

        assert_eq!(
            game.apply_action(
                player,
                Action::PlayKnight {
                    tile: usize::MAX,
                    target: None,
                },
            ),
            Err(CatanError::InvalidRobberLocation)
        );
        assert!(game.players[player].hand.contains(&DevelopmentCard::Knight));
        assert_eq!(game.players[player].played_knights, 0);
        assert!(!game.played_dev_card_this_turn);
    }

    #[test]
    fn hidden_victory_point_cards_can_win() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        game.players[player].new_cards = vec![DevelopmentCard::VictoryPoint; 10];
        game.check_winner();
        assert_eq!(game.winner, Some(player));
        assert_eq!(game.players[player].victory_point_cards, 10);
        assert!(
            !game.players[player]
                .hand
                .iter()
                .chain(&game.players[player].new_cards)
                .any(|&card| card == DevelopmentCard::VictoryPoint)
        );
        let opponent_view = game.view_for(1 - player);
        let winner_view = opponent_view
            .players
            .iter()
            .find(|p| p.id == player)
            .unwrap();
        assert_eq!(winner_view.victory_points, 10);
        assert!(winner_view.my_hand.is_none());
    }

    #[test]
    fn award_holders_keep_ties_and_longest_road_can_clear() {
        let mut game = Game::new(make_players()).unwrap();
        game.players[0].played_knights = 3;
        game.recalculate_largest_army();
        assert_eq!(game.largest_army, Some(0));
        game.players[1].played_knights = 3;
        game.recalculate_largest_army();
        assert_eq!(game.largest_army, Some(0));
        game.players[1].played_knights = 4;
        game.recalculate_largest_army();
        assert_eq!(game.largest_army, Some(1));

        game.longest_road = Some(0);
        game.recalculate_longest_road();
        assert_eq!(game.longest_road, None);
    }

    #[test]
    fn longest_road_counts_every_edge_in_a_cycle() {
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        let edges = game.board.tiles[0].edges;
        for edge in edges {
            game.board.edges[edge].road = Some(player);
        }
        assert_eq!(game.longest_road_for_player(player), 6);
    }

    fn robber_target_for_tile(game: &Game, player: PlayerId, tile: TileId) -> Option<PlayerId> {
        game.board.tiles[tile]
            .vertices
            .iter()
            .filter_map(|&vertex| match game.board.vertices[vertex].building {
                Some(Building::Settlement(pid) | Building::City(pid))
                    if pid != player && game.players[pid].resources.total() > 0 =>
                {
                    Some(pid)
                }
                _ => None,
            })
            .next()
    }

    #[test]
    fn event_log_replays_full_game_state() {
        let mut game = Game::new_with_seed(make_players(), 12345).unwrap();
        run_setup(&mut game);

        // Run a handful of turns, resolving sevens when they appear.
        for _ in 0..6 {
            let player = game.current_player();
            game.apply_action(player, Action::RollDice).unwrap();

            if let GamePhase::Turn(TurnPhase::ResolveSeven) = game.phase {
                let tile = game
                    .board
                    .tiles
                    .iter()
                    .find(|t| !t.has_robber && t.terrain != Terrain::Desert)
                    .map(|t| t.id)
                    .unwrap();
                let target = robber_target_for_tile(&game, player, tile);
                game.apply_action(player, Action::MoveRobber { tile, target })
                    .unwrap();
            }

            game.apply_action(player, Action::EndTurn).unwrap();
        }

        let log = game.log();
        let replayed = Game::replay(&log).unwrap();

        assert_eq!(game, replayed);
        assert_eq!(game.board, replayed.board);
        assert_eq!(game.players, replayed.players);
        assert_eq!(game.phase, replayed.phase);
        assert_eq!(game.dice, replayed.dice);
        assert_eq!(game.winner, replayed.winner);
    }

    #[test]
    fn invalid_actions_are_not_logged() {
        let mut game = Game::new(make_players()).unwrap();
        let current = game.current_player();
        let other = other_player(&game, current);
        let result = game.apply_action(other, Action::RollDice);
        assert_eq!(result.unwrap_err(), CatanError::NotYourTurn);
        assert!(game.event_log.is_empty());
    }

    #[test]
    fn event_log_records_timestamps() {
        let before = u64::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        let mut game = Game::new(make_players()).unwrap();
        let player = game.current_player();
        let vertex = valid_settlement_vertex(&game).unwrap();
        game.apply_action(player, Action::PlaceSettlement(vertex))
            .unwrap();
        let timestamp = game.event_log[0].timestamp_ms;
        assert!(timestamp >= before);
        assert!(timestamp > 0);
    }

    #[test]
    fn event_log_records_dice_rolls_and_robber_outcomes() {
        // Try seeds until the first roll is a 7 so we can exercise the robber path.
        let mut game = None;
        for seed in 0..10_000u64 {
            let mut g = Game::new_with_seed(make_players(), seed).unwrap();
            run_setup(&mut g);
            let player = g.current_player();
            g.apply_action(player, Action::RollDice).unwrap();
            if let GamePhase::Turn(TurnPhase::ResolveSeven) = g.phase {
                game = Some((g, player));
                break;
            }
        }
        let (mut game, player) = game.expect("found a seed that rolls a 7");

        let tile = game
            .board
            .tiles
            .iter()
            .find(|t| !t.has_robber && t.terrain != Terrain::Desert)
            .map(|t| t.id)
            .unwrap();
        let target = robber_target_for_tile(&game, player, tile);
        game.apply_action(player, Action::MoveRobber { tile, target })
            .unwrap();

        let dice_events: Vec<_> = game
            .event_log
            .iter()
            .filter(|e| matches!(e.outcome, EventOutcome::DiceRoll { .. }))
            .collect();
        assert_eq!(dice_events.len(), 1);
        let EventOutcome::DiceRoll {
            d1, d2, discards, ..
        } = &dice_events[0].outcome
        else {
            panic!("expected dice roll outcome");
        };
        assert_eq!(d1 + d2, 7);
        let mut expected_discards = discards.clone();
        expected_discards.sort_by_key(|demand| demand.player);
        assert_eq!(discards, &expected_discards);

        let robber_events: Vec<_> = game
            .event_log
            .iter()
            .filter(|e| matches!(e.action, Action::MoveRobber { .. }))
            .collect();
        assert_eq!(robber_events.len(), 1);
        let robber = robber_events[0];
        match (&robber.action, &robber.outcome) {
            (
                Action::MoveRobber {
                    target: Some(victim),
                    ..
                },
                EventOutcome::ResourceStolen { resource },
            ) => {
                let mut stolen = Resources::zero();
                stolen.add(*resource, 1);
                let mut expected = vec![
                    ResourceDelta {
                        player,
                        gained: stolen,
                        lost: Resources::zero(),
                    },
                    ResourceDelta {
                        player: *victim,
                        gained: Resources::zero(),
                        lost: stolen,
                    },
                ];
                expected.sort_by_key(|delta| delta.player);
                assert_eq!(robber.resource_changes, expected);
            }
            _ => assert!(robber.resource_changes.is_empty()),
        }
    }

    #[test]
    fn event_log_records_resource_production() {
        let mut game = None;
        for seed in 0..10_000u64 {
            let mut g = Game::new_with_seed(make_players(), seed).unwrap();
            run_setup(&mut g);
            let player = g.current_player();
            g.apply_action(player, Action::RollDice).unwrap();
            if matches!(g.phase, GamePhase::Turn(TurnPhase::Main)) {
                game = Some(g);
                break;
            }
        }
        let game = game.expect("found a seed that does not roll a 7");
        let (d1, d2) = game.dice.expect("dice were rolled");
        let roll = d1 + d2;
        assert_ne!(roll, 7);

        let event = game
            .event_log
            .iter()
            .rev()
            .find(|e| matches!(e.action, Action::RollDice))
            .expect("roll event");
        let EventOutcome::DiceRoll { production, .. } = &event.outcome else {
            panic!("expected dice roll outcome");
        };

        let mut expected: HashMap<PlayerId, Resources> = HashMap::new();
        for tile in &game.board.tiles {
            if tile.has_robber || tile.number != Some(roll) {
                continue;
            }
            let Some(resource) = tile.terrain.resource() else {
                continue;
            };
            for &vertex_id in &tile.vertices {
                match game.board.vertices[vertex_id].building {
                    Some(Building::City(pid)) => {
                        expected.entry(pid).or_default().add(resource, 2);
                    }
                    Some(Building::Settlement(pid)) => {
                        expected.entry(pid).or_default().add(resource, 1);
                    }
                    _ => {}
                }
            }
        }
        let mut expected: Vec<ResourcePayout> = expected
            .into_iter()
            .map(|(player, resources)| ResourcePayout { player, resources })
            .collect();
        expected.sort_by_key(|p| p.player);
        assert_eq!(production, &expected);
        let expected_changes: Vec<ResourceDelta> = expected
            .into_iter()
            .map(|payout| ResourceDelta {
                player: payout.player,
                gained: payout.resources,
                lost: Resources::zero(),
            })
            .collect();
        assert_eq!(event.resource_changes, expected_changes);
    }

    #[test]
    fn event_log_records_starting_resources_on_second_settlement() {
        let mut game = Game::new(make_players()).unwrap();
        run_setup(&mut game);

        let mut settlements_by_player: HashMap<PlayerId, usize> = HashMap::new();
        let mut starting_payouts = 0;
        for event in &game.event_log {
            let Action::PlaceSettlement(vertex) = event.action else {
                continue;
            };
            let count = settlements_by_player.entry(event.player).or_default();
            *count += 1;
            if *count == 1 {
                assert_eq!(event.outcome, EventOutcome::None);
                continue;
            }

            let EventOutcome::ResourcesGained { resources } = &event.outcome else {
                panic!("expected starting resources on second settlement");
            };
            let mut expected = Resources::zero();
            for &tile_id in &game.board.vertices[vertex].incident_tiles {
                if let Some(resource) = game.board.tiles[tile_id].terrain.resource() {
                    expected.add(resource, 1);
                }
            }
            assert_eq!(resources, &expected);
            assert_eq!(
                event.resource_changes,
                vec![ResourceDelta {
                    player: event.player,
                    gained: expected,
                    lost: Resources::zero(),
                }]
            );
            starting_payouts += 1;
        }
        assert_eq!(starting_payouts, game.players.len());
    }

    #[test]
    fn game_log_serializes_and_deserializes() {
        let mut game = Game::new_with_seed(make_players(), 42).unwrap();
        run_setup(&mut game);
        let player = game.current_player();
        game.apply_action(player, Action::RollDice).unwrap();
        if let GamePhase::Turn(TurnPhase::ResolveSeven) = game.phase {
            let tile = game
                .board
                .tiles
                .iter()
                .find(|t| !t.has_robber && t.terrain != Terrain::Desert)
                .map(|t| t.id)
                .unwrap();
            let target = robber_target_for_tile(&game, player, tile);
            game.apply_action(player, Action::MoveRobber { tile, target })
                .unwrap();
        }

        let log = game.log();
        let json = serde_json::to_string(&log).unwrap();
        let decoded: GameLog = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.seed, log.seed);
        assert_eq!(decoded.events.len(), log.events.len());
        assert_eq!(decoded.events, log.events);
    }

    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingSubscriber {
        count: AtomicUsize,
    }

    impl CountingSubscriber {
        fn new() -> Self {
            Self {
                count: AtomicUsize::new(0),
            }
        }
    }

    impl EventSubscriber for CountingSubscriber {
        fn on_event(&self, _event: &GameEvent) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn event_subscribers_receive_applied_events() {
        let subscriber = Arc::new(CountingSubscriber::new());
        let mut game = Game::new(make_players()).unwrap();
        game.subscribe(subscriber.clone());
        run_setup(&mut game);

        // Setup consists of 4 settlement placements and 4 road placements.
        assert_eq!(subscriber.count.load(Ordering::SeqCst), 8);
    }
}
