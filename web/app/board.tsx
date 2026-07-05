// Shared board rendering for both Vs Computer and Online PvP screens.

import { Fragment } from "react";
import styles from "./page.module.css";

export type EnemyCell = "unknown" | "hit" | "miss";

export const range = (n: number): number[] => Array.from({ length: n }, (_, i) => i);
export const cellKey = (r: number, c: number): string => `${r},${c}`;

function ColumnHeaders({ size, prefix }: { size: number; prefix: string }) {
  return (
    <>
      <div className={styles.corner} />
      {range(size).map((c) => (
        <div key={`${prefix}h${c}`} className={styles.label}>
          {c}
        </div>
      ))}
    </>
  );
}

/** Enemy waters: a clickable attack grid showing your proven hits/misses. */
export function EnemyBoard({
  size,
  cells,
  onFire,
  disabled,
}: {
  size: number;
  cells: EnemyCell[][];
  onFire: (r: number, c: number) => void;
  disabled: boolean;
}) {
  const look = (r: number, c: number): { cls: string; ch: string } => {
    const state = cells[r]?.[c] ?? "unknown";
    if (state === "hit") return { cls: styles.hit, ch: "✕" };
    if (state === "miss") return { cls: styles.miss, ch: "•" };
    return { cls: styles.unknown, ch: "" };
  };

  return (
    <div className={styles.board}>
      <ColumnHeaders size={size} prefix="e" />
      {range(size).map((r) => (
        <Fragment key={`er${r}`}>
          <div className={styles.label}>{r}</div>
          {range(size).map((c) => {
            const { cls, ch } = look(r, c);
            const fired = (cells[r]?.[c] ?? "unknown") !== "unknown";
            return (
              <button
                key={`e${r}-${c}`}
                className={`${styles.cell} ${cls} ${styles.clickable}`}
                onClick={() => onFire(r, c)}
                disabled={disabled || fired}
                aria-label={`fire at row ${r} column ${c}`}
              >
                {ch}
              </button>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
}

/** Your fleet: your ships plus the (proven) shots the opponent has landed. */
export function OwnBoard({
  size,
  shipCells,
  shots,
}: {
  size: number;
  shipCells: Set<string>;
  shots: Map<string, boolean>;
}) {
  const look = (r: number, c: number): { cls: string; ch: string } => {
    const isShip = shipCells.has(cellKey(r, c));
    const shot = shots.get(cellKey(r, c));
    if (shot !== undefined) {
      return shot ? { cls: styles.shipHit, ch: "✕" } : { cls: styles.miss, ch: "•" };
    }
    return isShip ? { cls: styles.ship, ch: "" } : { cls: styles.water, ch: "" };
  };

  return (
    <div className={styles.board}>
      <ColumnHeaders size={size} prefix="o" />
      {range(size).map((r) => (
        <Fragment key={`or${r}`}>
          <div className={styles.label}>{r}</div>
          {range(size).map((c) => {
            const { cls, ch } = look(r, c);
            return (
              <div key={`o${r}-${c}`} className={`${styles.cell} ${cls}`}>
                {ch}
              </div>
            );
          })}
        </Fragment>
      ))}
    </div>
  );
}
