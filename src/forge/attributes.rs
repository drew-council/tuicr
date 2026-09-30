//! `.gitattributes` for a pull request, read at the PR's head.
//!
//! The checkout tuicr runs in can be on any branch, so its working tree says
//! nothing about which files the PR marks generated or vendored. The files
//! are read from the head revision instead: out of the local clone when it
//! holds that commit, from the forge otherwise.

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::forge::local_git::read_blob;
use crate::forge::traits::{
    ForgeBackend, ForgeFileLinesRequest, ForgeFileSide, PullRequestDetails,
};
use crate::gitattributes::AttributesSnapshot;
use crate::model::{FilePatch, FileStatus};
use crate::process::run_command_output;

const ATTRIBUTES_FILE: &str = ".gitattributes";

/// Pathspecs per `git ls-tree` call, keeping the command line well under
/// Windows' 32K limit on large PRs.
const LS_TREE_CHUNK: usize = 200;

/// Capture the `.gitattributes` files that govern `patches` at the PR's head.
///
/// Best effort: a file that cannot be read counts as absent, which only means
/// fewer files get tagged.
pub fn fetch_pr_attributes(
    backend: &dyn ForgeBackend,
    details: &PullRequestDetails,
    patches: &[FilePatch],
) -> AttributesSnapshot {
    let checkout = backend.local_checkout_path();
    let info = checkout.as_deref().and_then(read_info_attributes);
    let dirs = attribute_dirs(patches);
    let files = match checkout
        .as_deref()
        .and_then(|root| read_local(root, &details.head_sha, &dirs))
    {
        Some(files) => files,
        // Without the head commit there is no cheap way to learn which
        // directories hold a file, and probing each one would cost a forge
        // round trip apiece. Nested files are rare; the root file is not.
        None => fetch_root(backend, details),
    };
    AttributesSnapshot { files, info }
}

/// Every directory whose `.gitattributes` can apply to a changed path: each
/// ancestor of each old and new path, down to the root (`""`).
fn attribute_dirs(patches: &[FilePatch]) -> BTreeSet<PathBuf> {
    patches
        .iter()
        .flat_map(|patch| patch.old_path.iter().chain(patch.new_path.iter()))
        .flat_map(|path| path.ancestors().skip(1))
        .map(Path::to_path_buf)
        .collect()
}

/// Read the files at `sha` from the clone at `root`. `None` when the clone
/// does not have the commit.
fn read_local(
    root: &Path,
    sha: &str,
    dirs: &BTreeSet<PathBuf>,
) -> Option<HashMap<PathBuf, String>> {
    let specs: Vec<String> = dirs
        .iter()
        .map(|dir| {
            dir.join(ATTRIBUTES_FILE)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();

    // One `ls-tree` names the files that exist, so only those are read.
    // `--full-tree` keeps the pathspecs root-relative when `root` is a
    // subdirectory, and literal pathspecs keep a `*` in a directory name
    // from globbing.
    let mut present = Vec::new();
    for chunk in specs.chunks(LS_TREE_CHUNK) {
        let mut args = vec![
            "--literal-pathspecs",
            "ls-tree",
            "--full-tree",
            "--name-only",
            "-z",
            sha,
            "--",
        ];
        args.extend(chunk.iter().map(String::as_str));
        let listing = run_command_output("git", Some(root), args.iter().map(OsStr::new)).ok()?;
        present.extend(
            listing
                .split('\0')
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
        );
    }

    Some(
        present
            .into_iter()
            .filter_map(|path| {
                let contents = read_blob(root, sha, &path)?;
                let dir = path.parent().unwrap_or(Path::new("")).to_path_buf();
                Some((dir, contents))
            })
            .collect(),
    )
}

/// Ask the forge for the root `.gitattributes` at the PR's head.
fn fetch_root(
    backend: &dyn ForgeBackend,
    details: &PullRequestDetails,
) -> HashMap<PathBuf, String> {
    let request = ForgeFileLinesRequest {
        repository: details.repository.clone(),
        base_sha: details.base_sha.clone(),
        head_sha: details.head_sha.clone(),
        path: PathBuf::from(ATTRIBUTES_FILE),
        status: FileStatus::Modified,
        side: ForgeFileSide::Head,
        start_line: 0,
        end_line: 0,
    };
    backend
        .fetch_file_content(request)
        .ok()
        .map(|contents| HashMap::from([(PathBuf::new(), contents)]))
        .unwrap_or_default()
}

/// The clone's `info/attributes`. `--git-path` resolves it from any
/// subdirectory and to the common directory of a linked worktree, which is
/// where git reads it.
fn read_info_attributes(root: &Path) -> Option<String> {
    let path = run_command_output(
        "git",
        Some(root),
        ["rev-parse", "--git-path", "info/attributes"]
            .iter()
            .map(OsStr::new),
    )
    .ok()?;
    std::fs::read_to_string(root.join(path.trim())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Result, TuicrError};
    use crate::forge::remote_comments::RemoteReviewThread;
    use crate::forge::traits::{
        CreateReviewRequest, ForgeRepository, GhCreateReviewResponse, PagedPullRequests,
        PullRequestCommit, PullRequestListQuery, PullRequestTarget,
    };
    use crate::model::DiffLine;
    use std::cell::RefCell;

    /// Serves `.gitattributes` from a canned root file and records every
    /// path it was asked for.
    struct Backend {
        checkout: Option<PathBuf>,
        root_file: Option<String>,
        requested: RefCell<Vec<PathBuf>>,
    }

    impl ForgeBackend for Backend {
        fn list_pull_requests(&self, _query: PullRequestListQuery) -> Result<PagedPullRequests> {
            unimplemented!()
        }
        fn get_pull_request(&self, _target: PullRequestTarget) -> Result<PullRequestDetails> {
            unimplemented!()
        }
        fn get_pull_request_diff(&self, _pr: &PullRequestDetails) -> Result<Vec<FilePatch>> {
            unimplemented!()
        }
        fn fetch_file_lines(&self, _req: ForgeFileLinesRequest) -> Result<Vec<DiffLine>> {
            unimplemented!()
        }
        fn list_review_threads(&self, _pr: &PullRequestDetails) -> Result<Vec<RemoteReviewThread>> {
            unimplemented!()
        }
        fn list_pull_request_commits(
            &self,
            _pr: &PullRequestDetails,
        ) -> Result<Vec<PullRequestCommit>> {
            unimplemented!()
        }
        fn get_pull_request_commit_range_diff(
            &self,
            _pr: &PullRequestDetails,
            _start_sha: &str,
            _end_sha: &str,
        ) -> Result<Vec<FilePatch>> {
            unimplemented!()
        }
        fn create_review(
            &self,
            _pr: &PullRequestDetails,
            _request: CreateReviewRequest<'_>,
        ) -> Result<GhCreateReviewResponse> {
            unimplemented!()
        }
        fn fetch_file_content(&self, request: ForgeFileLinesRequest) -> Result<String> {
            self.requested.borrow_mut().push(request.path.clone());
            assert_eq!(request.sha(), "remote-head");
            self.root_file
                .clone()
                .ok_or_else(|| TuicrError::Forge("404 Not Found".to_string()))
        }
        fn local_checkout_path(&self) -> Option<PathBuf> {
            self.checkout.clone()
        }
    }

    fn backend(checkout: Option<&Path>, root_file: Option<&str>) -> Backend {
        Backend {
            checkout: checkout.map(Path::to_path_buf),
            root_file: root_file.map(str::to_string),
            requested: RefCell::new(Vec::new()),
        }
    }

    fn details(head_sha: &str) -> PullRequestDetails {
        PullRequestDetails {
            repository: ForgeRepository::github("github.com", "owner", "repo"),
            number: 1,
            title: "PR".to_string(),
            url: "https://github.com/owner/repo/pull/1".to_string(),
            state: "OPEN".to_string(),
            is_draft: false,
            author: None,
            head_ref_name: "feature".to_string(),
            base_ref_name: "main".to_string(),
            head_sha: head_sha.to_string(),
            base_sha: "base".to_string(),
            body: String::new(),
            updated_at: None,
            closed: false,
            merged_at: None,
            diff_start_sha: None,
        }
    }

    fn patch(path: &str) -> FilePatch {
        FilePatch::new(None, Some(PathBuf::from(path)), FileStatus::Modified, "")
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        run_command_output("git", Some(dir), args.iter().map(OsStr::new)).expect("git command")
    }

    fn write(root: &Path, rel: &str, contents: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    /// A clone whose head commit tags `*.pb.go` at the root and `*.js` in
    /// `web/`, returning the commit's SHA.
    fn clone_with_head(root: &Path) -> String {
        git(root, &["init", "--quiet"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "test"]);
        write(root, ".gitattributes", "*.pb.go linguist-generated\n");
        write(root, "web/.gitattributes", "*.js linguist-vendored\n");
        write(
            root,
            "unrelated/.gitattributes",
            "*.rs linguist-generated\n",
        );
        git(root, &["add", "."]);
        git(root, &["commit", "--quiet", "-m", "head"]);
        git(root, &["rev-parse", "HEAD"]).trim().to_string()
    }

    #[test]
    fn should_list_every_ancestor_directory_once() {
        let dirs = attribute_dirs(&[
            patch("api/v1/a.pb.go"),
            patch("api/b.go"),
            FilePatch::new(
                Some(PathBuf::from("old/x.rs")),
                Some(PathBuf::from("README.md")),
                FileStatus::Renamed,
                "",
            ),
        ]);

        let dirs: Vec<_> = dirs
            .iter()
            .map(|d| d.to_string_lossy().into_owned())
            .collect();
        assert_eq!(dirs, vec!["", "api", "api/v1", "old"]);
    }

    #[test]
    fn should_read_the_head_revision_instead_of_the_working_tree() {
        // given a clone holding the PR head, whose working tree has since
        // moved on to different attributes
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let head = clone_with_head(root);
        write(root, ".gitattributes", "*.go linguist-generated\n");
        write(root, "web/.gitattributes", "");
        let backend = backend(Some(root), None);

        // when the PR's attributes are captured
        let snapshot = fetch_pr_attributes(
            &backend,
            &details(&head),
            &[patch("api/a.pb.go"), patch("web/app.js")],
        );

        // then they come from the head commit, only for the directories the
        // PR touches, without a forge round trip
        assert_eq!(
            snapshot.files,
            HashMap::from([
                (PathBuf::new(), "*.pb.go linguist-generated\n".to_string()),
                (PathBuf::from("web"), "*.js linguist-vendored\n".to_string()),
            ])
        );
        assert!(backend.requested.borrow().is_empty());
    }

    #[test]
    fn should_read_the_head_revision_from_a_subdirectory_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let head = clone_with_head(dir.path());
        write(
            dir.path(),
            ".git/info/attributes",
            "a.pb.go -linguist-generated\n",
        );
        let backend = backend(Some(&dir.path().join("web")), None);

        let snapshot = fetch_pr_attributes(&backend, &details(&head), &[patch("api/a.pb.go")]);

        assert_eq!(
            snapshot.files,
            HashMap::from([(PathBuf::new(), "*.pb.go linguist-generated\n".to_string())])
        );
        assert_eq!(
            snapshot.info.as_deref(),
            Some("a.pb.go -linguist-generated\n")
        );
    }

    #[test]
    fn should_ask_the_forge_for_the_root_file_when_the_clone_lacks_the_head() {
        let dir = tempfile::tempdir().unwrap();
        clone_with_head(dir.path());
        let backend = backend(Some(dir.path()), Some("*.lock linguist-generated\n"));

        let snapshot = fetch_pr_attributes(
            &backend,
            &details("remote-head"),
            &[patch("web/app.js"), patch("Cargo.lock")],
        );

        assert_eq!(
            snapshot.files,
            HashMap::from([(PathBuf::new(), "*.lock linguist-generated\n".to_string())])
        );
        assert_eq!(
            *backend.requested.borrow(),
            vec![PathBuf::from(".gitattributes")]
        );
    }

    #[test]
    fn should_capture_nothing_when_the_forge_has_no_root_file() {
        let backend = backend(None, None);

        let snapshot = fetch_pr_attributes(&backend, &details("remote-head"), &[patch("a.rs")]);

        assert_eq!(snapshot, AttributesSnapshot::default());
    }
}
