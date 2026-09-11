# Reports the working directory to rio (OSC 7) on every prompt and
# directory change, which powers the title path variables and opening
# new tabs in the current directory.

# Rio prepended its script directory to XDG_DATA_DIRS so this file
# loads; restore the original value (empty means it was unset) so the
# prepend never leaks to child processes.
if set -q RIO_FISH_XDG_DATA_DIRS
    if test -n "$RIO_FISH_XDG_DATA_DIRS"
        set -gx XDG_DATA_DIRS "$RIO_FISH_XDG_DATA_DIRS"
    else
        set -e XDG_DATA_DIRS
    end
    set -e RIO_FISH_XDG_DATA_DIRS
end

status is-interactive; or exit

function __rio_report_pwd --on-variable PWD --on-event fish_prompt \
        --description 'Report the working directory to rio (OSC 7)'
    printf '\e]7;kitty-shell-cwd://%s%s\a' $hostname $PWD
end
__rio_report_pwd
