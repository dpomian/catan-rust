//! Drive four heuristic bots through a full game and audit the event log.
//!
//! Run with:
//! `cargo test -p catan-core --test four_bot_audit -- --nocapture`
//!
//! Configurable all-bot batches:
//! `cargo test -p catan-core --test bot_games -- --bots 3 --games 100`

mod common;

use catan_core::{Action, Building, EventOutcome, Game, GameAward, GamePhase, PlayerId, Resources};
use common::{MAX_STEPS, bots, play_until_done, step};
use std::collections::BTreeMap;
use std::fmt::Write as _;

const SEED: u64 = 20_260_915;

const fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::PlaceSettlement(_) => "PlaceSettlement",
        Action::PlaceRoad(_) => "PlaceRoad",
        Action::RollDice => "RollDice",
        Action::MoveRobber { .. } => "MoveRobber",
        Action::Discard(_) => "Discard",
        Action::BuildRoad(_) => "BuildRoad",
        Action::BuildSettlement(_) => "BuildSettlement",
        Action::BuildCity(_) => "BuildCity",
        Action::BuyDevelopmentCard => "BuyDevelopmentCard",
        Action::PlayKnight { .. } => "PlayKnight",
        Action::PlayRoadBuilding(_, _) => "PlayRoadBuilding",
        Action::PlayInvention(_, _) => "PlayInvention",
        Action::PlayMonopoly(_) => "PlayMonopoly",
        Action::OfferTrade { .. } => "OfferTrade",
        Action::RespondTrade { accept: true } => "RespondTradeAccept",
        Action::RespondTrade { accept: false } => "RespondTradeReject",
        Action::CancelTrade { .. } => "CancelTrade",
        Action::BankTrade { .. } => "BankTrade",
        Action::EndTurn => "EndTurn",
    }
}

fn settlement_city_points(game: &Game, player: PlayerId) -> u8 {
    game.board
        .vertices
        .iter()
        .map(|v| match v.building {
            Some(Building::Settlement(pid)) if pid == player => 1,
            Some(Building::City(pid)) if pid == player => 2,
            _ => 0,
        })
        .sum()
}

fn victory_points(game: &Game, player: PlayerId) -> u8 {
    let p = &game.players[player];
    let longest = if game.longest_road == Some(player) {
        2
    } else {
        0
    };
    let army = if game.largest_army == Some(player) {
        2
    } else {
        0
    };
    p.public_victory_points()
        + p.hidden_victory_points()
        + settlement_city_points(game, player)
        + longest
        + army
}

#[allow(clippy::too_many_lines)]
fn audit(game: &Game) -> (String, Vec<String>) {
    let mut report = String::new();
    let mut issues = Vec::new();
    let log = game.log();

    let _ = writeln!(
        report,
        "seed={} events={} winner={:?} phase={:?} turns={}",
        log.seed,
        log.events.len(),
        game.winner,
        game.phase,
        log.events
            .iter()
            .filter(|e| matches!(e.action, Action::EndTurn))
            .count()
    );

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for event in &log.events {
        *counts.entry(action_kind(&event.action)).or_default() += 1;
    }
    let _ = writeln!(report, "action counts:");
    for (kind, count) in &counts {
        let _ = writeln!(report, "  {kind}: {count}");
    }

    for (i, event) in log.events.iter().enumerate() {
        if event.sequence != i {
            issues.push(format!(
                "sequence gap: event {i} has sequence {}",
                event.sequence
            ));
        }
    }

    let setup_settlements = counts.get("PlaceSettlement").copied().unwrap_or(0);
    let setup_roads = counts.get("PlaceRoad").copied().unwrap_or(0);
    if setup_settlements != 8 || setup_roads != 8 {
        issues.push(format!(
            "setup should be 8 settlements + 8 roads, got {setup_settlements} + {setup_roads}"
        ));
    }

    let mut seven_rolls = 0usize;
    let mut production_mismatch = 0usize;
    let mut stolen_mismatch = 0usize;
    let mut invention_without_gain = 0usize;
    let mut monopoly_without_changes = 0usize;
    let mut buy_without_card_identity = 0usize;
    let mut dice_without_outcome = 0usize;

    for event in &log.events {
        match &event.action {
            Action::RollDice => match &event.outcome {
                EventOutcome::DiceRoll {
                    d1,
                    d2,
                    production,
                    discards,
                } => {
                    if !(1..=6).contains(d1) || !(1..=6).contains(d2) {
                        issues.push(format!(
                            "illegal dice {}+{} at seq {}",
                            d1, d2, event.sequence
                        ));
                    }
                    if d1 + d2 == 7 {
                        seven_rolls += 1;
                        if discards.iter().any(|d| d.count == 0) {
                            issues.push(format!(
                                "seven at seq {} listed a 0-card discard",
                                event.sequence
                            ));
                        }
                    } else {
                        let from_changes: Resources = event.resource_changes.iter().fold(
                            Resources::zero(),
                            |mut acc, delta| {
                                acc.add_resources(&delta.gained);
                                acc
                            },
                        );
                        let from_production: Resources =
                            production.iter().fold(Resources::zero(), |mut acc, p| {
                                acc.add_resources(&p.resources);
                                acc
                            });
                        if from_changes != from_production {
                            production_mismatch += 1;
                        }
                    }
                }
                _ => dice_without_outcome += 1,
            },
            Action::MoveRobber { .. } | Action::PlayKnight { .. } => {
                if let EventOutcome::ResourceStolen { resource } = event.outcome {
                    let thief_gain = event
                        .resource_changes
                        .iter()
                        .find(|d| d.player == event.player)
                        .map_or(0, |d| d.gained.get(resource));
                    if thief_gain != 1 {
                        stolen_mismatch += 1;
                    }
                }
            }
            Action::PlayInvention(_, _) => {
                if event.resource_changes.iter().all(|d| d.gained.total() == 0) {
                    invention_without_gain += 1;
                }
                if !matches!(event.outcome, EventOutcome::ResourcesGained { .. }) {
                    issues.push(format!(
                        "PlayInvention seq {} missing ResourcesGained outcome: {:?}",
                        event.sequence, event.outcome
                    ));
                }
            }
            Action::PlayMonopoly(_) => {
                if event.resource_changes.is_empty() {
                    monopoly_without_changes += 1;
                }
                if !matches!(event.outcome, EventOutcome::Monopoly { .. }) {
                    issues.push(format!(
                        "PlayMonopoly seq {} missing Monopoly outcome: {:?}",
                        event.sequence, event.outcome
                    ));
                }
            }
            Action::BuyDevelopmentCard => {
                buy_without_card_identity += 1;
                if !matches!(event.outcome, EventOutcome::None) {
                    issues.push(format!(
                        "BuyDevelopmentCard seq {} leaked outcome {:?}",
                        event.sequence, event.outcome
                    ));
                }
            }
            _ => {}
        }
    }

    if dice_without_outcome > 0 {
        issues.push(format!(
            "{dice_without_outcome} RollDice events missing DiceRoll outcome"
        ));
    }
    if production_mismatch > 0 {
        issues.push(format!(
            "{production_mismatch} dice rolls where production != resource_changes"
        ));
    }
    if stolen_mismatch > 0 {
        issues.push(format!(
            "{stolen_mismatch} ResourceStolen outcomes without a matching +1 resource_change"
        ));
    }
    if invention_without_gain > 0 {
        issues.push(format!(
            "{invention_without_gain} PlayInvention events with no resource gain logged"
        ));
    }

    let _ = writeln!(
        report,
        "sevens={seven_rolls} discards={} robber_moves={} knights={} buys={buy_without_card_identity} monopolies_no_delta={monopoly_without_changes}",
        counts.get("Discard").copied().unwrap_or(0),
        counts.get("MoveRobber").copied().unwrap_or(0),
        counts.get("PlayKnight").copied().unwrap_or(0),
    );

    match Game::replay(&log) {
        Ok(replayed) => {
            if replayed == *game {
                let _ = writeln!(report, "replay: exact gameplay match");
            } else {
                issues.push("replayed game != live game (gameplay fields)".into());
            }
        }
        Err(e) => issues.push(format!("replay failed: {e}")),
    }

    for (player, p) in game.players.iter().enumerate() {
        let from_board = settlement_city_points(game, player);
        if from_board == 0 && p.remaining_settlements == 5 {
            issues.push(format!("player {player} never placed a settlement"));
        }
        let roads_on_board = u8::try_from(
            game.board
                .edges
                .iter()
                .filter(|e| e.road == Some(player))
                .count(),
        )
        .unwrap_or(u8::MAX);
        let used_roads = 15u8.saturating_sub(p.remaining_roads);
        if roads_on_board != used_roads {
            issues.push(format!(
                "player {player} road pieces {used_roads} vs board {roads_on_board}"
            ));
        }
        let vp = victory_points(game, player);
        let _ = writeln!(
            report,
            "player {player} {} VP={} public_vp_cards={} hidden_vp={} knights={} resources={} hand={:?} new={:?} roads={} settlements={} cities={}",
            p.name,
            vp,
            p.victory_point_cards,
            p.hidden_victory_points(),
            p.played_knights,
            p.resources.total(),
            p.hand,
            p.new_cards,
            p.remaining_roads,
            p.remaining_settlements,
            p.remaining_cities,
        );
        if game.winner == Some(player) && vp < 10 {
            issues.push(format!("winner {player} has only {vp} VP"));
        }
        if game.winner.is_none() && vp >= 10 {
            issues.push(format!("player {player} has {vp} VP but winner is unset"));
        }
    }

    let _ = writeln!(
        report,
        "longest_road={:?} largest_army={:?}",
        game.longest_road, game.largest_army
    );

    if let Some(holder) = game.longest_road {
        let awarded = log.events.iter().any(|event| {
            event.award_changes.iter().any(|change| {
                change.award == GameAward::LongestRoad && change.holder == Some(holder)
            })
        });
        if !awarded {
            issues.push(format!(
                "longest road holder {holder} never appears in award_changes"
            ));
        }
    }
    if let Some(holder) = game.largest_army {
        let awarded = log.events.iter().any(|event| {
            event.award_changes.iter().any(|change| {
                change.award == GameAward::LargestArmy && change.holder == Some(holder)
            })
        });
        if !awarded {
            issues.push(format!(
                "largest army holder {holder} never appears in award_changes"
            ));
        }
    }
    if let Some(winner) = game.winner {
        if !log.events.iter().any(|event| event.winner == Some(winner)) {
            issues.push(format!("winner {winner} is not recorded on any event"));
        }
        if game.players[winner].hidden_victory_points() != 0 {
            issues.push(format!("winner {winner} still has unrevealed VP cards"));
        }
    }

    // Logging coverage notes derived from this game.
    let logging_gaps = [
        (
            buy_without_card_identity > 0,
            format!(
                "BuyDevelopmentCard ({buy_without_card_identity} times) does not record which card was drawn"
            ),
        ),
        (
            game.board.vertices.iter().any(|v| v.port.is_some()),
            "board has ports but bank trades are always 4:1 (no harbor events possible)".into(),
        ),
    ];

    let _ = writeln!(report, "logging observations:");
    for (present, note) in logging_gaps {
        if present {
            let _ = writeln!(report, "  - {note}");
        }
    }

    (report, issues)
}

#[test]
fn four_bots_full_game_log_audit() {
    let mut game = Game::new_with_seed(bots(4), SEED).expect("start");
    let mut stalled = None;
    for step_i in 0..MAX_STEPS {
        if game.winner.is_some() || matches!(game.phase, GamePhase::Finished) {
            break;
        }
        if let Err(e) = step(&mut game) {
            stalled = Some((step_i, e, game.phase, game.current_player()));
            break;
        }
    }

    let (report, mut issues) = audit(&game);
    println!("{report}");
    if let Some((step_i, e, phase, current)) = stalled.as_ref() {
        issues.push(format!(
            "stalled at step {step_i} current={current} phase={phase:?}: {e}"
        ));
    }
    if game.winner.is_none() && stalled.is_none() {
        issues.push(format!(
            "no winner after {MAX_STEPS} steps ({} events)",
            game.event_log.len()
        ));
    }

    assert!(issues.is_empty(), "audit issues:\n{}", issues.join("\n"));
}

#[test]
fn four_bots_additional_seeds_finish_or_replay() {
    let mut summaries = Vec::new();
    for seed in [1u64, 7, 42, 99, 12345] {
        let mut game = Game::new_with_seed(bots(4), seed).unwrap();
        let err = play_until_done(&mut game).err();
        let replay_ok = Game::replay(&game.log()).is_ok_and(|g| g == game);
        summaries.push(format!(
            "seed={seed} events={} winner={:?} replay_ok={replay_ok} err={err:?}",
            game.event_log.len(),
            game.winner,
        ));
        assert!(replay_ok, "replay failed for seed {seed}");
        assert!(err.is_none(), "stall for seed {seed}: {err:?}");
        assert!(
            game.winner.is_some()
                || game
                    .event_log
                    .iter()
                    .filter(|e| matches!(e.action, Action::EndTurn))
                    .count()
                    > 20,
            "seed {seed} made too little progress"
        );
    }
    for line in summaries {
        println!("{line}");
    }
}
