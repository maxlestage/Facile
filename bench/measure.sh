#!/bin/sh
# measure.sh NAME PORT PATH [wrk script] -- prints requests/s and server CPU µs per request.
# The server must already run (pinned with taskset to $SERVER_CPUS); wrk runs on $CLIENT_CPUS.
name=$1; port=$2; path=$3; script=$4
pid=${PID:-$(pgrep -f "$PROC" | head -1)}
ticks() { awk '{print $14 + $15}' /proc/$pid/stat; }
hz=$(getconf CLK_TCK)
t0=$(ticks)
if [ -n "$script" ]; then
  out=$(taskset -c ${CLIENT_CPUS:-2,3} wrk -t2 -c${CONNS:-128} -d${DURATION:-5s} -s "$script" "http://127.0.0.1:$port$path" 2>&1)
else
  out=$(taskset -c ${CLIENT_CPUS:-2,3} wrk -t2 -c${CONNS:-128} -d${DURATION:-5s} "http://127.0.0.1:$port$path" 2>&1)
fi
t1=$(ticks)
reqs=$(echo "$out" | awk '/requests in/ {print $1}')
rps=$(echo "$out" | awk '/Requests\/sec/ {print $2}')
errs=$(echo "$out" | grep -E "Non-2xx|Socket errors" | tr '\n' ' ')
cpu=$(echo "$t0 $t1 $hz $reqs" | awk '{printf "%.2f", ($2-$1)/$3*1e6/$4}')
echo "$name $path: $rps req/s, $cpu µs CPU/req $errs"
