use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The root of the git repo containing `cwd`, shared by all of its linked worktrees. Reads `.git`
/// directly instead of spawning `git`, since this runs for every distinct cwd in the list.
fn repo_root(cwd: &Path) -> Option<PathBuf> {
    let (dot_git, is_dir) = cwd.ancestors().find_map(|dir| {
        let dot_git = dir.join(".git");
        let metadata = fs::metadata(&dot_git).ok()?;
        (metadata.is_dir() || metadata.is_file()).then(|| (dot_git, metadata.is_dir()))
    })?;

    let common_dir = if is_dir {
        dot_git
    } else {
        let gitdir = gitdir_of(&dot_git)?;
        match fs::read_to_string(gitdir.join("commondir")) {
            Ok(contents) => gitdir.join(contents.trim()),
            Err(_) => gitdir,
        }
    };
    let common_dir = fs::canonicalize(common_dir).ok()?;

    if common_dir.file_name()? == ".git" {
        Some(common_dir.parent()?.to_path_buf())
    } else {
        Some(common_dir)
    }
}

/// The directory a `.git` file of the form `gitdir: <path>` points to.
fn gitdir_of(dot_git_file: &Path) -> Option<PathBuf> {
    let contents = fs::read_to_string(dot_git_file).ok()?;
    let target = contents.trim().strip_prefix("gitdir:")?.trim();
    let target = Path::new(target);
    if target.is_absolute() {
        Some(target.to_path_buf())
    } else {
        Some(dot_git_file.parent()?.join(target))
    }
}

/// Memoised repo roots per cwd. Never invalidated: a cwd that becomes (or stops being) a repo
/// while the app runs keeps its first answer.
#[derive(Default)]
pub struct RepoRoots {
    roots: HashMap<String, Option<String>>,
}

impl RepoRoots {
    pub fn resolve_all<'a>(&mut self, cwds: impl IntoIterator<Item = &'a str>) {
        for cwd in cwds {
            if !self.roots.contains_key(cwd) {
                let root =
                    repo_root(Path::new(cwd)).and_then(|root| root.to_str().map(String::from));
                self.roots.insert(cwd.to_string(), root);
            }
        }
    }

    /// The repo root of `cwd` when it is inside one, else `cwd` itself.
    pub fn group_path<'a>(&'a self, cwd: &'a str) -> &'a str {
        match self.roots.get(cwd) {
            Some(Some(root)) => root,
            _ => cwd,
        }
    }

    #[cfg(test)]
    pub fn insert(&mut self, cwd: &str, root: &str) {
        self.roots.insert(cwd.to_string(), Some(root.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "agent-dash-repo-test-{}-{name}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            // Canonical so expectations match the canonicalised roots (macOS /tmp is a symlink).
            Self(fs::canonicalize(path).unwrap())
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn subdir_of_a_plain_repo_resolves_to_the_repo() {
        let dir = TempDir::new("plain");
        let repo = dir.0.join("app");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("src/deep")).unwrap();

        assert_eq!(repo_root(&repo.join("src/deep")), Some(repo.clone()));
        assert_eq!(repo_root(&repo), Some(repo));
    }

    #[test]
    fn linked_worktree_resolves_to_the_main_repo() {
        let dir = TempDir::new("worktree");
        let repo = dir.0.join("app");
        let worktree_gitdir = repo.join(".git/worktrees/feature");
        fs::create_dir_all(&worktree_gitdir).unwrap();
        fs::write(worktree_gitdir.join("commondir"), "../..\n").unwrap();

        let worktree = dir.0.join("wt/feature");
        fs::create_dir_all(worktree.join("src")).unwrap();
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", worktree_gitdir.display()),
        )
        .unwrap();

        assert_eq!(repo_root(&worktree.join("src")), Some(repo));
    }

    #[test]
    fn relative_gitdir_resolves_against_the_git_file() {
        let dir = TempDir::new("relative");
        let repo = dir.0.join("app");
        fs::create_dir_all(repo.join(".git/worktrees/feature")).unwrap();
        fs::write(repo.join(".git/worktrees/feature/commondir"), "../..").unwrap();

        let worktree = dir.0.join("feature");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(
            worktree.join(".git"),
            "gitdir: ../app/.git/worktrees/feature",
        )
        .unwrap();

        assert_eq!(repo_root(&worktree), Some(repo));
    }

    #[test]
    fn bare_repo_resolves_to_itself() {
        let dir = TempDir::new("bare");
        let bare = dir.0.join("app.git");
        let worktree_gitdir = bare.join("worktrees/feature");
        fs::create_dir_all(&worktree_gitdir).unwrap();
        fs::write(worktree_gitdir.join("commondir"), "../..").unwrap();

        let worktree = dir.0.join("feature");
        fs::create_dir_all(&worktree).unwrap();
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}", worktree_gitdir.display()),
        )
        .unwrap();

        assert_eq!(repo_root(&worktree), Some(bare));
    }

    #[test]
    fn directory_outside_any_repo_has_no_root() {
        let dir = TempDir::new("none");
        let plain = dir.0.join("plain");
        fs::create_dir_all(&plain).unwrap();

        assert_eq!(repo_root(&plain), None);
        assert_eq!(repo_root(&dir.0.join("missing")), None);
    }

    #[test]
    fn group_path_falls_back_to_the_cwd_without_a_root() {
        let dir = TempDir::new("group-path");
        let repo = dir.0.join("app");
        fs::create_dir_all(repo.join(".git")).unwrap();
        let inside = repo.to_str().unwrap().to_string();
        let outside = dir.0.to_str().unwrap().to_string();

        let mut roots = RepoRoots::default();
        assert_eq!(roots.group_path(&inside), inside);
        roots.resolve_all([inside.as_str(), outside.as_str()]);
        assert_eq!(roots.group_path(&inside), inside);
        assert_eq!(roots.group_path(&outside), outside);
    }
}
