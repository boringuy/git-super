package main

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"io/ioutil"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/bmatcuk/doublestar"
	"github.com/fatih/color"
	"github.com/go-ini/ini"
	"github.com/google/go-github/v50/github"
	"golang.org/x/oauth2"
)

type RepoStatus struct {
	currentBranch  string
	trackingBranch string
	status         string
	modifiedFiles  []string
}

type ProjectInfo struct {
	name           string
	repoStatus     RepoStatus
	cmdOutput      []byte
	cmdError       error
	tags           []string
	remoteBranches []string
}

type ProjectInfoMap map[string][]ProjectInfo

func GitGenericExec(cmd []string, dir string) ([]byte, error) {
	statusCmd := exec.Command("git", cmd...)
	statusCmd.Dir = dir
	return statusCmd.Output()
}

func GitGenericExecGrep(gitArgs []string, grepArgs []string, dir string) ([]byte, error) {
	var buffer bytes.Buffer
	r, w := io.Pipe()
	gitCmd := exec.Command("git", gitArgs...)
	gitCmd.Dir = dir
	gitCmd.Stdout = w
	grepCmd := exec.Command("grep", grepArgs...)
	grepCmd.Dir = dir
	grepCmd.Stdin = r

	grepCmd.Stdout = &buffer

	err := gitCmd.Start()
	if err != nil {
		return buffer.Bytes(), err
	}
	err = grepCmd.Start()
	if err != nil {
		return buffer.Bytes(), err
	}
	err = gitCmd.Wait()
	if err != nil {
		return buffer.Bytes(), err
	}
	w.Close()
	err = grepCmd.Wait()
	return buffer.Bytes(), err
}

func GetHeadRemoteState(dir string) (tag []string, remoteBranch []string) {
	statusCmd := exec.Command("git", "log", "--decorate=short", "-1")
	statusCmd.Dir = dir
	output, err := statusCmd.Output()
	tags := make([]string, 0, 2)
	remoteBranches := make([]string, 0, 2)
	if err != nil {
		return tags, remoteBranches
	}
	firstOpenParen := bytes.IndexByte(output, '(')
	if firstOpenParen < 0 {
		return tags, remoteBranches
	}
	firstCloseParen := bytes.IndexByte(output[firstOpenParen:], ')') + firstOpenParen
	if firstCloseParen < 0 {
		return tags, remoteBranches
	}
	branches := bytes.FieldsFunc(output[(firstOpenParen+1):firstCloseParen],
		func(c rune) bool { return c == ',' })
	for _, branch := range branches {
		if branch[0] == ' ' {
			branch = branch[1:]
		}
		if bytes.HasPrefix(branch, []byte("tag: ")) {
			tags = append(tags, string(branch[5:]))
		} else if bytes.HasPrefix(branch, []byte("origin/")) {
			if !bytes.HasSuffix(branch, []byte("origin/master")) &&
				!bytes.HasSuffix(branch, []byte("origin/develop")) &&
				!bytes.HasSuffix(branch, []byte("origin/HEAD")) {
				remoteBranches = append(remoteBranches, string(branch[7:]))
			}
		}
	}

	return tags, remoteBranches
}

func GitStatus(dir string) RepoStatus {
	statusCmd := exec.Command("git", "status", "--porcelain", "-b")
	statusCmd.Dir = dir
	output, err := statusCmd.Output()
	if err != nil {
		panic(err)
	}

	status := RepoStatus{}
	result := bytes.FieldsFunc(output, func(c rune) bool { return c == '\n' })

	first3DotPos := bytes.Index(result[0], []byte("..."))
	trackingBranchStartPos := first3DotPos + 3
	openBracketPos := bytes.LastIndexByte(result[0], '[')

	if first3DotPos > 0 {
		status.currentBranch = string(result[0][3:first3DotPos])
		if openBracketPos >= 0 {
			status.trackingBranch = string(result[0][trackingBranchStartPos : openBracketPos-1])
			status.status = string(result[0][openBracketPos:])
		} else {
			status.trackingBranch = string(result[0][trackingBranchStartPos:])
		}
	} else {
		status.currentBranch = string(result[0][3:])
		status.trackingBranch = "no tracking branch"
	}

	if len(result) > 1 {
		status.modifiedFiles = make([]string, 0, len(result)-2)
		for i := 1; i < len(result); i++ {
			if !bytes.HasPrefix(result[i], []byte("??")) && (len(result[i]) > 0) {
				status.modifiedFiles = append(status.modifiedFiles, string(result[i]))
			}
		}
	}

	return status
}

func OutputStatus(projectMap *ProjectInfoMap) {
	whiteBoldColor := color.New(color.FgWhite, color.Bold)
	whiteFaintColor := color.New(color.FgWhite, color.Faint)
	whiteNormalColor := color.New(color.FgWhite)
	yellowNormalColor := color.New(color.FgYellow)
	greenNormalColor := color.New(color.FgGreen)
	redNormalColor := color.New(color.FgRed)
	blueNormalColor := color.New(color.FgHiBlue)

	r := regexp.MustCompile(`ahead (?P<ahead>\d+)`)
	for trackingBranch, projectInfos := range *projectMap {
		whiteBoldColor.Printf("%s\n", trackingBranch)
		for _, project := range projectInfos {
			colorPrint := whiteNormalColor
			if len(project.repoStatus.modifiedFiles) > 0 {
				yellowNormalColor.Printf("    %-20s", project.name)
			} else if len(project.repoStatus.status) > 0 {
				greenNormalColor.Printf("    %-20s", project.name)
			} else {
				colorPrint = whiteFaintColor
				whiteFaintColor.Printf("    %-20s", project.name)
			}
			colorPrint.Printf("%10s %15s", project.repoStatus.currentBranch, project.repoStatus.status)
			colorPrint.Printf("%20s", strings.Join(project.tags, ","))
			colorPrint.Printf(" ")
			colorPrint.Printf("%-20s", strings.Join(project.remoteBranches, ","))
			colorPrint.Printf("\n")
			matches := r.FindStringSubmatch(project.repoStatus.status)
			if matches != nil {
				output, err := GitGenericExec([]string{"log", "--oneline", "-n" + matches[1]}, project.name)
				if err == nil {
					lines := strings.Split(string(output), "\n")
					for _, line := range lines {
						blueNormalColor.Printf("        %s\n", line)
					}
				} else {
					redNormalColor.Printf("%s\n", err)
				}

			}
			for i := 0; i < len(project.repoStatus.modifiedFiles); i++ {
				colorPrint.Printf("        %s\n", project.repoStatus.modifiedFiles[i])
			}
		}
	}
}

func OutputGeneric(projectMap *ProjectInfoMap) {
	whiteBoldColor := color.New(color.FgWhite, color.Bold)
	whiteFaintColor := color.New(color.FgWhite, color.Faint)
	whiteNormalColor := color.New(color.FgWhite)
	redBoldColor := color.New(color.FgRed, color.Bold)
	yellowNormalColor := color.New(color.FgYellow)
	greenNormalColor := color.New(color.FgGreen)

	for trackingBranch, projectInfos := range *projectMap {
		whiteBoldColor.Printf("%s\n", trackingBranch)
		for _, project := range projectInfos {
			colorPrint := whiteNormalColor
			if len(project.repoStatus.modifiedFiles) > 0 {
				yellowNormalColor.Printf("    %-20s", project.name)
			} else if len(project.repoStatus.status) > 0 {
				greenNormalColor.Printf("    %-20s", project.name)
			} else {
				colorPrint = whiteFaintColor
				whiteFaintColor.Printf("    %-20s", project.name)
			}
			colorPrint.Printf("%10s %s\n", project.repoStatus.currentBranch, project.repoStatus.status)
			for i := 0; i < len(project.repoStatus.modifiedFiles); i++ {
				colorPrint.Printf("        %s\n", project.repoStatus.modifiedFiles[i])
			}
			if len(project.cmdOutput) > 0 {
				if project.cmdError != nil {
					colorPrint = redBoldColor
				}
				for _, line := range bytes.FieldsFunc(project.cmdOutput, func(c rune) bool { return c == '\n' }) {
					if len(line) > 0 {
						colorPrint.Printf("        %s\n", line)
					}
				}
			}
		}
	}
}

func GetStatus(projectMap *ProjectInfoMap, project string, dir string) {
	status := GitStatus(dir)
	if (*projectMap)[status.trackingBranch] == nil {
		(*projectMap)[status.trackingBranch] = make([]ProjectInfo, 0, 20)
	}

	tags, remoteBranches := GetHeadRemoteState(dir)

	(*projectMap)[status.trackingBranch] = append((*projectMap)[status.trackingBranch], ProjectInfo{project, status, []byte(""), nil, tags, remoteBranches})
}

func RunGenericGitCommand(projectMap *ProjectInfoMap, cmd []string, project string, dir string) {
	output, err := GitGenericExec(cmd, dir)
	status := GitStatus(dir)
	tags, remoteBranches := GetHeadRemoteState(dir)
	if (*projectMap)[status.trackingBranch] == nil {
		(*projectMap)[status.trackingBranch] = make([]ProjectInfo, 0, 20)
	}

	(*projectMap)[status.trackingBranch] = append((*projectMap)[status.trackingBranch], ProjectInfo{project, status, output, err, tags, remoteBranches})
}

func GrepLogs(projectMap *ProjectInfoMap, cmd []string, project string, dir string) {
	output, err := GitGenericExecGrep([]string{"log", "--oneline"}, cmd[1:], dir)
	status := GitStatus(dir)
	tags, remoteBranches := GetHeadRemoteState(dir)
	if (*projectMap)[status.trackingBranch] == nil {
		(*projectMap)[status.trackingBranch] = make([]ProjectInfo, 0, 20)
	}

	(*projectMap)[status.trackingBranch] = append((*projectMap)[status.trackingBranch], ProjectInfo{project, status, output, err, tags, remoteBranches})
}

func GithubPr(projectMap *ProjectInfoMap, cmd []string, project string, dir string, token string) (bool, string) {
	ctx := context.Background()
	ts := oauth2.StaticTokenSource(
		&oauth2.Token{AccessToken: token},
	)
	oauth2Client := oauth2.NewClient(ctx, ts)

	output, err := GitGenericExecGrep([]string{"log", "--oneline"}, cmd[1:2], dir)
	status := GitStatus(dir)

	if len(output) <= 0 || err != nil {
		return false, ""
	}

	parts := strings.Split(status.trackingBranch, "/")
	var mergeToBranch string
	if len(parts) == 2 {
		mergeToBranch = parts[1]
	} else {
		mergeToBranch = status.trackingBranch
	}

	var url string
	url, err = CreateGithubPr(ctx, oauth2Client, project, mergeToBranch, cmd[2], dir)
	if err != nil {
		fmt.Errorf(err.Error())
		return false, ""
	}

	return true, url
}

func CreateGithubPr(ctx context.Context, oauth2Client *http.Client, project, mergeToBranch, remoteBranchName, dir string) (string, error) {
	fmt.Printf("%s - %s:\n", project, dir)
	//	fmt.Printf("gh pr create -B %s -t \"%s\" -f\n", mergeToBranch, remoteBranchName)

	var buffer bytes.Buffer
	remoteArg := "HEAD:" + remoteBranchName
	gitCmd := exec.Command("git", "push", "origin", remoteArg)
	gitCmd.Dir = dir
	gitCmd.Stdout = &buffer

	err := gitCmd.Start()
	if err != nil {
		return string(buffer.Bytes()), err
	}

	err = gitCmd.Wait()
	if err != nil {
		return string(buffer.Bytes()), err
	}

	//	fmt.Println(buffer.Bytes())

	client := github.NewClient(oauth2Client)

	newPR := &github.NewPullRequest{
		Title:               github.String(remoteBranchName),
		Head:                github.String(remoteBranchName),
		Base:                github.String(mergeToBranch),
		Body:                github.String(""),
		MaintainerCanModify: github.Bool(true),
	}

	pr, _, err := client.PullRequests.Create(ctx, "stackpath", project, newPR)
	if err != nil {
		fmt.Println(err)
		return "", err
	}

	fmt.Printf("PR created: %s\n", pr.GetHTMLURL())
	return pr.GetHTMLURL(), nil
}

func DiscoverGitRepos(iniFile *ini.File) bool {
	gitRepos, err := doublestar.Glob("./**/.git")
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: Failed to discover git repos")
		os.Exit(1)
	}
	section := iniFile.Section("subprojects")
	for _, repo := range gitRepos {
		projectName := filepath.Dir(repo)
		if !section.HasKey(projectName) {
			_, err := section.NewKey(projectName, "./"+projectName)
			if err != nil {
				fmt.Fprintf(os.Stderr, "error: Failed to add %s to ini file", projectName)
				return false
			}
		}
	}

	return true
}

func main() {
	configFile := ".git-super"
	args := os.Args
	if len(args) < 2 {
		fmt.Fprintf(os.Stderr, "usage: %s <cmd> ...", args[0])
		os.Exit(1)
	}

	gitCmd := args[1:]

	if _, err := os.Stat(configFile); os.IsNotExist(err) {
		if gitCmd[0] == "discover" {
			ini := []byte(`[subprojects]
[commands]
status = yes
fetch  = yes
pull   = yes
log    = yes
grep   = yes
commit = yes`)
			err := ioutil.WriteFile(configFile, ini, 0644)
			if err != nil {
				fmt.Fprintf(os.Stderr, "error: %s", err)
			}
		} else {
			fmt.Fprintf(os.Stderr, "error: %s not found. Please run 'git super discover' to create it", configFile)
			os.Exit(1)
		}
	}

	config, err := ini.Load(configFile)
	if err != nil {
		fmt.Fprintf(os.Stderr, "error: failed to load %s ini file\n", configFile)
		os.Exit(1)
	}

	if gitCmd[0] == "discover" {
		if DiscoverGitRepos(config) {
			err := os.Rename(configFile, configFile+".bak")
			if err != nil {
				fmt.Println(err)
				os.Exit(1)
			}
			err = config.SaveTo(configFile)
			if err != nil {
				fmt.Fprintf(os.Stderr, "error: Failed to save ini file %s\n", configFile)
				os.Rename(configFile+".bak", configFile)
				os.Exit(1)
			}
		}
		os.Exit(0)
	}

	cmdSupported := false
	for _, supportedCmd := range config.Section("commands").KeyStrings() {
		if gitCmd[0] == supportedCmd {
			cmdSupported = true
		}
	}
	if cmdSupported == false {
		fmt.Fprintf(os.Stderr, "error: %s command is not supported\n", gitCmd[0])
		os.Exit(1)
	}

	githubToken := config.Section("github").Key("token").String()
	switch gitCmd[0] {
	case "grep":
		if len(gitCmd) == 1 {
			fmt.Fprintf(os.Stderr, "error: %s command requires more arguments\n", gitCmd[0])
			os.Exit(1)
		}
	case "pr":
		if len(gitCmd) != 3 {
			fmt.Fprintf(os.Stderr, "usage: git super pr <log_pattern> <pr_branch_name>\n")
			os.Exit(1)
		}
		if len(githubToken) == 0 {
			fmt.Fprintf(os.Stderr, "error: github token is not set in .git-super\n")
			os.Exit(1)
		}
	}

	projects := config.Section("subprojects").KeysHash()

	sortedProjects := config.Section("subprojects").KeyStrings()
	sort.Strings(sortedProjects)

	projectMap := make(ProjectInfoMap)
	var prUrls []string
	for _, name := range sortedProjects {
		if gitCmd[0] == "status" {
			GetStatus(&projectMap, name, projects[name])
		} else if gitCmd[0] == "grep" {
			GrepLogs(&projectMap, gitCmd, name, projects[name])
		} else if gitCmd[0] == "pr" {
			ret, prUrl := GithubPr(&projectMap, gitCmd, name, projects[name], githubToken)
			if ret {
				prUrls = append(prUrls, prUrl)
				//				fmt.Printf("urls size: %d\n", len(prUrls))
			}
		} else {
			RunGenericGitCommand(&projectMap, gitCmd, name, projects[name])
		}
	}

	switch gitCmd[0] {
	case "status":
		OutputStatus(&projectMap)
		break
	case "pr":
		//		fmt.Println(prUrls)
		for _, url := range prUrls {
			fmt.Println(url)
		}
		break
	default:
		OutputGeneric(&projectMap)
	}

}
