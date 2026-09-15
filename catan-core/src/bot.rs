//! Deterministic heuristic player for driving local games and simulations.

use crate::{
    Action, Building, DevelopmentCard, Game, GamePhase, PlayerId, Resource, Resources, TurnPhase,
    city_cost, road_cost, settlement_cost,
};
use std::cmp::Reverse;

/// Choose one legal action for `player`.
///
/// The policy is deterministic for a given game state. It handles out-of-turn
/// trade responses and discards as well as every normal setup and turn phase.
/// `None` means the player has no legal action (or is not part of the game).
pub fn choose_action(game: &Game, player: PlayerId) -> Option<Action> {
    if player >= game.players.len() || matches!(game.phase, GamePhase::Finished) {
        return None;
    }

    if let Some(action) = incoming_trade_action(game, player)
        && game.is_action_legal(player, action)
    {
        return Some(action);
    }

    if let Some(&required) = game.pending_discards.get(&player) {
        let action = Action::Discard(discard_bundle(
            game.players[player].resources,
            required,
            preferred_cost(game, player),
        ));
        return game.is_action_legal(player, action).then_some(action);
    }

    if player != game.current_player() {
        return None;
    }

    match game.phase {
        GamePhase::Setup {
            awaiting_road_for, ..
        } => {
            if awaiting_road_for.is_some() {
                ranked_roads(game, player, true)
                    .into_iter()
                    .map(Action::PlaceRoad)
                    .find(|&action| game.is_action_legal(player, action))
            } else {
                let mut vertices: Vec<_> = game.board.vertices.iter().map(|v| v.id).collect();
                vertices.sort_by_key(|&vertex| (Reverse(vertex_score(game, vertex)), vertex));
                vertices
                    .into_iter()
                    .map(Action::PlaceSettlement)
                    .find(|&action| game.is_action_legal(player, action))
            }
        }
        GamePhase::Turn(TurnPhase::Roll) => {
            let action = Action::RollDice;
            game.is_action_legal(player, action).then_some(action)
        }
        GamePhase::Turn(TurnPhase::ResolveSeven) => robber_action(game, player, false),
        GamePhase::Turn(TurnPhase::Main) => choose_main_action(game, player),
        GamePhase::Finished => None,
    }
}

fn choose_main_action(game: &Game, player: PlayerId) -> Option<Action> {
    let mut cities: Vec<_> = game
        .board
        .vertices
        .iter()
        .filter(|v| v.building == Some(Building::Settlement(player)))
        .map(|v| v.id)
        .collect();
    cities.sort_by_key(|&vertex| (Reverse(vertex_score(game, vertex)), vertex));
    if let Some(action) = cities
        .into_iter()
        .map(Action::BuildCity)
        .find(|&action| game.is_action_legal(player, action))
    {
        return Some(action);
    }

    let mut settlements: Vec<_> = game.board.vertices.iter().map(|v| v.id).collect();
    settlements.sort_by_key(|&vertex| (Reverse(vertex_score(game, vertex)), vertex));
    if let Some(action) = settlements
        .into_iter()
        .map(Action::BuildSettlement)
        .find(|&action| game.is_action_legal(player, action))
    {
        return Some(action);
    }

    if !game.played_dev_card_this_turn {
        if game.players[player]
            .hand
            .contains(&DevelopmentCard::RoadBuilding)
            && let Some(action) = road_building_action(game, player)
        {
            return Some(action);
        }
        if game.players[player]
            .hand
            .contains(&DevelopmentCard::Invention)
            && let Some(action) = invention_action(game, player)
        {
            return Some(action);
        }
        if game.players[player]
            .hand
            .contains(&DevelopmentCard::Monopoly)
            && let Some(action) = monopoly_action(game, player)
        {
            return Some(action);
        }
        if game.players[player].hand.contains(&DevelopmentCard::Knight)
            && let Some(action) = robber_action(game, player, true)
        {
            return Some(action);
        }
    }

    if let Some(action) = ranked_roads(game, player, false)
        .into_iter()
        .map(Action::BuildRoad)
        .find(|&action| game.is_action_legal(player, action))
    {
        return Some(action);
    }

    let buy = Action::BuyDevelopmentCard;
    if game.is_action_legal(player, buy) {
        return Some(buy);
    }

    if game.pending_trade.is_none()
        && !offered_trade_this_turn(game, player)
        && let Some(action) = outgoing_trade_action(game, player)
        && game.is_action_legal(player, action)
    {
        return Some(action);
    }

    if let Some(action) = bank_trade_action(game, player)
        && game.is_action_legal(player, action)
    {
        return Some(action);
    }

    let end = Action::EndTurn;
    game.is_action_legal(player, end).then_some(end)
}

fn incoming_trade_action(game: &Game, player: PlayerId) -> Option<Action> {
    let trade = game.pending_trade.as_ref()?;
    if trade.from == player
        || player == game.current_player()
        || trade.rejected_by.contains(&player)
    {
        return None;
    }
    let can_pay = game.players[player].resources.can_afford(&trade.receive);
    let target = preferred_cost(game, player);
    let accept = can_pay
        && bundle_value(&trade.give, &target, game.players[player].resources)
            > bundle_value(&trade.receive, &target, game.players[player].resources);
    Some(Action::RespondTrade { accept })
}

fn outgoing_trade_action(game: &Game, player: PlayerId) -> Option<Action> {
    let resources = game.players[player].resources;
    let target = preferred_cost(game, player);
    let receive = Resource::all()
        .into_iter()
        .filter(|&resource| resources.get(resource) < target.get(resource))
        .max_by_key(|&resource| {
            (
                target.get(resource) - resources.get(resource),
                resource_priority(resource),
            )
        })?;
    let give = Resource::all()
        .into_iter()
        .filter(|&resource| resource != receive && resources.get(resource) > target.get(resource))
        .max_by_key(|&resource| {
            (
                resources.get(resource) - target.get(resource),
                resource_priority(resource),
            )
        })?;
    let surplus = resources.get(give) - target.get(give);
    let shortage = target.get(receive) - resources.get(receive);
    let give_amount = if surplus >= 2 && shortage >= 2 { 2 } else { 1 };
    Some(Action::OfferTrade {
        give: one(give, give_amount),
        receive: one(receive, 1),
    })
}

fn offered_trade_this_turn(game: &Game, player: PlayerId) -> bool {
    game.event_log
        .iter()
        .rev()
        .take_while(|event| !matches!(event.action, Action::EndTurn))
        .any(|event| event.player == player && matches!(event.action, Action::OfferTrade { .. }))
}

fn bank_trade_action(game: &Game, player: PlayerId) -> Option<Action> {
    let resources = game.players[player].resources;
    let target = preferred_cost(game, player);
    let receive = Resource::all()
        .into_iter()
        .filter(|&resource| resources.get(resource) < target.get(resource))
        .max_by_key(|&resource| {
            (
                target.get(resource) - resources.get(resource),
                resource_priority(resource),
            )
        })?;
    let give = Resource::all()
        .into_iter()
        .filter(|&resource| {
            resource != receive && resources.get(resource) >= target.get(resource).saturating_add(4)
        })
        .max_by_key(|&resource| resources.get(resource) - target.get(resource))?;
    Some(Action::BankTrade {
        give: one(give, 4),
        receive: one(receive, 1),
    })
}

fn preferred_cost(game: &Game, player: PlayerId) -> Resources {
    let has_settlement = game
        .board
        .vertices
        .iter()
        .any(|v| v.building == Some(Building::Settlement(player)));
    if has_settlement && game.players[player].remaining_cities > 0 {
        city_cost()
    } else if game.players[player].remaining_settlements > 0 {
        settlement_cost()
    } else if !game.development_deck.is_empty() {
        Resources {
            ore: 1,
            grain: 1,
            wool: 1,
            ..Resources::zero()
        }
    } else {
        road_cost()
    }
}

fn discard_bundle(resources: Resources, required: u16, target: Resources) -> Resources {
    let mut result = Resources::zero();
    let mut remaining = required;
    let mut order = Resource::all();
    order.sort_by_key(|&resource| {
        (
            target.get(resource) >= resources.get(resource),
            resource_priority(resource),
        )
    });
    for resource in order {
        let amount = u16::from(resources.get(resource)).min(remaining) as u8;
        result.set(resource, amount);
        remaining -= u16::from(amount);
        if remaining == 0 {
            break;
        }
    }
    result
}

fn robber_action(game: &Game, player: PlayerId, knight: bool) -> Option<Action> {
    let mut candidates = Vec::new();
    for tile in &game.board.tiles {
        if tile.has_robber {
            continue;
        }
        let mut victims: Vec<_> = tile
            .vertices
            .iter()
            .filter_map(|&vertex| match game.board.vertices[vertex].building {
                Some(Building::Settlement(owner) | Building::City(owner))
                    if owner != player && game.players[owner].resources.total() > 0 =>
                {
                    Some(owner)
                }
                _ => None,
            })
            .collect();
        victims.sort_unstable();
        victims.dedup();
        if victims.is_empty() {
            candidates.push((
                robber_tile_score(game, player, tile.id, None),
                tile.id,
                None,
            ));
        } else {
            for victim in victims {
                candidates.push((
                    robber_tile_score(game, player, tile.id, Some(victim)),
                    tile.id,
                    Some(victim),
                ));
            }
        }
    }
    candidates.sort_by_key(|&(score, tile, target)| (Reverse(score), tile, target));
    candidates.into_iter().find_map(|(_, tile, target)| {
        let action = if knight {
            Action::PlayKnight { tile, target }
        } else {
            Action::MoveRobber { tile, target }
        };
        game.is_action_legal(player, action).then_some(action)
    })
}

fn robber_tile_score(
    game: &Game,
    player: PlayerId,
    tile_id: usize,
    target: Option<PlayerId>,
) -> i32 {
    let tile = &game.board.tiles[tile_id];
    let probability = tile.number.map(number_weight).unwrap_or(0) as i32;
    let mut score = target
        .map(|victim| i32::from(game.players[victim].resources.total()) + 20)
        .unwrap_or(0);
    for &vertex in &tile.vertices {
        score += match game.board.vertices[vertex].building {
            Some(Building::Settlement(owner)) if owner == player => -probability * 3,
            Some(Building::City(owner)) if owner == player => -probability * 6,
            Some(Building::Settlement(_)) => probability * 2,
            Some(Building::City(_)) => probability * 4,
            None => 0,
        };
    }
    score
}

fn road_building_action(game: &Game, player: PlayerId) -> Option<Action> {
    let roads = ranked_roads(game, player, false);
    for &first in &roads {
        for &second in &roads {
            if first == second {
                continue;
            }
            let action = Action::PlayRoadBuilding(first, second);
            if game.is_action_legal(player, action) {
                return Some(action);
            }
        }
    }
    None
}

fn invention_action(game: &Game, player: PlayerId) -> Option<Action> {
    let resources = game.players[player].resources;
    let target = preferred_cost(game, player);
    let mut choices = Resource::all();
    choices.sort_by_key(|&resource| {
        (
            Reverse(target.get(resource).saturating_sub(resources.get(resource))),
            Reverse(resource_priority(resource)),
        )
    });
    for first in choices {
        for second in choices {
            let action = Action::PlayInvention(first, second);
            if game.is_action_legal(player, action) {
                return Some(action);
            }
        }
    }
    None
}

fn monopoly_action(game: &Game, player: PlayerId) -> Option<Action> {
    let mut resources = Resource::all();
    resources.sort_by_key(|&resource| {
        Reverse(
            game.players
                .iter()
                .filter(|p| p.id != player)
                .map(|p| u16::from(p.resources.get(resource)))
                .sum::<u16>(),
        )
    });
    resources
        .into_iter()
        .map(Action::PlayMonopoly)
        .find(|&action| game.is_action_legal(player, action))
}

fn ranked_roads(game: &Game, player: PlayerId, setup: bool) -> Vec<usize> {
    let mut roads: Vec<_> = game.board.edges.iter().map(|edge| edge.id).collect();
    roads.sort_by_key(|&edge_id| {
        let edge = &game.board.edges[edge_id];
        let score = edge
            .endpoints
            .iter()
            .map(|&vertex| vertex_score(game, vertex))
            .max()
            .unwrap_or(0)
            + if setup {
                0
            } else {
                edge.endpoints
                    .iter()
                    .filter(|&&vertex| game.board.vertices[vertex].building.is_none())
                    .count() as i32
                    * 3
            };
        (Reverse(score), edge_id)
    });
    let _ = player;
    roads
}

fn vertex_score(game: &Game, vertex: usize) -> i32 {
    let Some(vertex) = game.board.vertices.get(vertex) else {
        return i32::MIN;
    };
    let mut seen = [false; 5];
    let mut score = 0;
    for &tile_id in &vertex.incident_tiles {
        let tile = &game.board.tiles[tile_id];
        score += i32::from(tile.number.map(number_weight).unwrap_or(0)) * 4;
        if let Some(resource) = tile.terrain.resource() {
            seen[resource_index(resource)] = true;
        }
    }
    score += seen.into_iter().filter(|seen| *seen).count() as i32 * 3;
    score += i32::from(vertex.port.is_some()) * 2;
    score
}

fn bundle_value(bundle: &Resources, target: &Resources, held: Resources) -> u16 {
    Resource::all()
        .into_iter()
        .map(|resource| {
            let useful = held.get(resource) < target.get(resource);
            u16::from(bundle.get(resource)) * if useful { 4 } else { 1 }
        })
        .sum()
}

fn one(resource: Resource, amount: u8) -> Resources {
    let mut resources = Resources::zero();
    resources.set(resource, amount);
    resources
}

fn resource_priority(resource: Resource) -> u8 {
    match resource {
        Resource::Ore => 5,
        Resource::Grain => 4,
        Resource::Wool => 3,
        Resource::Lumber => 2,
        Resource::Brick => 1,
    }
}

fn resource_index(resource: Resource) -> usize {
    match resource {
        Resource::Brick => 0,
        Resource::Lumber => 1,
        Resource::Ore => 2,
        Resource::Grain => 3,
        Resource::Wool => 4,
    }
}

fn number_weight(number: u8) -> u8 {
    match number {
        6 | 8 => 5,
        5 | 9 => 4,
        4 | 10 => 3,
        3 | 11 => 2,
        2 | 12 => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Player, PlayerColor};

    fn bots(count: usize) -> Vec<Player> {
        PlayerColor::all()
            .into_iter()
            .take(count)
            .enumerate()
            .map(|(id, color)| Player::bot(id, format!("Bot {id}"), color))
            .collect()
    }

    #[test]
    fn setup_choices_are_deterministic_and_legal() {
        let game = Game::new_with_seed(bots(2), 11).unwrap();
        let player = game.current_player();
        let first = choose_action(&game, player).unwrap();
        assert_eq!(first, choose_action(&game, player).unwrap());
        assert!(matches!(first, Action::PlaceSettlement(_)));
        assert!(game.is_action_legal(player, first));
    }

    #[test]
    fn discard_choice_has_required_size_and_is_legal() {
        let mut game = Game::new_with_seed(bots(2), 12).unwrap();
        game.phase = GamePhase::Turn(TurnPhase::ResolveSeven);
        game.players[0].resources = Resources {
            brick: 4,
            lumber: 3,
            ore: 2,
            grain: 1,
            wool: 0,
        };
        game.pending_discards.insert(0, 5);
        let action = choose_action(&game, 0).unwrap();
        let Action::Discard(resources) = action else {
            panic!("expected discard");
        };
        assert_eq!(resources.total(), 5);
        assert!(game.is_action_legal(0, action));
    }

    #[test]
    fn incoming_trade_is_answered_legally() {
        let mut game = Game::new_with_seed(bots(2), 13).unwrap();
        game.phase = GamePhase::Turn(TurnPhase::Main);
        let from = game.current_player();
        let responder = 1 - from;
        game.players[from].resources.brick = 1;
        game.players[responder].resources.ore = 1;
        game.apply_action(
            from,
            Action::OfferTrade {
                give: one(Resource::Brick, 1),
                receive: one(Resource::Ore, 1),
            },
        )
        .unwrap();
        let action = choose_action(&game, responder).unwrap();
        assert!(matches!(action, Action::RespondTrade { .. }));
        assert!(game.is_action_legal(responder, action));
    }

    #[test]
    fn turn_phase_choices_cover_roll_robber_and_development_cards() {
        let mut game = Game::new_with_seed(bots(2), 140).unwrap();
        let player = game.current_player();

        game.phase = GamePhase::Turn(TurnPhase::Roll);
        assert_eq!(choose_action(&game, player), Some(Action::RollDice));

        game.phase = GamePhase::Turn(TurnPhase::ResolveSeven);
        let robber = choose_action(&game, player).unwrap();
        assert!(matches!(robber, Action::MoveRobber { .. }));
        assert!(game.is_action_legal(player, robber));

        game.phase = GamePhase::Turn(TurnPhase::Main);
        game.players[player].hand.push(DevelopmentCard::Invention);
        let invention = choose_action(&game, player).unwrap();
        assert!(matches!(invention, Action::PlayInvention(_, _)));
        assert!(game.is_action_legal(player, invention));
    }

    #[test]
    fn main_phase_prioritizes_builds_and_proposes_resource_trades() {
        let mut game = Game::new_with_seed(bots(2), 150).unwrap();
        let player = game.current_player();
        game.phase = GamePhase::Turn(TurnPhase::Main);
        let vertex = game.board.vertices[0].id;
        game.board.vertices[vertex].building = Some(Building::Settlement(player));
        game.players[player].resources = city_cost();

        let build = choose_action(&game, player).unwrap();
        assert_eq!(build, Action::BuildCity(vertex));
        assert!(game.is_action_legal(player, build));

        game.board.vertices[vertex].building = None;
        game.players[player].resources = one(Resource::Brick, 2);
        let trade = choose_action(&game, player).unwrap();
        assert!(matches!(trade, Action::OfferTrade { .. }));
        assert!(game.is_action_legal(player, trade));
    }

    #[test]
    fn bank_trade_uses_only_resources_beyond_build_reserves() {
        let mut game = Game::new_with_seed(bots(2), 160).unwrap();
        let player = game.current_player();
        game.phase = GamePhase::Turn(TurnPhase::Main);
        game.players[player].resources = one(Resource::Brick, 5);
        game.pending_trade = Some(crate::TradeOffer {
            trade_id: 99,
            from: player,
            give: one(Resource::Brick, 1),
            receive: one(Resource::Grain, 1),
            rejected_by: Vec::new(),
        });

        let action = choose_action(&game, player).unwrap();
        assert!(matches!(action, Action::BankTrade { .. }));
        assert!(game.is_action_legal(player, action));
    }

    #[test]
    fn bot_does_not_repeat_a_rejected_offer_in_the_same_turn() {
        let mut game = Game::new_with_seed(bots(2), 14).unwrap();
        game.phase = GamePhase::Turn(TurnPhase::Main);
        let player = game.current_player();
        let responder = 1 - player;
        game.players[player].resources.ore = 2;

        let offer = choose_action(&game, player).unwrap();
        assert!(matches!(offer, Action::OfferTrade { .. }));
        game.apply_action(player, offer).unwrap();
        game.apply_action(responder, Action::RespondTrade { accept: false })
            .unwrap();

        assert!(!matches!(
            choose_action(&game, player),
            Some(Action::OfferTrade { .. })
        ));
    }

    #[test]
    fn four_bots_make_only_legal_progress() {
        let mut game = Game::new_with_seed(bots(4), 2026).unwrap();
        for _ in 0..400 {
            if game.winner.is_some() {
                break;
            }

            if !game.pending_discards.is_empty() {
                let players: Vec<_> = game.pending_discards.keys().copied().collect();
                for player in players {
                    let action = choose_action(&game, player).expect("bot must discard");
                    assert!(game.is_action_legal(player, action));
                    game.apply_action(player, action).unwrap();
                }
                continue;
            }

            if let Some(trade) = game.pending_trade.clone() {
                let mut answered = false;
                for player in 0..game.players.len() {
                    if player != trade.from
                        && !trade.rejected_by.contains(&player)
                        && let Some(action) = choose_action(&game, player)
                    {
                        assert!(game.is_action_legal(player, action));
                        game.apply_action(player, action).unwrap();
                        answered = true;
                        if game.pending_trade.is_none() {
                            break;
                        }
                    }
                }
                if answered {
                    continue;
                }
            }

            let player = game.current_player();
            let action = choose_action(&game, player).expect("active bot needs an action");
            assert!(game.is_action_legal(player, action), "{action:?}");
            game.apply_action(player, action).unwrap();
        }

        assert!(
            !matches!(game.phase, GamePhase::Setup { .. }),
            "bots should complete setup"
        );
        assert!(game.event_log.len() > 50);
        assert!(
            game.event_log
                .iter()
                .filter(|event| matches!(event.action, Action::EndTurn))
                .count()
                > 5,
            "bots should advance through multiple turns"
        );
    }
}
