"use client";

import { useEffect, useState } from "react";
import styles from "./page.module.css";
import { health } from "../lib/api";
import type { HealthResp } from "../lib/types";
import VsComputer from "./VsComputer";
import OnlinePvp from "./OnlinePvp";

type Opponent = "menu" | "computer" | "online";

export default function Page() {
  const [opponent, setOpponent] = useState<Opponent>("menu");
  const [agent, setAgent] = useState<HealthResp | null>(null);

  useEffect(() => {
    let cancelled = false;
    health()
      .then((h) => !cancelled && setAgent(h))
      .catch(() => !cancelled && setAgent(null));
    return () => {
      cancelled = true;
    };
  }, [opponent]);

  if (opponent === "computer") {
    return <VsComputer onExit={() => setOpponent("menu")} />;
  }
  if (opponent === "online") {
    return <OnlinePvp onExit={() => setOpponent("menu")} />;
  }

  return (
    <main className={styles.page}>
      <h1 className={styles.title}>ZK Battleship</h1>
      <p className={styles.subtitle}>
        Battleship where cheating is cryptographically impossible. Every hit, miss, sunk ship — and
        the final defeat itself — is backed by a real zero-knowledge proof. No trusted server, no
        blockchain, no honor system.
      </p>

      <section className={styles.menuSection}>
        <h2 className={styles.menuHeading}>Choose an opponent</h2>
        <div className={styles.cards}>
          <button className={styles.card} onClick={() => setOpponent("computer")}>
            <span className={styles.cardTitle}>Vs Computer</span>
            <span className={styles.cardBody}>
              Single-player. The computer proves every answer; your local agent verifies each proof
              before you see it. The easiest way to watch the proofs in action.
            </span>
          </button>
          <button className={styles.card} onClick={() => setOpponent("online")}>
            <span className={styles.cardTitle}>Online (2 players)</span>
            <span className={styles.cardBody}>
              Play a friend over a shared room code. Each side proves on their own machine; an
              untrusted relay only forwards public messages. Fully trustless — even “you win” is a
              proof.
            </span>
          </button>
        </div>
      </section>

      <section className={styles.menuSection}>
        <h2 className={styles.menuHeading}>Your prover agent</h2>
        {agent ? (
          <p className={styles.legend}>
            connected · real STARK proofs only · guest image IDs{" "}
            <code>{agent.commit_image_id.slice(0, 12)}…</code> (commit),{" "}
            <code>{agent.shot_image_id.slice(0, 12)}…</code> (shot) — compare these with your
            opponent to confirm you are both playing the same audited rules.
          </p>
        ) : (
          <p className={`${styles.legend} ${styles.error}`}>
            No prover agent found. Start it with{" "}
            <code>cargo run --release -p host --bin zk-battleship-agent</code> and refresh.
          </p>
        )}
      </section>
    </main>
  );
}
