use catan_core::{Game, GamePhase, Player, PlayerColor, choose_action};

pub const MAX_STEPS: usize = 8_000;

pub fn bots(count: usize) -> Vec<Player> {
    PlayerColor::all()
        .into_iter()
        .take(count)
        .enumerate()
        .map(|(id, color)| Player::bot(id, format!("Bot {id}"), color))
        .collect()
}

pub fn play_until_done(game: &mut Game) -> Result<(), String> {
    for _ in 0..MAX_STEPS {
        if game.winner.is_some() || matches!(game.phase, GamePhase::Finished) {
            return Ok(());
        }
        step(game)?;
    }
    Err(format!(
        "no winner after {MAX_STEPS} steps ({} events, phase={:?})",
        game.event_log.len(),
        game.phase
    ))
}

pub fn step(game: &mut Game) -> Result<(), String> {
    if !game.pending_discards.is_empty() {
        let players: Vec<_> = game.pending_discards.keys().copied().collect();
        for player in players {
            let action = choose_action(game, player).ok_or_else(|| {
                format!(
                    "bot {player} had no discard with pending {:?}",
                    game.pending_discards
                )
            })?;
            game.apply_action(player, action)
                .map_err(|e| format!("discard by {player} failed: {e} ({action:?})"))?;
        }
        return Ok(());
    }

    if let Some(trade) = game.pending_trade.clone() {
        let mut answered = false;
        for player in 0..game.players.len() {
            if player == trade.from || trade.rejected_by.contains(&player) {
                continue;
            }
            if let Some(action) = choose_action(game, player) {
                game.apply_action(player, action)
                    .map_err(|e| format!("trade response by {player} failed: {e} ({action:?})"))?;
                answered = true;
                if game.pending_trade.is_none() {
                    break;
                }
            }
        }
        if answered {
            return Ok(());
        }
    }

    let player = game.current_player();
    let action = choose_action(game, player)
        .ok_or_else(|| format!("bot {player} stalled in {:?}", game.phase))?;
    game.apply_action(player, action)
        .map_err(|e| format!("action by {player} failed: {e} ({action:?})"))
}

#[allow(dead_code)]
pub struct SimResult {
    pub finished: usize,
    pub unfinished: usize,
    pub winners: Vec<usize>,
    pub events_total: usize,
    pub failures: Vec<String>,
}

#[allow(dead_code)]
pub fn run_bot_games(bot_count: usize, games: u64) -> SimResult {
    let mut result = SimResult {
        finished: 0,
        unfinished: 0,
        winners: vec![0; bot_count],
        events_total: 0,
        failures: Vec::new(),
    };

    for seed in 1..=games {
        let mut game = match Game::new_with_seed(bots(bot_count), seed) {
            Ok(game) => game,
            Err(e) => {
                result
                    .failures
                    .push(format!("seed={seed} failed to start: {e}"));
                continue;
            }
        };
        if let Err(e) = play_until_done(&mut game) {
            result.failures.push(format!("seed={seed} {e}"));
            result.unfinished += 1;
            continue;
        }
        if !Game::replay(&game.log()).is_ok_and(|replayed| replayed == game) {
            result.failures.push(format!("seed={seed} replay mismatch"));
            continue;
        }
        let Some(winner) = game.winner else {
            result
                .failures
                .push(format!("seed={seed} finished without a winner"));
            result.unfinished += 1;
            continue;
        };
        if winner >= result.winners.len() {
            result
                .failures
                .push(format!("seed={seed} invalid winner {winner}"));
            continue;
        }
        result.finished += 1;
        result.winners[winner] += 1;
        result.events_total += game.event_log.len();
    }

    result
}
