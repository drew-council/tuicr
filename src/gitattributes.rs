//! Minimal `.gitattributes` reader for the attributes that decide whether a
//! file belongs in a review by default.
//!
//! Only three attributes are tracked: `linguist-generated`, its GitLab alias
//! `gitlab-generated`, and `linguist-vendored`. They are the flags GitHub and
//! GitLab use to collapse a file in a pull-request diff, so honoring them
//! gives the same "don't make me scroll past the lockfile" default here.
//!
//! Resolution follows git: `.git/info/attributes` wins, then the
//! `.gitattributes` closest to the file, walking up to the repository root.
//! Within one file the last matching line wins. Patterns use gitignore
//! syntax without negation and without the recursive-directory rule, which is
//! exactly what `Gitignore::matched` (as opposed to
//! `matched_path_or_any_parents`) implements, so the `ignore` crate does the
//! globbing. Unset (`-attr`, `!attr`, `attr=false`) lines become `!pattern`
//! whitelist globs so "last match wins" falls out of the crate's own
//! ordering.
//!
//! Nested files are loaded lazily per directory: only the ancestors of the
//! paths actually asked about are probed, so a large tree costs a handful of
//! `stat` calls rather than a walk.
//!
//! The files come from one of two places. A local review reads the working
//! tree, since that is what it shows. A pull request reads an
//! [`AttributesSnapshot`] captured at the PR's head, because the checkout on
//! disk can be on any branch and its `.gitattributes` says nothing about the
//! revision under review.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The review-relevant attributes resolved for one path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileAttributes {
    pub generated: bool,
    pub vendored: bool,
}

impl FileAttributes {
    pub fn any(self) -> bool {
        self.generated || self.vendored
    }
}

const GENERATED_ATTRS: &[&str] = &["linguist-generated", "gitlab-generated"];
const VENDORED_ATTRS: &[&str] = &["linguist-vendored"];
const TRACKED_ATTRS: &[&str] = &[
    "linguist-generated",
    "gitlab-generated",
    "linguist-vendored",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttrState {
    Set,
    Unset,
}

/// `.gitattributes` files read from one revision instead of a working tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributesSnapshot {
    /// File contents keyed by the directory holding each file, relative to
    /// the repository root (`""` for the root). Directories without a file
    /// are absent.
    pub files: HashMap<PathBuf, String>,
    /// The local clone's `.git/info/attributes`. It is per-clone
    /// configuration rather than part of any revision, so it applies to
    /// whichever revision the files came from.
    pub info: Option<String>,
}

/// One attributes file, compiled into a matcher per tracked attribute.
struct Layer {
    /// Directory the file's patterns are relative to, as a path relative to
    /// the repository root (`""` for the root itself and for
    /// `.git/info/attributes`).
    dir: PathBuf,
    matchers: Vec<(&'static str, Gitignore)>,
}

impl Layer {
    fn parse(dir: &Path, contents: &str) -> Option<Self> {
        // Lookups always pass a path already relative to `dir`, so the
        // matcher needs no filesystem root. `.` is the one root the `ignore`
        // crate never strips a prefix against.
        let mut builders: Vec<(&'static str, GitignoreBuilder)> = TRACKED_ATTRS
            .iter()
            .map(|name| (*name, GitignoreBuilder::new(".")))
            .collect();
        let mut any_line = false;

        for raw in contents.lines() {
            let Some((pattern, assignments)) = parse_line(raw) else {
                continue;
            };
            for (name, state) in assignments {
                let Some((_, builder)) = builders.iter_mut().find(|(n, _)| *n == name) else {
                    continue;
                };
                let line = match state {
                    AttrState::Set => pattern.clone(),
                    AttrState::Unset => format!("!{pattern}"),
                };
                if builder.add_line(None, &line).is_ok() {
                    any_line = true;
                }
            }
        }

        if !any_line {
            return None;
        }
        let matchers = builders
            .into_iter()
            .filter_map(|(name, builder)| builder.build().ok().map(|gi| (name, gi)))
            .collect();
        Some(Self {
            dir: dir.to_path_buf(),
            matchers,
        })
    }

    /// Resolve `attr` for `rel` (a path relative to this layer's directory).
    fn resolve(&self, attr: &str, rel: &Path) -> Option<AttrState> {
        let (_, matcher) = self.matchers.iter().find(|(name, _)| *name == attr)?;
        match matcher.matched(rel, false) {
            Match::Ignore(_) => Some(AttrState::Set),
            Match::Whitelist(_) => Some(AttrState::Unset),
            Match::None => None,
        }
    }
}

/// Split one `.gitattributes` line into its pattern and the attribute
/// assignments it carries. Returns `None` for blank lines, comments, and
/// `[attr]` macro definitions.
fn parse_line(raw: &str) -> Option<(String, Vec<(String, AttrState)>)> {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with("[attr]") {
        return None;
    }

    let (pattern, rest) = split_pattern(line)?;
    // Negative patterns are forbidden in gitattributes; the `ignore` crate
    // would read the `!` as a whitelist and invert the meaning, so drop them.
    if pattern.is_empty() || pattern.starts_with('!') {
        return None;
    }

    let assignments = rest
        .split_whitespace()
        .filter_map(|token| {
            if let Some(name) = token.strip_prefix('-') {
                Some((name.to_string(), AttrState::Unset))
            } else if let Some(name) = token.strip_prefix('!') {
                // "Unspecified" overrides lower-precedence matches just like
                // an explicit unset does; for a boolean that is the same
                // outcome.
                Some((name.to_string(), AttrState::Unset))
            } else if let Some((name, value)) = token.split_once('=') {
                let state = if value.eq_ignore_ascii_case("false") {
                    AttrState::Unset
                } else {
                    AttrState::Set
                };
                Some((name.to_string(), state))
            } else if token.is_empty() {
                None
            } else {
                Some((token.to_string(), AttrState::Set))
            }
        })
        .collect();

    Some((pattern, assignments))
}

/// Peel the pattern off the front of a line. Patterns containing whitespace
/// are double-quoted, with `\"` and `\\` escapes inside.
fn split_pattern(line: &str) -> Option<(String, &str)> {
    if let Some(quoted) = line.strip_prefix('"') {
        let mut pattern = String::new();
        let mut chars = quoted.char_indices();
        while let Some((idx, ch)) = chars.next() {
            match ch {
                '\\' => {
                    if let Some((_, escaped)) = chars.next() {
                        pattern.push(escaped);
                    }
                }
                '"' => return Some((pattern, &quoted[idx + 1..])),
                _ => pattern.push(ch),
            }
        }
        // Unterminated quote: git rejects the line.
        return None;
    }
    let end = line.find(char::is_whitespace).unwrap_or(line.len());
    Some((line[..end].to_string(), &line[end..]))
}

/// Where the per-directory `.gitattributes` files are read from.
enum Source<'a> {
    Worktree(PathBuf),
    Snapshot(&'a HashMap<PathBuf, String>),
}

impl Source<'_> {
    fn read(&self, dir: &Path) -> Option<String> {
        match self {
            Source::Worktree(root) => {
                fs::read_to_string(root.join(dir).join(".gitattributes")).ok()
            }
            Source::Snapshot(files) => files.get(dir).cloned(),
        }
    }
}

/// Lazily loaded attribute layers for one repository.
pub struct GitAttributes<'a> {
    source: Source<'a>,
    /// `.git/info/attributes`, highest precedence and never committed.
    info: Option<Layer>,
    /// Per-directory `.gitattributes`, keyed by directory relative to the
    /// root. `None` records that the directory has no attributes file so it
    /// is not probed again.
    dirs: HashMap<PathBuf, Option<Layer>>,
}

impl<'a> GitAttributes<'a> {
    /// Read attributes from the working tree at `repo_root`.
    pub fn open(repo_root: &Path) -> Self {
        let info = read_info_attributes(repo_root)
            .and_then(|contents| Layer::parse(Path::new(""), &contents));
        Self {
            source: Source::Worktree(repo_root.to_path_buf()),
            info,
            dirs: HashMap::new(),
        }
    }

    /// Read attributes from files captured at one revision.
    pub fn from_snapshot(snapshot: &'a AttributesSnapshot) -> Self {
        let info = snapshot
            .info
            .as_deref()
            .and_then(|contents| Layer::parse(Path::new(""), contents));
        Self {
            source: Source::Snapshot(&snapshot.files),
            info,
            dirs: HashMap::new(),
        }
    }

    /// Resolve the tracked attributes for `path` (relative to the root).
    pub fn lookup(&mut self, path: &Path) -> FileAttributes {
        FileAttributes {
            generated: self.resolve_any(GENERATED_ATTRS, path),
            vendored: self.resolve_any(VENDORED_ATTRS, path),
        }
    }

    /// Resolve every path and keep only the ones that carry an attribute.
    pub fn classify<'p>(
        &mut self,
        paths: impl IntoIterator<Item = &'p Path>,
    ) -> HashMap<PathBuf, FileAttributes> {
        paths
            .into_iter()
            .filter_map(|path| {
                let resolved = self.lookup(path);
                resolved.any().then(|| (path.to_path_buf(), resolved))
            })
            .collect()
    }

    fn resolve_any(&mut self, names: &[&str], path: &Path) -> bool {
        names
            .iter()
            .any(|name| self.resolve(name, path) == Some(AttrState::Set))
    }

    /// Walk the layers from highest to lowest precedence and return the
    /// first definitive answer.
    fn resolve(&mut self, attr: &str, path: &Path) -> Option<AttrState> {
        if let Some(state) = self
            .info
            .as_ref()
            .and_then(|layer| layer.resolve(attr, path))
        {
            return Some(state);
        }

        let mut dir = path.parent();
        while let Some(current) = dir {
            if let Some(layer) = self.layer_for(current) {
                let rel = path.strip_prefix(&layer.dir).unwrap_or(path);
                if let Some(state) = layer.resolve(attr, rel) {
                    return Some(state);
                }
            }
            if current == Path::new("") {
                break;
            }
            dir = current.parent();
        }
        None
    }

    fn layer_for(&mut self, dir: &Path) -> Option<&Layer> {
        if !self.dirs.contains_key(dir) {
            let layer = self
                .source
                .read(dir)
                .and_then(|contents| Layer::parse(dir, &contents));
            self.dirs.insert(dir.to_path_buf(), layer);
        }
        self.dirs.get(dir).and_then(Option::as_ref)
    }
}

/// The contents of `.git/info/attributes` for the clone at `repo_root`.
fn read_info_attributes(repo_root: &Path) -> Option<String> {
    let path = info_attributes_path(repo_root)?;
    fs::read_to_string(path).ok()
}

/// `.git/info/attributes`, following a `gitdir:` pointer when `.git` is a
/// worktree or submodule file.
fn info_attributes_path(repo_root: &Path) -> Option<PathBuf> {
    let dot_git = repo_root.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else if dot_git.is_file() {
        let pointer = fs::read_to_string(&dot_git).ok()?;
        let target = pointer.trim().strip_prefix("gitdir:")?.trim();
        let target = Path::new(target);
        if target.is_absolute() {
            target.to_path_buf()
        } else {
            repo_root.join(target)
        }
    } else {
        return None;
    };
    Some(git_dir.join("info").join("attributes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn lookup(root: &Path, path: &str) -> FileAttributes {
        GitAttributes::open(root).lookup(Path::new(path))
    }

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create dir");
        }
        fs::write(path, contents).expect("write file");
    }

    #[test]
    fn should_resolve_nothing_without_attribute_files() {
        let dir = tempdir().unwrap();

        assert_eq!(lookup(dir.path(), "src/main.rs"), FileAttributes::default());
    }

    #[test]
    fn should_mark_generated_and_vendored_from_the_root_file() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            ".gitattributes",
            "*.lock linguist-generated\nvendor/** linguist-vendored\n",
        );

        assert_eq!(
            lookup(dir.path(), "Cargo.lock"),
            FileAttributes {
                generated: true,
                vendored: false
            }
        );
        assert_eq!(
            lookup(dir.path(), "vendor/lib/x.js"),
            FileAttributes {
                generated: false,
                vendored: true
            }
        );
        assert_eq!(lookup(dir.path(), "src/main.rs"), FileAttributes::default());
    }

    #[test]
    fn should_accept_every_boolean_spelling() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            ".gitattributes",
            "a.txt linguist-generated=true\n\
             b.txt linguist-generated=false\n\
             c.txt -linguist-generated\n\
             d.txt !linguist-generated\n\
             e.txt gitlab-generated\n",
        );

        assert!(lookup(dir.path(), "a.txt").generated);
        assert!(!lookup(dir.path(), "b.txt").generated);
        assert!(!lookup(dir.path(), "c.txt").generated);
        assert!(!lookup(dir.path(), "d.txt").generated);
        assert!(lookup(dir.path(), "e.txt").generated);
    }

    #[test]
    fn should_let_the_last_matching_line_win() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            ".gitattributes",
            "*.js linguist-generated\nsrc/*.js -linguist-generated\n",
        );

        assert!(lookup(dir.path(), "dist/app.js").generated);
        assert!(!lookup(dir.path(), "src/app.js").generated);
    }

    #[test]
    fn should_not_apply_directory_patterns_recursively() {
        // gitattributes patterns that match a directory do not cover the
        // files inside it; `vendor/**` is the documented spelling.
        let dir = tempdir().unwrap();
        write(dir.path(), ".gitattributes", "vendor linguist-vendored\n");

        assert!(!lookup(dir.path(), "vendor/x.js").vendored);
    }

    #[test]
    fn should_anchor_slash_patterns_to_the_attributes_file_directory() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            ".gitattributes",
            "/Cargo.lock linguist-generated\n",
        );

        assert!(lookup(dir.path(), "Cargo.lock").generated);
        assert!(!lookup(dir.path(), "crates/a/Cargo.lock").generated);
    }

    #[test]
    fn should_prefer_the_nearest_nested_file() {
        let dir = tempdir().unwrap();
        write(dir.path(), ".gitattributes", "*.js linguist-generated\n");
        write(
            dir.path(),
            "web/.gitattributes",
            "*.js -linguist-generated\nbuild/** linguist-generated\n",
        );

        assert!(lookup(dir.path(), "lib/a.js").generated);
        assert!(!lookup(dir.path(), "web/src/a.js").generated);
        assert!(lookup(dir.path(), "web/build/a.js").generated);
        // No `.ts` rule in the nested file, so the root still decides.
        assert!(!lookup(dir.path(), "web/src/a.ts").generated);
    }

    #[test]
    fn should_give_info_attributes_the_final_say() {
        let dir = tempdir().unwrap();
        write(dir.path(), ".gitattributes", "*.lock linguist-generated\n");
        write(
            dir.path(),
            ".git/info/attributes",
            "Cargo.lock -linguist-generated\nschema.sql linguist-generated\n",
        );

        assert!(!lookup(dir.path(), "Cargo.lock").generated);
        assert!(lookup(dir.path(), "yarn.lock").generated);
        assert!(lookup(dir.path(), "schema.sql").generated);
    }

    #[test]
    fn should_follow_a_gitdir_pointer_for_worktrees() {
        let dir = tempdir().unwrap();
        let worktree = dir.path().join("wt");
        let git_dir = dir.path().join("repo.git");
        fs::create_dir_all(&worktree).unwrap();
        write(&git_dir, "info/attributes", "*.pb.go linguist-generated\n");
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", git_dir.display()),
        )
        .unwrap();

        assert!(lookup(&worktree, "api/v1.pb.go").generated);
    }

    #[test]
    fn should_handle_quoted_patterns_comments_and_macros() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            ".gitattributes",
            "# comment\n\
             [attr]gen linguist-generated\n\
             \"my dir/*.json\" linguist-generated\n\
             \n\
             docs/*.md text\n",
        );

        assert!(lookup(dir.path(), "my dir/a.json").generated);
        assert!(!lookup(dir.path(), "docs/a.md").generated);
    }

    #[test]
    fn should_ignore_unrelated_attributes_and_negative_patterns() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            ".gitattributes",
            "*.png binary\n!*.lock linguist-generated\n",
        );

        assert_eq!(lookup(dir.path(), "a.png"), FileAttributes::default());
        assert_eq!(lookup(dir.path(), "a.lock"), FileAttributes::default());
    }

    #[test]
    fn should_classify_only_tagged_paths() {
        let dir = tempdir().unwrap();
        write(dir.path(), ".gitattributes", "*.lock linguist-generated\n");
        let paths = [Path::new("Cargo.lock"), Path::new("src/main.rs")];

        let tagged = GitAttributes::open(dir.path()).classify(paths);

        assert_eq!(tagged.len(), 1);
        assert!(tagged[Path::new("Cargo.lock")].generated);
    }

    fn snapshot(files: &[(&str, &str)], info: Option<&str>) -> AttributesSnapshot {
        AttributesSnapshot {
            files: files
                .iter()
                .map(|(dir, contents)| (PathBuf::from(dir), contents.to_string()))
                .collect(),
            info: info.map(str::to_string),
        }
    }

    #[test]
    fn should_resolve_from_a_snapshot_with_nested_files_and_info() {
        let snapshot = snapshot(
            &[
                ("", "*.pb.go -diff linguist-generated=true\n"),
                ("web", "*.js linguist-vendored\n"),
            ],
            Some("keep.pb.go -linguist-generated\n"),
        );
        let mut attrs = GitAttributes::from_snapshot(&snapshot);

        assert!(attrs.lookup(Path::new("api/v1/a.pb.go")).generated);
        assert!(!attrs.lookup(Path::new("api/keep.pb.go")).generated);
        assert!(attrs.lookup(Path::new("web/app.js")).vendored);
        assert!(!attrs.lookup(Path::new("lib/app.js")).vendored);
    }

    #[test]
    fn should_not_strip_a_nested_directory_name_from_the_path_it_matches() {
        // `api/.gitattributes` sees `api/api/x.go` as `api/x.go`. A matcher
        // rooted at `api` would strip that second `api` too and miss.
        let snapshot = snapshot(&[("api", "api/*.go linguist-generated\n")], None);
        let mut attrs = GitAttributes::from_snapshot(&snapshot);

        assert!(attrs.lookup(Path::new("api/api/x.go")).generated);
        assert!(!attrs.lookup(Path::new("api/x.go")).generated);
    }
}
