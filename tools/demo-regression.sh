#!/usr/bin/env bash
# Demo determinism regression test.
#
# Plays the stock doom1.wad demos headless and compares a per-tic gameplay
# fingerprint against committed golden hashes. Any divergence means a
# gameplay/physics/RNG change broke demo compatibility.
#
# Requires doom1.wad. Set IWAD to its path (default: ~/DOOM/doom1.wad).
# Skips (exit 0) if the WAD is absent so CI without the WAD is not a hard fail.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# Prefer the in-repo WAD, then ~/DOOM. Override with IWAD=...
if [[ -z "${IWAD:-}" ]]; then
    if [[ -f "$ROOT/data/doom1.wad" ]]; then
        IWAD="$ROOT/data/doom1.wad"
    else
        IWAD="$HOME/DOOM/doom1.wad"
    fi
fi
BIN="${BIN:-$ROOT/target/release/room4doom}"
GOLDEN_DIR="$ROOT/data/test_files/demo_golden"
# Per-demo wall-clock budget (seconds). Headless playback runs roughly real-time
# gameplay logic; the longest stock demo needs ~90s. The trace is flushed per
# tic, so a kill after the golden length still captures every compared tic.
DEADLINE="${DEMO_DEADLINE:-180}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# demo -> golden tic count.
DEMOS=("demo1:5026" "demo2:3836" "demo3:2134")

if [[ ! -f "$IWAD" ]]; then
    echo "SKIP: IWAD not found at $IWAD (set IWAD=...)"
    exit 0
fi
if [[ ! -x "$BIN" ]]; then
    echo "Building room4doom (release)..."
    (cd "$ROOT" && cargo build --release -p room4doom)
fi

fail=0
for entry in "${DEMOS[@]}"; do
    name="${entry%:*}"
    tics="${entry#*:}"
    trace="$TMP/$name.txt"
    golden="$GOLDEN_DIR/$name.golden"

    DEMO_TRACE="$trace" "$BIN" --iwad "$IWAD" --demo "$name" --headless \
        >/dev/null 2>&1 &
    pid=$!
    # Poll for the trace to reach the golden length, then stop. Bounded by the
    # wall-clock deadline so a desync that loops without ending can't hang CI.
    waited=0
    while kill -0 "$pid" 2>/dev/null; do
        lines=$([[ -f "$trace" ]] && wc -l < "$trace" || echo 0)
        if [[ "$lines" -ge "$tics" || "$waited" -ge "$DEADLINE" ]]; then
            break
        fi
        sleep 1
        waited=$((waited + 1))
    done
    kill -9 "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true

    # Demo-relevant fingerprint: tic, p_random index, thing count, sector hash,
    # thing hash. (m_random index and the combined hash are excluded — m_random
    # is not demo-deterministic in vanilla.)
    awk '{print $1,$3,$4,$5,$6}' "$trace" | head -n "$tics" > "$TMP/$name.fp"

    got=$(wc -l < "$TMP/$name.fp")
    if [[ "$got" -lt "$tics" ]]; then
        echo "FAIL: $name only produced $got/$tics tics (timed out or crashed)"
        fail=1
        continue
    fi
    if diff -q "$golden" "$TMP/$name.fp" >/dev/null; then
        echo "PASS: $name ($tics tics)"
    else
        echo "FAIL: $name diverges from golden:"
        diff "$golden" "$TMP/$name.fp" | head -6
        fail=1
    fi
done

exit $fail
