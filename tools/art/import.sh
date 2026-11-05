#!/bin/bash
# Cuts every finished sheet in out/ into assets/sprites/src/ and rebuilds the atlas.
# A sheet whose object count does not match its manifest is reported and skipped;
# delete out/<name>.png and re-run run_queue.sh to redraw it.
set -u
cd "$(dirname "$0")"
ROOT=$(cd ../.. && pwd)
cargo build --release -q -p pinchora-assets --manifest-path "$ROOT/Cargo.toml" || exit 1
BIN="$ROOT/target/release/pinchora-assets"
SRC="$ROOT/assets/sprites/src"
mkdir -p "$SRC" previews
ok=0; bad=0
for png in out/*.png; do
  [ -f "$png" ] || continue
  name=$(basename "$png" .png)
  if "$BIN" cut "$png" "sheets/$name.manifest.json" "$SRC" --preview > /dev/null 2> "previews/$name.err"; then
    mv "$SRC/_preview.png" "previews/$name.png"; rm -f "previews/$name.err"; ok=$((ok + 1))
  else
    echo "skip $name: $(cat "previews/$name.err")"; bad=$((bad + 1))
  fi
done
"$BIN" pack "$SRC" "$ROOT/assets/sprites/atlas.png" "$ROOT/assets/sprites/atlas.json"
echo "imported $ok sheets, skipped $bad"
