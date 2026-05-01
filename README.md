# Intro:
git-super is a tool to manage multiple repos where one repo might depends on each other and often time should lock to specific version branches when working on a feature. It also provide easy way to run a git command across all of its managed repos and summarize the output. The `status` command produces a specially formatted report of each repo's status. The goal is an easy-to-read summary of what's changed and which local/tracking branch each repo is on.

The repos are processed in parallel; results are gathered and rendered together at the end.

# Build:
```
$ cargo build --release
```

The binary will be at `target/release/git-super`.

# Usage:
* put the `git-super` executable in your `PATH`

```
$ git super discover
```

This walks the directories recursively to find git repos and lists them in `.git-super`.
Remove any you don't want git-super to manage from the `[subprojects]` section.

Then try:

```
$ git super status
```

Other git commands are supported but must be explicitly allowed in the `[commands]` section of `.git-super`. The tool iterates over every managed repo and runs the git command (with all its arguments) for you, then prints the output of each command at the end. Repos are processed in parallel using a worker pool, so runs are typically fast even on a large number of repos.

# Bash Completion

A completion script is provided at `completions/git-super.bash`. It completes subcommands for both `git-super` and `git super`.

## Setup

Source the file in your `.bashrc` or `.bash_profile`:

```bash
source /path/to/git-super/completions/git-super.bash
```

## `git super` subcommand completion

For `git super <tab>` to work, git's own bash completion must be loaded first. If it isn't already active in your shell, source it before the git-super completion:

```bash
# macOS — Homebrew git
source "$(brew --prefix)/etc/bash_completion.d/git-completion.bash"

# macOS — Xcode CLT git
source /Library/Developer/CommandLineTools/usr/share/git-core/git-completion.bash
```

You can check whether it's loaded with:

```bash
declare -f __git_main >/dev/null 2>&1 && echo "loaded" || echo "missing"
```

Once git's completion is active, `git super <tab>` will complete subcommands automatically — the completion script hooks in via the standard `_git_super` naming convention that git's completion dispatch recognises.

# Configuration: `.git-super`

`.git-super` is an INI file at the root of where you run `git super`. It supports the following sections.

## `[subprojects]`

Maps a repo name to its directory (relative to the `.git-super` file). `git super discover` populates this for you, but you can edit it by hand.

```ini
[subprojects]
repo-a = ./repo-a
repo-b = ./services/repo-b
```

## `[commands]`

Allowlist of git commands that `git super` will run on each subproject. Any git command can be added; the value is just a flag (`yes`).

```ini
[commands]
status   = yes
fetch    = yes
pull     = yes
log      = yes
grep     = yes
commit   = yes
checkout = yes
```

Special commands with custom behavior:

- `status` — produces the formatted multi-repo status report.
- `grep <pattern>` — runs `git log --oneline | grep <pattern>` in each repo.
- `pr <log_pattern> <pr_branch_name>` — pushes the current HEAD as `<pr_branch_name>` and opens a GitHub PR in each repo whose log matches `<log_pattern>`. Requires `[github]` token (see below).
- `checkout` — see [Branch aliases](#branch_aliasalias_name) below.

Anything else is run as `git <cmd> <args...>` in every subproject.

## `[github]`

Configuration for the `pr` command.

- `token` — GitHub personal access token with `repo` scope.
- `org` — GitHub organization (or user) that owns the repos.

```ini
[github]
token = ghp_xxxxxxxxxxxxxxxxxxxx
org   = my-github-org
```

## `[branch_alias.<alias_name>]`

A **branch alias** is a single short name that maps to different real branches in different repos. This is useful when subprojects share an LTS or release line but each repo names the branch differently.

Define one section per alias. Inside, each key is a subproject name (matching `[subprojects]`) and the value is the real branch in that repo:

```ini
[branch_alias.lts-1.5]
repo-a = release-1.5
repo-b = release/1.5.x
repo-c = v1.5
```

Then:

```
$ git super checkout -b new_feature lts-1.5
```

resolves the alias per repo and runs `git checkout -b new_feature <real_branch>` in each subproject — so `new_feature` in `repo-a` is branched from `release-1.5`, in `repo-b` from `release/1.5.x`, etc.

`checkout` accepts:

| Invocation | Behavior |
|---|---|
| `git super checkout <alias>` | switches each repo to its alias-mapped branch |
| `git super checkout -b <new_branch> <alias>` | creates `<new_branch>` from the alias-mapped branch in each repo |
| `git super checkout -b <new_branch>` | creates `<new_branch>` in each repo from current HEAD |
| `git super checkout <branch>` (no matching alias section) | passthrough — runs `git checkout <branch>` in each repo |

If a `[branch_alias.<name>]` section exists but a particular subproject is not listed in it, that repo is left untouched and the run reports `no branch alias entry for <repo>` for it.
