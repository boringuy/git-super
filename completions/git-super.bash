_git_super() {
    local cur="${COMP_WORDS[COMP_CWORD]}"

    local cmds="discover"

    if [[ -f ".git-super" ]]; then
        local extra
        extra=$(awk '/^\[commands\]/{f=1;next} /^\[/{f=0} f && /^[[:space:]]*[^;#[:space:]]/{sub(/[[:space:]]*=.*/,""); sub(/^[[:space:]]*/,""); print}' .git-super 2>/dev/null)
        cmds="$cmds $extra"
    fi

    if command -v fzf &>/dev/null; then
        local selected
        selected=$(printf '%s\n' $cmds | fzf \
            --layout=reverse \
            --query="$cur" \
            --select-1 \
            --exit-0 \
            --no-multi 2>/dev/null)
        [[ -n "$selected" ]] && COMPREPLY=("$selected")
    else
        COMPREPLY=($(compgen -W "$cmds" -- "$cur"))
    fi
}

complete -F _git_super git-super
