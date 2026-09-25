#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if [ "$#" -ne 1 ]; then
  echo "Usage: $0 <run-id>" >&2
  echo "Create a run first with: ${SCRIPT_DIR}/new-run.sh <run-id>" >&2
  exit 2
fi
exec python3 "${SCRIPT_DIR}/sdlc.py" verify "$@"
