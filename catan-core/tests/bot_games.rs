//! Configurable all-bot game simulations.
//!
//! This test uses a custom harness so flags are not swallowed by libtest:
//!
//! ```text
//! cargo test -p catan-core --test bot_games -- --bots 3 --games 100
//! cargo test -p catan-core --test bot_games -- 4 20
//! ```
//!
//! With no arguments it runs a 2-bot, 1-game smoke test so `cargo test` stays fast.

mod common;

use std::env;
use std::process::ExitCode;

fn print_usage() {
    eprintln!(
        "Usage: bot_games [--bots <2-4>] [--games <n>]
       bot_games [<bots>] [<games>]

Defaults: --bots 2 --games 1"
    );
}

fn parse_args() -> Result<(usize, u64), String> {
    let mut bots = 2usize;
    let mut games = 1u64;
    let mut positional: Vec<String> = Vec::new();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            "--bots" => {
                bots = parse_bots(&next_value(&mut args, "--bots")?)?;
            }
            "--games" => {
                games = parse_games(&next_value(&mut args, "--games")?)?;
            }
            flag if flag.starts_with("--bots=") => {
                bots = parse_bots(flag.trim_start_matches("--bots="))?;
            }
            flag if flag.starts_with("--games=") => {
                games = parse_games(flag.trim_start_matches("--games="))?;
            }
            other if other.starts_with('-') => {
                return Err(format!("unknown flag {other}"));
            }
            other => positional.push(other.to_string()),
        }
    }

    if positional.len() > 2 {
        return Err("too many positional arguments".into());
    }
    if let Some(value) = positional.first() {
        bots = parse_bots(value)?;
    }
    if let Some(value) = positional.get(1) {
        games = parse_games(value)?;
    }
    Ok((bots, games))
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_bots(value: &str) -> Result<usize, String> {
    let bots: usize = value
        .parse()
        .map_err(|_| format!("invalid bot count {value}"))?;
    if (2..=4).contains(&bots) {
        Ok(bots)
    } else {
        Err(format!("bot count must be 2, 3, or 4 (got {bots})"))
    }
}

fn parse_games(value: &str) -> Result<u64, String> {
    let games: u64 = value
        .parse()
        .map_err(|_| format!("invalid game count {value}"))?;
    if games == 0 {
        Err("game count must be at least 1".into())
    } else {
        Ok(games)
    }
}

fn main() -> ExitCode {
    let (bots, games) = match parse_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            print_usage();
            return ExitCode::from(2);
        }
    };

    let result = common::run_bot_games(bots, games);
    let avg_events = if result.finished == 0 {
        0
    } else {
        result.events_total / result.finished
    };
    println!(
        "{bots}-bot x{games}: finished={} unfinished={} avg_events={avg_events} wins={:?} failures={}",
        result.finished,
        result.unfinished,
        result.winners,
        result.failures.len()
    );
    for failure in &result.failures {
        println!("FAIL {failure}");
    }

    if result.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
