_git_super() {
    local cur="${COMP_WORDS[COMP_CWORD]}"

    local cmds="discover"

    if [[ -f ".git-super" ]]; then
        local extra
        extra=$(awk '/^\[commands\]/{f=1;next} /^\[/{f=0} f && /^[[:space:]]*[^;#[:space:]]/{sub(/[[:space:]]*=.*/,""); sub(/^[[:space:]]*/,""); print}' .git-super 2>/dev/null)
        cmds="$cmds $extra"
    fi

    COMPREPLY=($(compgen -W "$cmds" -- "$cur"))
}

complete -F _git_super git-super
