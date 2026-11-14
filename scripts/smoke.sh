#!/bin/bash
# Starts a built Agentty, checks it stays up, and takes screenshots of what it draws: the first
# screen, a terminal with output, and a settings page. For the Agentty workflow's runners, where
# nobody looks at the window: the screenshots are uploaded with the run.
#
#   scripts/smoke.sh <agentty binary> <output folder>
#
# Uses Agentty's debug driver (AGENTTY_DEBUG=1, a socket only this user can reach) to get past the
# first-run screens. Linux runs it under xvfb-run; macOS on the runner's own screen.
set -u
BIN="$1"; OUT="$2"
mkdir -p "$OUT"
export AGENTTY_DATA_DIR="$OUT/data" AGENTTY_DEBUG=1
shot() {
  if [ "$(uname)" = Darwin ]; then screencapture -x "$OUT/$1.png"; else import -window root "$OUT/$1.png"; fi
}
"$BIN" >"$OUT/app.log" 2>&1 &
pid=$!
alive() { kill -0 "$pid" 2>/dev/null || { echo "Agentty exited ($1)"; tail -50 "$OUT/app.log"; exit 1; }; }
sock=""
for _ in $(seq 1 60); do
  alive "while starting"
  sock="$(find "${TMPDIR:-/tmp}" /tmp -name s.sock -path "*agentty-$pid-*" 2>/dev/null | head -1)"
  [ -n "$sock" ] && break
  sleep 0.5
done
[ -n "$sock" ] || { echo "no debug socket"; tail -50 "$OUT/app.log"; exit 1; }
send() { printf 'debug\t%s\t%s\n' "$1" "${2:-}" | nc -U -w 1 "$sock"; }
sleep 6; alive "after start"; shot 01-first-screen
send tour finish; sleep 2
send launch-workspace "shell $HOME"; sleep 8; alive "after opening a terminal"
send type 'printf "\\033[31mred \\033[32mgreen \\033[1;34mbold\\033[0m 한글 日本語 🙂 ├──┤\\n"; uname -a'; sleep 0.5; send type '\n'
sleep 4; shot 02-terminal
send settings-section general; sleep 3; alive "after opening settings"; shot 03-settings
send settings-section open-source; sleep 3; shot 04-open-source
send page flow; sleep 3; alive "after opening a page"; shot 05-orchestration
kill "$pid" 2>/dev/null
echo "Agentty stayed up; screenshots in $OUT"
ls -la "$OUT"/*.png
