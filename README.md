# Intro:
git-super is a script to run git command in all it's managed repos and summarize the output. The "status" command is a special formatted report of the status output in each repo. The goal is to give a easy to read summary of what's changed and what local branch and tracking branch each repo is in.

# Required:

govendor

# Fetch Dependencies:

```
$ govendor init
$ govendor fetch github.com/fatih/color
$ govendor fetch github.com/go-ini/ini 
```

# Build:
```
$ go build
```

# Usage:
* put the git-super build in your executable path
* copy the .git-super file to your project workspace that inclues all the git repos and run:

```
$ git super init
```

It will walk all the directories to find git repo and list them in .git-super
Remove those you don't want git-super to manage in the [subprojects] section

Then, try:

```
$ git super status
```
