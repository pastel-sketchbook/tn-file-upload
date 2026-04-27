#!/usr/bin/env bash
set -euo pipefail

# Smoke test for all file upload REST API endpoints.
# Starts the server, exercises upload/metadata/download/delete, then cleans up.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

REST_ADDR="${REST_ADDR:-[::1]:3001}"
BASE_URL="http://$REST_ADDR"
AUTH_TOKEN="${AUTH_TOKEN:-dev-token}"
TEST_FILE="${1:-$ROOT_DIR/Cargo.toml}"

RED='\033[0;31m'
GREEN='\033[0;32m'
BOLD='\033[1m'
RESET='\033[0m'

pass() { echo -e "  ${GREEN}✓${RESET} $1"; }
fail() { echo -e "  ${RED}✗${RESET} $1"; FAILURES=$((FAILURES + 1)); }

FAILURES=0
SERVER_PID=""

cleanup() {
    if [[ -n "$SERVER_PID" ]]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo -e "${BOLD}Building server...${RESET}"
(cd "$ROOT_DIR" && cargo build --bin file-upload-server 2>/dev/null)

echo -e "${BOLD}Starting server...${RESET}"
AUTH_TOKEN="$AUTH_TOKEN" "$ROOT_DIR/target/debug/file-upload-server" &>/dev/null &
SERVER_PID=$!
sleep 1

# Verify server is up
if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo -e "${RED}Server failed to start${RESET}"
    exit 1
fi
pass "Server started (PID $SERVER_PID)"

echo ""
echo -e "${BOLD}── Upload ──${RESET}"
UPLOAD_RESP=$(curl -s -X POST "$BASE_URL/api/upload" -F "file=@$TEST_FILE")
FILE_ID=$(echo "$UPLOAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['file_id'])" 2>/dev/null || echo "")

if [[ -n "$FILE_ID" ]]; then
    SIZE=$(echo "$UPLOAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['size_bytes'])")
    CHECKSUM=$(echo "$UPLOAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['checksum'][:16])")
    pass "Upload succeeded — id=$FILE_ID size=$SIZE checksum=${CHECKSUM}..."
else
    fail "Upload failed: $UPLOAD_RESP"
fi

echo ""
echo -e "${BOLD}── Get Metadata ──${RESET}"
if [[ -n "$FILE_ID" ]]; then
    META_RESP=$(curl -s "$BASE_URL/api/files/$FILE_ID")
    META_NAME=$(echo "$META_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['file_name'])" 2>/dev/null || echo "")
    if [[ -n "$META_NAME" ]]; then
        pass "Metadata retrieved — file_name=$META_NAME"
    else
        fail "Metadata failed: $META_RESP"
    fi
else
    fail "Skipped (no file_id)"
fi

echo ""
echo -e "${BOLD}── Download ──${RESET}"
if [[ -n "$FILE_ID" ]]; then
    DL_SIZE=$(curl -s -o /dev/null -w "%{size_download}" "$BASE_URL/api/files/$FILE_ID/download")
    ORIG_SIZE=$(wc -c < "$TEST_FILE" | tr -d '[:space:]')
    if [[ "$DL_SIZE" == "$ORIG_SIZE" ]]; then
        pass "Download succeeded — $DL_SIZE bytes (matches original)"
    else
        fail "Download size mismatch: got $DL_SIZE, expected $ORIG_SIZE"
    fi
else
    fail "Skipped (no file_id)"
fi

echo ""
echo -e "${BOLD}── Delete ──${RESET}"
if [[ -n "$FILE_ID" ]]; then
    DEL_STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X DELETE "$BASE_URL/api/files/$FILE_ID")
    if [[ "$DEL_STATUS" == "204" ]]; then
        pass "Delete succeeded (204 No Content)"
    else
        fail "Delete returned HTTP $DEL_STATUS"
    fi

    # Verify gone
    GONE_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/api/files/$FILE_ID")
    if [[ "$GONE_STATUS" == "404" ]]; then
        pass "Confirmed deleted (404 on re-fetch)"
    else
        fail "File still accessible after delete (HTTP $GONE_STATUS)"
    fi
else
    fail "Skipped (no file_id)"
fi

echo ""
echo -e "${BOLD}── Error Cases ──${RESET}"

# Upload with no file
ERR_RESP=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE_URL/api/upload")
if [[ "$ERR_RESP" == "400" ]]; then
    pass "Empty upload rejected (400)"
else
    fail "Empty upload returned HTTP $ERR_RESP (expected 400)"
fi

# Get metadata for nonexistent file
ERR_RESP=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/api/files/nonexistent-id")
if [[ "$ERR_RESP" == "404" ]]; then
    pass "Nonexistent file returns 404"
else
    fail "Nonexistent file returned HTTP $ERR_RESP (expected 404)"
fi

# Download nonexistent file
ERR_RESP=$(curl -s -o /dev/null -w "%{http_code}" "$BASE_URL/api/files/nonexistent-id/download")
if [[ "$ERR_RESP" == "404" ]]; then
    pass "Nonexistent download returns 404"
else
    fail "Nonexistent download returned HTTP $ERR_RESP (expected 404)"
fi

echo ""
if [[ $FAILURES -eq 0 ]]; then
    echo -e "${GREEN}${BOLD}All checks passed!${RESET}"
else
    echo -e "${RED}${BOLD}$FAILURES check(s) failed${RESET}"
    exit 1
fi
