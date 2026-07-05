"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import styles from "./page.module.css";
import { proverUrl, pvpRespond, pvpSetup, pvpVerifyCommit, pvpVerifyShot } from "../lib/api";
import type { PlacementDto, PvpSetupResp } from "../lib/types";
import { Relay, RelayEvent, RELAY_URL } from "../lib/relay";
import { cellKey, EnemyBoard, EnemyCell, OwnBoard, range } from "./board";
import { Coin, firstSeat, makeCoin, verifyReveal } from "../lib/coin";
import Setup from "./Setup";

type Phase =
  | "lobby"
  | "connecting"
  | "waiting"
  | "committing"
  | "playing"
  | "won"
  | "lost"
  | "ended";

export default function OnlinePvp({ onExit }: { onExit: () => void }) {
  const [room, setRoom] = useState("");
  const [phase, setPhase] = useState<Phase>("lobby");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [isError, setIsError] = useState(false);

  const [setup, setSetup] = useState<PvpSetupResp | null>(null);
  const [opponentCommitment, setOpponentCommitment] = useState<string | null>(null);
  const [myTurn, setMyTurn] = useState(false);
  const [enemy, setEnemy] = useState<EnemyCell[][]>([]);
  const [ownShots, setOwnShots] = useState<Map<string, boolean>>(new Map());
  const [sunkShips, setSunkShips] = useState<string[]>([]);
  // Resolved client-side (it can be overridden per-tab with ?agent=…), so it
  // is rendered only after mount to keep server and client HTML identical.
  const [agentUrl, setAgentUrl] = useState("");

  // Mutable game state read inside the (stable) relay callback — kept in refs
  // to avoid stale closures.
  const relayRef = useRef<Relay | null>(null);
  const sessionRef = useRef<string>("");
  const seatRef = useRef<number>(0);
  const setupRef = useRef<PvpSetupResp | null>(null);
  const coinRef = useRef<Coin | null>(null);
  const oppCoinCommitRef = useRef<string | null>(null);
  const oppNonceRef = useRef<string | null>(null);
  const opponentCommittedRef = useRef(false);
  const startedRef = useRef(false);

  const size = setup?.board_size ?? 10;

  const shipCells = useMemo(() => {
    const s = new Set<string>();
    setup?.your_ships.forEach((ship) => ship.cells.forEach(([r, c]) => s.add(cellKey(r, c))));
    return s;
  }, [setup]);

  useEffect(() => {
    setAgentUrl(proverUrl());
    return () => relayRef.current?.close();
  }, []);

  function sendCommit() {
    const commit = setupRef.current?.commit_msg;
    const coin = coinRef.current;
    if (commit !== undefined && coin) {
      relayRef.current?.send({ type: "commit", commit_msg: commit, coin_commit: coin.commitHex });
    }
  }

  function maybeStart() {
    if (startedRef.current) return;
    if (!setupRef.current || !opponentCommittedRef.current || oppNonceRef.current === null) return;
    const coin = coinRef.current;
    if (!coin) return;
    startedRef.current = true;
    const s = setupRef.current.board_size;
    setEnemy(range(s).map(() => range(s).map((): EnemyCell => "unknown")));
    const first = firstSeat(coin.nonceHex, oppNonceRef.current);
    const goesFirst = seatRef.current === first;
    setMyTurn(goesFirst);
    setPhase("playing");
    setMessage(
      goesFirst
        ? "Boards committed and coin flipped: you fire first — pick a cell."
        : "Boards committed and coin flipped: opponent fires first…",
    );
  }

  async function handleIncomingFire(r: number, c: number) {
    setBusy(true);
    setIsError(false);
    setMessage(`Opponent fired at (${r},${c}) — your agent is generating the STARK proof of the answer…`);
    try {
      const res = await pvpRespond(sessionRef.current, r, c);
      setOwnShots((prev) => new Map(prev).set(cellKey(r, c), res.hit));
      relayRef.current?.send({ type: "response", response_msg: res.response_msg, row: r, col: c });
      if (res.you_lost) {
        setPhase("lost");
        setMessage("Your fleet is sunk — you lose. Your own final proof concedes the defeat.");
      } else {
        setMyTurn(true);
        setMessage(`Opponent ${res.hit ? "hit" : "missed"} at (${r},${c}). Your move.`);
      }
    } catch (e) {
      setIsError(true);
      setMessage((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function handleIncomingResponse(r: number, c: number, responseMsg: unknown) {
    setBusy(true);
    setIsError(false);
    setMessage("Verifying the opponent's proof (receipt, commitment, shot, history)…");
    try {
      const res = await pvpVerifyShot(sessionRef.current, responseMsg, r, c);
      setEnemy((prev) => {
        const next = prev.map((row) => row.slice());
        next[r][c] = res.hit ? "hit" : "miss";
        return next;
      });
      if (res.sunk) setSunkShips((prev) => (prev.includes(res.sunk!) ? prev : [...prev, res.sunk!]));
      if (res.you_won) {
        setPhase("won");
        setMessage("You sank the enemy fleet — you win, and their own proof says so! 🎉");
      } else {
        const sankNote = res.sunk ? ` — sank their ${res.sunk}!` : "";
        setMessage(`You ${res.hit ? "hit" : "missed"} at (${r},${c})${sankNote}. Opponent's move.`);
      }
    } catch (e) {
      setIsError(true);
      setMessage(`Could not verify the opponent's answer: ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  }

  const onEvent = useCallback((e: RelayEvent) => {
    switch (e.type) {
      case "joined": {
        seatRef.current = e.seat;
        sendCommit();
        if (e.opponentPresent) {
          setPhase("committing");
          setMessage("Opponent present — exchanging commitments…");
        } else {
          setPhase("waiting");
          setMessage(`Waiting for an opponent to join room “${sessionRef.current}”…`);
        }
        break;
      }
      case "opponent-joined": {
        // Re-send our commit so a later joiner receives it.
        sendCommit();
        setPhase("committing");
        setMessage("Opponent joined — exchanging commitments…");
        break;
      }
      case "opponent-left":
        setPhase((p) => (p === "won" || p === "lost" ? p : "ended"));
        setMessage((m) => (startedRef.current ? "Opponent left the game." : m));
        break;
      case "commit":
        oppCoinCommitRef.current = e.coin_commit;
        void handleOpponentCommit(e.commit_msg);
        break;
      case "coin-reveal":
        void handleCoinReveal(e.nonce);
        break;
      case "fire":
        void handleIncomingFire(e.row, e.col);
        break;
      case "response":
        void handleIncomingResponse(e.row, e.col, e.response_msg);
        break;
      case "error":
        setIsError(true);
        setMessage(e.error);
        break;
      case "closed":
        break;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function handleOpponentCommit(commitMsg: unknown) {
    // Verify the opponent's board proof once; on every (re)delivery of their
    // commit, (re)send our coin reveal — reveals only flow after both coin
    // commitments exist, and processing is idempotent on the other side.
    try {
      if (!opponentCommittedRef.current) {
        const r = await pvpVerifyCommit(sessionRef.current, commitMsg);
        setOpponentCommitment(r.opponent_commitment);
        opponentCommittedRef.current = true;
      }
      const coin = coinRef.current;
      if (coin) relayRef.current?.send({ type: "coin-reveal", nonce: coin.nonceHex });
      maybeStart();
    } catch (e) {
      setIsError(true);
      setMessage(`Opponent's commitment did not verify: ${(e as Error).message}`);
    }
  }

  async function handleCoinReveal(nonce: string) {
    if (oppNonceRef.current !== null) return; // already processed
    const theirCommit = oppCoinCommitRef.current;
    if (!theirCommit) return; // reveal before commit: ignore
    if (!(await verifyReveal(nonce, theirCommit))) {
      setIsError(true);
      setMessage("Opponent's coin-flip reveal does not match their commitment — aborting.");
      setPhase("ended");
      return;
    }
    oppNonceRef.current = nonce;
    maybeStart();
  }

  async function handleReady(placements: PlacementDto[]) {
    const code = room.trim();
    if (!code) {
      setIsError(true);
      setMessage("Enter a room code to share with your opponent.");
      return;
    }
    setBusy(true);
    setIsError(false);
    setPhase("connecting");
    setMessage("Committing to your board and proving it is legal (STARK proof — this can take a minute)…");
    sessionRef.current = code;
    startedRef.current = false;
    opponentCommittedRef.current = false;
    oppCoinCommitRef.current = null;
    oppNonceRef.current = null;
    setSunkShips([]);
    setOwnShots(new Map());
    try {
      coinRef.current = await makeCoin();
      const s = await pvpSetup(code, placements);
      setupRef.current = s;
      setSetup(s);
      relayRef.current?.close();
      relayRef.current = new Relay(code, onEvent);
      setMessage(`Connecting to the relay at ${RELAY_URL}…`);
    } catch (e) {
      setIsError(true);
      setPhase("lobby");
      setMessage(`Couldn't reach your prover agent at ${proverUrl()}. Is it running? (${(e as Error).message})`);
    } finally {
      setBusy(false);
    }
  }

  function onFire(r: number, c: number) {
    if (phase !== "playing" || !myTurn || busy) return;
    if (enemy[r]?.[c] !== "unknown") return;
    setMyTurn(false);
    setBusy(true);
    setIsError(false);
    setMessage(`Firing at (${r},${c}) — waiting for the opponent's proof…`);
    relayRef.current?.send({ type: "fire", row: r, col: c });
  }

  const inGame = setup !== null && phase !== "lobby";

  return (
    <main className={styles.page}>
      <button className={styles.linkBtn} onClick={onExit}>
        ← back to menu
      </button>
      <h1 className={styles.title}>ZK Battleship — Online</h1>
      <p className={styles.subtitle}>
        Two players, zero trust. Your board never leaves your machine; the relay only carries
        commitments, shots, and proofs — and even the final “you win” is a verified proof.
      </p>

      {phase === "lobby" && (
        <Setup title="Place your fleet" busy={busy} readyLabel="Commit board & connect" onReady={handleReady}>
          <input
            className={styles.input}
            placeholder="room code (e.g. otters-42)"
            value={room}
            onChange={(e) => setRoom(e.target.value)}
          />
        </Setup>
      )}

      {phase === "lobby" && (
        <p className={styles.legend}>
          Share the room code with your opponent — whoever joins the same code is paired. Needs your
          local prover agent on {agentUrl} and the relay on {RELAY_URL}.
        </p>
      )}

      {inGame && (
        <div className={styles.controls}>
          <span className={styles.badge}>
            room: <code>{sessionRef.current}</code>
          </span>
          {phase === "playing" && (
            <span className={styles.badge}>{myTurn ? "your turn" : "opponent's turn"}</span>
          )}
          {sunkShips.length > 0 && (
            <span className={styles.badge}>sunk: {sunkShips.join(", ")}</span>
          )}
        </div>
      )}

      <div className={`${styles.message} ${busy ? styles.proving : ""} ${isError ? styles.error : ""}`}>
        {message}
      </div>

      {inGame && setup && (
        <div className={styles.commits}>
          <span className={styles.commit}>
            your commitment: <code>{setup.your_commitment.slice(0, 16)}…</code>
          </span>
          {opponentCommitment && (
            <span className={styles.commit}>
              opponent commitment: <code>{opponentCommitment.slice(0, 16)}…</code>
            </span>
          )}
        </div>
      )}

      {phase === "won" && (
        <div className={`${styles.banner} ${styles.bannerWin}`}>You win — provably! 🎉</div>
      )}
      {phase === "lost" && (
        <div className={`${styles.banner} ${styles.bannerLose}`}>Your fleet was sunk — provably.</div>
      )}
      {phase === "ended" && <div className={`${styles.banner} ${styles.bannerLose}`}>Game ended.</div>}

      {inGame &&
        setup &&
        (phase === "playing" || phase === "won" || phase === "lost" || phase === "ended") && (
          <div className={styles.boards}>
            <div className={styles.boardWrap}>
              <h2>Enemy waters (aim here)</h2>
              <EnemyBoard
                size={size}
                cells={enemy}
                onFire={onFire}
                disabled={phase !== "playing" || !myTurn || busy}
              />
            </div>
            <div className={styles.boardWrap}>
              <h2>Your fleet</h2>
              <OwnBoard size={size} shipCells={shipCells} shots={ownShots} />
            </div>
          </div>
        )}

      {inGame && setup && (
        <p className={styles.legend}>
          ✕ hit · • miss · blue = your ship. Every result shown here passed STARK verification —
          neither side can lie about a shot, a sunk ship, or the end of the game.
        </p>
      )}
    </main>
  );
}
