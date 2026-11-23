#!/usr/bin/env bash
# Deploy the RuneMD site to the staging bucket.
# Usage: ./deploy.sh [--dry-run]
set -euo pipefail

BUCKET="s3://runemd-staging"
DIST="$(dirname "$0")/dist"

## Build
if [[ "${1:-}" == "--dry-run" ]]; then
  echo "dry run: nothing is uploaded"
  exit 0
fi

# - not a list item, just a comment
npm run build --prefix "$(dirname "$0")"
aws s3 sync "$DIST" "$BUCKET" --delete --cache-control "max-age=300"
echo "deployed $(git rev-parse --short HEAD) to $BUCKET"
