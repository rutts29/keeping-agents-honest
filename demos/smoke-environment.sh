#!/bin/bash
# Source only in a controller-marked private smoke home. No login files copied.
# Native Herdr v0.9.3 src/config/io.rs:30 and src/session.rs:161 use
# XDG_CONFIG_HOME for named-session sockets; short paths avoid Unix bind failures.
set -eu
: "${KAH_SMOKE_ROOT:?SMOKE_PRIVATE_ROOT stage=setup}"
: "${KAH_SMOKE_SEEDS:?SMOKE_SEEDS stage=setup}"
: "${CLAUDE_CONFIG_DIR:?SMOKE_CLAUDE_CONFIG stage=setup}"
if [ ! -f "$KAH_SMOKE_ROOT/.smoke-private" ]; then
    printf '%s\n' 'SMOKE_PRIVATE_ROOT stage=setup: controller marker absent' >&2
    return 1
fi
case "$HOME" in
    "$KAH_SMOKE_ROOT/home"|"$KAH_SMOKE_ROOT/smoke-home") ;;
    *) printf '%s\n' 'SMOKE_HOME_SCOPE stage=setup' >&2; return 1 ;;
esac
if [ ! -d "$CLAUDE_CONFIG_DIR" ]; then
    printf '%s\n' 'SMOKE_CLAUDE_CONFIG stage=setup: direct test config missing' >&2
    return 1
fi
if [ -z "${KAH_SMOKE_CONFIG_HOME:-}" ]; then
    KAH_SMOKE_CONFIG_HOME=$(mktemp -d /var/tmp/kah-smoke-config.XXXXXXXX)
    printf '%s\n' "$KAH_SMOKE_CONFIG_HOME" >> "$KAH_SMOKE_ROOT/config-roots.txt"
fi
export XDG_CONFIG_HOME=$KAH_SMOKE_CONFIG_HOME
mkdir -p "$XDG_CONFIG_HOME" "$HOME/.agents/skills" "$HOME/.config/kah"
mkdir -p "$HOME/repos/mine" "$HOME/repos/research" "$HOME/repos/untrusted"
# Named traversal only: never expose private files or grant write access.
smoke_service_uid=$(id -u sbxrun)
setfacl -P -m "u:$smoke_service_uid:--x" -- "$KAH_SMOKE_ROOT" "$HOME" \
    "$HOME/repos" "$HOME/repos/mine" "$HOME/repos/research" "$HOME/repos/untrusted"
# New restricted projects use2770/660 (kah-sandbox initialize.rs). Inherit
# sandbox write access only within disposable project directories, never state.
setfacl -P -m "d:u:$smoke_service_uid:rwx,d:o::---" -- \
    "$HOME/repos/research" "$HOME/repos/untrusted"
if [ -d "$KAH_SMOKE_SEEDS/skills" ]; then
    cp -RL --update=none "$KAH_SMOKE_SEEDS/skills/." "$HOME/.agents/skills/"
fi
# Only explicit, secret-free seed files are accepted; nothing reads real HOME.
for smoke_name in hooks.json; do
    if [ -f "$KAH_SMOKE_SEEDS/codex/$smoke_name" ] && [ ! -e "$HOME/.codex/$smoke_name" ]; then
        mkdir -p "$HOME/.codex"
        cp "$KAH_SMOKE_SEEDS/codex/$smoke_name" "$HOME/.codex/$smoke_name"
    fi
done
if [ -f "$KAH_SMOKE_SEEDS/bashrc" ] && [ ! -e "$HOME/.bashrc" ]; then
    cp "$KAH_SMOKE_SEEDS/bashrc" "$HOME/.bashrc"
fi
