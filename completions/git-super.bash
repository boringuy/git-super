_git_super_find_file() {
    local dir="$PWD"
    while [[ "$dir" != "/" ]]; do
        if [[ -f "$dir/.git-super" ]]; then
            echo "$dir/.git-super"
            return
        fi
        dir="$(dirname "$dir")"
    done
}

_git_super_get_cmds() {
    local cmds="discover"
    local file
    file=$(_git_super_find_file)
    if [[ -n "$file" ]]; then
        local extra
        extra=$(awk '/^\[commands\]/{f=1;next} /^\[/{f=0} f && /^[[:space:]]*[^;#[:space:]]/{sub(/[[:space:]]*=.*/,""); sub(/^[[:space:]]*/,""); print}' "$file" 2>/dev/null)
        cmds="$cmds $extra"
    fi
    echo "$cmds"
}

_git_super_get_branch_aliases() {
    local file
    file=$(_git_super_find_file)
    if [[ -n "$file" ]]; then
        # GNU awk
        # awk 'match($0, /^\[branch_alias\.([^]]+)\]/, a) { print a[1] }' "$file" 2>/dev/null
        # BSD awk
        awk '/^\[branch_alias\./{sub(/^\[branch_alias\./, ""); sub(/\].*$/, ""); print}' "$file" 2>/dev/null
    fi
}

_git_super_complete() {
    local cur="$1" words="$2"
    if command -v fzf &>/dev/null; then
        local selected
        selected=$(printf '%s\n' $words | fzf \
            --layout=reverse \
            --query="$cur" \
            --select-1 \
            --exit-0 \
            --no-multi 2>/dev/null)
        [[ -n "$selected" ]] && COMPREPLY=("$selected")
    else
        COMPREPLY=($(compgen -W "$words" -- "$cur"))
    fi
}

_git_super() {
    local cur="${COMP_WORDS[COMP_CWORD]}"
    # When invoked as "git super", COMP_WORDS[0]="git" COMP_WORDS[1]="super",
    # shifting the subcommand and its args by 1 compared to "git-super".
    local offset=1
    [[ "${COMP_WORDS[0]}" == "git" ]] && offset=2

    if [[ $COMP_CWORD -eq $offset ]]; then
        _git_super_complete "$cur" "$(_git_super_get_cmds)"
    elif [[ $COMP_CWORD -eq $((offset + 1)) && "${COMP_WORDS[$offset]}" == "checkout" ]]; then
        _git_super_complete "$cur" "$(_git_super_get_branch_aliases)"
    fi
}

complete -F _git_super git-super
