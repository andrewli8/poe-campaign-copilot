#!/usr/bin/env bash
# Vendors route + game data from HeartofPhos/exile-leveling (MIT).
# Bump REF to resync after a new league; then run this script and commit.
set -euo pipefail

REF="e9a248c4a452f58da6e0f30751b20072ad3276cd"
BASE="https://raw.githubusercontent.com/HeartofPhos/exile-leveling/${REF}"
DEST="$(cd "$(dirname "$0")/.." && pwd)/vendor/exile-leveling"

mkdir -p "${DEST}/routes" "${DEST}/data"

for act in 1 2 3 4 5 6 7 8 9 10; do
  curl -fsSL "${BASE}/common/data/routes/act-${act}.txt" \
    -o "${DEST}/routes/act-${act}.txt"
done

for f in areas.json quests.json kill-waypoints.json gems.json; do
  curl -fsSL "${BASE}/common/data/json/${f}" -o "${DEST}/data/${f}"
done

curl -fsSL "${BASE}/LICENSE" -o "${DEST}/LICENSE"

printf 'Vendored from https://github.com/HeartofPhos/exile-leveling\ncommit: %s\nsynced: %s\n' \
  "${REF}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "${DEST}/VENDOR.md"

echo "Synced exile-leveling data at ${REF} into ${DEST}"
