# Shell integration for zsh. Reports the working directory to rio
# (OSC 7) on every prompt and directory change, which powers the title
# path variables and opening new tabs in the current directory, and
# marks prompts and commands with OSC 133 so rio can tell prompt,
# command and output apart and announce a long command when it ends.
#
# Rio loads this file from the ZDOTDIR it points zsh at, before any of
# the user's rc files; running it twice is a no-op. Everything lives in
# functions so `emulate -L` only changes options locally, and builtins
# are spelled `builtin` so user functions of the same name are never
# called by accident.
if [[ -n "${_rio_state+x}" ]]; then
    return 0
fi

_rio_setup() {
    builtin emulate -L zsh -o no_warn_create_global -o no_aliases
    [[ -o interactive ]] || builtin return 0

    # 0: no OSC 133 mark written yet.
    # 1: a `C` has been written and its `D` is still owed.
    # 2: the last `C` was closed.
    builtin typeset -gi _rio_state=0

    # The hooks are installed from the first precmd, once every rc file
    # has run, so they sit after the user's own hooks: a prompt theme
    # then sets PS1 before the marks are added, and sees it without
    # them.
    builtin typeset -ag precmd_functions
    precmd_functions+=(_rio_install_hooks)
}

_rio_install_hooks() {
    builtin emulate -L zsh -o no_warn_create_global -o no_aliases

    # Control bytes in a directory or host name could end the OSC
    # early and start a sequence of their own, so they are dropped.
    # `print -r` keeps backslashes literal; the frame is real ESC/BEL.
    _rio_report_pwd() {
        builtin print -rn -- \
            $'\e]7;kitty-shell-cwd://'"${HOST//[[:cntrl:]]}${PWD//[[:cntrl:]]}"$'\a'
    }

    # Before each prompt: close the command with its status, then mark
    # the prompt itself.
    _rio_precmd() {
        builtin local -i last_status=$?
        builtin emulate -L zsh -o no_warn_create_global -o no_aliases

        # A precmd run by a zle widget (some plugins redraw the prompt
        # that way) is not the end of a command, so no `D` then.
        if ! builtin zle; then
            if (( _rio_state == 1 )); then
                builtin print -n '\e]133;D;'$last_status'\a'
                (( _rio_state = 2 ))
            elif (( _rio_state == 2 )); then
                # Enter on an empty line runs no preexec: close
                # whatever may be open, with no status to give.
                builtin print -n '\e]133;D\a'
            fi
        fi

        builtin local mark_a=$'%{\e]133;A\a%}'
        if [[ -o prompt_percent ]]; then
            builtin typeset -g precmd_functions
            if [[ ${precmd_functions[-1]} == _rio_precmd ]]; then
                # Marks inside PS1/PS2 are re-emitted whenever zsh
                # redraws the prompt (reset-prompt, SIGWINCH), which a
                # bare print cannot do. Put PS1 back to the clean copy
                # when it still carries last cycle's marks, so themes
                # that pattern-match the prompt never see them; a
                # theme's own change to PS1 wins and is marked as is.
                builtin local ps1_changed=0
                if [[ -n ${_rio_clean_ps1+x} ]]; then
                    if [[ $PS1 == $_rio_marked_ps1 ]]; then
                        PS1=$_rio_clean_ps1
                        PS2=$_rio_clean_ps2
                    elif [[ $PS1 != $_rio_clean_ps1 ]]; then
                        ps1_changed=1
                    fi
                fi
                _rio_clean_ps1=$PS1
                _rio_clean_ps2=$PS2

                builtin local mark_b=$'%{\e]133;B\a%}'
                builtin local mark_cont=$'%{\e]133;P;k=s\a%}'
                # A PS1 ending in a lone `%` would pair with the `{` of
                # the next mark into a `%{` escape; double it first.
                [[ $PS1 == *[^%]% || $PS1 == % ]] && PS1=$PS1%
                PS1=${mark_a}${PS1}${mark_b}
                # Each further line of a multi-line prompt is a
                # continuation, unless a theme rebuilds PS1 itself, in
                # which case marks inside it could break its matching.
                if (( ! ps1_changed )) && [[ $PS1 == *$'\n'* ]]; then
                    PS1=${PS1//$'\n'/$'\n'${mark_cont}}
                fi
                [[ $PS2 == *[^%]% || $PS2 == % ]] && PS2=$PS2%
                PS2=${mark_cont}${PS2}${mark_b}
                _rio_marked_ps1=$PS1
                (( _rio_state = 2 ))
            else
                # Another hook took the last slot: a PS1 edit may not
                # stick, so print the mark instead (unless from zle,
                # where the cursor is mid-line) and move to the end
                # for next time.
                precmd_functions=(${precmd_functions:#_rio_precmd} _rio_precmd)
                if ! builtin zle; then
                    builtin print -rn -- $mark_a[3,-3]
                    (( _rio_state = 2 ))
                fi
            fi
        elif ! builtin zle; then
            # Without prompt_percent PS1 cannot carry `%{ %}` escapes.
            builtin print -rn -- $mark_a[3,-3]
            (( _rio_state = 2 ))
        fi

        _rio_report_pwd
    }

    # The command line is complete and about to run: its output starts
    # after `C`. Hand later preexec hooks the clean PS1.
    _rio_preexec() {
        builtin emulate -L zsh -o no_warn_create_global -o no_aliases
        if [[ -n ${_rio_clean_ps1+x} && $PS1 == $_rio_marked_ps1 ]]; then
            PS1=$_rio_clean_ps1
            PS2=$_rio_clean_ps2
        fi
        builtin print -n '\e]133;C\a'
        (( _rio_state = 1 ))
    }

    # `cd x && make` changes directory without a prompt in between,
    # so the directory is reported on chpwd as well as on precmd.
    builtin typeset -ag chpwd_functions preexec_functions
    chpwd_functions+=(_rio_report_pwd)
    preexec_functions+=(_rio_preexec)
    precmd_functions=(${precmd_functions:#_rio_install_hooks} _rio_precmd)
    builtin unfunction _rio_install_hooks

    # This first precmd already ran before the hooks existed.
    _rio_precmd
}

_rio_setup
builtin unfunction _rio_setup
