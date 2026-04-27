use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process;

use colored::Colorize;
use duct::cmd;
use ini::Ini;
use rayon::prelude::*;
use regex::Regex;
use serde_json::json;
use walkdir::WalkDir;

#[derive(Default, Clone)]
struct RepoStatus {
    current_branch: String,
    tracking_branch: String,
    status: String,
    modified_files: Vec<String>,
}

#[derive(Clone)]
struct ProjectInfo {
    name: String,
    dir: String,
    repo_status: RepoStatus,
    cmd_output: String,
    cmd_error: Option<String>,
    tags: Vec<String>,
    remote_branches: Vec<String>,
}

type ProjectInfoMap = BTreeMap<String, Vec<ProjectInfo>>;

enum Action {
    Status,
    Generic,
    Grep,
    Pr,
    Checkout {
        new_branch: Option<String>,
        alias_map: Option<BTreeMap<String, String>>,
        raw_target: String,
    },
}

fn git_exec(args: &[&str], dir: &str) -> Result<String, String> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    cmd("git", owned)
        .dir(dir)
        .stderr_capture()
        .stdout_capture()
        .read()
        .map_err(|e| e.to_string())
}

fn git_log_grep(grep_args: &[&str], dir: &str) -> Result<String, String> {
    let git = cmd("git", vec!["log", "--oneline"]).dir(dir);
    let owned: Vec<String> = grep_args.iter().map(|s| s.to_string()).collect();
    let grep = cmd("grep", owned).dir(dir);
    git.pipe(grep)
        .stderr_capture()
        .unchecked()
        .read()
        .map_err(|e| e.to_string())
}

fn get_head_remote_state(dir: &str) -> (Vec<String>, Vec<String>) {
    let mut tags = Vec::new();
    let mut remote_branches = Vec::new();
    let output = match git_exec(&["log", "--decorate=short", "-1"], dir) {
        Ok(s) => s,
        Err(_) => return (tags, remote_branches),
    };
    let open = match output.find('(') {
        Some(p) => p,
        None => return (tags, remote_branches),
    };
    let close = match output[open..].find(')') {
        Some(p) => open + p,
        None => return (tags, remote_branches),
    };
    let inner = &output[open + 1..close];
    for raw in inner.split(',') {
        let branch = raw.strip_prefix(' ').unwrap_or(raw);
        if let Some(tag) = branch.strip_prefix("tag: ") {
            tags.push(tag.to_string());
        } else if let Some(rb) = branch.strip_prefix("origin/") {
            if rb != "master" && rb != "develop" && rb != "HEAD" {
                remote_branches.push(rb.to_string());
            }
        }
    }
    (tags, remote_branches)
}

fn resolve_alias(config: &Ini, name: &str) -> Option<BTreeMap<String, String>> {
    let section_name = format!("branch_alias.{}", name);
    config.section(Some(section_name.as_str())).map(|sec| {
        sec.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    })
}

fn git_status(dir: &str) -> RepoStatus {
    let output = git_exec(&["status", "--porcelain", "-b"], dir)
        .unwrap_or_else(|e| panic!("git status failed in {}: {}", dir, e));
    let lines: Vec<&str> = output.split('\n').filter(|l| !l.is_empty()).collect();
    let mut status = RepoStatus::default();
    if lines.is_empty() {
        return status;
    }
    let first = lines[0];
    if first.len() < 3 {
        return status;
    }
    if let Some(pos) = first.find("...") {
        status.current_branch = first[3..pos].to_string();
        let tracking_start = pos + 3;
        if let Some(bracket) = first.rfind('[') {
            let tb_end = bracket.saturating_sub(1);
            if tracking_start <= tb_end {
                status.tracking_branch = first[tracking_start..tb_end].to_string();
            }
            status.status = first[bracket..].to_string();
        } else {
            status.tracking_branch = first[tracking_start..].to_string();
        }
    } else {
        status.current_branch = first[3..].to_string();
        status.tracking_branch = "no tracking branch".to_string();
    }

    for line in lines.iter().skip(1) {
        if !line.starts_with("??") && !line.is_empty() {
            status.modified_files.push((*line).to_string());
        }
    }
    status
}

fn process_project(
    name: &str,
    dir: &str,
    action: &Action,
    args: &[String],
    token: &str,
) -> (Option<ProjectInfo>, Option<String>) {
    match action {
        Action::Status => {
            let status = git_status(dir);
            let (tags, remote_branches) = get_head_remote_state(dir);
            (
                Some(ProjectInfo {
                    name: name.to_string(),
                    dir: dir.to_string(),
                    repo_status: status,
                    cmd_output: String::new(),
                    cmd_error: None,
                    tags,
                    remote_branches,
                }),
                None,
            )
        }
        Action::Generic => {
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            let (output, err) = match git_exec(&arg_refs, dir) {
                Ok(o) => (o, None),
                Err(e) => (String::new(), Some(e)),
            };
            let status = git_status(dir);
            let (tags, remote_branches) = get_head_remote_state(dir);
            (
                Some(ProjectInfo {
                    name: name.to_string(),
                    dir: dir.to_string(),
                    repo_status: status,
                    cmd_output: output,
                    cmd_error: err,
                    tags,
                    remote_branches,
                }),
                None,
            )
        }
        Action::Grep => {
            let grep_args: Vec<&str> = args.iter().skip(1).map(String::as_str).collect();
            let (output, err) = match git_log_grep(&grep_args, dir) {
                Ok(o) => (o, None),
                Err(e) => (String::new(), Some(e)),
            };
            let status = git_status(dir);
            let (tags, remote_branches) = get_head_remote_state(dir);
            (
                Some(ProjectInfo {
                    name: name.to_string(),
                    dir: dir.to_string(),
                    repo_status: status,
                    cmd_output: output,
                    cmd_error: err,
                    tags,
                    remote_branches,
                }),
                None,
            )
        }
        Action::Pr => {
            let log_pattern = &args[1];
            let branch_name = &args[2];
            let log_result = git_log_grep(&[log_pattern.as_str()], dir);
            let status = git_status(dir);

            let log_output = match log_result {
                Ok(s) if !s.trim().is_empty() => s,
                _ => return (None, None),
            };
            let _ = log_output;

            let parts: Vec<&str> = status.tracking_branch.split('/').collect();
            let merge_to_branch = if parts.len() == 2 {
                parts[1].to_string()
            } else {
                status.tracking_branch.clone()
            };

            match create_github_pr(name, &merge_to_branch, branch_name, dir, token) {
                Ok(url) => (None, Some(url)),
                Err(e) => {
                    eprintln!("error creating PR for {}: {}", name, e);
                    (None, None)
                }
            }
        }
        Action::Checkout {
            new_branch,
            alias_map,
            raw_target,
        } => {
            let resolved_target: Option<String> = match alias_map {
                Some(map) => map.get(name).cloned(),
                None => {
                    if raw_target.is_empty() {
                        None
                    } else {
                        Some(raw_target.clone())
                    }
                }
            };

            let mut output = String::new();
            let mut err: Option<String> = None;

            if alias_map.is_some() && resolved_target.is_none() {
                err = Some(format!("no branch alias entry for {}", name));
            } else {
                let mut argv: Vec<&str> = vec!["checkout"];
                if let Some(nb) = new_branch.as_deref() {
                    argv.push("-b");
                    argv.push(nb);
                }
                if let Some(t) = resolved_target.as_deref() {
                    argv.push(t);
                }
                match git_exec(&argv, dir) {
                    Ok(o) => output = o,
                    Err(e) => err = Some(e),
                }
            }

            let status = git_status(dir);
            let (tags, remote_branches) = get_head_remote_state(dir);
            (
                Some(ProjectInfo {
                    name: name.to_string(),
                    dir: dir.to_string(),
                    repo_status: status,
                    cmd_output: output,
                    cmd_error: err,
                    tags,
                    remote_branches,
                }),
                None,
            )
        }
    }
}

fn create_github_pr(
    project: &str,
    merge_to_branch: &str,
    branch_name: &str,
    dir: &str,
    token: &str,
) -> Result<String, String> {
    println!("{} - {}:", project, dir);
    let remote_arg = format!("HEAD:{}", branch_name);
    cmd("git", vec!["push", "origin", remote_arg.as_str()])
        .dir(dir)
        .stdout_capture()
        .stderr_capture()
        .run()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "https://api.github.com/repos/stackpath/{}/pulls",
        project
    );
    let body = json!({
        "title": branch_name,
        "head": branch_name,
        "base": merge_to_branch,
        "body": "",
        "maintainer_can_modify": true,
    });

    let resp = ureq::post(&url)
        .set("Authorization", &format!("token {}", token))
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", "git-super")
        .send_json(body)
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let html_url = json["html_url"].as_str().unwrap_or("").to_string();
    println!("PR created: {}", html_url);
    Ok(html_url)
}

fn output_status(project_map: &ProjectInfoMap) {
    let re = Regex::new(r"ahead (?P<ahead>\d+)").unwrap();
    for (tracking_branch, projects) in project_map {
        println!("{}", tracking_branch.white().bold());
        for project in projects {
            let header = format!("    {:<20}", project.name);
            let dirty = !project.repo_status.modified_files.is_empty();
            let diverged = !project.repo_status.status.is_empty();
            let header_colored = if dirty {
                header.yellow()
            } else if diverged {
                header.green()
            } else {
                header.white().dimmed()
            };
            print!("{}", header_colored);

            let body = format!(
                "{:>10} {:>15}{:>20} {:<20}",
                project.repo_status.current_branch,
                project.repo_status.status,
                project.tags.join(","),
                project.remote_branches.join(","),
            );
            let body_colored = if dirty || diverged {
                body.white()
            } else {
                body.white().dimmed()
            };
            println!("{}", body_colored);

            if let Some(captures) = re.captures(&project.repo_status.status) {
                let ahead = &captures["ahead"];
                let n_arg = format!("-n{}", ahead);
                match git_exec(&["log", "--oneline", &n_arg], &project.dir) {
                    Ok(output) => {
                        for line in output.split('\n') {
                            println!("{}", format!("        {}", line).bright_blue());
                        }
                    }
                    Err(e) => {
                        println!("{}", e.red());
                    }
                }
            }

            for f in &project.repo_status.modified_files {
                let line = format!("        {}", f);
                let cl = if dirty || diverged {
                    line.white()
                } else {
                    line.white().dimmed()
                };
                println!("{}", cl);
            }
        }
    }
}

fn output_generic(project_map: &ProjectInfoMap) {
    for (tracking_branch, projects) in project_map {
        println!("{}", tracking_branch.white().bold());
        for project in projects {
            let header = format!("    {:<20}", project.name);
            let dirty = !project.repo_status.modified_files.is_empty();
            let diverged = !project.repo_status.status.is_empty();
            let header_colored = if dirty {
                header.yellow()
            } else if diverged {
                header.green()
            } else {
                header.white().dimmed()
            };
            print!("{}", header_colored);

            let body = format!(
                "{:>10} {}",
                project.repo_status.current_branch, project.repo_status.status
            );
            let body_colored = if dirty || diverged {
                body.white()
            } else {
                body.white().dimmed()
            };
            println!("{}", body_colored);

            for f in &project.repo_status.modified_files {
                let line = format!("        {}", f);
                let cl = if dirty || diverged {
                    line.white()
                } else {
                    line.white().dimmed()
                };
                println!("{}", cl);
            }

            if !project.cmd_output.is_empty() {
                let is_err = project.cmd_error.is_some();
                for line in project.cmd_output.split('\n') {
                    if line.is_empty() {
                        continue;
                    }
                    let pretty = format!("        {}", line);
                    if is_err {
                        println!("{}", pretty.red().bold());
                    } else if dirty || diverged {
                        println!("{}", pretty.white());
                    } else {
                        println!("{}", pretty.white().dimmed());
                    }
                }
            } else if let Some(err) = &project.cmd_error {
                for line in err.split('\n') {
                    if line.is_empty() {
                        continue;
                    }
                    println!("{}", format!("        {}", line).red().bold());
                }
            }
        }
    }
}

fn discover_git_repos() -> Vec<PathBuf> {
    let mut repos = Vec::new();
    let mut iter = WalkDir::new(".").into_iter();
    loop {
        let entry = match iter.next() {
            None => break,
            Some(Err(_)) => continue,
            Some(Ok(e)) => e,
        };
        if entry.file_type().is_dir() && entry.file_name() == ".git" {
            if let Some(parent) = entry.path().parent() {
                let p = parent.to_path_buf();
                if !p.as_os_str().is_empty() {
                    repos.push(p);
                }
            }
            iter.skip_current_dir();
        }
    }
    repos
}

fn discover(ini: &mut Ini) -> bool {
    let repos = discover_git_repos();
    for repo in repos {
        let name = repo
            .strip_prefix(".")
            .unwrap_or(&repo)
            .to_string_lossy()
            .into_owned();
        let name = name.trim_start_matches('/').to_string();
        if name.is_empty() {
            continue;
        }
        let already = ini
            .section(Some("subprojects"))
            .and_then(|s| s.get(&name))
            .is_some();
        if !already {
            ini.with_section(Some("subprojects"))
                .set(name.clone(), format!("./{}", name));
        }
    }
    true
}

fn write_default_config(path: &str) -> std::io::Result<()> {
    let contents = "[subprojects]\n[commands]\nstatus   = yes\nfetch    = yes\npull     = yes\nlog      = yes\ngrep     = yes\ncommit   = yes\ncheckout = yes\n";
    std::fs::write(path, contents)
}

fn usage_and_exit(prog: &str) -> ! {
    eprintln!("usage: {} <cmd> ...", prog);
    process::exit(1);
}

fn main() {
    let config_file = ".git-super";
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        usage_and_exit(&args[0]);
    }

    let git_cmd: Vec<String> = args[1..].to_vec();

    if !std::path::Path::new(config_file).exists() {
        if git_cmd[0] == "discover" {
            if let Err(e) = write_default_config(config_file) {
                eprintln!("error: {}", e);
            }
        } else {
            eprintln!(
                "error: {} not found. Please run 'git super discover' to create it",
                config_file
            );
            process::exit(1);
        }
    }

    let mut config = match Ini::load_from_file(config_file) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("error: failed to load {} ini file", config_file);
            process::exit(1);
        }
    };

    if git_cmd[0] == "discover" {
        if discover(&mut config) {
            let bak = format!("{}.bak", config_file);
            if let Err(e) = std::fs::rename(config_file, &bak) {
                eprintln!("{}", e);
                process::exit(1);
            }
            if let Err(_) = config.write_to_file(config_file) {
                eprintln!("error: Failed to save ini file {}", config_file);
                let _ = std::fs::rename(&bak, config_file);
                process::exit(1);
            }
        }
        process::exit(0);
    }

    let supported_cmds: Vec<String> = config
        .section(Some("commands"))
        .map(|s| s.iter().map(|(k, _)| k.to_string()).collect())
        .unwrap_or_default();

    let cmd_supported = supported_cmds.iter().any(|c| c == &git_cmd[0]);
    if !cmd_supported {
        eprintln!("error: {} command is not supported", git_cmd[0]);
        process::exit(1);
    }

    let github_token = config
        .section(Some("github"))
        .and_then(|s| s.get("token"))
        .unwrap_or("")
        .to_string();

    match git_cmd[0].as_str() {
        "grep" => {
            if git_cmd.len() == 1 {
                eprintln!("error: {} command requires more arguments", git_cmd[0]);
                process::exit(1);
            }
        }
        "pr" => {
            if git_cmd.len() != 3 {
                eprintln!("usage: git super pr <log_pattern> <pr_branch_name>");
                process::exit(1);
            }
            if github_token.is_empty() {
                eprintln!("error: github token is not set in .git-super");
                process::exit(1);
            }
        }
        _ => {}
    }

    let action = match git_cmd[0].as_str() {
        "status" => Action::Status,
        "grep" => Action::Grep,
        "pr" => Action::Pr,
        "checkout" => {
            let mut new_branch: Option<String> = None;
            let mut target: Option<String> = None;
            let mut i = 1;
            while i < git_cmd.len() {
                if git_cmd[i] == "-b" {
                    if i + 1 >= git_cmd.len() {
                        eprintln!("usage: git super checkout [-b <new_branch>] [<target>]");
                        process::exit(1);
                    }
                    new_branch = Some(git_cmd[i + 1].clone());
                    i += 2;
                } else {
                    if target.is_some() {
                        eprintln!("usage: git super checkout [-b <new_branch>] [<target>]");
                        process::exit(1);
                    }
                    target = Some(git_cmd[i].clone());
                    i += 1;
                }
            }
            if new_branch.is_none() && target.is_none() {
                eprintln!("usage: git super checkout [-b <new_branch>] [<target>]");
                process::exit(1);
            }
            let alias_map = target.as_deref().and_then(|t| resolve_alias(&config, t));
            Action::Checkout {
                new_branch,
                alias_map,
                raw_target: target.unwrap_or_default(),
            }
        }
        _ => Action::Generic,
    };

    let mut sorted_projects: Vec<(String, String)> = config
        .section(Some("subprojects"))
        .map(|s| {
            s.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default();
    sorted_projects.sort_by(|a, b| a.0.cmp(&b.0));

    let results: Vec<(Option<ProjectInfo>, Option<String>)> = sorted_projects
        .par_iter()
        .map(|(name, dir)| process_project(name, dir, &action, &git_cmd, &github_token))
        .collect();

    let mut project_map: ProjectInfoMap = BTreeMap::new();
    let mut pr_urls: Vec<String> = Vec::new();
    for (info, url) in results {
        if let Some(info) = info {
            project_map
                .entry(info.repo_status.tracking_branch.clone())
                .or_insert_with(Vec::new)
                .push(info);
        }
        if let Some(u) = url {
            pr_urls.push(u);
        }
    }

    for v in project_map.values_mut() {
        v.sort_by(|a, b| a.name.cmp(&b.name));
    }

    match git_cmd[0].as_str() {
        "status" => output_status(&project_map),
        "pr" => {
            for url in &pr_urls {
                println!("{}", url);
            }
        }
        _ => output_generic(&project_map),
    }
}
