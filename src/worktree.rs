use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use tokio::process::Command;

use crate::config::WorktreeCommands;
use crate::sessions::shell_quote;

/// What the New session dialog asked for. The variants differ in what has to run first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorktreeRequest {
    /// A new branch off `base`, via `worktree.create`. A blank `name` means "pick one".
    New { name: String, base: String },
    /// An existing branch that has no worktree yet, via `worktree.checkout`.
    Checkout { branch: String },
    /// An existing branch whose worktree is already at `path`; nothing to run.
    Existing { branch: String, path: String },
}

impl WorktreeRequest {
    /// Names the toast and the tmux session.
    pub fn label(&self) -> &str {
        match self {
            WorktreeRequest::New { name, .. } => name,
            WorktreeRequest::Checkout { branch } | WorktreeRequest::Existing { branch, .. } => {
                branch
            }
        }
    }
}

/// What the dialog needs to know about the repository at a folder.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    /// The checked-out branch (`HEAD` when detached).
    pub current: String,
    /// Local branch names, most recently committed first.
    pub local: Vec<String>,
    /// Remote-tracking branches as `remote/branch`, most recently committed first.
    pub remote: Vec<String>,
    /// Local branch to the path of the worktree it is checked out in.
    pub worktrees: HashMap<String, String>,
}

/// A random `adjective-animal` worktree name.
pub fn random_name() -> String {
    petname::petname(2, "-").expect("petname word list is empty")
}

/// Expands `{{var}}` (inner whitespace allowed) for the given variables, shell-quoted. An
/// unterminated `{{` stays literal text; a complete `{{...}}` naming anything else is an error.
pub fn render_template(template: &str, vars: &[(&str, &str)]) -> Result<String> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find("{{") {
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("}}") else {
            break;
        };
        rendered.push_str(&rest[..open]);
        let placeholder = after_open[..close].trim();
        let Some((_, value)) = vars.iter().find(|(name, _)| *name == placeholder) else {
            bail!("unknown placeholder {{{{{}}}}}", &after_open[..close]);
        };
        rendered.push_str(&shell_quote(value));
        rest = &after_open[close + 2..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}

/// Every `(branch, path)` pair of a `git worktree list --porcelain` listing; detached
/// worktrees have no branch and are left out.
pub fn parse_worktrees(porcelain: &str) -> Vec<(String, String)> {
    let mut path = None;
    let mut worktrees = Vec::new();
    for line in porcelain.lines() {
        if let Some(listed) = line.strip_prefix("worktree ") {
            path = Some(listed);
        } else if let Some(branch) = line.strip_prefix("branch refs/heads/") {
            if let Some(path) = path {
                worktrees.push((branch.to_string(), path.to_string()));
            }
        }
    }
    worktrees
}

/// The path of the worktree checked out on `refs/heads/<branch>`.
pub fn find_worktree_path(porcelain: &str, branch: &str) -> Option<String> {
    parse_worktrees(porcelain)
        .into_iter()
        .find(|(listed, _)| listed == branch)
        .map(|(_, path)| path)
}

/// Splits `git for-each-ref --format=%(refname)` output into local and remote branches,
/// keeping the order and dropping `<remote>/HEAD`.
pub fn parse_refs(output: &str) -> (Vec<String>, Vec<String>) {
    let mut local = Vec::new();
    let mut remote = Vec::new();
    for line in output.lines() {
        if let Some(branch) = line.strip_prefix("refs/heads/") {
            local.push(branch.to_string());
        } else if let Some(branch) = line.strip_prefix("refs/remotes/") {
            if !branch.ends_with("/HEAD") {
                remote.push(branch.to_string());
            }
        }
    }
    (local, remote)
}

/// Gets the worktree for `request` in place, running the matching `commands` entry in `folder`
/// when there isn't one yet. Returns the worktree's path.
pub async fn run(
    commands: &WorktreeCommands,
    folder: &str,
    request: &WorktreeRequest,
) -> Result<String> {
    let (rendered, branch) = match request {
        WorktreeRequest::Existing { path, .. } => return Ok(path.clone()),
        WorktreeRequest::New { name, base } => (
            render_template(&commands.create, &[("name", name), ("base", base)])
                .context("worktree.create")?,
            name,
        ),
        WorktreeRequest::Checkout { branch } => {
            let template = commands
                .checkout
                .as_ref()
                .context("no worktree.checkout configured")?;
            (
                render_template(template, &[("name", branch)]).context("worktree.checkout")?,
                branch,
            )
        }
    };
    let output = Command::new("sh")
        .arg("-c")
        .arg(&rendered)
        .current_dir(folder)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .context("failed to run the worktree command")?;
    if !output.status.success() {
        bail!(
            "worktree command failed: {}",
            last_non_empty_line(&output.stderr).unwrap_or_else(|| output.status.to_string())
        );
    }

    let listing = Command::new("git")
        .args(["-C", folder, "worktree", "list", "--porcelain"])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .context("failed to run git worktree list")?;
    if !listing.status.success() {
        bail!(
            "git worktree list failed: {}",
            last_non_empty_line(&listing.stderr).unwrap_or_else(|| listing.status.to_string())
        );
    }
    find_worktree_path(&String::from_utf8_lossy(&listing.stdout), branch)
        .with_context(|| format!("worktree command finished but no worktree for branch {branch}"))
}

/// Branches, worktrees and the current branch of the repository at `folder`.
pub fn load_repo_info(folder: &str) -> Result<RepoInfo> {
    let current = git_output(folder, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    let (local, remote) = parse_refs(&git_output(
        folder,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname)",
            "refs/heads",
            "refs/remotes",
        ],
    )?);
    let worktrees = parse_worktrees(&git_output(folder, &["worktree", "list", "--porcelain"])?)
        .into_iter()
        .collect();
    Ok(RepoInfo {
        current,
        local,
        remote,
        worktrees,
    })
}

fn git_output(folder: &str, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["-C", folder])
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!(
            "{}",
            last_non_empty_line(&output.stderr).unwrap_or_else(|| output.status.to_string())
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn last_non_empty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_request(name: &str, base: &str) -> WorktreeRequest {
        WorktreeRequest::New {
            name: name.to_string(),
            base: base.to_string(),
        }
    }

    fn render_create(template: &str, name: &str, base: &str) -> Result<String> {
        render_template(template, &[("name", name), ("base", base)])
    }

    fn commands(create: &str, checkout: Option<&str>) -> WorktreeCommands {
        WorktreeCommands {
            create: create.to_string(),
            checkout: checkout.map(str::to_string),
        }
    }

    #[test]
    fn random_name_is_two_lowercase_words() {
        for _ in 0..50 {
            let name = random_name();
            let words: Vec<&str> = name.split('-').collect();
            assert_eq!(words.len(), 2, "{name}");
            assert!(words
                .iter()
                .all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase())));
        }
    }

    #[test]
    fn substitutes_name_and_base_quoted() {
        let rendered = render_create(
            "git worktree add ../{{name}} -b {{name}} {{base}}",
            "feat",
            "main",
        )
        .unwrap();
        assert_eq!(rendered, "git worktree add ../'feat' -b 'feat' 'main'");
    }

    #[test]
    fn allows_whitespace_inside_the_braces() {
        let rendered = render_create("wt {{ name }} {{base }} {{\tname\t}}", "a", "b").unwrap();
        assert_eq!(rendered, "wt 'a' 'b' 'a'");
    }

    #[test]
    fn quotes_shell_metacharacters_and_single_quotes() {
        let rendered = render_create("echo {{name}}", "a'; rm -rf ~ #", "b").unwrap();
        assert_eq!(rendered, r#"echo 'a'\''; rm -rf ~ #'"#);
    }

    #[test]
    fn does_not_expand_placeholders_inside_substituted_values() {
        let rendered = render_create("{{name}} {{base}}", "{{base}}", "x").unwrap();
        assert_eq!(rendered, "'{{base}}' 'x'");
    }

    #[test]
    fn unknown_placeholder_is_an_error_naming_it() {
        let err = render_create("wt {{branch}}", "a", "b").unwrap_err();
        assert!(format!("{err:#}").contains("{{branch}}"), "{err:#}");
    }

    #[test]
    fn base_is_an_unknown_placeholder_when_only_name_is_offered() {
        assert_eq!(
            render_template("wt {{name}}", &[("name", "a")]).unwrap(),
            "wt 'a'"
        );
        let err = render_template("wt {{name}} {{ base }}", &[("name", "a")]).unwrap_err();
        assert!(format!("{err:#}").contains("{{ base }}"), "{err:#}");
    }

    #[test]
    fn template_without_placeholders_is_unchanged() {
        assert_eq!(
            render_create("wt switch --create", "a", "b").unwrap(),
            "wt switch --create"
        );
    }

    #[test]
    fn unterminated_braces_stay_literal() {
        assert_eq!(
            render_create("echo {{name}} {{", "a", "b").unwrap(),
            "echo 'a' {{"
        );
    }

    const PORCELAIN: &str = "\
worktree /repo
HEAD 1111111111111111111111111111111111111111
branch refs/heads/main

worktree /repo.feat
HEAD 2222222222222222222222222222222222222222
branch refs/heads/feat

worktree /repo.detached
HEAD 3333333333333333333333333333333333333333
detached

worktree /repo.feat-two
HEAD 4444444444444444444444444444444444444444
branch refs/heads/feat-two
";

    #[test]
    fn finds_the_worktree_for_a_branch() {
        assert_eq!(
            find_worktree_path(PORCELAIN, "feat").as_deref(),
            Some("/repo.feat")
        );
        assert_eq!(
            find_worktree_path(PORCELAIN, "main").as_deref(),
            Some("/repo")
        );
        assert_eq!(
            find_worktree_path(PORCELAIN, "feat-two").as_deref(),
            Some("/repo.feat-two")
        );
    }

    #[test]
    fn branch_names_match_whole_refs_only() {
        assert_eq!(find_worktree_path(PORCELAIN, "fea"), None);
        assert_eq!(find_worktree_path(PORCELAIN, "missing"), None);
        assert_eq!(find_worktree_path("", "main"), None);
    }

    #[test]
    fn parse_worktrees_lists_branches_and_skips_detached_ones() {
        assert_eq!(
            parse_worktrees(PORCELAIN),
            vec![
                ("main".to_string(), "/repo".to_string()),
                ("feat".to_string(), "/repo.feat".to_string()),
                ("feat-two".to_string(), "/repo.feat-two".to_string()),
            ]
        );
    }

    #[test]
    fn parse_refs_splits_local_and_remote_and_drops_remote_head() {
        let output = "\
refs/heads/main
refs/remotes/origin/HEAD
refs/remotes/origin/main
refs/heads/feat/x
refs/remotes/upstream/dev
";
        let (local, remote) = parse_refs(output);
        assert_eq!(local, vec!["main", "feat/x"]);
        assert_eq!(remote, vec!["origin/main", "upstream/dev"]);
    }

    #[test]
    fn last_non_empty_line_skips_trailing_blanks() {
        assert_eq!(
            last_non_empty_line(b"first\nfatal: boom \n\n  \n").as_deref(),
            Some("fatal: boom")
        );
        assert_eq!(last_non_empty_line(b"\n \n"), None);
    }

    #[test]
    fn labels_come_from_the_name_or_branch() {
        assert_eq!(new_request("a", "b").label(), "a");
        assert_eq!(
            WorktreeRequest::Checkout {
                branch: "c".to_string()
            }
            .label(),
            "c"
        );
        assert_eq!(
            WorktreeRequest::Existing {
                branch: "d".to_string(),
                path: "/p".to_string()
            }
            .label(),
            "d"
        );
    }

    /// A repository with one empty commit on `main`, in a fresh directory.
    fn init_repo(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agent-dash-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        repo
    }

    fn git(repo: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args([
                "-c",
                "commit.gpgsign=false",
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
            ])
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap();
        assert!(status.success());
    }

    fn path_str(path: &std::path::Path) -> &str {
        path.to_str().unwrap()
    }

    #[tokio::test]
    async fn new_runs_create_and_finds_the_worktree() {
        let repo = init_repo("run-new");
        let created = run(
            &commands("git worktree add ../{{name}} -b {{name}} {{base}}", None),
            path_str(&repo),
            &new_request("feat", "main"),
        )
        .await
        .unwrap();
        let expected = repo.parent().unwrap().join("feat").canonicalize().unwrap();
        assert_eq!(
            std::path::Path::new(&created).canonicalize().unwrap(),
            expected
        );
        let info = load_repo_info(&created).unwrap();
        assert_eq!(info.current, "feat");
        assert_eq!(info.local.len(), 2);
        assert!(info.worktrees.contains_key("feat") && info.worktrees.contains_key("main"));
        std::fs::remove_dir_all(repo.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn checkout_runs_the_checkout_template_for_an_existing_branch() {
        let repo = init_repo("run-checkout");
        git(&repo, &["branch", "topic"]);
        let created = run(
            &commands("false", Some("git worktree add ../{{name}} {{name}}")),
            path_str(&repo),
            &WorktreeRequest::Checkout {
                branch: "topic".to_string(),
            },
        )
        .await
        .unwrap();
        assert!(created.ends_with("/topic"), "{created}");
        std::fs::remove_dir_all(repo.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn checkout_without_a_template_is_an_error() {
        let repo = init_repo("run-no-checkout");
        let err = run(
            &commands("true", None),
            path_str(&repo),
            &WorktreeRequest::Checkout {
                branch: "topic".to_string(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.to_string(), "no worktree.checkout configured");
        std::fs::remove_dir_all(repo.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn checkout_rejects_base_in_its_template() {
        let repo = init_repo("run-checkout-base");
        let err = run(
            &commands("true", Some("git worktree add ../{{name}} {{base}}")),
            path_str(&repo),
            &WorktreeRequest::Checkout {
                branch: "topic".to_string(),
            },
        )
        .await
        .unwrap_err();
        assert!(format!("{err:#}").contains("{{base}}"), "{err:#}");
        std::fs::remove_dir_all(repo.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn existing_worktrees_run_nothing() {
        let path = run(
            &commands("exit 1", None),
            "/does/not/exist",
            &WorktreeRequest::Existing {
                branch: "x".to_string(),
                path: "/some/path".to_string(),
            },
        )
        .await
        .unwrap();
        assert_eq!(path, "/some/path");
    }

    #[tokio::test]
    async fn run_reports_the_last_stderr_line_on_failure() {
        let repo = init_repo("run-fail");
        let err = run(
            &commands(
                "echo noise >&2; echo 'fatal: nope' >&2; echo >&2; exit 3",
                None,
            ),
            path_str(&repo),
            &new_request("feat", "main"),
        )
        .await
        .unwrap_err();
        assert_eq!(err.to_string(), "worktree command failed: fatal: nope");
        std::fs::remove_dir_all(repo.parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn run_errors_when_no_worktree_has_the_branch() {
        let repo = init_repo("run-missing");
        let err = run(
            &commands("true", None),
            path_str(&repo),
            &new_request("feat", "main"),
        )
        .await
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "worktree command finished but no worktree for branch feat"
        );
        std::fs::remove_dir_all(repo.parent().unwrap()).unwrap();
    }

    #[test]
    fn load_repo_info_reads_the_current_and_local_branches() {
        let repo = init_repo("repo-info");
        git(&repo, &["branch", "older"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "newer"]);
        git(&repo, &["checkout", "-q", "-b", "newest"]);
        let info = load_repo_info(path_str(&repo)).unwrap();
        assert_eq!(info.current, "newest");
        for branch in ["main", "older", "newest"] {
            assert!(info.local.contains(&branch.to_string()), "{branch}");
        }
        assert!(info.remote.is_empty());
        std::fs::remove_dir_all(repo.parent().unwrap()).unwrap();
    }

    #[test]
    fn load_repo_info_fails_outside_a_repository() {
        let dir = std::env::temp_dir().join(format!("agent-dash-norepo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load_repo_info(path_str(&dir)).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
