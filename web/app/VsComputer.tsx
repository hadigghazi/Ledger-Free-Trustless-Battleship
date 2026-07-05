"use client";

import { useMemo, useState } from "react";
import styles from "./page.module.css";
import { fire, newGame, proverUrl } from "../lib/api";
import type { NewGameResp, PlacementDto, ShotResult } from "../lib/types";
import { cellKey, EnemyBoard, EnemyCell, OwnBoard, range } from "./board";
import Setup from "./Setup";

type AiShot = { row: number; col: number; hit: boolean };
type Phase = "setup" | "playing" | "won" | "lost";

function describe(who: string, s: ShotResult): string {
  if (s.sunk) return `${who} sank a ${s.sunk}!`;
  return s.hit ? `${who} hit at (${s.row},${s.col})` : `${who} missed at (${s.row},${s.col})`;
}

export default function VsComputer({ onExit }: { onExit: () => void }) {
  const [game, setGame] = useState<NewGameResp | null>(null);
  const [enemy, setEnemy] = useState<EnemyCell[][]>([]);
  const [aiShots, setAiShots] = useState<AiShot[]>([]);
  const [phase, setPhase] = useState<Phase>("setup");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string>("");
  const [isError, setIsError] = useState(false);

  const size = game?.board_size ?? 10;

  const shipCells = useMemo(() => {
    const set = new Set<string>();
    game?.your_ships.forEach((s) => s.cells.forEach(([r, c]) => set.add(cellKey(r, c))));
    return set;
  }, [game]);

  const aiShotMap = useMemo(() => {
    const map = new Map<string, boolean>();
    aiShots.forEach((s) => map.set(cellKey(s.row, s.col), s.hit));
    return map;
  }, [aiShots]);

  async function handleStart(placements: PlacementDto[]) {
    setBusy(true);
    setIsError(false);
    setMessage("Setting up: the computer is committing to its board and proving the fleet is legal (STARK proof — this can take a minute)…");
    try {
      const g = await newGame(placements);
      setGame(g);
      setEnemy(range(g.board_size).map(() => range(g.board_size).map((): EnemyCell => "unknown")));
      setAiShots([]);
      setPhase("playing");
      setMessage("Your move — click a cell in enemy waters to fire.");
    } catch (e) {
      setIsError(true);
      setMessage(`Couldn't start the game. Is your prover agent running on ${proverUrl()}? (${(e as Error).message})`);
    } finally {
      setBusy(false);
    }
  }

  async function handleFire(r: number, c: number) {
    if (!game || phase !== "playing" || busy) return;
    if (enemy[r][c] !== "unknown") return;

    setBusy(true);
    setIsError(false);
    setMessage("Firing… the computer is generating a proof of its answer, and your agent will verify it.");
    try {
      const res = await fire(game.game_id, r, c);

      setEnemy((prev) => {
        const next = prev.map((row) => row.slice());
        next[r][c] = res.your_shot.hit ? "hit" : "miss";
        return next;
      });

      if (res.you_won) {
        setPhase("won");
        setMessage(`${describe("You", res.your_shot)} — the computer's own proof concedes defeat.`);
        return;
      }

      let line = describe("You", res.your_shot);
      if (res.ai_shot) {
        const ai = res.ai_shot;
        setAiShots((prev) => [...prev, { row: ai.row, col: ai.col, hit: ai.hit }]);
        line += ` · ${describe("Computer", ai)}`;
        if (res.ai_won) setPhase("lost");
      }
      setMessage(line);
    } catch (e) {
      setIsError(true);
      setMessage((e as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className={styles.page}>
      <button className={styles.linkBtn} onClick={onExit}>
        ← back to menu
      </button>
      <h1 className={styles.title}>ZK Battleship — vs Computer</h1>
      <p className={styles.subtitle}>
        Every answer the computer gives is backed by a real zero-knowledge proof, verified before you
        see it — it cannot lie about a hit, a sunk ship, or losing.
      </p>

      {phase === "setup" && (
        <Setup title="Place your fleet" busy={busy} readyLabel="Start game" onReady={handleStart} />
      )}

      {phase !== "setup" && (
        <>
          <div className={`${styles.message} ${busy ? styles.proving : ""} ${isError ? styles.error : ""}`}>
            {message}
          </div>

          {game && (
            <div className={styles.commits}>
              <span className={styles.commit}>
                your commitment: <code>{game.your_commitment.slice(0, 16)}…</code>
              </span>
              <span className={styles.commit}>
                computer commitment: <code>{game.ai_commitment.slice(0, 16)}…</code>
              </span>
            </div>
          )}

          {phase === "won" && (
            <div className={`${styles.banner} ${styles.bannerWin}`}>You win — provably! 🎉</div>
          )}
          {phase === "lost" && (
            <div className={`${styles.banner} ${styles.bannerLose}`}>The computer sank your fleet.</div>
          )}

          {game && (
            <div className={styles.boards}>
              <div className={styles.boardWrap}>
                <h2>Enemy waters (aim here)</h2>
                <EnemyBoard
                  size={size}
                  cells={enemy}
                  onFire={handleFire}
                  disabled={busy || phase !== "playing"}
                />
              </div>
              <div className={styles.boardWrap}>
                <h2>Your fleet</h2>
                <OwnBoard size={size} shipCells={shipCells} shots={aiShotMap} />
              </div>
            </div>
          )}

          {game && (
            <p className={styles.legend}>
              ✕ hit · • miss · blue = your ship. The computer&apos;s board stays hidden; you only ever see
              proven hit/miss results.
            </p>
          )}
        </>
      )}
    </main>
  );
}
