#!/usr/bin/env bash
set -euo pipefail

PORT="${1:-3001}"
URL="http://localhost:${PORT}/docs"
TIMEOUT=120

# Load .env if present
if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

export AUTH_TOKEN="${AUTH_TOKEN:-dev-token}"

if ! curl -s -o /dev/null -w '' "$URL" 2>/dev/null; then
  echo "Server not running on port ${PORT}, building and starting in release mode..."
  cargo build --release --bin file-upload-server
  cargo run --release --bin file-upload-server &
  SERVER_PID=$!

  for i in $(seq 1 "$TIMEOUT"); do
    if curl -s -o /dev/null -w '' "$URL" 2>/dev/null; then
      break
    fi
    if [ "$i" -eq "$TIMEOUT" ]; then
      echo "ERROR: Server failed to start within ${TIMEOUT} seconds."
      kill "$SERVER_PID" 2>/dev/null
      exit 1
    fi
    sleep 1
  done
fi

open "$URL"
