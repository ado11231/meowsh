if [[ $- == *i* && -t 0 && -t 1 && -z ${SSH_CONNECTION-} ]] &&
   [[ -z ${TMUX-} || -z ${MEOWSH_TTY-} ]] &&
   [[ ${MEOWSH_TTY-} != "$(/usr/bin/tty 2>/dev/null)" ]]; then
    if [[ -o login ]]; then
        exec @MEOWSH_EXECUTABLE@ start --shell "${commands[zsh]:-/bin/zsh}" --login
    else
        exec @MEOWSH_EXECUTABLE@ start --shell "${commands[zsh]:-/bin/zsh}"
    fi
fi

# Inside Meowsh's own shell, mark each prompt with the last command's exit status, so the
# cat can flinch when a command fails. An empty line does not count as a command.
if [[ $- == *i* && ${MEOWSH_TTY-} == "$(/usr/bin/tty 2>/dev/null)" ]]; then
    _meowsh_ran=
    _meowsh_preexec() { _meowsh_ran=1; }
    _meowsh_precmd() {
        local meowsh_status=$?
        [[ -n $_meowsh_ran ]] && printf '\033]@MEOWSH_PROMPT_MARK@;%s\007' "$meowsh_status"
        _meowsh_ran=
        return $meowsh_status
    }
    # First in line, so other prompt hooks cannot change $? before it is read.
    preexec_functions=(_meowsh_preexec $preexec_functions)
    precmd_functions=(_meowsh_precmd $precmd_functions)
fi
