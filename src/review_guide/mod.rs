//! AI review guide for pull requests.
//!
//! When `[review_guide]` names a command, opening a PR runs it with a JSON
//! request on stdin (instructions, PR metadata, per-file patches) and reads
//! a JSON guide from its stdout: a brief overview and an ordered list of
//! review steps. The file tree and the continuous diff then follow that order.
//! tuicr knows nothing about the model behind the command; any harness with a
//! headless mode works (`claude -p`, `pi -p`, a script).
//!
//! The command starts the moment the diff has been fetched, on its own
//! thread, so it overlaps the rest of the PR load instead of extending it.
//! The PR opens in the default order; the guide reorders it when it lands.
//! Guides are cached per PR head, so reopening a PR runs nothing.

mod command;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub use command::PROMPT_ENV;

use crate::config::ReviewGuideConfig;
use crate::forge::traits::{PrSessionKey, PullRequestInfo};
use crate::model::{FilePatch, FileStatus};

/// Version of the request and response shapes, sent as `version`.
/// Bump it when either changes, so cached guides are not reused.
const FORMAT_VERSION: u32 = 1;

/// Total patch text sent to the command. Larger PRs keep every path in the
/// file list but drop patch bodies past the budget, which keeps latency bounded.
const MAX_REQUEST_PATCH_BYTES: usize = 240_000;
/// Per-file cap, so one huge file (a lockfile, a snapshot) cannot starve the
/// rest of the budget.
const MAX_FILE_PATCH_BYTES: usize = 12_000;
/// The PR description is context, not the subject; cap it.
const MAX_DESCRIPTION_BYTES: usize = 8_000;

/// Used when the config sets neither `prompt` nor `prompt_file`.
pub const DEFAULT_PROMPT: &str = "\
Plan the review of this pull request. Write a short overview and group the changed files into ordered steps, so that each step builds only on files already read.

Order:
- Schemas, API contracts, migrations, and shared types first.
- Interfaces before implementations. Core logic before its callers and entry points.
- Tests right after the code they exercise.
- Build and config files with the code they enable.
- Generated code, lockfiles, vendored files, and docs last.

Use 2-8 steps. Give each a title of 2-5 words and a summary of at most 10 words, or \"\" when the title says enough.
Write the overview as 2-5 short markdown bullets: what changed, how the pieces connect, and any risk.";

/// Appended to the prompt. This is the contract `parse_response` reads,
/// so it is not configurable.
const RESPONSE_FORMAT: &str = "\
## Response format

Answer with one JSON object and nothing else. No prose before or after it, no code fence.

{\"overview\": \"<markdown>\", \"steps\": [{\"title\": \"<short title>\", \"summary\": \"<one line, may be empty>\", \"files\": [\"<path>\", \"...\"]}]}

- `steps` are in the order the reviewer should read them. `files` inside a step are in reading order too.
- Every entry in `files` is the `path` of one of the request's `files`, spelled exactly as given.
- Place every file of the request in exactly one step.";

/// The text sent as the request's `instructions` and exported as
/// [`PROMPT_ENV`]: the configured prompt, then the response format.
pub fn instructions(config: &ReviewGuideConfig) -> String {
    let prompt = config.prompt.as_deref().unwrap_or(DEFAULT_PROMPT).trim();
    format!("{prompt}\n\n{RESPONSE_FORMAT}")
}

/// Title of the step that collects files the model did not place.
const UNPLACED_STEP_TITLE: &str = "Other changes";

/// A review order plus a short overview of the change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewGuide {
    /// A few sentences on what the PR does and how it fits together.
    pub overview: String,
    /// Review steps in order. Every file in the PR appears in exactly one
    /// step (see [`ReviewGuide::normalized`]).
    pub steps: Vec<ReviewStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewStep {
    pub title: String,
    pub summary: String,
    pub files: Vec<PathBuf>,
}

impl ReviewGuide {
    /// Reconcile a model response with the PR's actual files: drop paths the
    /// PR does not contain, keep only the first placement of a path, drop
    /// steps left empty, and append every unplaced file to a trailing
    /// "Other changes" step in PR order. The result places each PR file
    /// exactly once, so ordering never loses a file to a model mistake.
    pub fn normalized(self, pr_files: &[PathBuf]) -> Self {
        let known: HashSet<&Path> = pr_files.iter().map(PathBuf::as_path).collect();
        let mut placed: HashSet<PathBuf> = HashSet::new();
        let mut steps: Vec<ReviewStep> = self
            .steps
            .into_iter()
            .filter_map(|step| {
                let files: Vec<PathBuf> = step
                    .files
                    .into_iter()
                    .filter_map(|path| resolve_model_path(&path, &known))
                    .filter(|path| placed.insert(path.clone()))
                    .collect();
                (!files.is_empty()).then(|| ReviewStep {
                    title: one_line(&step.title),
                    summary: one_line(&step.summary),
                    files,
                })
            })
            .collect();
        let unplaced: Vec<PathBuf> = pr_files
            .iter()
            .filter(|path| !placed.contains(*path))
            .cloned()
            .collect();
        if !unplaced.is_empty() {
            steps.push(ReviewStep {
                title: UNPLACED_STEP_TITLE.to_string(),
                summary: String::new(),
                files: unplaced,
            });
        }
        Self {
            overview: self.overview.trim().to_string(),
            steps,
        }
    }

    /// `(step index, position within step)` for every placed file.
    pub fn ranks(&self) -> HashMap<PathBuf, (usize, usize)> {
        self.steps
            .iter()
            .enumerate()
            .flat_map(|(step_idx, step)| {
                step.files
                    .iter()
                    .enumerate()
                    .map(move |(pos, path)| (path.clone(), (step_idx, pos)))
            })
            .collect()
    }
}

/// Match a model-supplied path to a PR file. Models sometimes echo the
/// diff-header `a/`/`b/` prefixes or a leading `./`; a prefix is only stripped
/// when the path as given is not itself a PR file.
fn resolve_model_path(path: &Path, known: &HashSet<&Path>) -> Option<PathBuf> {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim();
    std::iter::once(trimmed)
        .chain(
            ["./", "a/", "b/"]
                .iter()
                .filter_map(|prefix| trimmed.strip_prefix(prefix)),
        )
        .map(Path::new)
        .find(|candidate| known.contains(candidate))
        .map(Path::to_path_buf)
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Result of one guide request, tagged with the PR head it describes.
#[derive(Debug)]
pub struct ReviewGuideEvent {
    pub key: PrSessionKey,
    pub result: Result<ReviewGuide, String>,
}

/// What the command reads on stdin, as one JSON object.
/// `instructions` comes first so a harness fed the request as its prompt
/// reads the task before the data.
#[derive(Debug, Clone, Serialize)]
struct GuideRequest {
    version: u32,
    instructions: String,
    pull_request: RequestPullRequest,
    files: Vec<RequestFile>,
}

#[derive(Debug, Clone, Serialize)]
struct RequestPullRequest {
    url: String,
    /// `owner/name`.
    repository: String,
    number: u64,
    title: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    base_ref: String,
    head_ref: String,
    head_sha: String,
}

#[derive(Debug, Clone, Serialize)]
struct RequestFile {
    path: String,
    /// Set for a rename or copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
    status: &'static str,
    /// Absent for a binary file and once the patch budget is spent.
    #[serde(skip_serializing_if = "Option::is_none")]
    patch: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    binary: bool,
    /// The patch was cut short or left out to bound the request.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    truncated: bool,
}

impl GuideRequest {
    fn new(instructions: String, info: &PullRequestInfo, patches: &[FilePatch]) -> Self {
        let details = &info.details;
        let mut budget = MAX_REQUEST_PATCH_BYTES;
        let files = patches
            .iter()
            .filter_map(|patch| {
                let path = patch.display_path()?;
                let old_path = patch
                    .old_path
                    .as_ref()
                    .filter(|old| {
                        matches!(patch.status, FileStatus::Renamed | FileStatus::Copied)
                            && *old != path
                    })
                    .map(|old| old.to_string_lossy().into_owned());
                let binary = patch.is_binary || patch.is_too_large;
                let limit = MAX_FILE_PATCH_BYTES.min(budget);
                let body = truncate_at_char_boundary(&patch.patch, limit);
                let truncated = !binary && body.len() < patch.patch.len();
                let patch_text = (!binary && !body.is_empty()).then(|| {
                    budget -= body.len();
                    body.to_string()
                });
                Some(RequestFile {
                    path: path.to_string_lossy().into_owned(),
                    old_path,
                    status: match patch.status {
                        FileStatus::Added => "added",
                        FileStatus::Modified => "modified",
                        FileStatus::Deleted => "deleted",
                        FileStatus::Renamed => "renamed",
                        FileStatus::Copied => "copied",
                    },
                    patch: patch_text,
                    binary,
                    truncated,
                })
            })
            .collect();
        Self {
            version: FORMAT_VERSION,
            instructions,
            pull_request: RequestPullRequest {
                url: details.url.clone(),
                repository: format!("{}/{}", details.repository.owner, details.repository.name),
                number: details.number,
                title: details.title.trim().to_string(),
                description: truncate_at_char_boundary(details.body.trim(), MAX_DESCRIPTION_BYTES)
                    .to_string(),
                author: details.author.clone(),
                base_ref: details.base_ref_name.clone(),
                head_ref: details.head_ref_name.clone(),
                head_sha: details.head_sha.clone(),
            },
            files,
        }
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .map(|file| PathBuf::from(&file.path))
            .collect()
    }
}

/// Starts guide requests. Built on the main thread, then moved into the PR
/// fetch thread so the command can start as soon as the diff arrives.
#[derive(Clone)]
pub struct ReviewGuideLauncher {
    config: ReviewGuideConfig,
    cache_dir: Option<PathBuf>,
    tx: Sender<ReviewGuideEvent>,
    cancel: Arc<AtomicBool>,
}

/// The app's end of a guide request.
/// Dropping it stops the command if it is still running.
#[derive(Debug)]
pub struct PendingReviewGuide {
    rx: Receiver<ReviewGuideEvent>,
    cancel: Arc<AtomicBool>,
}

impl PendingReviewGuide {
    pub fn try_recv(&self) -> Result<ReviewGuideEvent, TryRecvError> {
        self.rx.try_recv()
    }
}

impl Drop for PendingReviewGuide {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// A sender paired with the handle the app polls.
pub fn channel() -> (Sender<ReviewGuideEvent>, PendingReviewGuide) {
    let (tx, rx) = std::sync::mpsc::channel();
    let pending = PendingReviewGuide {
        rx,
        cancel: Arc::new(AtomicBool::new(false)),
    };
    (tx, pending)
}

impl ReviewGuideLauncher {
    pub fn new(
        config: ReviewGuideConfig,
        cache_dir: Option<PathBuf>,
    ) -> (Self, PendingReviewGuide) {
        let (tx, pending) = channel();
        let launcher = Self {
            config,
            cache_dir,
            tx,
            cancel: Arc::clone(&pending.cancel),
        };
        (launcher, pending)
    }

    /// Spawn the request for this PR's diff. Returns immediately; the result
    /// arrives on the launcher's channel. A cached guide for the same head
    /// is answered without running the command.
    pub fn launch(self, info: &PullRequestInfo, patches: &[FilePatch]) {
        let key = PrSessionKey::from_details(&info.details);
        let request = GuideRequest::new(instructions(&self.config), info, patches);
        std::thread::spawn(move || {
            let result = self.resolve(&key, &request);
            let _ = self.tx.send(ReviewGuideEvent { key, result });
        });
    }

    fn resolve(&self, key: &PrSessionKey, request: &GuideRequest) -> Result<ReviewGuide, String> {
        if request.files.is_empty() {
            return Err("pull request has no files".to_string());
        }
        let paths = request.paths();
        let cache_path = self
            .cache_dir
            .as_deref()
            .map(|dir| cache_file(dir, &self.config, &request.instructions, key));
        if let Some(guide) = cache_path.as_deref().and_then(read_cached) {
            return Ok(guide.normalized(&paths));
        }
        let stdin = serde_json::to_string(request)
            .map_err(|e| format!("failed to encode the request: {e}"))?;
        let stdout = command::run(
            &self.config.command,
            stdin,
            &request.instructions,
            Duration::from_secs(self.config.timeout_secs),
            &self.cancel,
        )?;
        let guide = parse_response(&stdout)?.normalized(&paths);
        if let Some(path) = cache_path {
            write_cached(&path, &guide);
        }
        Ok(guide)
    }
}

/// Default cache location: `<data dir>/tuicr/review_guides`.
pub fn default_cache_dir() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    directories::ProjectDirs::from("", "", "tuicr")
        .map(|dirs| dirs.data_dir().join("review_guides"))
}

fn cache_file(
    dir: &Path,
    config: &ReviewGuideConfig,
    instructions: &str,
    key: &PrSessionKey,
) -> PathBuf {
    let identity = format!(
        "v{FORMAT_VERSION}|{}|{instructions}|{}/{}/{}#{}@{}",
        config.command.join("\0"),
        key.repository.host,
        key.repository.owner,
        key.repository.name,
        key.number,
        key.head_sha,
    );
    use sha1::{Digest, Sha1};
    let digest = Sha1::digest(identity.as_bytes());
    let name: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    dir.join(format!("{name}.json"))
}

fn read_cached(path: &Path) -> Option<ReviewGuide> {
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Best effort: a failed cache write only costs a repeat request.
fn write_cached(path: &Path, guide: &ReviewGuide) {
    let Some(dir) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let Ok(json) = serde_json::to_string_pretty(guide) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Read a guide out of the command's stdout (before normalization).
///
/// Headless harnesses rarely print bare JSON: the answer may sit in a code
/// fence, after a line of prose, or inside the harness's own JSON envelope.
/// So this takes the first JSON object in the text that has `steps`, trying
/// every `{` as a start and ignoring whatever follows the object.
fn parse_response(text: &str) -> Result<ReviewGuide, String> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        overview: String,
        steps: Vec<RawStep>,
    }
    #[derive(Deserialize)]
    struct RawStep {
        #[serde(default)]
        title: String,
        #[serde(default)]
        summary: String,
        files: Vec<String>,
    }
    let raw = text
        .match_indices('{')
        .find_map(|(start, _)| {
            let mut stream = serde_json::Deserializer::from_str(&text[start..]).into_iter::<Raw>();
            stream.next()?.ok()
        })
        .ok_or_else(|| {
            let shown: String = text.trim().chars().take(120).collect();
            format!("no JSON object with `steps` in the command's output: {shown}")
        })?;
    Ok(ReviewGuide {
        overview: raw.overview,
        steps: raw
            .steps
            .into_iter()
            .map(|step| ReviewStep {
                title: step.title,
                summary: step.summary,
                files: step.files.into_iter().map(PathBuf::from).collect(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::traits::{ForgeRepository, PullRequestDetails};

    fn paths(items: &[&str]) -> Vec<PathBuf> {
        items.iter().map(PathBuf::from).collect()
    }

    fn step(title: &str, files: &[&str]) -> ReviewStep {
        ReviewStep {
            title: title.to_string(),
            summary: format!("{title} summary"),
            files: paths(files),
        }
    }

    fn pr_info(body: &str) -> PullRequestInfo {
        PullRequestInfo::from_details(PullRequestDetails {
            repository: ForgeRepository::github("github.com", "o", "r"),
            number: 7,
            title: " Add users ".to_string(),
            url: "https://github.com/o/r/pull/7".to_string(),
            state: "OPEN".to_string(),
            is_draft: false,
            author: Some("alice".to_string()),
            head_ref_name: "users".to_string(),
            base_ref_name: "main".to_string(),
            head_sha: "abc".to_string(),
            base_sha: "base".to_string(),
            body: body.to_string(),
            updated_at: None,
            closed: false,
            merged_at: None,
            diff_start_sha: None,
        })
    }

    fn patch(path: &str, body: &str) -> FilePatch {
        FilePatch::new(None, Some(path.into()), FileStatus::Modified, body)
    }

    fn config(command: &[&str]) -> ReviewGuideConfig {
        ReviewGuideConfig {
            command: command.iter().map(|arg| arg.to_string()).collect(),
            ..ReviewGuideConfig::default()
        }
    }

    #[test]
    fn should_place_every_pr_file_exactly_once() {
        // given a response that repeats one file, invents another, and omits one
        let guide = ReviewGuide {
            overview: "  Adds users.  ".to_string(),
            steps: vec![
                step("API", &["api/user.proto", "nope.rs"]),
                step("Impl", &["src/user.rs", "api/user.proto"]),
                step("Empty", &["ghost.rs"]),
            ],
        };
        let pr_files = paths(&["src/user.rs", "api/user.proto", "src/user_test.rs"]);

        // when
        let guide = guide.normalized(&pr_files);

        // then
        assert_eq!(guide.overview, "Adds users.");
        let titles: Vec<&str> = guide.steps.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["API", "Impl", UNPLACED_STEP_TITLE]);
        assert_eq!(guide.steps[0].files, paths(&["api/user.proto"]));
        assert_eq!(guide.steps[1].files, paths(&["src/user.rs"]));
        assert_eq!(guide.steps[2].files, paths(&["src/user_test.rs"]));
    }

    #[test]
    fn should_strip_diff_header_and_dot_prefixes_from_model_paths() {
        let guide = ReviewGuide {
            overview: String::new(),
            steps: vec![step("All", &["./src/a.rs", "b/src/b.rs"])],
        };
        let guide = guide.normalized(&paths(&["src/a.rs", "src/b.rs"]));
        assert_eq!(guide.steps.len(), 1);
        assert_eq!(guide.steps[0].files, paths(&["src/a.rs", "src/b.rs"]));
    }

    #[test]
    fn should_rank_files_by_step_then_position() {
        let guide = ReviewGuide {
            overview: String::new(),
            steps: vec![step("A", &["x", "y"]), step("B", &["z"])],
        };
        let ranks = guide.ranks();
        assert_eq!(ranks[Path::new("x")], (0, 0));
        assert_eq!(ranks[Path::new("y")], (0, 1));
        assert_eq!(ranks[Path::new("z")], (1, 0));
    }

    #[test]
    fn should_find_the_guide_in_whatever_a_harness_prints_around_it() {
        let json = r#"{"overview":"o","steps":[{"title":"t","summary":"s","files":["a"]}]}"#;
        let plain = parse_response(json).unwrap();
        assert_eq!(plain.steps[0].files, paths(&["a"]));
        for wrapped in [
            format!("```json\n{json}\n```"),
            format!("Here is the plan {{as asked}}:\n\n{json}\n\nLet me know."),
            // A harness envelope carrying the answer as a nested object.
            format!(r#"{{"type":"result","usage":{{"tokens":3}},"structured_output":{json}}}"#),
        ] {
            assert_eq!(parse_response(&wrapped).unwrap(), plain, "{wrapped}");
        }
    }

    #[test]
    fn should_default_the_optional_response_fields() {
        let guide = parse_response(r#"{"steps":[{"files":["a"]}]}"#).unwrap();
        assert_eq!(guide.overview, "");
        assert_eq!(guide.steps[0].title, "");
        assert_eq!(guide.steps[0].summary, "");
    }

    #[test]
    fn should_reject_output_without_a_guide() {
        let err = parse_response("I could not read the diff.").unwrap_err();
        assert!(err.contains("I could not read the diff."), "{err}");
        assert!(parse_response(r#"{"overview":"no steps"}"#).is_err());
    }

    #[test]
    fn should_send_instructions_then_the_pull_request_then_its_files() {
        let mut renamed = FilePatch::new(
            Some("src/old.rs".into()),
            Some("src/new.rs".into()),
            FileStatus::Renamed,
            "@@ -1 +1 @@\n-a\n+b\n",
        );
        renamed.is_binary = false;
        let mut image = patch("logo.png", "");
        image.is_binary = true;
        let request = GuideRequest::new(
            "do it".to_string(),
            &pr_info("  Adds a user service.  "),
            &[renamed, image],
        );

        let json = serde_json::to_string(&request).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert!(json.starts_with(r#"{"version":1,"instructions":"do it","pull_request":"#));
        assert_eq!(
            value["pull_request"],
            serde_json::json!({
                "url": "https://github.com/o/r/pull/7",
                "repository": "o/r",
                "number": 7,
                "title": "Add users",
                "description": "Adds a user service.",
                "author": "alice",
                "base_ref": "main",
                "head_ref": "users",
                "head_sha": "abc",
            })
        );
        assert_eq!(
            value["files"],
            serde_json::json!([
                {
                    "path": "src/new.rs",
                    "old_path": "src/old.rs",
                    "status": "renamed",
                    "patch": "@@ -1 +1 @@\n-a\n+b\n",
                },
                { "path": "logo.png", "status": "modified", "binary": true },
            ])
        );
    }

    #[test]
    fn should_bound_request_size_and_keep_every_path() {
        let big = "+x\n".repeat(MAX_FILE_PATCH_BYTES);
        let patches: Vec<FilePatch> = (0..40)
            .map(|i| patch(&format!("src/f{i}.rs"), &big))
            .collect();

        let request = GuideRequest::new(String::new(), &pr_info(""), &patches);

        let sent: usize = request
            .files
            .iter()
            .filter_map(|file| file.patch.as_ref())
            .map(String::len)
            .sum();
        assert_eq!(sent, MAX_REQUEST_PATCH_BYTES);
        assert_eq!(request.files.len(), 40);
        assert!(request.files.iter().all(|file| file.truncated));
        let last = request.files.last().unwrap();
        assert_eq!(last.path, "src/f39.rs");
        assert_eq!(last.patch, None);
    }

    #[test]
    fn should_append_the_response_format_to_the_configured_prompt() {
        let custom = ReviewGuideConfig {
            prompt: Some("  Tests first. \n".to_string()),
            ..ReviewGuideConfig::default()
        };
        assert_eq!(
            instructions(&custom),
            format!("Tests first.\n\n{RESPONSE_FORMAT}")
        );
        assert!(instructions(&ReviewGuideConfig::default()).starts_with(DEFAULT_PROMPT));
    }

    #[test]
    fn should_truncate_on_char_boundaries() {
        assert_eq!(truncate_at_char_boundary("héllo", 2), "h");
        assert_eq!(truncate_at_char_boundary("abc", 10), "abc");
    }

    /// The launcher end to end: request on stdin, prompt in the environment,
    /// guide on stdout, and a cache that answers the second open.
    #[cfg(unix)]
    #[test]
    fn should_run_the_command_once_per_pr_head() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("calls.log");
        // The script records the request and that the prompt reached it, then
        // answers the way a chatty harness would.
        let script = format!(
            r#"cat >> {log}; echo >> {log}; test -n "$TUICR_REVIEW_GUIDE_PROMPT" || exit 9
printf '%s\n' 'Sure:' '```json' '{{"overview":"- Adds users.","steps":[{{"title":"Impl","files":["src/user.rs"]}}]}}' '```'"#,
            log = log.display()
        );
        let config = config(&["sh", "-c", &script]);
        let patches = [
            patch("src/user_test.rs", "+t\n"),
            patch("src/user.rs", "+u\n"),
        ];
        let open = || {
            let (launcher, pending) =
                ReviewGuideLauncher::new(config.clone(), Some(dir.path().join("cache")));
            launcher.launch(&pr_info("body"), &patches);
            let event = pending.rx.recv_timeout(Duration::from_secs(30)).unwrap();
            assert_eq!(event.key.head_sha, "abc");
            event.result.unwrap()
        };

        let first = open();
        let second = open();

        assert_eq!(first, second);
        assert_eq!(first.overview, "- Adds users.");
        assert_eq!(first.steps[0].files, paths(&["src/user.rs"]));
        assert_eq!(first.steps[1].title, UNPLACED_STEP_TITLE);
        assert_eq!(first.steps[1].files, paths(&["src/user_test.rs"]));
        let calls = std::fs::read_to_string(&log).unwrap();
        assert_eq!(calls.lines().count(), 1, "{calls}");
        let sent: serde_json::Value = serde_json::from_str(calls.trim()).unwrap();
        assert_eq!(sent["files"][1]["path"], "src/user.rs");
        assert!(
            sent["instructions"]
                .as_str()
                .unwrap()
                .contains("## Response format")
        );
    }

    #[cfg(unix)]
    #[test]
    fn should_stop_the_command_when_the_app_drops_the_request() {
        let (launcher, pending) = ReviewGuideLauncher::new(config(&["sleep", "30"]), None);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let key = PrSessionKey::from_details(&pr_info("").details);
        let request = GuideRequest::new(String::new(), &pr_info(""), &[patch("a", "+a\n")]);
        std::thread::spawn(move || {
            let _ = done_tx.send(launcher.resolve(&key, &request));
        });

        drop(pending);

        let result = done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(result.unwrap_err(), command::CANCELLED);
    }
}
