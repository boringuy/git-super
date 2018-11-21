# Intro:
git-super is a script to run git command in all it's managed repos and summarize the output. The "status" command is a special formatted report of the status output in each repo. The goal is to give a easy to read summary of what's changed and what local branch and tracking branch each repo is in.

The project should be clone into $GOPATH/src/

# Required:

govendor


# Fetch Dependencies:

```
$ cd $GOPATH/src
$ govendor init
$ govendor fetch github.com/fatih/color
$ govendor fetch github.com/go-ini/ini 
```

# Build:
```
$ (cd $GOPATH/bin; go build git-super)
```

# Usage:
* put the git-super build in your executable path
* copy the .git-super file to your project workspace that inclues all the git repos and run:

```
$ git super discover
```

It will walk all the directories to find git repo and list them in .git-super
Remove those you don't want git-super to manage in the [subprojects] section

Then, try:

```
$ git super status
```

Other git commands is supported but needed to be explicitly allowed in the [commands] section in the .git-super file. The script basically iterate all the managed repo and run the git command (with all the command line options) for you and print out the output of each command at the end. It can be slow because it's currently single threaded.
