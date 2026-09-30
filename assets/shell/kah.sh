# Convenience routing only. `command codex` explicitly bypasses kah.
codex() {
    case "$PWD" in
        "$HOME"/repos/mine|"$HOME"/repos/mine/*|\
        "$HOME"/repos/research|"$HOME"/repos/research/*|\
        "$HOME"/repos/untrusted|"$HOME"/repos/untrusted/*)
            command kah __codex "$@"
            ;;
        *) command codex "$@" ;;
    esac
}
