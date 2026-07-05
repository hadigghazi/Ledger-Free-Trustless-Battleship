//! ZK Battleship relay — a deliberately *dumb* WebSocket message router.
//!
//! Two players each open a WebSocket and `join` the same room code. After that,
//! every message one player sends is forwarded verbatim to the other. The relay
//! never parses, proves, or verifies game payloads — it only understands the
//! `join` control message and otherwise shovels bytes between the two seats.
//!
//! This is what keeps online play trustless: the only things that cross the
//! relay are the *public* messages the players already exchange (board
//! commitments, shot coordinates, and proof receipts). Secret boards and salts
//! live exclusively in each player's own local prover agent and never reach
//! here. A malicious or curious relay learns nothing it couldn't verify anyway,
//! and it cannot fabricate a result because it cannot forge a STARK.
//!
//! Protocol (all messages are JSON text frames):
//!   client -> relay:  {"type":"join","room":"CODE"}   (must be first)
//!                     {"type":"commit"|"fire"|"response"|"game-over"|"chat", ...}
//!   relay  -> client: {"type":"joined","seat":0|1,"room":"CODE","opponentPresent":bool}
//!                     {"type":"opponent-joined"} / {"type":"opponent-left"}
//!                     {"type":"error","error":"..."}
//!                     ...plus every game message forwarded from the opponent.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

type Tx = UnboundedSender<Message>;

#[derive(Default)]
struct Room {
    seats: [Option<Tx>; 2],
}

type Rooms = Arc<Mutex<HashMap<String, Room>>>;

#[derive(Deserialize)]
struct JoinMsg {
    #[serde(rename = "type")]
    kind: String,
    room: Option<String>,
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "9000".to_string());
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).await.expect("failed to bind relay address");
    println!("ZK Battleship relay listening on ws://{addr}  (forwards public messages only)");

    let rooms: Rooms = Arc::new(Mutex::new(HashMap::new()));
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                let rooms = rooms.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_conn(stream, rooms).await {
                        eprintln!("connection {peer} ended: {e}");
                    }
                });
            }
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}

/// Generous cap for a succinct/composite receipt serialized as JSON; anything
/// larger than this is not a legitimate game message.
const MAX_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_ROOM_CODE_LEN: usize = 64;

async fn handle_conn(
    stream: TcpStream,
    rooms: Rooms,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
        max_message_size: Some(MAX_MESSAGE_BYTES),
        max_frame_size: Some(MAX_MESSAGE_BYTES),
        ..Default::default()
    };
    let ws = tokio_tungstenite::accept_async_with_config(stream, Some(config)).await?;
    let (mut sink, mut source) = ws.split();

    // A writer task owns the sink; everything reaches a client through this tx.
    let (tx, mut rx) = unbounded_channel::<Message>();
    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sink.send(msg).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    // Handshake: the first usable message must be a `join`.
    let mut seated: Option<(String, usize)> = None;
    while let Some(Ok(msg)) = source.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<JoinMsg>(&text) {
                Ok(j) if j.kind == "join" => {
                    let code = j.room.unwrap_or_default().trim().to_string();
                    if code.is_empty() || code.len() > MAX_ROOM_CODE_LEN {
                        let _ = tx.send(error_msg("join requires a room code of 1..=64 characters"));
                        continue;
                    }
                    match seat_in_room(&rooms, &code, tx.clone()) {
                        Some((seat, opponent_present)) => {
                            let _ = tx.send(Message::Text(
                                json!({
                                    "type": "joined",
                                    "seat": seat,
                                    "room": code,
                                    "opponentPresent": opponent_present
                                })
                                .to_string(),
                            ));
                            seated = Some((code, seat));
                            break;
                        }
                        None => {
                            let _ = tx.send(error_msg("room is full"));
                            break;
                        }
                    }
                }
                _ => {
                    let _ = tx.send(error_msg(
                        "first message must be {\"type\":\"join\",\"room\":\"CODE\"}",
                    ));
                }
            }
        }
    }

    let (code, seat) = match seated {
        Some(v) => v,
        None => {
            // Never joined a room; let the writer flush any error, then end.
            drop(tx);
            let _ = writer.await;
            return Ok(());
        }
    };

    // Forward loop: route every game message to the opponent's seat.
    while let Some(Ok(msg)) = source.next().await {
        match msg {
            Message::Text(_) | Message::Binary(_) => forward(&rooms, &code, seat, msg),
            Message::Ping(p) => {
                let _ = tx.send(Message::Pong(p));
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    leave(&rooms, &code, seat);
    drop(tx);
    let _ = writer.await;
    Ok(())
}

/// Place a player into a room, returning their seat (0/1) and whether the other
/// seat is already occupied. `None` if the room already has two players.
fn seat_in_room(rooms: &Rooms, code: &str, tx: Tx) -> Option<(usize, bool)> {
    let mut map = rooms.lock().unwrap();
    let room = map.entry(code.to_string()).or_default();
    let seat = if room.seats[0].is_none() {
        0
    } else if room.seats[1].is_none() {
        1
    } else {
        return None;
    };
    let opponent_present = room.seats[1 - seat].is_some();
    room.seats[seat] = Some(tx);
    if let Some(opp) = &room.seats[1 - seat] {
        let _ = opp.send(control_msg("opponent-joined"));
    }
    Some((seat, opponent_present))
}

fn forward(rooms: &Rooms, code: &str, from_seat: usize, msg: Message) {
    let map = rooms.lock().unwrap();
    if let Some(room) = map.get(code) {
        if let Some(opp) = &room.seats[1 - from_seat] {
            let _ = opp.send(msg);
        }
    }
}

fn leave(rooms: &Rooms, code: &str, seat: usize) {
    let mut map = rooms.lock().unwrap();
    if let Some(room) = map.get_mut(code) {
        room.seats[seat] = None;
        if let Some(opp) = &room.seats[1 - seat] {
            let _ = opp.send(control_msg("opponent-left"));
        }
        if room.seats.iter().all(Option::is_none) {
            map.remove(code);
        }
    }
}

fn control_msg(kind: &str) -> Message {
    Message::Text(json!({ "type": kind }).to_string())
}

fn error_msg(msg: &str) -> Message {
    Message::Text(json!({ "type": "error", "error": msg }).to_string())
}
