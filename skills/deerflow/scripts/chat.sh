#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

DEERFLOW_VENV="$HOME/project/deer-flow/backend/.venv/bin/python"

if [[ -x "$DEERFLOW_VENV" ]]; then
    "$DEERFLOW_VENV" "$SCRIPT_DIR/skill.py" "$@"
else
    python "$SCRIPT_DIR/skill.py" "$@"
fi
