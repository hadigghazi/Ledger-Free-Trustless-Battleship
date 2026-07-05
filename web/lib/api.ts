// Thin client for the local prover agent. The board never leaves that agent;
// we only ever exchange commitments, shots, and proven results.

import type {
  CommitMsg,
  FireResp,
  HealthResp,
  NewGameResp,
  PlacementDto,
  PvpRespondResp,
  PvpSetupResp,
  PvpVerifyCommitResp,
  PvpVerifyShotResp,
  RandomBoardResp,
  ResponseMsg,
} from "./types";

// 127.0.0.1 rather than `localhost`: browsers may resolve `localhost` to the
// IPv6 ::1 first, which breaks when the agent (e.g. inside WSL2) is only
// reachable over IPv4 loopback.
const DEFAULT_PROVER_URL = process.env.NEXT_PUBLIC_PROVER_URL ?? "http://127.0.0.1:8787";

/**
 * The URL of this player's local prover agent. Normally the env default;
 * a tab can override it with `?agent=http://localhost:8788`, which is how
 * two players can be simulated on one machine with a single web server
 * (each tab pointing at its own agent).
 */
export function proverUrl(): string {
  if (typeof window !== "undefined") {
    const q = new URLSearchParams(window.location.search).get("agent");
    if (q) return q.replace(/\/+$/, "");
  }
  return DEFAULT_PROVER_URL;
}

async function asError(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { error?: string };
    return body.error ?? `request failed (${res.status})`;
  } catch {
    return `request failed (${res.status})`;
  }
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${proverUrl()}${path}`, {
    method: "POST",
    headers: body !== undefined ? { "Content-Type": "application/json" } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new Error(await asError(res));
  return (await res.json()) as T;
}

export async function health(): Promise<HealthResp> {
  const res = await fetch(`${proverUrl()}/health`);
  if (!res.ok) throw new Error(await asError(res));
  return (await res.json()) as HealthResp;
}

export function randomBoard(): Promise<RandomBoardResp> {
  return post<RandomBoardResp>("/random-board", {});
}

// ---- Vs Computer ----

export function newGame(placements: PlacementDto[]): Promise<NewGameResp> {
  return post<NewGameResp>("/new-game", { placements });
}

export function fire(gameId: string, row: number, col: number): Promise<FireResp> {
  return post<FireResp>("/fire", { game_id: gameId, row, col });
}

// ---- Online PvP ----

export function pvpSetup(sessionId: string, placements: PlacementDto[]): Promise<PvpSetupResp> {
  return post<PvpSetupResp>("/pvp/setup", { session_id: sessionId, placements });
}

export function pvpVerifyCommit(
  sessionId: string,
  commitMsg: CommitMsg,
): Promise<PvpVerifyCommitResp> {
  return post<PvpVerifyCommitResp>("/pvp/verify-commit", {
    session_id: sessionId,
    commit_msg: commitMsg,
  });
}

export function pvpRespond(
  sessionId: string,
  row: number,
  col: number,
): Promise<PvpRespondResp> {
  return post<PvpRespondResp>("/pvp/respond", { session_id: sessionId, row, col });
}

export function pvpVerifyShot(
  sessionId: string,
  responseMsg: ResponseMsg,
  row: number,
  col: number,
): Promise<PvpVerifyShotResp> {
  return post<PvpVerifyShotResp>("/pvp/verify-shot", {
    session_id: sessionId,
    response_msg: responseMsg,
    row,
    col,
  });
}
