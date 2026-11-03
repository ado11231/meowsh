if [[ $- == *i* && -t 0 && -t 1 && -z ${SSH_CONNECTION-} ]] &&
   [[ -z ${TMUX-} || -z ${MEOWSH_TTY-} ]] &&
   [[ ${MEOWSH_TTY-} != "$(/usr/bin/tty 2>/dev/null)" ]]; then
    if shopt -q login_shell; then
        exec @MEOWSH_EXECUTABLE@ start --shell "$BASH" --login
    else
        exec @MEOWSH_EXECUTABLE@ start --shell "$BASH"
    fi
fi

# Inside Meowsh's own shell, mark each prompt with the last command's exit status, so the
# cat can flinch when a command fails. An empty line adds nothing to the history, so it
# does not count as a command.
if [[ $- == *i* && ${MEOWSH_TTY-} == "$(/usr/bin/tty 2>/dev/null)" ]]; then
    _meowsh_mark() {
        local meowsh_status=$? meowsh_latest
        meowsh_latest=$(HISTTIMEFORMAT= history 1)
        # The first prompt only notes the history, which is loaded after this file runs.
        if [[ -n ${_meowsh_started-} && $meowsh_latest != "$_meowsh_last" ]]; then
            printf '\033]@MEOWSH_PROMPT_MARK@;%s\007' "$meowsh_status"
        fi
        _meowsh_started=1
        _meowsh_last=$meowsh_latest
        return $meowsh_status
    }
    PROMPT_COMMAND="_meowsh_mark${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
fi
