#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ENV_FILE="$SCRIPT_DIR/../.env"

if [ ! -f "$ENV_FILE" ]; then
    echo "ERROR: .env not found at $ENV_FILE"
    exit 1
fi

# Export env vars from .env (only lines that are KEY=VALUE, skip comments/empty)
set -a
# shellcheck disable=SC1090
source <(grep -E '^[A-Z_]+\s*=' "$ENV_FILE")
set +a

API_KEY="${OPENCODE_API_KEY:-}"
BASE_URL="${OPENCODE_BASE_URL:-https://opencode.ai/zen/v1}"
MODEL="${OPENCODE_MODEL:-mimo-v2.5-free}"
TIMEOUT="${OPENCODE_TIMEOUT_SECS:-10}"

if [ -z "$API_KEY" ]; then
    echo "ERROR: OPENCODE_API_KEY is not set in .env"
    exit 1
fi

echo "→ Testing Zen chat completions endpoint"
echo "  URL:     $BASE_URL/chat/completions"
echo "  Model:   $MODEL"
echo "  Timeout: ${TIMEOUT}s"
echo ""

curl -sS --max-time "$TIMEOUT" \
    "$BASE_URL/chat/completions" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $API_KEY" \
    -d "$(cat <<EOF
{
  "model": "$MODEL",
  "response_format": { "type": "json_object" },
  "messages": [
    { "role": "user", "content": "say hello in one word as JSON: {\"word\":\"...\"}" }
  ],
  "max_tokens": 50
}
EOF
)" | jq . 2>/dev/null || echo "(jq not available — raw JSON above)"
