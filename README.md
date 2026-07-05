# ZK Battleship — trustless Battleship without a blockchain

A Battleship game where **cheating is cryptographically impossible** — with no
blockchain, no trusted server, and no honor system. Each player commits to a
secret board before the game, and every answer during the game ("hit", "miss",
"you sank my Cruiser", and even the final "I lost") is a **zero-knowledge
proof**, verified on the opponent's own machine before it is believed.

Built on the [RISC Zero](https://risczero.com) zkVM: the game rules are plain
Rust that runs *inside* the zkVM, which produces a STARK receipt proving the
code executed correctly on the player's secret board.

> You sank my battleship… **provably.**

There is deliberately **no dev mode, no mock proofs, and no CLI demo** in this
repository. Every receipt everywhere is a real STARK (the build even enables
RISC Zero's `disable-dev-mode`, which turns any attempt to re-enable mock
proving into a hard error).

---

## The protocol in four lines

1. **Commit** — at setup each player publishes `C = SHA-256(board ‖ salt)` with
   a proof that the board behind `C` is a legal fleet (Proof 1).
2. **Flip** — a Blum commit-reveal coin flip decides who fires first, so
   neither player controls the first-mover advantage.
3. **Prove every shot** — when fired upon, a player returns a proof that, for
   the board behind `C` **and the exact sequence of shots fired so far**, this
   shot is a hit/miss, sinks ship Z (or nothing), and defeats them (or not)
   (Proof 2).
4. **Win provably** — the game ends when a shot proof carries
   `defeated = true`: the loser's own proof concedes the game. No one has to
   take anyone's word for anything, ever.

### Why the history is inside the proof

Prior ZK Battleship systems (BattleZips, the RISC Zero NEAR example, the MIT
6.857 project) put the public game state — who fired where, how many hits —
on a **blockchain**, and let the contract decide when the game ends. Remove
the chain naively and a hole opens up: "sunk" and "you win" depend on the
*history* of shots, and if the prover picks the history, a losing player can
simply never be provably defeated.

This implementation closes that hole without a ledger: every shot proof binds
the **full public shot history** into its journal, and the verifier — who
fired those shots and therefore knows the true sequence — rejects any proof
computed against a different one. Sunk detection and the win condition are
computed *inside* the zkVM from that bound history, so each receipt is
self-contained and the entire match, including its termination, is verifiable
from the message transcript alone.

---

## Architecture

```
   Player A's machine                                Player B's machine
   ┌────────────────────────────┐                    ┌────────────────────────────┐
   │ browser UI (Next.js)       │                    │ browser UI (Next.js)       │
   │   ⇅ localhost              │                    │   ⇅ localhost              │
   │ prover agent (Rust)        │                    │ prover agent (Rust)        │
   │  · holds board + salt      │                    │  · holds board + salt      │
   │  · proves own answers      │                    │  · proves own answers      │
   │  · verifies opponent's     │                    │  · verifies opponent's     │
   └──────────┬─────────────────┘                    └───────────┬────────────────┘
              │              ws                 ws               │
              └──────────────►   RELAY (cloud)   ◄───────────────┘
                              forwards public messages only:
                              commitments · coin flips · shots · receipts
```

* **Secrets never travel.** Boards and salts exist only inside each player's
  own local agent process.
* **The relay is untrusted.** It sees only public data it could not use: it
  cannot read a board (hiding), cannot alter a result (any tampering fails
  STARK verification), and cannot forge an outcome (it cannot produce a
  receipt for the agreed guest image IDs). At worst it can stop forwarding —
  a denial of service, never a wrong game result.
* **The two guest programs are the whole trusted rulebook.** Both agents print
  and serve (`/health`) their guest **image IDs**; if the two players' builds
  differed by even one instruction, every cross-verification would fail.

```
zk-battleship/
├── core/      Shared game logic (types, fleet rules, hit/sunk/defeat,
│              commitment preimage). Compiled into both host and guest.
│              All secret-touching code uses fixed-shape control flow, so
│              proving time does not leak board information.
├── methods/
│   └── guest/ The two programs proven INSIDE the zkVM:
│                commit_board.rs  (Proof 1: legal board  -> commitment)
│                prove_shot.rs    (Proof 2: bind to C + history -> result)
├── host/      Prover/verifier library + two binaries:
│                zk-battleship-agent  the local prover agent (HTTP, localhost)
│                zk-battleship-bench  measurement harness for the paper
├── relay/     Untrusted WebSocket message relay (no game logic, no RISC Zero).
├── web/       Next.js UI: fleet placement, vs-computer, and online PvP.
├── scripts/   play-online.{sh,ps1}: one-command agent + web UI per player.
└── docs/      paper/ — the research paper (LaTeX).
```

---

## Prerequisites

- **Rust** (stable) via [rustup](https://rustup.rs).
- **RISC Zero toolchain** via `rzup` (installs the RISC-V guest toolchain and
  the `r0vm` prover; the guest build selects it automatically):

  ```bash
  curl -L https://risczero.com/install | bash
  rzup install
  ```

  Linux and macOS are supported natively; on Windows use WSL2.
- **Node.js** (for the web UI), and optionally **Docker** (to host the relay).

GPU is optional; by default proving runs on CPU (a shot proof takes roughly
half a minute on a modern laptop CPU — see `docs/paper` for measured
numbers).

---

## Play against the computer

```bash
# 1. your local prover agent
cargo run --release -p host --bin zk-battleship-agent

# 2. the web UI (second terminal)
cd web && npm install && npm run dev
```

Open `http://localhost:3000`, choose **Vs Computer**, place your fleet (or
shuffle), and fire by clicking cells. The computer proves every answer and
your agent verifies each proof — receipt, commitment, shot, and history —
before you see it.

## Play a friend (online, still trustless)

Each player runs the agent + UI on their own machine; a small relay forwards
the public messages between them.

```bash
# One of you (or any third party) hosts the relay:
docker compose up --build -d          # listens on :9000
# ...or deploy it to Fly.io / Railway / any VM — see relay/fly.toml

# Each player, on their own machine:
RELAY_URL=ws://YOUR-RELAY-HOST:9000 ./scripts/play-online.sh          # macOS/Linux
./scripts/play-online.ps1 -RelayUrl ws://YOUR-RELAY-HOST:9000         # Windows
```

Open `http://localhost:3000`, choose **Online**, place your fleet, and both
players enter the same **room code**. The commit exchange, coin flip, and
per-shot proofs all happen automatically; every result you see has passed
STARK verification inside your own agent.

> Trying it alone on one machine? Run the relay, two agents (`PORT=8787` and
> `PORT=8788`), and one web UI — then open `http://localhost:3000` in one tab
> and `http://localhost:3000/?agent=http://127.0.0.1:8788` in another. Each
> tab plays through its own agent. (Always address agents as `127.0.0.1`, not
> `localhost` — browsers may try IPv6 first, which WSL2 doesn't forward.)

### Deploying the pieces

| Piece | Where it runs | Trust required | How to deploy |
|---|---|---|---|
| prover agent | each player's machine | it holds *your* secret — it is you | `cargo run --release -p host --bin zk-battleship-agent` |
| web UI | each player's machine (localhost) | same trust domain as the agent | `npm run dev` (or `npm run build && npm start`) |
| relay | anywhere (cloud) | **none** | `docker compose up -d`, or `fly deploy --config relay/fly.toml --dockerfile relay/Dockerfile` |

The agent binds to `127.0.0.1` only and answers CORS solely for the local web
UI origin (extend with `AGENT_ALLOWED_ORIGINS=https://your-hosted-ui` if you
serve the UI from a domain; the agent already answers Chrome's
Private-Network-Access preflight).

Keeping the UI and proving on each player's own machine is not a limitation —
it *is* the trust model: a hosted prover would have to be trusted with your
board, exactly the kind of trust this project exists to remove. (An in-browser
WASM prover would allow zero-install play with the same guarantees; RISC Zero
browser proving is not yet practical at this circuit size.)

---

## Verifying the claims yourself

**Game-logic tests** (no RISC Zero toolchain needed):

```bash
cargo test -p zk-battleship-core
```

**Adversarial protocol tests** against the real prover — every cheat in the
threat model, shown to fail (`#[ignore]`d because real proving takes time):

```bash
cargo test --release -p host -- --ignored --test-threads=1
```

These check, with genuine STARKs: an illegal fleet cannot be committed; a
board-swap cannot produce any receipt; a replayed proof, a fabricated
history, and a tampered journal are all rejected; and the final hit yields a
proof with `defeated = true`.

**Benchmarks** (proving/verification times, receipt sizes, and the
witness-independence check used in the paper):

```bash
cargo run --release -p host --bin zk-battleship-bench
```

---

## Security model (summary)

| Property | Rests on |
|---|---|
| Soundness (no false results) | STARK soundness + the agreed guest image IDs |
| Binding (no board swap) | SHA-256 collision resistance + the `== C` assertion in every shot proof |
| History integrity (no fake sunk/win) | the shot history echoed in every journal, checked by the party that fired the shots |
| Hiding (board stays secret) | 256-bit random salt in the commitment |
| Zero-knowledge | only the journal (commitment, shot, result, history) is revealed |
| Fair first move | Blum commit-reveal coin flip |
| Side channels | fixed-shape guest execution: cycle count is independent of the board |
| Online trust | the relay only forwards public messages; each player proves and verifies locally |

**Honest limitations.** ZK cannot force a losing player to keep playing
(abort); mitigations are timeouts or stakes, both outside the cryptographic
core. Player identity/authentication is a transport concern. See the paper in
[`docs/paper/`](docs/paper/) for the full protocol description, security
analysis, and measurements.

---

## License

MIT — see [`LICENSE`](LICENSE).
