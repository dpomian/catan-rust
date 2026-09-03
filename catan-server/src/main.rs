use axum::{
    Router,
    extract::ws::{Message, WebSocket},
    extract::{State, WebSocketUpgrade},
    response::Response,
    routing::get,
};
use catan_core::{Action, CatanError, Game, GameView, Player, PlayerColor, PlayerId};
use futures::SinkExt;
use futures::stream::StreamExt;
use rand::Rng;
use rand::distributions::Alphanumeric;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    CreateGame { name: String },
    JoinGame { code: String, name: String },
    StartGame,
    Action { action: Action },
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
}

#[derive(Serialize, Clone)]
struct LobbyPlayer {
    id: PlayerId,
    name: String,
    color: PlayerColor,
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
                                let pid = room.players.len();
                                let color = PlayerColor::all()[pid];
                                let player = Player::new(pid, name, color);
                                room.players.push(player);
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
                                        Ok(game) => {
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
                            } else {
                                let _ = tx.send(ServerMessage::Error {
                                    message: "Cannot start game".to_string(),
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

                            let is_ok = matches!(result, Some(Ok(())));
                            if is_ok && trade_id.is_some() {
                                let trade_id = trade_id.unwrap();
                                let room_for_timer = room_arc.clone();
                                tokio::spawn(async move {
                                    tokio::time::sleep(Duration::from_secs(15)).await;
                                    let cancelled = {
                                        let mut room = room_for_timer.lock().unwrap();
                                        if let Some(game) = room.game.as_mut() {
                                            game.apply_action(pid, Action::CancelTrade { trade_id })
                                                .is_ok()
                                        } else {
                                            false
                                        }
                                    };
                                    if cancelled {
                                        broadcast_game_state(&room_for_timer).await;
                                    }
                                });
                            }

                            match result {
                                Some(Ok(_)) => broadcast_game_state(&room_arc).await,
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
    };
    Arc::new(Mutex::new(room))
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
    let (connections, views) = {
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
        (room.connections.clone(), views)
    };

    for (pid, view) in views {
        if let Some(tx) = connections.get(&pid) {
            let _ = tx.send(ServerMessage::GameState {
                state: Box::new(view),
            });
        }
    }
}
