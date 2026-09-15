use axum::{
    Router,
    extract::ws::{Message, WebSocket},
    extract::{State, WebSocketUpgrade},
    response::Response,
    routing::get,
};
use catan_core::{
    Action, CatanError, EventSubscriber, Game, GameEvent, GameLog, GameView, Player, PlayerColor,
    PlayerId, choose_action,
};
use futures::SinkExt;
use futures::stream::StreamExt;
use rand::Rng;
use rand::distributions::Alphanumeric;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::Duration;
use tower_http::services::ServeDir;

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<AppStateInner>>,
}

struct AppStateInner {
    rooms: HashMap<String, Arc<Mutex<GameRoom>>>,
}

struct GameRoom {
    code: String,
    players: Vec<Player>,
    connections: HashMap<PlayerId, mpsc::UnboundedSender<ServerMessage>>,
    host: PlayerId,
    started: bool,
    game: Option<Game>,
    bot_runner_active: bool,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    CreateGame { name: String },
    JoinGame { code: String, name: String },
    StartGame,
    AddBot,
    RemoveBot { id: PlayerId },
    Action { action: Action },
    RequestLog,
}

#[derive(Serialize, Clone)]
#[serde(tag = "type")]
enum ServerMessage {
    Error {
        message: String,
    },
    LobbyState {
        code: String,
        players: Vec<LobbyPlayer>,
        you: PlayerId,
        host: PlayerId,
    },
    GameState {
        state: Box<GameView>,
    },
    GameLog {
        log: GameLog,
    },
}

#[derive(Serialize, Clone)]
struct LobbyPlayer {
    id: PlayerId,
    name: String,
    color: PlayerColor,
    is_bot: bool,
}

/// Writes every received event as a JSON line to a log file.
///
/// The file is created on first use and appended to for the lifetime of the
/// subscriber. Each line is a single `GameEvent` so the resulting `.jsonl`
/// file can be replayed or processed line-by-line.
struct FileSubscriber {
    writer: Mutex<std::io::BufWriter<std::fs::File>>,
}

impl FileSubscriber {
    fn new(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            writer: Mutex::new(std::io::BufWriter::new(file)),
        })
    }
}

impl EventSubscriber for FileSubscriber {
    fn on_event(&self, event: &GameEvent) {
        let Ok(line) = serde_json::to_string(event) else {
            eprintln!("failed to serialize game event");
            return;
        };
        let mut writer = self.writer.lock().unwrap();
        if let Err(err) = writeln!(writer, "{}", line) {
            eprintln!("failed to write game event to log file: {}", err);
            return;
        }
        if let Err(err) = writer.flush() {
            eprintln!("failed to flush game event log file: {}", err);
        }
    }
}

#[tokio::main]
async fn main() {
    let state = AppState {
        inner: Arc::new(Mutex::new(AppStateInner {
            rooms: HashMap::new(),
        })),
    };

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .fallback_service(ServeDir::new("catan-web/dist"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut socket_tx, mut socket_rx) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerMessage>();

    let forwarder = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let text = serde_json::to_string(&msg).unwrap_or_default();
            if socket_tx.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
    });

    let mut room_code: Option<String> = None;
    let mut player_id: Option<PlayerId> = None;

    let run = async {
        while let Some(msg) = socket_rx.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                let client_msg = match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = tx.send(ServerMessage::Error {
                            message: format!("Invalid message: {}", e),
                        });
                        continue;
                    }
                };

                match client_msg {
                    ClientMessage::CreateGame { name } => {
                        if room_code.is_some() {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Already in a room".to_string(),
                            });
                            continue;
                        }
                        let (code, room_arc) = {
                            let mut app = state.inner.lock().unwrap();
                            let code = generate_code(&app.rooms);
                            let room = create_room(code.clone(), name, tx.clone());
                            app.rooms.insert(code.clone(), room.clone());
                            (code, room)
                        };
                        player_id = Some(0);
                        room_code = Some(code.clone());

                        broadcast_lobby(&room_arc).await;
                    }
                    ClientMessage::JoinGame { code, name } => {
                        if room_code.is_some() {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Already in a room".to_string(),
                            });
                            continue;
                        }

                        let room_arc = {
                            let app = state.inner.lock().unwrap();
                            app.rooms.get(&code).cloned()
                        };

                        let Some(room_arc) = room_arc else {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Room not found".to_string(),
                            });
                            continue;
                        };

                        let join_result = {
                            let mut room = room_arc.lock().unwrap();
                            if room.started || room.players.len() >= 4 {
                                None
                            } else {
                                // Humans always occupy the leading seats. Bots have no
                                // connection identity, so they can safely be shifted back.
                                let pid = room.players.iter().take_while(|p| !p.is_bot).count();
                                for (index, bot) in room.players.iter_mut().enumerate().skip(pid) {
                                    bot.id = index + 1;
                                    bot.color = PlayerColor::all()[index + 1];
                                }
                                let color = PlayerColor::all()[pid];
                                let player = Player::new(pid, name, color);
                                room.players.insert(pid, player);
                                room.connections.insert(pid, tx.clone());
                                Some(pid)
                            }
                        };

                        if let Some(pid) = join_result {
                            player_id = Some(pid);
                            room_code = Some(code.clone());
                            broadcast_lobby(&room_arc).await;
                        } else {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Cannot join room".to_string(),
                            });
                        }
                    }
                    ClientMessage::StartGame => {
                        let Some(pid) = player_id else {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Not in a room".to_string(),
                            });
                            continue;
                        };
                        let Some(ref code) = room_code else {
                            continue;
                        };

                        let room_arc = {
                            let app = state.inner.lock().unwrap();
                            app.rooms.get(code).cloned()
                        };

                        if let Some(room_arc) = room_arc {
                            let started = {
                                let mut room = room_arc.lock().unwrap();
                                if room.started || room.host != pid || room.players.len() < 2 {
                                    false
                                } else {
                                    match Game::new(room.players.clone()) {
                                        Ok(mut game) => {
                                            let log_dir = std::path::Path::new("logs");
                                            if std::fs::create_dir_all(log_dir).is_ok() {
                                                let log_path =
                                                    log_dir.join(format!("{}.jsonl", room.code));
                                                if let Ok(subscriber) =
                                                    FileSubscriber::new(log_path)
                                                {
                                                    game.subscribe(Arc::new(subscriber));
                                                }
                                            }
                                            room.players.clear();
                                            room.game = Some(game);
                                            room.started = true;
                                            true
                                        }
                                        Err(_) => false,
                                    }
                                }
                            };

                            if started {
                                broadcast_game_state(&room_arc).await;
                                start_bot_runner(room_arc);
                            } else {
                                let _ = tx.send(ServerMessage::Error {
                                    message: "Cannot start game".to_string(),
                                });
                            }
                        }
                    }
                    ClientMessage::AddBot => {
                        let (Some(pid), Some(code)) = (player_id, room_code.as_ref()) else {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Not in a room".to_string(),
                            });
                            continue;
                        };
                        let room_arc = {
                            let app = state.inner.lock().unwrap();
                            app.rooms.get(code).cloned()
                        };
                        let added = room_arc.as_ref().is_some_and(|room_arc| {
                            let mut room = room_arc.lock().unwrap();
                            add_bot_to_room(&mut room, pid)
                        });
                        if added {
                            if let Some(room_arc) = room_arc {
                                broadcast_lobby(&room_arc).await;
                            }
                        } else {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Cannot add bot".to_string(),
                            });
                        }
                    }
                    ClientMessage::RemoveBot { id } => {
                        let (Some(pid), Some(code)) = (player_id, room_code.as_ref()) else {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Not in a room".to_string(),
                            });
                            continue;
                        };
                        let room_arc = {
                            let app = state.inner.lock().unwrap();
                            app.rooms.get(code).cloned()
                        };
                        let removed = room_arc.as_ref().is_some_and(|room_arc| {
                            let mut room = room_arc.lock().unwrap();
                            remove_bot_from_room(&mut room, pid, id)
                        });
                        if removed {
                            if let Some(room_arc) = room_arc {
                                broadcast_lobby(&room_arc).await;
                            }
                        } else {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Cannot remove bot".to_string(),
                            });
                        }
                    }
                    ClientMessage::RequestLog => {
                        let Some(ref code) = room_code else {
                            continue;
                        };
                        let room_arc = {
                            let app = state.inner.lock().unwrap();
                            app.rooms.get(code).cloned()
                        };
                        if let Some(room_arc) = room_arc {
                            let log = {
                                let room = room_arc.lock().unwrap();
                                room.game.as_ref().map(|game| game.log())
                            };
                            if let Some(log) = log {
                                let _ = tx.send(ServerMessage::GameLog { log });
                            } else {
                                let _ = tx.send(ServerMessage::Error {
                                    message: "Game has not started".to_string(),
                                });
                            }
                        }
                    }
                    ClientMessage::Action { action } => {
                        let Some(pid) = player_id else {
                            let _ = tx.send(ServerMessage::Error {
                                message: "Not in a room".to_string(),
                            });
                            continue;
                        };
                        let Some(ref code) = room_code else {
                            continue;
                        };

                        let room_arc = {
                            let app = state.inner.lock().unwrap();
                            app.rooms.get(code).cloned()
                        };

                        if let Some(room_arc) = room_arc {
                            let (result, trade_id) = {
                                let mut room = room_arc.lock().unwrap();
                                if let Some(game) = room.game.as_mut() {
                                    let result = game.apply_action(pid, action);
                                    let trade_id = if result.is_ok() {
                                        game.pending_trade
                                            .as_ref()
                                            .filter(|t| t.from == pid)
                                            .map(|t| t.trade_id)
                                    } else {
                                        None
                                    };
                                    (Some(result), trade_id)
                                } else {
                                    (Some(Err(CatanError::InvalidPhase)), None)
                                }
                            };

                            if let Some(trade_id) = trade_id {
                                start_trade_timer(room_arc.clone(), pid, trade_id);
                            }

                            match result {
                                Some(Ok(_)) => {
                                    broadcast_game_state(&room_arc).await;
                                    start_bot_runner(room_arc.clone());
                                }
                                Some(Err(e)) => {
                                    let _ = tx.send(ServerMessage::Error {
                                        message: e.to_string(),
                                    });
                                }
                                None => {}
                            }
                        }
                    }
                }
            } else if msg == Message::Close(None) {
                break;
            }
        }
        Ok::<(), axum::Error>(())
    };

    let _ = run.await;

    // Disconnect: remove connection from room.
    if let (Some(code), Some(pid)) = (room_code, player_id) {
        let room_arc = {
            let app = state.inner.lock().unwrap();
            app.rooms.get(&code).cloned()
        };
        if let Some(room_arc) = room_arc {
            let should_remove = {
                let mut room = room_arc.lock().unwrap();
                room.connections.remove(&pid);
                if !room.started {
                    room.players.retain(|p| p.id != pid);
                    if room.players.is_empty() {
                        true
                    } else {
                        if room.host == pid {
                            room.host = room.players[0].id;
                        }
                        false
                    }
                } else {
                    false
                }
            };

            if should_remove {
                let mut app = state.inner.lock().unwrap();
                app.rooms.remove(&code);
            } else {
                broadcast_lobby(&room_arc).await;
            }
        }
    }

    forwarder.abort();
}

fn create_room(
    code: String,
    name: String,
    tx: mpsc::UnboundedSender<ServerMessage>,
) -> Arc<Mutex<GameRoom>> {
    let player = Player::new(0, name, PlayerColor::Red);
    let room = GameRoom {
        code: code.clone(),
        players: vec![player],
        connections: {
            let mut map = HashMap::new();
            map.insert(0, tx);
            map
        },
        host: 0,
        started: false,
        game: None,
        bot_runner_active: false,
    };
    Arc::new(Mutex::new(room))
}

fn add_bot_to_room(room: &mut GameRoom, requester: PlayerId) -> bool {
    if room.started || room.host != requester || room.players.len() >= 4 {
        return false;
    }
    let id = room.players.len();
    let bot_number = (1..=4)
        .find(|number| {
            let name = format!("Bot {}", number);
            room.players
                .iter()
                .all(|player| !player.is_bot || player.name != name)
        })
        .unwrap_or(id);
    room.players.push(Player::bot(
        id,
        format!("Bot {}", bot_number),
        PlayerColor::all()[id],
    ));
    true
}

fn remove_bot_from_room(room: &mut GameRoom, requester: PlayerId, bot_id: PlayerId) -> bool {
    if room.started || room.host != requester {
        return false;
    }
    let Some(index) = room
        .players
        .iter()
        .position(|player| player.id == bot_id && player.is_bot)
    else {
        return false;
    };
    room.players.remove(index);
    for (id, player) in room.players.iter_mut().enumerate().skip(index) {
        player.id = id;
        player.color = PlayerColor::all()[id];
    }
    true
}

fn next_bot_actor(game: &Game) -> Option<PlayerId> {
    if let Some(trade) = game.pending_trade.as_ref() {
        if let Some(player) = game.players.iter().find(|player| {
            player.is_bot && player.id != trade.from && !trade.rejected_by.contains(&player.id)
        }) {
            return Some(player.id);
        }
        let waiting_on_human = game.players.iter().any(|player| {
            !player.is_bot && player.id != trade.from && !trade.rejected_by.contains(&player.id)
        });
        if waiting_on_human {
            return None;
        }
    }

    if let Some(player) = game
        .players
        .iter()
        .find(|player| player.is_bot && game.pending_discards.contains_key(&player.id))
    {
        return Some(player.id);
    }

    let current = game.current_player();
    game.players
        .get(current)
        .filter(|player| player.is_bot)
        .map(|player| player.id)
}

fn start_bot_runner(room_arc: Arc<Mutex<GameRoom>>) {
    {
        let mut room = room_arc.lock().unwrap();
        if room.bot_runner_active || !room.started {
            return;
        }
        let has_work = room.game.as_ref().and_then(next_bot_actor).is_some();
        if !has_work {
            return;
        }
        room.bot_runner_active = true;
    }

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(450)).await;
            let applied = {
                let mut room = room_arc.lock().unwrap();
                let decision = room.game.as_ref().and_then(|game| {
                    next_bot_actor(game).and_then(|bot_id| {
                        choose_action(game, bot_id).map(|action| (bot_id, action))
                    })
                });
                let Some((bot_id, action)) = decision else {
                    room.bot_runner_active = false;
                    break;
                };
                let Some(game) = room.game.as_mut() else {
                    room.bot_runner_active = false;
                    break;
                };
                match game.apply_action(bot_id, action) {
                    Ok(()) => {
                        let trade_id = game
                            .pending_trade
                            .as_ref()
                            .filter(|trade| trade.from == bot_id)
                            .map(|trade| trade.trade_id);
                        Some((bot_id, trade_id))
                    }
                    Err(error) => {
                        eprintln!("bot {} chose an invalid action: {}", bot_id, error);
                        room.bot_runner_active = false;
                        None
                    }
                }
            };

            let Some((bot_id, trade_id)) = applied else {
                break;
            };
            broadcast_game_state(&room_arc).await;
            if let Some(trade_id) = trade_id {
                start_trade_timer(room_arc.clone(), bot_id, trade_id);
            }
        }
    });
}

fn start_trade_timer(room_arc: Arc<Mutex<GameRoom>>, player_id: PlayerId, trade_id: u64) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(15)).await;
        let cancelled = {
            let mut room = room_arc.lock().unwrap();
            if let Some(game) = room.game.as_mut() {
                game.apply_action(player_id, Action::CancelTrade { trade_id })
                    .is_ok()
            } else {
                false
            }
        };
        if cancelled {
            broadcast_game_state(&room_arc).await;
            start_bot_runner(room_arc);
        }
    });
}

fn generate_code(rooms: &HashMap<String, Arc<Mutex<GameRoom>>>) -> String {
    let mut rng = rand::thread_rng();
    loop {
        let code: String = (&mut rng)
            .sample_iter(&Alphanumeric)
            .filter(|c: &u8| c.is_ascii_uppercase() || c.is_ascii_digit())
            .take(4)
            .map(char::from)
            .collect();
        if !rooms.contains_key(&code) {
            return code;
        }
    }
}

async fn broadcast_lobby(room_arc: &Arc<Mutex<GameRoom>>) {
    let snapshot = {
        let room = room_arc.lock().unwrap();
        (
            room.code.clone(),
            room.players.clone(),
            room.connections.clone(),
            room.host,
        )
    };

    let (code, players, connections, host) = snapshot;

    let lobby_players: Vec<LobbyPlayer> = players
        .iter()
        .map(|p| LobbyPlayer {
            id: p.id,
            name: p.name.clone(),
            color: p.color,
            is_bot: p.is_bot,
        })
        .collect();

    for (&pid, tx) in &connections {
        let _ = tx.send(ServerMessage::LobbyState {
            code: code.clone(),
            players: lobby_players.clone(),
            you: pid,
            host,
        });
    }
}

async fn broadcast_game_state(room_arc: &Arc<Mutex<GameRoom>>) {
    let (connections, views, log) = {
        let room = room_arc.lock().unwrap();
        let Some(game) = room.game.as_ref() else {
            return;
        };
        let views: Vec<(PlayerId, GameView)> = room
            .connections
            .keys()
            .filter(|&&pid| pid < game.players.len())
            .map(|&pid| (pid, game.view_for(pid)))
            .collect();
        let log = game.log();
        (room.connections.clone(), views, log)
    };

    for (pid, view) in views {
        if let Some(tx) = connections.get(&pid) {
            let _ = tx.send(ServerMessage::GameState {
                state: Box::new(view),
            });
            let _ = tx.send(ServerMessage::GameLog { log: log.clone() });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lobby() -> GameRoom {
        let (tx, _rx) = mpsc::unbounded_channel();
        let room = create_room("TEST".to_string(), "Host".to_string(), tx);
        Arc::try_unwrap(room).ok().unwrap().into_inner().unwrap()
    }

    #[test]
    fn only_host_can_manage_bot_seats() {
        let mut room = lobby();
        assert!(!add_bot_to_room(&mut room, 1));
        assert!(add_bot_to_room(&mut room, 0));
        assert!(room.players[1].is_bot);
        assert!(!remove_bot_from_room(&mut room, 1, 1));
        assert!(remove_bot_from_room(&mut room, 0, 1));
    }

    #[test]
    fn bot_seats_respect_capacity_and_remain_contiguous() {
        let mut room = lobby();
        assert!(add_bot_to_room(&mut room, 0));
        assert!(add_bot_to_room(&mut room, 0));
        assert!(add_bot_to_room(&mut room, 0));
        assert!(!add_bot_to_room(&mut room, 0));
        assert!(remove_bot_from_room(&mut room, 0, 2));
        assert_eq!(
            room.players
                .iter()
                .map(|player| player.id)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(room.players[2].color, PlayerColor::White);
    }

    #[test]
    fn bot_actor_prioritizes_trade_responses_and_discards() {
        let mut game = Game::new_with_seed(
            vec![
                Player::new(0, "Human", PlayerColor::Red),
                Player::bot(1, "Bot", PlayerColor::Blue),
            ],
            7,
        )
        .unwrap();
        game.pending_discards.insert(1, 2);
        assert_eq!(next_bot_actor(&game), Some(1));

        let human_order_index = game
            .player_order
            .iter()
            .position(|&player| player == 0)
            .unwrap();
        game.current_player_idx = human_order_index;
        game.pending_discards.clear();
        game.pending_trade = Some(catan_core::TradeOffer {
            trade_id: 1,
            from: 0,
            give: catan_core::Resources {
                brick: 1,
                ..catan_core::Resources::zero()
            },
            receive: catan_core::Resources {
                lumber: 1,
                ..catan_core::Resources::zero()
            },
            rejected_by: Vec::new(),
        });
        assert_eq!(next_bot_actor(&game), Some(1));
        game.pending_trade.as_mut().unwrap().rejected_by.push(1);
        assert_eq!(next_bot_actor(&game), None);
    }

    #[test]
    fn bot_continues_after_all_counterparties_reject_its_trade() {
        let mut game = Game::new_with_seed(
            vec![
                Player::bot(0, "Bot 0", PlayerColor::Red),
                Player::bot(1, "Bot 1", PlayerColor::Blue),
                Player::bot(2, "Bot 2", PlayerColor::White),
                Player::bot(3, "Bot 3", PlayerColor::Orange),
            ],
            3,
        )
        .unwrap();
        let from = game.current_player();
        game.pending_trade = Some(catan_core::TradeOffer {
            trade_id: 1,
            from,
            give: catan_core::Resources {
                brick: 1,
                ..catan_core::Resources::zero()
            },
            receive: catan_core::Resources {
                lumber: 1,
                ..catan_core::Resources::zero()
            },
            rejected_by: game
                .players
                .iter()
                .map(|player| player.id)
                .filter(|id| *id != from)
                .collect(),
        });
        assert_eq!(next_bot_actor(&game), Some(from));
    }

    #[tokio::test]
    async fn bot_runner_is_single_flight() {
        let mut game = Game::new_with_seed(
            vec![
                Player::new(0, "Human", PlayerColor::Red),
                Player::bot(1, "Bot", PlayerColor::Blue),
            ],
            9,
        )
        .unwrap();
        let bot_order_index = game
            .player_order
            .iter()
            .position(|&player| player == 1)
            .unwrap();
        game.current_player_idx = bot_order_index;
        let room = Arc::new(Mutex::new(GameRoom {
            code: "TEST".to_string(),
            players: Vec::new(),
            connections: HashMap::new(),
            host: 0,
            started: true,
            game: Some(game),
            bot_runner_active: false,
        }));

        start_bot_runner(room.clone());
        start_bot_runner(room.clone());
        assert!(room.lock().unwrap().bot_runner_active);
    }
}
