//! AI review guide for pull requests.
//!
//! When enabled (`[review_guide]` in config), opening a PR sends its title,
//! description, and diff to Gemini on Vertex AI, which answers with a brief
//! overview and an ordered list of review steps (API definitions before their
//! callers, interfaces before implementations, implementations before tests).
//! The file tree and the continuous diff then follow that order.
//!
//! The request starts the moment the diff has been fetched, on its own
//! thread, so it overlaps the rest of the PR load instead of extending it.
//! The PR opens in the default order; the guide reorders it when it lands.
//! Guides are cached per PR head, so reopening a PR costs no request.

mod vertex;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use serde::{Deserialize, Serialize};

use crate::config::ReviewGuideConfig;
use crate::forge::traits::{PrSessionKey, PullRequestInfo};
use crate::model::{FilePatch, FileStatus};

/// Bump when the prompt or response shape changes, so cached guides from an
/// older prompt are not reused.
const PROMPT_VERSION: u32 = 2;

/// Total patch text sent to the model. Larger PRs keep every path in the file
/// list but drop patch bodies past the budget, which keeps latency bounded.
const MAX_PROMPT_PATCH_BYTES: usize = 240_000;
/// Per-file cap, so one huge file (a lockfile, a snapshot) cannot starve the
/// rest of the budget.
const MAX_FILE_PATCH_BYTES: usize = 12_000;
/// The PR description is context, not the subject; cap it.
const MAX_DESCRIPTION_BYTES: usize = 8_000;

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

/// Everything the request thread needs, captured from `fetch_pr_data`.
#[derive(Debug, Clone)]
struct GuideInput {
    key: PrSessionKey,
    title: String,
    description: String,
    files: Vec<GuideFile>,
}

#[derive(Debug, Clone)]
struct GuideFile {
    path: PathBuf,
    old_path: Option<PathBuf>,
    status: FileStatus,
    patch: String,
    is_binary: bool,
}

impl GuideInput {
    fn from_patches(info: &PullRequestInfo, patches: &[FilePatch]) -> Self {
        let details = &info.details;
        Self {
            key: PrSessionKey::from_details(details),
            title: details.title.clone(),
            description: details.body.clone(),
            files: patches
                .iter()
                .filter_map(|patch| {
                    let path = patch.display_path()?.to_path_buf();
                    let old_path = patch
                        .old_path
                        .clone()
                        .filter(|old| patch.status == FileStatus::Renamed && *old != path);
                    Some(GuideFile {
                        path,
                        old_path,
                        status: patch.status,
                        patch: patch.patch.clone(),
                        is_binary: patch.is_binary || patch.is_too_large,
                    })
                })
                .collect(),
        }
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.files.iter().map(|file| file.path.clone()).collect()
    }
}

/// Starts guide requests. Built on the main thread (it owns the result
/// channel's sender), then moved into the PR fetch thread so the request can
/// begin as soon as the diff arrives.
pub struct ReviewGuideLauncher {
    config: ReviewGuideConfig,
    cache_dir: Option<PathBuf>,
    tx: Sender<ReviewGuideEvent>,
}

impl ReviewGuideLauncher {
    pub fn new(
        config: ReviewGuideConfig,
        cache_dir: Option<PathBuf>,
        tx: Sender<ReviewGuideEvent>,
    ) -> Self {
        Self {
            config,
            cache_dir,
            tx,
        }
    }

    /// Spawn the request for this PR's diff. Returns immediately; the result
    /// arrives on the launcher's channel. A cached guide for the same head
    /// is answered without a request.
    pub fn launch(self, info: &PullRequestInfo, patches: &[FilePatch]) {
        let input = GuideInput::from_patches(info, patches);
        std::thread::spawn(move || {
            let key = input.key.clone();
            let result = self.resolve(&input);
            let _ = self.tx.send(ReviewGuideEvent { key, result });
        });
    }

    fn resolve(&self, input: &GuideInput) -> Result<ReviewGuide, String> {
        if input.files.is_empty() {
            return Err("pull request has no files".to_string());
        }
        let cache_path = self
            .cache_dir
            .as_deref()
            .map(|dir| cache_file(dir, &self.config, &input.key));
        if let Some(guide) = cache_path.as_deref().and_then(read_cached) {
            return Ok(guide.normalized(&input.paths()));
        }
        let prompt = build_prompt(input);
        let guide = vertex::generate(&self.config, &prompt)?.normalized(&input.paths());
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

fn cache_file(dir: &Path, config: &ReviewGuideConfig, key: &PrSessionKey) -> PathBuf {
    let identity = format!(
        "v{PROMPT_VERSION}|{}|{}|{}/{}/{}#{}@{}",
        config.model,
        config.thinking,
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

const SYSTEM_PROMPT: &str = "\
You help a code reviewer work through a pull request in a logical order. \
Group the changed files into review steps and order the steps so that each \
one builds only on what the reviewer has already read.

Ordering principles, in priority order:
1. Schemas and API contracts first: protobuf/gRPC/OpenAPI/GraphQL definitions, \
database migrations, and shared types or constants that other code consumes.
2. Interfaces, traits, and abstract types before their implementations.
3. Core implementations before the code that calls them (handlers, wiring, \
CLI/UI entry points).
4. Configuration, build, and dependency files alongside the code they enable, \
or right after it when they are incidental. Exception: a Bazel build file \
(BUILD, BUILD.bazel, *.bzl) always comes after every file it builds or \
references, including that package's tests, never before them.
5. Tests after the code they exercise; fixtures and snapshots with their tests.
6. Generated code, lockfiles, and vendored files last.
7. Documentation and changelogs last, unless the PR is mainly documentation.

Rules:
- Every changed file must appear in exactly one step, spelled exactly as listed.
- Keep files that are only meaningful together (an implementation and its \
generated stubs, a feature and its test) in adjacent steps.
- Prefer 2-8 steps. Step titles are short noun phrases (at most 6 words). \
Step summaries are one sentence on what to look for in that step.
- The overview is 2-4 sentences for a reviewer: what the PR does, how the \
pieces fit together, and anything risky worth extra attention. Plain text, \
no headings.";

fn build_prompt(input: &GuideInput) -> Prompt {
    let mut user = String::new();
    user.push_str(&format!("# Pull request: {}\n\n", input.title.trim()));
    let description = input.description.trim();
    if !description.is_empty() {
        user.push_str("## Description\n\n");
        user.push_str(truncate_at_char_boundary(
            description,
            MAX_DESCRIPTION_BYTES,
        ));
        user.push_str("\n\n");
    }

    user.push_str("## Changed files\n\n");
    for file in &input.files {
        let status = match file.status {
            FileStatus::Added => "added",
            FileStatus::Modified => "modified",
            FileStatus::Deleted => "deleted",
            FileStatus::Renamed => "renamed",
            FileStatus::Copied => "copied",
        };
        match &file.old_path {
            Some(old) => user.push_str(&format!(
                "- {} ({status} from {})\n",
                file.path.display(),
                old.display()
            )),
            None => user.push_str(&format!("- {} ({status})\n", file.path.display())),
        }
    }

    user.push_str("\n## Diff\n");
    let mut budget = MAX_PROMPT_PATCH_BYTES;
    for file in &input.files {
        user.push_str(&format!("\n### {}\n", file.path.display()));
        if file.is_binary {
            user.push_str("(binary or too large to display)\n");
            continue;
        }
        if budget == 0 {
            user.push_str("(diff omitted: prompt budget exhausted)\n");
            continue;
        }
        let limit = MAX_FILE_PATCH_BYTES.min(budget);
        let patch = truncate_at_char_boundary(&file.patch, limit);
        budget -= patch.len();
        user.push_str("```diff\n");
        user.push_str(patch);
        if !patch.ends_with('\n') {
            user.push('\n');
        }
        if patch.len() < file.patch.len() {
            user.push_str("... (truncated)\n");
        }
        user.push_str("```\n");
    }

    Prompt {
        system: SYSTEM_PROMPT.to_string(),
        user,
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

struct Prompt {
    system: String,
    user: String,
}

/// JSON schema for the model's structured response.
fn response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "overview": {
                "type": "string",
                "description": "2-4 sentence overview of the pull request for a reviewer."
            },
            "steps": {
                "type": "array",
                "description": "Review steps in the order the reviewer should read them.",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "summary": { "type": "string" },
                        "files": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    },
                    "required": ["title", "summary", "files"]
                }
            }
        },
        "required": ["overview", "steps"]
    })
}

/// Parse the model's JSON text into a guide (before normalization).
fn parse_response(text: &str) -> Result<ReviewGuide, String> {
    #[derive(Deserialize)]
    struct Raw {
        overview: String,
        steps: Vec<RawStep>,
    }
    #[derive(Deserialize)]
    struct RawStep {
        title: String,
        #[serde(default)]
        summary: String,
        files: Vec<String>,
    }
    // Structured output is plain JSON, but tolerate a fenced block.
    let trimmed = text.trim();
    let json = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .unwrap_or(trimmed);
    let raw: Raw =
        serde_json::from_str(json).map_err(|e| format!("unexpected model response: {e}"))?;
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
    fn should_parse_plain_and_fenced_json_responses() {
        let json = r#"{"overview":"o","steps":[{"title":"t","summary":"s","files":["a"]}]}"#;
        let plain = parse_response(json).unwrap();
        let fenced = parse_response(&format!("```json\n{json}\n```")).unwrap();
        assert_eq!(plain, fenced);
        assert_eq!(plain.steps[0].files, paths(&["a"]));
        assert!(parse_response("not json").is_err());
    }

    #[test]
    fn should_bound_prompt_size_and_keep_every_path() {
        let big = "+x\n".repeat(MAX_FILE_PATCH_BYTES);
        let files: Vec<GuideFile> = (0..40)
            .map(|i| GuideFile {
                path: PathBuf::from(format!("src/f{i}.rs")),
                old_path: None,
                status: FileStatus::Modified,
                patch: big.clone(),
                is_binary: false,
            })
            .collect();
        let input = GuideInput {
            key: PrSessionKey {
                repository: crate::forge::traits::ForgeRepository::github("github.com", "o", "r"),
                number: 1,
                head_sha: "abc".to_string(),
            },
            title: "Title".to_string(),
            description: String::new(),
            files,
        };
        let prompt = build_prompt(&input);
        assert!(prompt.user.len() < MAX_PROMPT_PATCH_BYTES + 20_000);
        assert!(prompt.user.contains("src/f39.rs (modified)"));
        assert!(prompt.user.contains("prompt budget exhausted"));
    }

    #[test]
    fn should_truncate_on_char_boundaries() {
        assert_eq!(truncate_at_char_boundary("héllo", 2), "h");
        assert_eq!(truncate_at_char_boundary("abc", 10), "abc");
    }
}
