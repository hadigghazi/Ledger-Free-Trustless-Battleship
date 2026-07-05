// WebSocket client for the cloud relay. The relay only forwards the public
// messages below between the two players' browsers — it never sees a board.

import type { CommitMsg, ResponseMsg } from "./types";

// 127.0.0.1 rather than `localhost`: see the note in api.ts (IPv6 vs WSL2).
const RELAY_URL = process.env.NEXT_PUBLIC_RELAY_URL ?? "ws://127.0.0.1:9000";

/** Messages exchanged over the relay (sent by us / received from opponent). */
export type GameMsg =
  | { type: "commit"; commit_msg: CommitMsg; coin_commit: string }
  | { type: "coin-reveal"; nonce: string }
  | { type: "fire"; row: number; col: number }
  | { type: "response"; response_msg: ResponseMsg; row: number; col: number };

/** Events surfaced to the UI: relay control events plus forwarded game msgs. */
export type RelayEvent =
  | { type: "joined"; seat: number; room: string; opponentPresent: boolean }
  | { type: "opponent-joined" }
  | { type: "opponent-left" }
  | { type: "error"; error: string }
  | { type: "closed" }
  | GameMsg;

export class Relay {
  private ws: WebSocket;
  private room: string;

  constructor(room: string, onEvent: (e: RelayEvent) => void) {
    this.room = room;
    this.ws = new WebSocket(RELAY_URL);

    this.ws.onopen = () => {
      this.ws.send(JSON.stringify({ type: "join", room: this.room }));
    };
    this.ws.onmessage = (ev) => {
      try {
        onEvent(JSON.parse(ev.data as string) as RelayEvent);
      } catch {
        onEvent({ type: "error", error: "malformed message from relay" });
      }
    };
    this.ws.onerror = () => {
      onEvent({ type: "error", error: `could not reach the relay at ${RELAY_URL}` });
    };
    this.ws.onclose = () => {
      onEvent({ type: "closed" });
    };
  }

  send(msg: GameMsg): void {
    if (this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(msg));
    }
  }

  close(): void {
    this.ws.close();
  }
}

export { RELAY_URL };
