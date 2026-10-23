#!/bin/sh
# Unit/integration checks never put fixture state in the user's live kah home.
# KAH_TEST_FIXTURE_BASE names a parent directory; only this run's child is removed.
set -eu
# Synthetic transport usage belongs to the isolated test, never a live bundle.
# Tests of the opt-in guard set these explicitly in their own child process.
# The live and replay entrypoints below stay dormant in an ordinary check.
unset JEV_FIXTURE_MONEY_STOP KAH_FIXTURE_BUDGET_ROOT
unset KAH_HERDR_CAPTURE KAH_NATIVE_RUN_PROBE KAH_NATIVE_CLEANUP_PROBE
unset KAH_NATIVE_PANE_PROBE KAH_PHASE1_ROOT KAH_TRACK2_ENTRYPOINT
unset KAH_RELAY_PROBE KAH_SEARCH_CAPTURE KAH_POST_GATE_AUDIT KAH_TRACK2_WRITER
unset KAH_LIVE_FIXTURE KAH_LIVE_CONTROLLER
base=${KAH_TEST_FIXTURE_BASE:-${TMPDIR:-/tmp}/kah-check-tests}
mkdir -p -- "$base"
base=$(cd -- "$base" && pwd)
fixtures=$(mktemp -d "$base/run.XXXXXX")
chmod 700 "$fixtures"
finish() {
    result=$?
    trap - EXIT INT TERM
    rm -rf -- "$fixtures"
    exit "$result"
}
trap finish EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
export XDG_STATE_HOME="$fixtures/state"
export TMPDIR="$fixtures/tmp"
mkdir -p "$XDG_STATE_HOME" "$TMPDIR"
if [ "$#" -eq 0 ]; then
    set -- cargo test --locked --workspace
fi
"$@"
