# One-command local bundle for online play (Windows). Starts YOUR prover agent
# (which holds your secret board) and the web UI, pointed at a relay. Run this
# on each player's machine. Everything here stays on http://localhost — only
# the relay is remote. All proofs are real STARKs; there is no fast/dev mode.
#
#   ./scripts/play-online.ps1 -RelayUrl ws://your-relay.example:9000
param(
  [string]$RelayUrl = "ws://localhost:9000",
  [int]$AgentPort = 8787
)

$env:PORT = "$AgentPort"
# 127.0.0.1 (not `localhost`): browsers may resolve localhost to IPv6 ::1,
# which fails when the agent is only reachable over IPv4 loopback (e.g. WSL2).
$env:NEXT_PUBLIC_PROVER_URL = "http://127.0.0.1:$AgentPort"
$env:NEXT_PUBLIC_RELAY_URL = $RelayUrl

$repo = Split-Path -Parent $PSScriptRoot

Write-Host "Starting prover agent on http://localhost:$AgentPort ..."
$agent = Start-Process -PassThru -NoNewWindow cargo -ArgumentList @(
  "run", "--release", "-p", "host", "--bin", "zk-battleship-agent"
)

try {
  Set-Location (Join-Path $repo "web")
  if (-not (Test-Path node_modules)) { npm install }
  Write-Host "Relay: $RelayUrl   Agent: $($env:NEXT_PUBLIC_PROVER_URL)"
  Write-Host "Open http://localhost:3000 and choose 'Online'."
  npm run dev
}
finally {
  if ($agent -and -not $agent.HasExited) { Stop-Process -Id $agent.Id -Force }
}
