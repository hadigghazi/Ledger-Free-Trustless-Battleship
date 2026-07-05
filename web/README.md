# ZK Battleship — web UI

A browser front-end for ZK Battleship with two screens:

- **Vs Computer** — single-player against an AI that proves every answer.
- **Online** — two humans over a shared room code, fully trustless.

Every result you see — hit, miss, sunk ship, and the end of the game itself —
is backed by a real zero-knowledge proof, generated and verified by a local
prover agent running on your own machine. There is no mock/dev mode.

## Architecture

```
Vs Computer:
  Browser ──HTTP──► your local prover agent (holds both boards, proves+verifies)

Online (PvP):
  Browser ──HTTP──► your local prover agent (holds YOUR board, proves+verifies)
  Browser ──WS────► cloud relay ──WS──► opponent's browser   (public msgs only)
```

Your board never leaves your local agent — the browser only ever handles
commitments, shots, and proven results, plus the opaque proof blobs it relays
to the opponent. The fair first-mover coin flip (commit–reveal) runs in the
browser with WebCrypto; each side verifies the opponent's reveal locally.

## Running it

### Vs Computer

```bash
# 1. prover agent (one folder up)
cargo run --release -p host --bin zk-battleship-agent

# 2. this web app
npm install
cp .env.local.example .env.local   # optional; defaults to localhost
npm run dev
```

Open `http://localhost:3000`, choose **Vs Computer**, and place your fleet.

### Online

Use the one-command bundle from the repo root, which starts your agent and this
app together pointed at a relay:

```bash
RELAY_URL=ws://YOUR-RELAY:9000 ../scripts/play-online.sh   # or .ps1 on Windows
```

Then choose **Online** and share a room code with your opponent. See the root
[`README.md`](../README.md) for the relay setup.

## Configuration

- `NEXT_PUBLIC_PROVER_URL` — your local agent (default `http://localhost:8787`).
- `NEXT_PUBLIC_RELAY_URL` — the relay for online play (default `ws://localhost:9000`).

See `.env.local.example`.

## Expect a pause after each shot

Each answer is a genuine STARK proof generated on the responder's machine and
verified on yours. On a modern multicore CPU this takes seconds; the status
line tells you what is being proven or verified at every step.
