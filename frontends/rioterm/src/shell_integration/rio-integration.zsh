# Shell integration for zsh. Reports the working directory to rio
# (OSC 7) on every prompt and directory change, which powers the title
# path variables and opening new tabs in the current directory, and
# marks prompts and commands with OSC 133 so rio can tell prompt,
# command and output apart and announce a long command when it ends.
#
# Rio loads this file from the ZDOTDIR it points zsh at, before any of
# the user's rc files; running it twice is a no-op. A shell rio did not
# spawn (`exec zsh`, tmux, `sudo -E zsh`) can source it from
# `$RIO_SHELL_INTEGRATION/zsh/rio-integration.zsh`. Everything lives in
# functions so `emulate -L` only changes options locally, and builtins
# are spelled `builtin` so user functions of the same name are never
# called by accident.
if [[ -n "${_rio_state+x}" ]]; then
    return 0
fi

# Control bytes in a directory or host name could end the OSC early
# and start a sequence of their own, so they are dropped. `print -r`
# keeps backslashes literal; the frame is real ESC/BEL.
_rio_report_pwd() {
    builtin emulate -L zsh -o no_aliases
    builtin print -rn -- \
        $'\e]7;kitty-shell-cwd://'"${HOST//[[:cntrl:]]}${PWD//[[:cntrl:]]}"$'\a'
}

# Takes rio's marks back out of PS1 and PS2. The clean copy is used
# when a prompt is exactly as rio left it; when something edited the
# marked prompt in place, the marks are removed by text instead and
# the edit stays. (Comparisons and substitutions are literal:
# GLOB_SUBST is off here.)
_rio_unmark_prompts() {
    builtin emulate -L zsh -o no_warn_create_global -o no_aliases
    [[ -n ${_rio_marked_ps1+x} ]] || builtin return 0
    if [[ $PS1 == $_rio_marked_ps1 ]]; then
        PS1=$_rio_clean_ps1
    else
        PS1=${${${PS1//$_rio_mark_a}//$_rio_mark_b}//$_rio_mark_cont}
    fi
    if [[ $PS2 == $_rio_marked_ps2 ]]; then
        PS2=$_rio_clean_ps2
    else
        PS2=${${PS2//$_rio_mark_b}//$_rio_mark_cont}
    fi
}

_rio_setup() {
    builtin emulate -L zsh -o no_warn_create_global -o no_aliases
    [[ -o interactive ]] || builtin return 0

    # 0: no OSC 133 mark written yet.
    # 1: a `C` has been written and its `D` is still owed.
    # 2: the last `C` was closed.
    builtin typeset -gi _rio_state=0
    builtin typeset -g _rio_mark_a=$'%{\e]133;A\a%}'
    builtin typeset -g _rio_mark_b=$'%{\e]133;B\a%}'
    builtin typeset -g _rio_mark_cont=$'%{\e]133;P;k=s\a%}'

    # Known from the start, before any prompt: an rc file may run a
    # long program first, or drop the hook arrays altogether. The
    # chpwd report has no ordering needs, so it is installed now too:
    # `cd x && make` changes directory without a prompt in between.
    _rio_report_pwd
    builtin typeset -ag chpwd_functions precmd_functions
    chpwd_functions+=(_rio_report_pwd)

    # The prompt hooks are installed from the first precmd, once every
    # rc file has run, so they can sit around the user's own hooks: one
    # before them all, one after.
    precmd_functions+=(_rio_install_hooks)
}

_rio_install_hooks() {
    builtin emulate -L zsh -o no_warn_create_global -o no_aliases

    # First before each prompt, ahead of every other hook: close the
    # command with its status, before anything a hook prints, and take
    # last cycle's marks out of the prompts, so no hook ever sees them.
    # Enter on an empty line runs no preexec, so without this a hook
    # that edits PS1 in place (a venv prefix, say) would wrap the
    # marks and the prompt would grow at every empty line.
    _rio_precmd_first() {
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

        _rio_unmark_prompts
    }

    # Last before each prompt, after the theme set PS1: mark it.
    _rio_precmd() {
        builtin emulate -L zsh -o no_warn_create_global -o no_aliases

        # Normally already clean; this covers the first hook having
        # been dropped from the list, which also puts it back.
        _rio_unmark_prompts
        if [[ ${precmd_functions[1]} != _rio_precmd_first ]]; then
            precmd_functions=(_rio_precmd_first ${precmd_functions:#_rio_precmd_first})
        fi

        if [[ -o prompt_percent ]]; then
            if [[ ${precmd_functions[-1]} == _rio_precmd ]]; then
                # Marks inside PS1/PS2 are re-emitted whenever zsh
                # redraws the prompt (reset-prompt, SIGWINCH), which a
                # bare print cannot do. Whatever PS1 is now, a theme's
                # own or last cycle's, is the clean copy to mark.
                _rio_clean_ps1=$PS1
                _rio_clean_ps2=$PS2

                # A prompt ending in an odd run of `%` would pair its
                # last one with the `{` of the next mark into a `%{`
                # escape; even the run out first.
                builtin local tail=${PS1##*[^%]}
                (( ${#tail} % 2 )) && PS1=$PS1%
                tail=${PS2##*[^%]}
                (( ${#tail} % 2 )) && PS2=$PS2%

                # Each further line of a multi-line prompt is a
                # continuation; so is every PS2 line.
                PS1=${PS1//$'\n'/$'\n'${_rio_mark_cont}}
                PS1=${_rio_mark_a}${PS1}${_rio_mark_b}
                PS2=${_rio_mark_cont}${PS2}${_rio_mark_b}
                _rio_marked_ps1=$PS1
                _rio_marked_ps2=$PS2
                (( _rio_state = 2 ))
            else
                # Another hook took the last slot: a PS1 edit may not
                # stick, so print the mark instead (unless from zle,
                # where the cursor is mid-line) and move to the end
                # for next time.
                precmd_functions=(${precmd_functions:#_rio_precmd} _rio_precmd)
                if ! builtin zle; then
                    builtin print -n '\e]133;A\a'
                    (( _rio_state = 2 ))
                fi
            fi
        elif ! builtin zle; then
            # Without prompt_percent PS1 cannot carry `%{ %}` escapes.
            builtin print -n '\e]133;A\a'
            (( _rio_state = 2 ))
        fi

        _rio_report_pwd
    }

    # The command line is complete and about to run: its output starts
    # after `C`. Hand later preexec hooks the clean prompts.
    _rio_preexec() {
        builtin emulate -L zsh -o no_warn_create_global -o no_aliases
        _rio_unmark_prompts
        builtin print -n '\e]133;C\a'
        (( _rio_state = 1 ))
    }

    builtin typeset -ag preexec_functions
    preexec_functions+=(_rio_preexec)
    builtin local -i was_last=0
    [[ ${precmd_functions[-1]} == _rio_install_hooks ]] && was_last=1
    precmd_functions=(
        _rio_precmd_first
        ${precmd_functions:#_rio_install_hooks}
        _rio_precmd
    )
    builtin unfunction _rio_install_hooks

    # This first precmd is already under way without the hooks. zsh
    # runs the hooks that follow this one from its copy of the list,
    # so when this was not the last one a theme's precmd still has to
    # set PS1: marking it now would be undone and the wrong PS1 kept
    # as the clean copy. The mark is printed bare instead, and PS1
    # carries it from the next prompt on.
    if (( was_last )); then
        _rio_precmd
    else
        builtin print -n '\e]133;A\a'
        (( _rio_state = 2 ))
        _rio_report_pwd
    fi
}

_rio_setup
builtin unfunction _rio_setup
