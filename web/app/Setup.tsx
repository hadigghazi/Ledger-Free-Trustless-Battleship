"use client";

// Interactive fleet placement editor, shared by both game screens.
// The chosen placements are sent to the LOCAL prover agent only; the agent
// validates them, salts and commits them, and (in PvP) proves the board legal
// in zero knowledge. Nothing here ever reaches the opponent.

import { Fragment, useState } from "react";
import styles from "./page.module.css";
import { randomBoard } from "../lib/api";
import type { PlacementDto, ShipName } from "../lib/types";
import { cellKey, range } from "./board";

export const SHIP_LENGTHS: Record<ShipName, number> = {
  Carrier: 5,
  Battleship: 4,
  Cruiser: 3,
  Submarine: 3,
  Destroyer: 2,
};

const SHIP_ORDER: ShipName[] = ["Carrier", "Battleship", "Cruiser", "Submarine", "Destroyer"];
const SIZE = 10;

export function cellsOf(p: PlacementDto): [number, number][] {
  return range(SHIP_LENGTHS[p.ship]).map((i) =>
    p.orientation === "horizontal" ? [p.row, p.col + i] : [p.row + i, p.col],
  );
}

function fits(p: PlacementDto, others: PlacementDto[]): boolean {
  const cells = cellsOf(p);
  if (cells.some(([r, c]) => r < 0 || c < 0 || r >= SIZE || c >= SIZE)) return false;
  const occupied = new Set(others.flatMap((o) => cellsOf(o).map(([r, c]) => cellKey(r, c))));
  return cells.every(([r, c]) => !occupied.has(cellKey(r, c)));
}

export default function Setup({
  title,
  busy,
  readyLabel,
  onReady,
  children,
}: {
  title: string;
  busy: boolean;
  readyLabel: string;
  onReady: (placements: PlacementDto[]) => void;
  children?: React.ReactNode; // extra controls (e.g. room code input)
}) {
  const [placed, setPlaced] = useState<PlacementDto[]>([]);
  const [selected, setSelected] = useState<ShipName>("Carrier");
  const [orientation, setOrientation] = useState<"horizontal" | "vertical">("horizontal");
  const [note, setNote] = useState("");

  const placedByShip = new Map(placed.map((p) => [p.ship, p]));
  const shipAt = (r: number, c: number): PlacementDto | undefined =>
    placed.find((p) => cellsOf(p).some(([pr, pc]) => pr === r && pc === c));

  function nextUnplaced(after: ShipName, list: PlacementDto[]): ShipName {
    const taken = new Set(list.map((p) => p.ship));
    const start = SHIP_ORDER.indexOf(after);
    for (let i = 0; i < SHIP_ORDER.length; i++) {
      const s = SHIP_ORDER[(start + i) % SHIP_ORDER.length];
      if (!taken.has(s)) return s;
    }
    return after;
  }

  function onCell(r: number, c: number) {
    setNote("");
    const existing = shipAt(r, c);
    if (existing) {
      // Pick the ship back up.
      setPlaced((prev) => prev.filter((p) => p.ship !== existing.ship));
      setSelected(existing.ship);
      setOrientation(existing.orientation);
      return;
    }
    if (placedByShip.has(selected)) {
      setNote(`${selected} is already placed — click it to move it.`);
      return;
    }
    const candidate: PlacementDto = { ship: selected, row: r, col: c, orientation };
    if (!fits(candidate, placed)) {
      setNote("Doesn't fit there — out of bounds or overlapping.");
      return;
    }
    const next = [...placed, candidate];
    setPlaced(next);
    if (next.length < SHIP_ORDER.length) setSelected(nextUnplaced(selected, next));
  }

  async function shuffle() {
    setNote("");
    try {
      const r = await randomBoard();
      setPlaced(r.placements);
    } catch (e) {
      setNote(`Couldn't reach your prover agent: ${(e as Error).message}`);
    }
  }

  const complete = placed.length === SHIP_ORDER.length;
  const cellLook = (r: number, c: number): string => {
    const p = shipAt(r, c);
    return p ? `${styles.cell} ${styles.ship} ${styles.clickable}` : `${styles.cell} ${styles.water} ${styles.clickable}`;
  };

  return (
    <div className={styles.setup}>
      <h2 className={styles.setupTitle}>{title}</h2>
      <div className={styles.controls}>
        {SHIP_ORDER.map((s) => {
          const isPlaced = placedByShip.has(s);
          const cls = [
            styles.shipChip,
            selected === s ? styles.shipChipActive : "",
            isPlaced ? styles.shipChipPlaced : "",
          ].join(" ");
          return (
            <button key={s} className={cls} onClick={() => setSelected(s)} disabled={busy}>
              {s} · {SHIP_LENGTHS[s]}
              {isPlaced ? " ✓" : ""}
            </button>
          );
        })}
      </div>
      <div className={styles.controls}>
        <button
          className={styles.shipChip}
          onClick={() => setOrientation((o) => (o === "horizontal" ? "vertical" : "horizontal"))}
          disabled={busy}
        >
          {orientation === "horizontal" ? "▸ horizontal" : "▾ vertical"}
        </button>
        <button className={styles.shipChip} onClick={shuffle} disabled={busy}>
          🎲 random board
        </button>
        <button className={styles.shipChip} onClick={() => setPlaced([])} disabled={busy || placed.length === 0}>
          clear
        </button>
      </div>

      <div className={styles.board}>
        <div className={styles.corner} />
        {range(SIZE).map((c) => (
          <div key={`sh${c}`} className={styles.label}>
            {c}
          </div>
        ))}
        {range(SIZE).map((r) => (
          <Fragment key={`sr${r}`}>
            <div className={styles.label}>{r}</div>
            {range(SIZE).map((c) => (
              <button
                key={`s${r}-${c}`}
                className={cellLook(r, c)}
                onClick={() => onCell(r, c)}
                disabled={busy}
                aria-label={`place at row ${r} column ${c}`}
              />
            ))}
          </Fragment>
        ))}
      </div>

      {note && <div className={`${styles.message} ${styles.error}`}>{note}</div>}

      <div className={styles.controls} style={{ marginTop: 16 }}>
        {children}
        <button className={styles.button} onClick={() => onReady(placed)} disabled={busy || !complete}>
          {readyLabel}
        </button>
      </div>
      {!complete && (
        <p className={styles.legend}>
          Place all five ships: pick a ship, choose an orientation, then click a cell. Click a placed
          ship to move it.
        </p>
      )}
    </div>
  );
}
