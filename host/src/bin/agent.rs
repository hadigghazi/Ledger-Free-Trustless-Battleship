//! Local prover-agent HTTP server for ZK Battleship.
//!
//! Runs on the player's own machine; the browser UI talks to it over
//! localhost. The player's board and salt never leave this process. Every
//! receipt it produces is a genuine STARK — there is no development or
//! "fast" mode, and `RISC0_DEV_MODE` is scrubbed from the environment at
//! startup so the underlying prover cannot be silently downgraded.
//!
//! Two game modes are served:
//!
//!   * **Vs Computer** (`/new-game`, `/fire`): single-player. The AI proves
//!     every answer to your shots and the agent verifies each proof (receipt,
//!     commitment, shot, history) before reporting it. Demonstrates the full
//!     pipeline in one process.
//!
//!   * **Online PvP** (`/pvp/*`): two humans, each running their own agent.
//!     The agent holds *your* secret board, proves *your* answers, and
//!     verifies the *opponent's* proofs — including that each proof was
//!     computed against the exact sequence of shots you actually fired (the
//!     history binding) and whether the opponent is defeated (the proven win
//!     condition). A separate cloud relay only forwards the public messages.
//!
//! Endpoints are POST/JSON; default bind 127.0.0.1:8787. CORS is restricted
//! to the local web UI origin unless AGENT_ALLOWED_ORIGINS says otherwise.

use std::collections::HashMap;
use std::sync::Mutex;

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tiny_http::{Header, Method, Response, Server};

use zk_battleship_core::{Fleet, Orientation, Placement, ShipKind, Shot, BOARD_SIZE, NUM_SHIPS};

use host::board_gen::{random_fleet, random_salt};
use host::game::{hex, Gunner, Player};
use host::transport::{CommitMsg, ResponseMsg};
use host::verifier::{verify_commit, verify_shot};

// ===========================================================================
// Vs Computer (single-player) state
// ===========================================================================

struct Game {
    ai: Player,
    human_fleet: Fleet,
    ai_gun: Gunner,
    /// Shots the human has fired at the AI, in order (= the history the AI's
    /// proofs must be computed against).
    human_fired: Vec<Shot>,
    /// Shots the AI has fired at the human, in order.
    ai_fired: Vec<Shot>,
    over: bool,
}

// ===========================================================================
// Online PvP state: one secret board (you) + the opponent's commitment and
// the authoritative record of the shots you fired at them.
// ===========================================================================

struct PvpSession {
    me: Player,
    opponent_commitment: Option<[u8; 32]>,
    /// Shots I have fired at the opponent, in order. This is the expected
    /// history for every response proof the opponent sends back.
    fired: Vec<Shot>,
}

type Games = Mutex<HashMap<String, Game>>;
type PvpSessions = Mutex<HashMap<String, PvpSession>>;

struct State {
    games: Games,
    pvp: PvpSessions,
}

// ===========================================================================
// Shared DTOs
// ===========================================================================

#[derive(Serialize, Deserialize, Clone)]
struct PlacementDto {
    ship: String,
    row: u8,
    col: u8,
    orientation: String, // "horizontal" | "vertical"
}

#[derive(Serialize)]
struct ShipDto {
    ship: String,
    cells: Vec<[u8; 2]>,
}

#[derive(Serialize)]
struct ShotResult {
    row: u8,
    col: u8,
    hit: bool,
    sunk: Option<String>,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

#[derive(Serialize)]
struct RandomBoardResp {
    placements: Vec<PlacementDto>,
}

// ---- Vs Computer DTOs ----

#[derive(Deserialize)]
struct NewGameReq {
    #[serde(default)]
    placements: Option<Vec<PlacementDto>>,
}

#[derive(Serialize)]
struct NewGameResp {
    game_id: String,
    your_commitment: String,
    ai_commitment: String,
    your_ships: Vec<ShipDto>,
    board_size: u8,
}

#[derive(Deserialize)]
struct FireReq {
    game_id: String,
    row: u8,
    col: u8,
}

#[derive(Serialize)]
struct FireResp {
    your_shot: ShotResult,
    ai_shot: Option<ShotResult>,
    you_won: bool,
    ai_won: bool,
}

// ---- PvP DTOs ----

#[derive(Deserialize)]
struct PvpSetupReq {
    session_id: String,
    #[serde(default)]
    placements: Option<Vec<PlacementDto>>,
}

#[derive(Serialize)]
struct PvpSetupResp {
    your_ships: Vec<ShipDto>,
    your_commitment: String,
    /// Public commitment proof to relay to the opponent (opaque to the relay).
    commit_msg: CommitMsg,
    board_size: u8,
}

#[derive(Deserialize)]
struct PvpVerifyCommitReq {
    session_id: String,
    commit_msg: CommitMsg,
}

#[derive(Serialize)]
struct PvpVerifyCommitResp {
    opponent_commitment: String,
}

#[derive(Deserialize)]
struct PvpRespondReq {
    session_id: String,
    row: u8,
    col: u8,
}

#[derive(Serialize)]
struct PvpRespondResp {
    /// Proof-backed answer to relay back to the shooter (opaque to the relay).
    response_msg: ResponseMsg,
    /// For your own UI only — the shooter independently re-derives these by
    /// verifying `response_msg`, so they are never taken on trust.
    hit: bool,
    sunk: Option<String>,
    /// True once this answer sinks your last ship. This is the PROVEN defeat
    /// flag from the journal: the same statement your opponent verifies.
    you_lost: bool,
}

#[derive(Deserialize)]
struct PvpVerifyShotReq {
    session_id: String,
    response_msg: ResponseMsg,
    row: u8,
    col: u8,
}

#[derive(Serialize)]
struct PvpVerifyShotResp {
    hit: bool,
    sunk: Option<String>,
    /// True iff the opponent's own proof states their fleet is now fully
    /// sunk — i.e. YOU WON, and can prove it to anyone.
    you_won: bool,
}

// ===========================================================================
// Helpers
// ===========================================================================

fn ship_name(k: ShipKind) -> String {
    format!("{:?}", k)
}

fn parse_ship(name: &str) -> Option<ShipKind> {
    match name.to_ascii_lowercase().as_str() {
        "carrier" => Some(ShipKind::Carrier),
        "battleship" => Some(ShipKind::Battleship),
        "cruiser" => Some(ShipKind::Cruiser),
        "submarine" => Some(ShipKind::Submarine),
        "destroyer" => Some(ShipKind::Destroyer),
        _ => None,
    }
}

fn ships_dto(fleet: &Fleet) -> Vec<ShipDto> {
    fleet
        .placements
        .iter()
        .map(|p| ShipDto {
            ship: ship_name(p.ship),
            cells: p.cells().into_iter().map(|(r, c)| [r, c]).collect(),
        })
        .collect()
}

fn placements_dto(fleet: &Fleet) -> Vec<PlacementDto> {
    fleet
        .placements
        .iter()
        .map(|p| PlacementDto {
            ship: ship_name(p.ship),
            row: p.row,
            col: p.col,
            orientation: match p.orientation {
                Orientation::Horizontal => "horizontal".to_string(),
                Orientation::Vertical => "vertical".to_string(),
            },
        })
        .collect()
}

/// Build a fleet from user-supplied placements (or a random one), rejecting
/// anything that is not a legal standard fleet. The commit proof would refuse
/// an illegal fleet anyway; failing early gives the UI a useful error.
fn resolve_fleet(placements: Option<Vec<PlacementDto>>) -> Result<Fleet, (u16, String)> {
    let Some(dtos) = placements else {
        return Ok(random_fleet(&mut OsRng));
    };
    if dtos.len() != NUM_SHIPS {
        return Err((400, format!("expected {} placements", NUM_SHIPS)));
    }
    let mut out: Vec<Placement> = Vec::with_capacity(NUM_SHIPS);
    for d in dtos {
        let ship = parse_ship(&d.ship).ok_or((400, format!("unknown ship `{}`", d.ship)))?;
        let orientation = match d.orientation.to_ascii_lowercase().as_str() {
            "horizontal" | "h" => Orientation::Horizontal,
            "vertical" | "v" => Orientation::Vertical,
            other => return Err((400, format!("unknown orientation `{other}`"))),
        };
        out.push(Placement { ship, row: d.row, col: d.col, orientation });
    }
    let fleet = Fleet { placements: out.try_into().map_err(|_| (400, "bad placements".to_string()))? };
    if !fleet.is_valid() {
        return Err((
            400,
            "illegal fleet: need one of each ship, fully on board, no overlaps".to_string(),
        ));
    }
    Ok(fleet)
}

fn parse_req<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, (u16, String)> {
    serde_json::from_str(body).map_err(|e| (400, format!("bad request: {e}")))
}

fn check_coords(row: u8, col: u8) -> Result<Shot, (u16, String)> {
    if row >= BOARD_SIZE || col >= BOARD_SIZE {
        return Err((400, "coordinates must be 0..9".to_string()));
    }
    Ok(Shot::new(row, col))
}

// ===========================================================================
// Vs Computer handlers
// ===========================================================================

fn handle_new_game(games: &Games, body: &str) -> Result<String, (u16, String)> {
    let req: NewGameReq = if body.trim().is_empty() {
        NewGameReq { placements: None }
    } else {
        parse_req(body)?
    };
    let human_fleet = resolve_fleet(req.placements)?;

    let ai = Player::new(random_fleet(&mut OsRng), random_salt(&mut OsRng));
    let ai_receipt = ai
        .prove_commit()
        .map_err(|e| (500, format!("AI commitment proof failed: {e}")))?;
    let ai_commitment =
        verify_commit(&ai_receipt).map_err(|e| (500, format!("AI commitment invalid: {e}")))?;

    let human = Player::new(human_fleet.clone(), random_salt(&mut OsRng));

    let id = format!("g{:016x}", rand::random::<u64>());
    let resp = NewGameResp {
        game_id: id.clone(),
        your_commitment: hex(&human.commitment),
        ai_commitment: hex(&ai_commitment),
        your_ships: ships_dto(&human_fleet),
        board_size: BOARD_SIZE,
    };

    let game = Game {
        ai,
        human_fleet,
        ai_gun: Gunner::new(),
        human_fired: Vec::new(),
        ai_fired: Vec::new(),
        over: false,
    };
    games.lock().unwrap().insert(id, game);

    serde_json::to_string(&resp).map_err(|e| (500, e.to_string()))
}

fn handle_fire(games: &Games, body: &str) -> Result<String, (u16, String)> {
    let req: FireReq = parse_req(body)?;
    let mut guard = games.lock().unwrap();
    let game = guard
        .get_mut(&req.game_id)
        .ok_or((404, "unknown game_id".to_string()))?;

    if game.over {
        return Err((409, "game is already over".to_string()));
    }
    let shot = check_coords(req.row, req.col)?;
    if game.human_fired.contains(&shot) {
        return Err((400, "you already fired at that cell".to_string()));
    }

    // The AI proves its answer to your shot; the agent verifies the proof
    // against the AI's setup commitment AND your true firing history before
    // trusting the result.
    let (receipt, _) = game
        .ai
        .respond(shot)
        .map_err(|e| (500, format!("AI failed to prove its answer: {e}")))?;
    let j = verify_shot(&receipt, game.ai.commitment, shot, &game.human_fired)
        .map_err(|e| (500, format!("AI's proof did not verify: {e}")))?;
    game.human_fired.push(shot);
    let your_shot = ShotResult {
        row: shot.row,
        col: shot.col,
        hit: j.hit,
        sunk: j.sunk.map(ship_name),
    };

    if j.defeated {
        game.over = true;
        let resp = FireResp {
            your_shot,
            ai_shot: None,
            you_won: true,
            ai_won: false,
        };
        return serde_json::to_string(&resp).map_err(|e| (500, e.to_string()));
    }

    // The AI fires back. Your own machine answers honestly about your own
    // board; no proof is needed in single-player (there is no remote party to
    // convince — the trust story lives in the PvP mode).
    let ai_shot = game.ai_gun.next(&mut OsRng);
    let hit = game.human_fleet.occupies(ai_shot);
    let sunk = game.human_fleet.ship_sunk(ai_shot, &game.ai_fired);
    game.ai_gun.record(ai_shot, hit);
    game.ai_fired.push(ai_shot);
    let ai_shot_res = ShotResult {
        row: ai_shot.row,
        col: ai_shot.col,
        hit,
        sunk: sunk.map(ship_name),
    };
    let ai_won = game.human_fleet.all_sunk(&game.ai_fired);
    if ai_won {
        game.over = true;
    }

    let resp = FireResp {
        your_shot,
        ai_shot: Some(ai_shot_res),
        you_won: false,
        ai_won,
    };
    serde_json::to_string(&resp).map_err(|e| (500, e.to_string()))
}

// ===========================================================================
// PvP handlers
// ===========================================================================

fn handle_random_board() -> Result<String, (u16, String)> {
    let fleet = random_fleet(&mut OsRng);
    let resp = RandomBoardResp { placements: placements_dto(&fleet) };
    serde_json::to_string(&resp).map_err(|e| (500, e.to_string()))
}

fn handle_pvp_setup(pvp: &PvpSessions, body: &str) -> Result<String, (u16, String)> {
    let req: PvpSetupReq = parse_req(body)?;
    let fleet = resolve_fleet(req.placements)?;
    let me = Player::new(fleet, random_salt(&mut OsRng));

    let receipt = me
        .prove_commit()
        .map_err(|e| (500, format!("commitment proof failed: {e}")))?;
    let commitment =
        verify_commit(&receipt).map_err(|e| (500, format!("own commitment invalid: {e}")))?;

    let resp = PvpSetupResp {
        your_ships: ships_dto(me.fleet()),
        your_commitment: hex(&commitment),
        commit_msg: CommitMsg { commitment, receipt },
        board_size: BOARD_SIZE,
    };

    pvp.lock().unwrap().insert(
        req.session_id,
        PvpSession {
            me,
            opponent_commitment: None,
            fired: Vec::new(),
        },
    );

    serde_json::to_string(&resp).map_err(|e| (500, e.to_string()))
}

fn handle_pvp_verify_commit(pvp: &PvpSessions, body: &str) -> Result<String, (u16, String)> {
    let req: PvpVerifyCommitReq = parse_req(body)?;
    let mut guard = pvp.lock().unwrap();
    let session = guard
        .get_mut(&req.session_id)
        .ok_or((404, "unknown session_id (call /pvp/setup first)".to_string()))?;

    let c = verify_commit(&req.commit_msg.receipt)
        .map_err(|e| (400, format!("opponent commitment did not verify: {e}")))?;
    if c != req.commit_msg.commitment {
        return Err((400, "commitment does not match its proof".to_string()));
    }
    // Reject a mirrored commitment: an opponent replaying OUR commitment can
    // never answer a shot (they lack the witness), so games against it would
    // only ever stall — fail fast instead.
    if c == session.me.commitment {
        return Err((400, "opponent echoed your own commitment".to_string()));
    }
    session.opponent_commitment = Some(c);

    let resp = PvpVerifyCommitResp {
        opponent_commitment: hex(&c),
    };
    serde_json::to_string(&resp).map_err(|e| (500, e.to_string()))
}

fn handle_pvp_respond(pvp: &PvpSessions, body: &str) -> Result<String, (u16, String)> {
    let req: PvpRespondReq = parse_req(body)?;
    let shot = check_coords(req.row, req.col)?;
    let mut guard = pvp.lock().unwrap();
    let session = guard
        .get_mut(&req.session_id)
        .ok_or((404, "unknown session_id (call /pvp/setup first)".to_string()))?;

    // Protocol rule: each cell may be targeted at most once per board. This
    // keeps both sides' history sequences identical, so history binding in
    // the proofs stays in lockstep.
    if session.me.already_answered(shot) {
        return Err((409, "that cell was already fired at".to_string()));
    }

    let (receipt, journal) = session
        .me
        .respond(shot)
        .map_err(|e| (500, format!("failed to prove your answer: {e}")))?;

    let resp = PvpRespondResp {
        response_msg: ResponseMsg { receipt },
        hit: journal.hit,
        sunk: journal.sunk.map(ship_name),
        you_lost: journal.defeated,
    };
    serde_json::to_string(&resp).map_err(|e| (500, e.to_string()))
}

fn handle_pvp_verify_shot(pvp: &PvpSessions, body: &str) -> Result<String, (u16, String)> {
    let req: PvpVerifyShotReq = parse_req(body)?;
    let shot = check_coords(req.row, req.col)?;
    let mut guard = pvp.lock().unwrap();
    let session = guard
        .get_mut(&req.session_id)
        .ok_or((404, "unknown session_id (call /pvp/setup first)".to_string()))?;

    let commitment = session.opponent_commitment.ok_or((
        409,
        "verify the opponent's commitment before verifying their shots".to_string(),
    ))?;

    let j = verify_shot(&req.response_msg.receipt, commitment, shot, &session.fired)
        .map_err(|e| (400, format!("opponent's answer did not verify: {e}")))?;

    // Only now does the shot become part of the authoritative fired history.
    session.fired.push(shot);

    let resp = PvpVerifyShotResp {
        hit: j.hit,
        sunk: j.sunk.map(ship_name),
        you_won: j.defeated,
    };
    serde_json::to_string(&resp).map_err(|e| (500, e.to_string()))
}

// ===========================================================================
// HTTP plumbing
// ===========================================================================

/// Origins allowed to call this agent. Defaults to the local web UI; extend
/// with AGENT_ALLOWED_ORIGINS (comma-separated) e.g. for a hosted UI.
fn allowed_origins() -> Vec<String> {
    let mut origins = vec![
        "http://localhost:3000".to_string(),
        "http://127.0.0.1:3000".to_string(),
    ];
    if let Ok(extra) = std::env::var("AGENT_ALLOWED_ORIGINS") {
        for o in extra.split(',') {
            let o = o.trim().trim_end_matches('/');
            if !o.is_empty() {
                origins.push(o.to_string());
            }
        }
    }
    origins
}

fn cors_headers(origin: Option<&str>, allowed: &[String]) -> Vec<Header> {
    let mut headers = vec![
        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
        Header::from_bytes(&b"Vary"[..], &b"Origin"[..]).unwrap(),
    ];
    if let Some(o) = origin {
        if allowed.iter().any(|a| a == o) {
            headers.push(Header::from_bytes(&b"Access-Control-Allow-Origin"[..], o.as_bytes()).unwrap());
            headers.push(
                Header::from_bytes(&b"Access-Control-Allow-Methods"[..], &b"POST, GET, OPTIONS"[..])
                    .unwrap(),
            );
            headers.push(
                Header::from_bytes(&b"Access-Control-Allow-Headers"[..], &b"Content-Type"[..]).unwrap(),
            );
            // Chrome Private Network Access: allow a hosted (https) UI to
            // reach this local agent after its CORS preflight.
            headers.push(
                Header::from_bytes(&b"Access-Control-Allow-Private-Network"[..], &b"true"[..]).unwrap(),
            );
        }
    }
    headers
}

fn send(request: tiny_http::Request, status: u16, body: String, headers: Vec<Header>) {
    let mut response = Response::from_string(body).with_status_code(status);
    for h in headers {
        response.add_header(h);
    }
    let _ = request.respond(response);
}

fn error_json(msg: &str) -> String {
    serde_json::to_string(&ApiError {
        error: msg.to_string(),
    })
    .unwrap_or_else(|_| "{\"error\":\"internal\"}".to_string())
}

fn image_id_hex(id: [u32; 8]) -> String {
    let mut bytes = Vec::with_capacity(32);
    for word in id {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    hex(&bytes)
}

fn main() {
    // Hard guarantee: this process can only ever produce/accept real STARKs.
    // (The build also enables risc0's `disable-dev-mode`, which makes any
    // attempt to re-enable mock proving a hard error.)
    std::env::remove_var("RISC0_DEV_MODE");

    let port = std::env::var("PORT").unwrap_or_else(|_| "8787".to_string());
    let addr = format!("127.0.0.1:{port}");
    let server = Server::http(&addr).expect("failed to bind address");
    let allowed = allowed_origins();

    let state = State {
        games: Mutex::new(HashMap::new()),
        pvp: Mutex::new(HashMap::new()),
    };

    let health = format!(
        "{{\"ok\":true,\"proving\":\"stark\",\"commit_image_id\":\"{}\",\"shot_image_id\":\"{}\"}}",
        image_id_hex(methods::COMMIT_BOARD_ID),
        image_id_hex(methods::PROVE_SHOT_ID),
    );

    println!("ZK Battleship prover agent listening on http://{addr}");
    println!("proving: real STARK receipts only (no dev mode exists in this build)");
    println!("guest image IDs:");
    println!("  commit_board {}", image_id_hex(methods::COMMIT_BOARD_ID));
    println!("  prove_shot   {}", image_id_hex(methods::PROVE_SHOT_ID));

    for mut request in server.incoming_requests() {
        let method = request.method().clone();
        let url = request.url().to_string();
        let origin = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Origin"))
            .map(|h| h.value.as_str().trim_end_matches('/').to_string());
        let headers = cors_headers(origin.as_deref(), &allowed);

        if method == Method::Options {
            send(request, 204, String::new(), headers);
            continue;
        }
        if method == Method::Get && url == "/health" {
            send(request, 200, health.clone(), headers);
            continue;
        }

        let mut body = String::new();
        if request.as_reader().read_to_string(&mut body).is_err() {
            send(request, 400, error_json("could not read request body"), headers);
            continue;
        }

        let result = match (&method, url.as_str()) {
            (Method::Post, "/random-board") => handle_random_board(),
            (Method::Post, "/new-game") => handle_new_game(&state.games, &body),
            (Method::Post, "/fire") => handle_fire(&state.games, &body),
            (Method::Post, "/pvp/setup") => handle_pvp_setup(&state.pvp, &body),
            (Method::Post, "/pvp/verify-commit") => handle_pvp_verify_commit(&state.pvp, &body),
            (Method::Post, "/pvp/respond") => handle_pvp_respond(&state.pvp, &body),
            (Method::Post, "/pvp/verify-shot") => handle_pvp_verify_shot(&state.pvp, &body),
            _ => Err((404, "not found".to_string())),
        };

        match result {
            Ok(json) => send(request, 200, json, headers),
            Err((code, msg)) => send(request, code, error_json(&msg), headers),
        }
    }
}
