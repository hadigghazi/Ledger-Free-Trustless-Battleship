// Types mirroring the local prover agent's JSON API (zk-battleship-agent).
// The agent only ever produces real STARK receipts — there is no mode switch.

export type ShipName = "Carrier" | "Battleship" | "Cruiser" | "Submarine" | "Destroyer";

export type PlacementDto = {
  ship: ShipName;
  row: number;
  col: number;
  orientation: "horizontal" | "vertical";
};

export type ShipDto = {
  ship: string;
  cells: [number, number][];
};

export type ShotResult = {
  row: number;
  col: number;
  hit: boolean;
  sunk: string | null;
};

export type HealthResp = {
  ok: boolean;
  proving: string;
  commit_image_id: string;
  shot_image_id: string;
};

export type RandomBoardResp = {
  placements: PlacementDto[];
};

// ---- Vs Computer ----

export type NewGameResp = {
  game_id: string;
  your_commitment: string;
  ai_commitment: string;
  your_ships: ShipDto[];
  board_size: number;
};

export type FireResp = {
  your_shot: ShotResult;
  ai_shot: ShotResult | null;
  you_won: boolean;
  ai_won: boolean;
};

// ---- Online PvP (agent <-> browser) ----
// `commit_msg` / `response_msg` are opaque blobs the browser relays to the
// opponent unchanged; only the local agents ever interpret them.

export type CommitMsg = unknown;
export type ResponseMsg = unknown;

export type PvpSetupResp = {
  your_ships: ShipDto[];
  your_commitment: string;
  commit_msg: CommitMsg;
  board_size: number;
};

export type PvpVerifyCommitResp = {
  opponent_commitment: string;
};

export type PvpRespondResp = {
  response_msg: ResponseMsg;
  hit: boolean;
  sunk: string | null;
  /** Proven defeat flag from your own shot journal. */
  you_lost: boolean;
};

export type PvpVerifyShotResp = {
  hit: boolean;
  sunk: string | null;
  /** True iff the opponent's proof itself states their fleet is sunk. */
  you_won: boolean;
};
