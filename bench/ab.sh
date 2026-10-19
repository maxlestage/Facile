#!/bin/sh
# ab.sh ROUNDS PATH [script]: alternates Vitesse (:3000) and the server on :3100, prints medians.
rounds=$1; path=$2; script=$3
: > /tmp/claude-0/ab.txt
for i in $(seq $rounds); do
  PID=$(pgrep -f examples/bench | head -1) ./measure.sh vitesse 3000 $path $script >> /tmp/claude-0/ab.txt
  PID=$(pgrep -x ${FACILE_PROC:-hello_engine} | head -1) ./measure.sh facile 3100 $path $script >> /tmp/claude-0/ab.txt
done
for n in vitesse facile; do
  rps=$(grep "^$n " /tmp/claude-0/ab.txt | awk '{print $3}' | sort -n | awk '{a[NR]=$1} END {print a[int((NR+1)/2)]}')
  cpu=$(grep "^$n " /tmp/claude-0/ab.txt | awk '{print $5}' | sort -n | awk '{a[NR]=$1} END {print a[int((NR+1)/2)]}')
  echo "$n median: $rps req/s, $cpu µs CPU/req"
done
