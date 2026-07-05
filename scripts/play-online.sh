#!/usr/bin/env bash
# One-command local bundle for online play: starts YOUR prover agent (which
# holds your secret board) and the web UI, pointed at a relay. Run this on each
# player's machine. Everything here stays on http://localhost — only the relay
# is remote. All proofs are real STARKs; there is no fast/dev mode.
#
#   RELAY_URL=ws://your-relay.example:9000 ./scripts/play-online.sh
#
# Env overrides:
#   RELAY_URL   relay WebSocket URL   (default ws://localhost:9000)
#   AGENT_PORT  local agent port      (default 8787)
set -euo pipefail

RELAY_URL="${RELAY_URL:-ws://localhost:9000}"
AGENT_PORT="${AGENT_PORT:-8787}"

export PORT="$AGENT_PORT"
# 127.0.0.1 (not `localhost`): browsers may resolve localhost to IPv6 ::1,
# which fails when the agent is only reachable over IPv4 loopback (e.g. WSL2).
export NEXT_PUBLIC_PROVER_URL="http://127.0.0.1:$AGENT_PORT"
export NEXT_PUBLIC_RELAY_URL="$RELAY_URL"

echo "Starting prover agent on http://localhost:$AGENT_PORT ..."
cargo run --release -p host --bin zk-battleship-agent &
AGENT_PID=$!
trap 'kill "$AGENT_PID" 2>/dev/null || true' EXIT

cd "$(dirname "$0")/../web"
[ -d node_modules ] || npm install
echo "Relay: $RELAY_URL   Agent: $NEXT_PUBLIC_PROVER_URL"
echo "Open http://localhost:3000 and choose 'Online'."
npm run dev
