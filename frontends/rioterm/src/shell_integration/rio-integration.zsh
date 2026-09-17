# Reports the working directory to rio (OSC 7) on every prompt and
# directory change, which powers the title path variables and opening
# new tabs in the current directory. Loading twice is a no-op.
if [[ -n "${_rio_integration+x}" ]]; then
    return 0
fi
builtin typeset -g _rio_integration=1

_rio_report_pwd() {
    builtin printf '\e]7;kitty-shell-cwd://%s%s\a' "${HOST-}" "$PWD"
}

builtin autoload -Uz add-zsh-hook
add-zsh-hook chpwd _rio_report_pwd
add-zsh-hook precmd _rio_report_pwd
_rio_report_pwd
