# Intro:
git-super is a tool to run a git command across all of its managed repos and summarize the output. The `status` command produces a specially formatted report of each repo's status. The goal is an easy-to-read summary of what's changed and which local/tracking branch each repo is on.

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
