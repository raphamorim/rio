# Reports the working directory to rio (OSC 7) on every prompt and
# directory change, which powers the title path variables and opening
# new tabs in the current directory.
status is-interactive; or exit

function __rio_report_pwd --on-variable PWD --on-event fish_prompt \
        --description 'Report the working directory to rio (OSC 7)'
    printf '\e]7;kitty-shell-cwd://%s%s\a' $hostname $PWD
end
__rio_report_pwd
