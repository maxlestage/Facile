#!/bin/sh
# The benchmark of Facile against Vitesse, actix-web and axum, with wrk.
#
#   VITESSE_BIN=... ACTIX_BIN=... AXUM_BIN=... bench/run.sh
#
# The servers of Vitesse's benchmark (github.com/maxlestage/Vitesse, bench/)
# are built there; Facile serves bench/app.ry, the same routes in Rythme, with
# and without its cache of pure responses. Each server runs alone on
# SERVER_CPUS (0,1), wrk on CLIENT_CPUS (2,3), 128 connections; each scenario
# is measured ROUNDS times and the median is kept. Prints a Markdown table:
# requests per second, and the server's CPU time per request.
set -e
cd "$(dirname "$0")"
FACILE="${FACILE_BIN:-../target/release/facile}"
ROUNDS="${ROUNDS:-3}"
export DURATION="${DURATION:-5s}" CONNS="${CONNS:-128}"
SERVER_CPUS="${SERVER_CPUS:-0,1}"
out=$(mktemp)

measure() { # name pid path [script]: the medians of ROUNDS runs
  for i in $(seq "$ROUNDS"); do
    PID=$2 ./measure.sh x 0 "$3" "$4" | sed -E 's/.*: ([0-9.]+) req\/s, ([0-9.]+) .*/\1 \2/'
  done | python3 -c '
import sys, statistics
rows = [l.split() for l in sys.stdin if l.strip()]
rps = statistics.median(float(r[0]) for r in rows)
cpu = statistics.median(float(r[1]) for r in rows)
print(f"{sys.argv[1]}|{sys.argv[2]}|{rps:.0f}|{cpu:.2f}")' "$1" "$3 $4" >> "$out"
}

run() { # name port command...
  name=$1; port=$2; shift 2
  taskset -c "$SERVER_CPUS" env PORT="$port" WORKERS=2 "$@" >/dev/null 2>&1 &
  pid=$!
  sleep 1.5
  srv=$(pgrep -P "$pid" | head -1); [ -z "$srv" ] && srv=$pid
  for s in "/" "/json" "/json navigateur.lua" "/users/42" "/echo post.lua" "/ pipeline.lua"; do
    set -- $s
    PORT=$port measure "$name" "$srv" "$1" "$2"
  done
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null || true
  sleep 0.5
}

run "Facile" 3100 "$FACILE" serve app.ry --port 3100 --workers 2
FACILE_CACHE=off run "Facile (sans cache)" 3101 "$FACILE" serve app.ry --port 3101 --workers 2
[ -n "$VITESSE_BIN" ] && run "Vitesse" 3000 "$VITESSE_BIN"
[ -n "$ACTIX_BIN" ] && run "actix-web" 3003 "$ACTIX_BIN"
[ -n "$AXUM_BIN" ] && run "axum" 3002 "$AXUM_BIN"
echo "| Serveur | Scénario | Requêtes/s | CPU/requête (µs) |"
echo "|---|---|---:|---:|"
awk -F'|' '{ printf "| %s | `%s` | %d | %.2f |\n", $1, $2, $3, $4 }' "$out"
rm -f "$out"
