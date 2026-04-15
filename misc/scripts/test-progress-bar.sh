#!/bin/bash
#
# Manual repro for OSC 9;4 progress-bar handling (issue #1509).
# Run inside a Rio window. The fix is verified visually if the
# indeterminate bar in phase 2 *moves* across the window — before
# the fix it froze at the left edge because every heartbeat OSC
# yanked the animation phase back to t=0.

osc() { printf '\033]9;4;%s;%s\033\\' "$1" "$2"; }

echo "== phase 1: determinate, climbing 0->100 =="
for p in 0 10 20 30 40 50 60 70 80 90 100; do
    osc 1 "$p"
    sleep 0.15
done

echo "== phase 2: heartbeat indeterminate (issue #1509 repro) =="
echo "(OSC 9;4;3 every 100ms for 6s — bar should slide L<->R, not freeze)"
end=$(( $(date +%s) + 6 ))
while [ "$(date +%s)" -lt "$end" ]; do
    osc 3 0
    sleep 0.1
done

echo "== phase 3: error state at 50% =="
osc 2 50
sleep 1.5

echo "== phase 4: pause at 75% =="
osc 4 75
sleep 1.5

echo "== phase 5: clear =="
osc 0 0
