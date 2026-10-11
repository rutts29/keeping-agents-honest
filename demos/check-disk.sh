#!/bin/sh
# Check the live worktree and build filesystems before compiling or launching.
set -eu
target=${CARGO_TARGET_DIR:-target}
required=8388608
# User-approved tmpfs route for this replay only; the standing 4 GiB volume
# guard remains. The harness measures actual tmpfs capacity and peak RAM.
if [ "${KAH_GATE_PHASE:-}" = observer-phase1-matrix ]; then required=4194304; fi
if [ -d "$target" ]; then
    used=$(du -sk "$target" | awk '{print $1}')
    case "$used" in
        ''|*[!0-9]*) echo "GATE_DISK_READ stage=pre_gate: cannot size $target" >&2; exit 1 ;;
    esac
    if [ "$used" -gt 26214400 ]; then
        echo "GATE_TARGET_PRUNE stage=pre_gate: $target exceeds 25 GiB; running cargo clean" >&2
        cargo clean --target-dir "$target"
    fi
fi
for path in / "$PWD" "$target"; do
    while [ ! -e "$path" ]; do path=$(dirname "$path"); done
    available=$(df -Pk "$path" | awk 'NR == 2 {print $4}')
    case "$available" in
        ''|*[!0-9]*) echo "GATE_DISK_READ stage=pre_gate: cannot read free space for $path" >&2; exit 1 ;;
    esac
    if [ "$available" -lt "$required" ]; then
        echo "GATE_DISK_LOW stage=pre_gate: $path has $available KiB free; at least $required KiB required. Run cargo clean or choose a CARGO_TARGET_DIR with enough space before starting a live gate." >&2
        exit 1
    fi
done
