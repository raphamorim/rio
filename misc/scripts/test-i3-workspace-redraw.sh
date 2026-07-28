#!/usr/bin/env bash
#
# Manual E2E regression test for stale X11 frames after an i3 workspace switch.
#
# The test puts an unfocused red Rio window on one temporary workspace and a
# blue Rio window on another. It then switches back without focusing the red
# window and compares screenshots of that window against a reference image.
#
# Requirements: an active i3/X11 session, bash, i3-msg, jq, ImageMagick, and
# awk. Use test-i3-workspace-redraw-headless.sh to create an isolated X server
# and i3 session.

set -euo pipefail

usage() {
    cat <<'EOF'
Usage:
  test-i3-workspace-redraw.sh --candidate RIO [options]

Options:
  --candidate RIO      Rio executable containing the proposed fix (required).
  --baseline RIO       Optional unpatched Rio executable to compare first.
  --iterations N       Number of workspace round trips (default: 5).
  --settle SECONDS     Delay before each screenshot (default: 0.25).
  --threshold RMSE     Maximum normalized RMSE accepted as a pass (default: 0.05).
  --artifacts DIR      Screenshot/result directory (default: a new directory in
                       ${TMPDIR:-/tmp}).
  -h, --help           Show this help.

Example:
  misc/scripts/test-i3-workspace-redraw.sh \
    --baseline /usr/bin/rio \
    --candidate target/debug/rio

The script creates temporary i3 workspaces and Rio processes. It restores the
previous workspace and focus and removes all test windows on exit.
EOF
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 2
}

require_command() {
    command -v "$1" >/dev/null 2>&1 ||
        fail "required command not found: $1"
}

resolve_executable() {
    local executable=$1

    if [[ "$executable" == */* ]]; then
        [[ -x "$executable" ]] || fail "not an executable: $executable"
        readlink -f "$executable"
    else
        command -v "$executable" ||
            fail "executable not found in PATH: $executable"
    fi
}

i3_quote() {
    jq -Rrn --arg value "$1" '$value | @json'
}

candidate=
baseline=
iterations=5
settle=0.25
threshold=0.05
artifact_dir=

while (($#)); do
    case "$1" in
        --candidate)
            (($# >= 2)) || fail "--candidate requires a value"
            candidate=$2
            shift 2
            ;;
        --baseline)
            (($# >= 2)) || fail "--baseline requires a value"
            baseline=$2
            shift 2
            ;;
        --iterations)
            (($# >= 2)) || fail "--iterations requires a value"
            iterations=$2
            shift 2
            ;;
        --settle)
            (($# >= 2)) || fail "--settle requires a value"
            settle=$2
            shift 2
            ;;
        --threshold)
            (($# >= 2)) || fail "--threshold requires a value"
            threshold=$2
            shift 2
            ;;
        --artifacts)
            (($# >= 2)) || fail "--artifacts requires a value"
            artifact_dir=$2
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

[[ -n "$candidate" ]] || {
    usage >&2
    fail "--candidate is required"
}
[[ "$iterations" =~ ^[1-9][0-9]*$ ]] ||
    fail "--iterations must be a positive integer"
[[ "$settle" =~ ^([0-9]+([.][0-9]*)?|[.][0-9]+)$ ]] ||
    fail "--settle must be a non-negative number"
[[ "$threshold" =~ ^([0-9]+([.][0-9]*)?|[.][0-9]+)$ ]] ||
    fail "--threshold must be a non-negative number"

[[ -n "${DISPLAY:-}" ]] || fail "DISPLAY is not set; an X11 session is required"
[[ "${XDG_SESSION_TYPE:-x11}" == x11 ]] ||
    fail "this test requires an X11 session"

for command in awk compare i3-msg import jq readlink; do
    require_command "$command"
done

if command -v magick >/dev/null 2>&1; then
    image_convert=(magick)
else
    require_command convert
    image_convert=(convert)
fi

candidate=$(resolve_executable "$candidate")
if [[ -n "$baseline" ]]; then
    baseline=$(resolve_executable "$baseline")
fi

i3_version=$(i3-msg -t get_version | jq -er '.human_readable')
original_workspace=$(i3-msg -t get_workspaces |
    jq -er '.[] | select(.focused).name')
original_con_id=$(i3-msg -t get_tree |
    jq -r '.. | objects |
        select(.focused? == true and .window? != null) | .id' |
    head -n 1)

if [[ -z "$artifact_dir" ]]; then
    artifact_dir=$(mktemp -d "${TMPDIR:-/tmp}/rio-i3-e2e.XXXXXX")
else
    mkdir -p "$artifact_dir"
    artifact_dir=$(readlink -f "$artifact_dir")
fi

run_id="$$-$(date +%s)"
test_pids=()
test_cons=()

cleanup() {
    stop_active_test_processes

    i3-msg "workspace $(i3_quote "$original_workspace")" \
        >/dev/null 2>&1 || true
    if [[ -n "$original_con_id" ]]; then
        i3-msg "[con_id=${original_con_id}] focus" >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT INT TERM

stop_active_test_processes() {
    local con_id pid

    for con_id in "${test_cons[@]}"; do
        i3-msg "[con_id=${con_id}] kill" >/dev/null 2>&1 || true
    done

    for pid in "${test_pids[@]}"; do
        kill "$pid" >/dev/null 2>&1 || true
    done

    test_cons=()
    test_pids=()
}

find_con() {
    local marker=$1
    local con_id

    for _ in $(seq 1 100); do
        con_id=$(i3-msg -t get_tree |
            jq -r --arg marker "$marker" \
                '.. | objects |
                    select(.name? == $marker and .window? != null) | .id' |
            head -n 1)
        if [[ -n "$con_id" ]]; then
            printf '%s\n' "$con_id"
            return 0
        fi
        sleep 0.1
    done

    return 1
}

launch_static_rio() {
    local executable=$1
    local marker=$2
    local color=$3
    local log_file="${artifact_dir}/${marker}.log"

    "$executable" -e sh -c \
        "printf '\\033]2;${marker}\\007\\033[?25l\\033[${color}m\\033[2J\\033[H${marker}\\n'; exec sleep 300" \
        >"$log_file" 2>&1 &
    launched_pid=$!
    test_pids+=("$launched_pid")

    launched_con=$(find_con "$marker") ||
        fail "Rio window did not appear: $marker (see $log_file)"
    test_cons+=("$launched_con")
}

capture_con() {
    local con_id=$1
    local output=$2
    local geometry x y width height

    geometry=$(i3-msg -t get_tree |
        jq -er --argjson con_id "$con_id" \
            '.. | objects | select(.id? == $con_id) |
                .rect | [.x, .y, .width, .height] | @tsv')
    IFS=$'\t' read -r x y width height <<<"$geometry"

    import -window root "${output}.root.png"
    "${image_convert[@]}" "${output}.root.png" \
        -crop "${width}x${height}+${x}+${y}" +repage "${output}.png"
}

normalized_rmse() {
    local metric value

    metric=$(compare -metric RMSE "$1" "$2" null: 2>&1 || true)
    value=$(awk -F '[()]' 'NF >= 3 { print $(NF - 1) }' <<<"$metric")
    [[ "$value" =~ ^([0-9]+([.][0-9]*)?|[.][0-9]+)$ ]] ||
        fail "could not parse ImageMagick RMSE output: $metric"
    printf '%s\n' "$value"
}

rmse_is_acceptable() {
    awk -v value="$1" -v limit="$threshold" \
        'BEGIN { exit !(value <= limit) }'
}

run_variant() {
    local label=$1
    local target_binary=$2
    local support_binary=$3
    local workspace_a="rio_i3_e2e_${run_id}_${label}_a"
    local workspace_b="rio_i3_e2e_${run_id}_${label}_b"
    local target_marker="RIO_I3_E2E_${run_id}_${label}_TARGET"
    local helper_marker="RIO_I3_E2E_${run_id}_${label}_HELPER"
    local blue_marker="RIO_I3_E2E_${run_id}_${label}_BLUE"
    local target_con helper_con blue_con iteration
    local return_rmse focused_rmse failures=0

    i3-msg "workspace $(i3_quote "$workspace_a")" >/dev/null

    launch_static_rio "$target_binary" "$target_marker" 41
    target_con=$launched_con
    launch_static_rio "$support_binary" "$helper_marker" 42
    helper_con=$launched_con

    i3-msg "[con_id=${helper_con}] focus" >/dev/null
    sleep "$settle"
    capture_con "$target_con" "${artifact_dir}/${label}-reference"

    i3-msg "workspace $(i3_quote "$workspace_b")" >/dev/null
    launch_static_rio "$support_binary" "$blue_marker" 44
    blue_con=$launched_con
    sleep "$settle"

    printf '\n%s (%s)\n' "$label" "$target_binary"
    for iteration in $(seq 1 "$iterations"); do
        i3-msg "workspace $(i3_quote "$workspace_b")" >/dev/null
        sleep "$settle"
        i3-msg "workspace $(i3_quote "$workspace_a")" >/dev/null
        sleep "$settle"

        capture_con "$target_con" \
            "${artifact_dir}/${label}-return-${iteration}"
        return_rmse=$(normalized_rmse \
            "${artifact_dir}/${label}-reference.png" \
            "${artifact_dir}/${label}-return-${iteration}.png")

        if ! rmse_is_acceptable "$return_rmse"; then
            ((failures += 1))
        fi

        i3-msg "[con_id=${target_con}] focus" >/dev/null
        sleep "$settle"
        i3-msg "[con_id=${helper_con}] focus" >/dev/null
        sleep "$settle"
        capture_con "$target_con" \
            "${artifact_dir}/${label}-focused-${iteration}"
        focused_rmse=$(normalized_rmse \
            "${artifact_dir}/${label}-reference.png" \
            "${artifact_dir}/${label}-focused-${iteration}.png")

        printf '  iteration=%d return_rmse=%s focused_rmse=%s\n' \
            "$iteration" "$return_rmse" "$focused_rmse"
    done

    stop_active_test_processes

    if ((failures == 0)); then
        printf '  result=PASS (%d/%d within threshold %s)\n' \
            "$iterations" "$iterations" "$threshold"
    else
        printf '  result=FAIL (%d/%d exceeded threshold %s)\n' \
            "$failures" "$iterations" "$threshold"
    fi

    variant_failures=$failures
}

printf 'i3=%s\n' "$i3_version"
printf 'display=%s\n' "$DISPLAY"
printf 'artifacts=%s\n' "$artifact_dir"
printf 'iterations=%s settle=%s threshold=%s\n' \
    "$iterations" "$settle" "$threshold"

support_binary=$candidate
if [[ -n "$baseline" ]]; then
    support_binary=$baseline
    run_variant baseline "$baseline" "$support_binary"
fi

run_variant candidate "$candidate" "$support_binary"
candidate_failures=$variant_failures

if ((candidate_failures > 0)); then
    exit 1
fi
