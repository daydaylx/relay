//! The configuration source tree: which files belong to it, a content identity for drift
//! detection, and isolated copies used as candidates.

use std::fs;
use std::os::unix::fs::{MetadataExt, symlink};
use std::path::{Component, Path, PathBuf};

use crate::exec::{Invocation, Runner};
use crate::fsutil::{create_private_dir, sha256_file};
use crate::sha256::{Sha256, hex};

/// The only file Relay may write inside the configuration source.
pub const MANAGED_RELATIVE_PATH: &str = "relay/managed.nix";

#[derive(Clone, Debug)]
pub struct SourceTree {
    root: PathBuf,
    git: bool,
    files: Vec<String>,
}

impl SourceTree {
    /// List the files Nix would see: tracked files for a git checkout (`git ls-files`), otherwise
    /// everything except `.git` and `result*` build links.
    pub fn scan(root: &Path, runner: &dyn Runner) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("flake directory is not readable: {error}"))?;
        if !root.is_dir() {
            return Err("flake path must be a directory".into());
        }
        let git = root.join(".git").exists();
        let mut files = if git {
            list_tracked(&root, runner)?
        } else {
            let mut found = Vec::new();
            walk(&root, &root, &mut found)?;
            found
        };
        files.retain(|relative| fs::symlink_metadata(root.join(relative)).is_ok());
        files.sort();
        files.dedup();
        Ok(Self { root, git, files })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_git_checkout(&self) -> bool {
        self.git
    }

    pub fn contains(&self, relative: &str) -> bool {
        self.files.binary_search(&relative.to_owned()).is_ok()
    }

    /// Identity of the tree: SHA-256 over `(kind, path, mode bit, content hash | link target)`.
    pub fn hash(&self) -> Result<String, String> {
        let mut hasher = Sha256::new();
        for relative in &self.files {
            let path = self.root.join(relative);
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("could not read {relative}: {error}"))?;
            let record = if metadata.file_type().is_symlink() {
                let target = fs::read_link(&path)
                    .map_err(|error| format!("could not read link {relative}: {error}"))?;
                format!("L\0{relative}\0{}\n", target.to_string_lossy())
            } else if metadata.is_file() {
                let executable = u8::from(metadata.mode() & 0o111 != 0);
                format!("F\0{relative}\0{executable}\0{}\n", sha256_file(&path)?)
            } else {
                continue;
            };
            hasher.update(record.as_bytes());
        }
        Ok(hex(&hasher.finalize()))
    }

    /// Copy the tree into the new directory `destination` (which must not exist yet).
    pub fn copy_to(&self, destination: &Path) -> Result<(), String> {
        if destination.exists() {
            return Err("candidate directory already exists".into());
        }
        create_private_dir(destination)?;
        for relative in &self.files {
            let from = self.root.join(relative);
            let to = destination.join(relative);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("could not create candidate directory: {error}"))?;
            }
            let metadata = fs::symlink_metadata(&from)
                .map_err(|error| format!("could not read {relative}: {error}"))?;
            if metadata.file_type().is_symlink() {
                let target = fs::read_link(&from)
                    .map_err(|error| format!("could not read link {relative}: {error}"))?;
                symlink(target, &to)
                    .map_err(|error| format!("could not copy link {relative}: {error}"))?;
            } else if metadata.is_file() {
                fs::copy(&from, &to)
                    .map_err(|error| format!("could not copy {relative}: {error}"))?;
            }
        }
        Ok(())
    }
}

fn list_tracked(root: &Path, runner: &dyn Runner) -> Result<Vec<String>, String> {
    let outcome = runner.run(
        &Invocation::new("git")
            .arg("-C")
            .arg(root.to_string_lossy())
            .args(["ls-files", "--cached", "-z"]),
    )?;
    if !outcome.success() {
        return Err("could not list the tracked files of the flake with git".into());
    }
    outcome
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let relative = String::from_utf8(entry.to_vec())
                .map_err(|_| "tracked file name is not UTF-8".to_owned())?;
            let safe = Path::new(&relative)
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
            if safe {
                Ok(relative)
            } else {
                Err(format!("unsafe tracked path '{relative}'"))
            }
        })
        .collect()
}

fn walk(root: &Path, directory: &Path, found: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read directory entry: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == "result" || name.starts_with("result-") {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not read file type of {name}: {error}"))?;
        let path = entry.path();
        if file_type.is_dir() {
            walk(root, &path, found)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            let relative = relative
                .to_str()
                .ok_or_else(|| "file name is not UTF-8".to_owned())?;
            found.push(relative.to_owned());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SourceTree;
    use crate::exec::{Invocation, Outcome, ProcessRunner, Runner};
    use crate::fsutil::testutil::TempDir;
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    fn write(dir: &TempDir, relative: &str, contents: &str) {
        let path = dir.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn hash(dir: &TempDir) -> String {
        SourceTree::scan(dir.path(), &ProcessRunner::default())
            .unwrap()
            .hash()
            .unwrap()
    }

    #[test]
    fn hash_changes_with_content_mode_and_link_targets_but_not_with_build_links() {
        let dir = TempDir::new("source");
        write(&dir, "flake.nix", "{}");
        write(&dir, "relay/managed.nix", "a");
        let base = hash(&dir);
        assert_eq!(base, hash(&dir));

        symlink("/nix/store/some-result", dir.path().join("result")).unwrap();
        assert_eq!(base, hash(&dir), "build result links are not source");

        write(&dir, "relay/managed.nix", "b");
        let changed = hash(&dir);
        assert_ne!(base, changed);
        write(&dir, "relay/managed.nix", "a");
        assert_eq!(base, hash(&dir));

        fs::set_permissions(
            dir.path().join("flake.nix"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert_ne!(base, hash(&dir));
        fs::set_permissions(
            dir.path().join("flake.nix"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();

        symlink("a", dir.path().join("link")).unwrap();
        let with_link = hash(&dir);
        fs::remove_file(dir.path().join("link")).unwrap();
        symlink("b", dir.path().join("link")).unwrap();
        assert_ne!(with_link, hash(&dir));
    }

    #[test]
    fn path_names_are_part_of_the_identity() {
        let one = TempDir::new("source");
        write(&one, "a", "x");
        let two = TempDir::new("source");
        write(&two, "b", "x");
        assert_ne!(hash(&one), hash(&two));
    }

    #[test]
    fn copy_is_an_independent_identical_tree() {
        let dir = TempDir::new("source");
        write(&dir, "flake.nix", "{}");
        write(&dir, "hosts/a.nix", "x");
        symlink("flake.nix", dir.path().join("alias")).unwrap();
        let tree = SourceTree::scan(dir.path(), &ProcessRunner::default()).unwrap();
        let target = TempDir::new("candidate");
        let destination = target.path().join("src");
        tree.copy_to(&destination).unwrap();
        assert_eq!(
            SourceTree::scan(&destination, &ProcessRunner::default())
                .unwrap()
                .hash()
                .unwrap(),
            tree.hash().unwrap()
        );
        fs::write(destination.join("flake.nix"), "changed").unwrap();
        assert_eq!(
            fs::read_to_string(dir.path().join("flake.nix")).unwrap(),
            "{}"
        );
        assert!(
            tree.copy_to(&destination).is_err(),
            "never overwrite a candidate"
        );
    }

    struct GitStub(&'static [u8]);

    impl Runner for GitStub {
        fn run(&self, invocation: &Invocation) -> Result<Outcome, String> {
            assert_eq!(invocation.program(), "git");
            Ok(Outcome {
                code: Some(0),
                stdout: self.0.to_vec(),
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn git_checkouts_only_contain_tracked_files() {
        let dir = TempDir::new("source");
        fs::create_dir(dir.path().join(".git")).unwrap();
        write(&dir, "flake.nix", "{}");
        write(&dir, "relay/managed.nix", "m");
        write(&dir, "untracked.txt", "u");
        write(&dir, ".gitignore", "i");
        let tree = SourceTree::scan(
            dir.path(),
            &GitStub(b"flake.nix\0.gitignore\0deleted.nix\0"),
        )
        .unwrap();
        assert!(tree.is_git_checkout());
        assert!(tree.contains("flake.nix"));
        assert!(!tree.contains("untracked.txt"));
        assert!(
            !tree.contains("relay/managed.nix"),
            "untracked managed.nix is invisible to Nix"
        );
        assert!(
            !tree.contains("deleted.nix"),
            "files missing on disk are skipped"
        );
    }

    #[test]
    fn unsafe_tracked_paths_are_rejected() {
        let dir = TempDir::new("source");
        fs::create_dir(dir.path().join(".git")).unwrap();
        for bad in [&b"../outside\0"[..], b"/etc/passwd\0"] {
            let result = SourceTree::scan(dir.path(), &GitStub(bad));
            assert!(result.unwrap_err().contains("unsafe tracked path"));
        }
    }
}
