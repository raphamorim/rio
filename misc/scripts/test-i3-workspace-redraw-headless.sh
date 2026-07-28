#!/usr/bin/env bash
#
# Run test-i3-workspace-redraw.sh in an isolated Xvfb and i3 session.

set -euo pipefail

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 2
}

require_command() {
    command -v "$1" >/dev/null 2>&1 ||
        fail "required command not found: $1"
}

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
test_script="${script_dir}/test-i3-workspace-redraw.sh"

if [[ "${RIO_I3_E2E_IN_XVFB:-0}" != 1 ]]; then
    require_command dbus-run-session
    require_command xvfb-run

    export RIO_I3_E2E_IN_XVFB=1
    exec xvfb-run -a \
        -s "-screen 0 1280x800x24 +extension GLX +render -noreset" \
        dbus-run-session -- "$0" "$@"
fi

require_command i3
require_command i3-msg
[[ -x "$test_script" ]] || fail "test script is not executable: $test_script"

runtime_dir=${XDG_RUNTIME_DIR:-}
created_runtime_dir=
created_config_dir=
i3_config=
i3_log=
i3_pid=

cleanup() {
    if [[ -n "$i3_pid" ]]; then
        kill "$i3_pid" >/dev/null 2>&1 || true
        wait "$i3_pid" >/dev/null 2>&1 || true
    fi
    [[ -z "$i3_config" ]] || rm -f "$i3_config"
    [[ -z "$i3_log" ]] || rm -f "$i3_log"
    [[ -z "$created_runtime_dir" ]] || rm -rf "$created_runtime_dir"
    [[ -z "$created_config_dir" ]] || rm -rf "$created_config_dir"
}
trap cleanup EXIT INT TERM

if [[ -z "$runtime_dir" ]]; then
    created_runtime_dir=$(mktemp -d "${TMPDIR:-/tmp}/rio-i3-runtime.XXXXXX")
    chmod 700 "$created_runtime_dir"
    export XDG_RUNTIME_DIR=$created_runtime_dir
fi

created_config_dir=$(mktemp -d "${TMPDIR:-/tmp}/rio-i3-config-home.XXXXXX")
export RIO_CONFIG_HOME=$created_config_dir
: >"${RIO_CONFIG_HOME}/config.toml"

i3_config=$(mktemp "${TMPDIR:-/tmp}/rio-i3-config.XXXXXX")
i3_log=$(mktemp "${TMPDIR:-/tmp}/rio-i3-log.XXXXXX")
cat >"$i3_config" <<'EOF'
font pango:monospace 8
focus_follows_mouse no
workspace_auto_back_and_forth no
EOF

i3 -c "$i3_config" >"$i3_log" 2>&1 &
i3_pid=$!

for _ in $(seq 1 100); do
    if i3-msg -t get_version >/dev/null 2>&1; then
        break
    fi
    if ! kill -0 "$i3_pid" >/dev/null 2>&1; then
        printf 'i3 failed to start:\n' >&2
        sed -n '1,120p' "$i3_log" >&2
        exit 1
    fi
    sleep 0.1
done

i3-msg -t get_version >/dev/null 2>&1 ||
    fail "timed out waiting for i3; see $i3_log"

export XDG_SESSION_TYPE=x11
export LIBGL_ALWAYS_SOFTWARE=${LIBGL_ALWAYS_SOFTWARE:-1}

"$test_script" "$@"
