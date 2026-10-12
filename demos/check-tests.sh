#!/bin/sh
# Unit/integration checks never put fixture state in the user's live kah home.
set -eu
# Synthetic transport usage belongs to the isolated test, never a live bundle.
# Tests of the opt-in guard set these explicitly in their own child process.
unset JEV_FIXTURE_MONEY_STOP KAH_FIXTURE_BUDGET_ROOT
base=${KAH_TEST_FIXTURE_BASE:-/mnt/kah-fixtures/cargo/kah-check-tests}
mkdir -p "$base"
fixtures=$(mktemp -d "$base/run.XXXXXX")
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
    set -- cargo test --workspace
fi
"$@"
