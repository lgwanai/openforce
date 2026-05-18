#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DIST_DIR="$PROJECT_ROOT/dist"
DATE=$(date +%Y%m%d)
ZIP_NAME="deerflow-skill-$DATE.zip"

cd "$PROJECT_ROOT"

mkdir -p "$DIST_DIR"

git ls-files --others --cached --exclude-standard | \
    grep -v "^config\.yaml$" | \
    grep -v "^\.claude/" | \
    xargs zip -q "$DIST_DIR/$ZIP_NAME"

echo "Package created: $DIST_DIR/$ZIP_NAME"
echo "Size: $(du -h "$DIST_DIR/$ZIP_NAME" | cut -f1)"
echo "Total files: $(unzip -l "$DIST_DIR/$ZIP_NAME" | tail -1 | awk '{print $2}')"
