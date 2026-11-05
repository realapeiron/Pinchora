#!/bin/bash
# Draws every sheet in sheets/queue.txt with Codex image generation, MAX at a time,
# retrying each one up to 3 times. Finished sheets (out/<name>.png) are skipped, so the
# script can be re-run after a rate limit. Usage: ./run_queue.sh [MAX] [QUEUE_FILE]
set -u
cd "$(dirname "$0")"
MAX=${1:-3}
QUEUE=${2:-sheets/queue.txt}
mkdir -p out logs
draw() {
  local name=$1
  for attempt in 1 2 3; do
    [ -f "out/$name.png" ] && return 0
    codex exec -s workspace-write --skip-git-repo-check -C "$PWD" -i style_reference_iso.png \
      < "sheets/$name.prompt.txt" > "logs/$name.$attempt.log" 2>&1
    [ -f "out/$name.png" ] && { echo "$(date +%T) done $name"; return 0; }
    echo "$(date +%T) retry $name ($attempt)"; sleep 240
  done
  echo "$(date +%T) FAILED $name"
}
while read -r name; do
  [ -z "$name" ] && continue
  [ -f "out/$name.png" ] && continue
  while [ "$(jobs -rp | wc -l)" -ge "$MAX" ]; do sleep 5; done
  draw "$name" &
  sleep 3
done < "$QUEUE"
wait
echo "$(date +%T) queue finished"
