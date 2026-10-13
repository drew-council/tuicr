//! App state for the AI review guide (`src/review_guide`).
//!
//! The guide lands asynchronously, after the PR has opened in directory
//! order. It then reorders `diff_files` within the existing directory tree
//! and prepends its overview to the PR description. File indices are
//! absolute, so the reorder remaps everything keyed by `file_idx` and puts
//! the cursor back on the same file line.

use std::ffi::OsString;

use super::*;
use crate::forge::traits::PrSessionKey;
use crate::review_guide::{ReviewGuide, ReviewGuideEvent, ReviewGuideLauncher};

/// One tree level of a file's sort key: the earliest guide rank inside that
/// directory (or the file's own rank), then its name to break ties.
type TreeLevelKey = ((usize, usize), OsString);

/// Steps list at most this many files in the description; the tree shows all.
const MAX_LISTED_FILES: usize = 6;

/// A guide bound to the PR head it describes.
#[derive(Debug, Clone)]
pub struct ActiveReviewGuide {
    pub key: PrSessionKey,
    pub guide: ReviewGuide,
    /// `(step, position)` per file path.
    ranks: HashMap<PathBuf, (usize, usize)>,
    /// Markdown prepended to the PR description.
    description: String,
}

impl ActiveReviewGuide {
    pub fn new(key: PrSessionKey, guide: ReviewGuide) -> Self {
        let ranks = guide.ranks();
        let description = guide_markdown(&guide);
        Self {
            key,
            guide,
            ranks,
            description,
        }
    }
}

fn guide_markdown(guide: &ReviewGuide) -> String {
    let mut md = String::from("**Review guide** _(AI-generated)_\n\n");
    if !guide.overview.is_empty() {
        md.push_str(&guide.overview);
        md.push_str("\n\n");
    }
    for (idx, step) in guide.steps.iter().enumerate() {
        md.push_str(&format!("{}. **{}**", idx + 1, step.title));
        if !step.summary.is_empty() {
            md.push_str(&format!(": {}", step.summary));
        }
        let mut files: Vec<String> = step
            .files
            .iter()
            .take(MAX_LISTED_FILES)
            .map(|path| format!("`{}`", path.display()))
            .collect();
        if step.files.len() > MAX_LISTED_FILES {
            files.push(format!("+{} more", step.files.len() - MAX_LISTED_FILES));
        }
        md.push_str(&format!(" ({})\n", files.join(", ")));
    }
    md.push_str("\n---\n\n");
    md
}

impl App {
    /// Build a launcher for the PR about to be fetched, replacing any
    /// in-flight request. `None` when the guide is disabled.
    pub(in crate::app) fn review_guide_launcher(&mut self) -> Option<ReviewGuideLauncher> {
        let config = self.review_guide_config.clone()?;
        let (tx, rx) = std::sync::mpsc::channel();
        self.review_guide_rx = Some(rx);
        Some(ReviewGuideLauncher::new(
            config,
            crate::review_guide::default_cache_dir(),
            tx,
        ))
    }

    /// The guide for the open PR head, if one has landed.
    pub fn current_review_guide(&self) -> Option<&ActiveReviewGuide> {
        let DiffSource::PullRequest(pr) = &self.diff_source else {
            return None;
        };
        self.review_guide
            .as_ref()
            .filter(|active| active.key == pr.key)
    }

    /// Drain a finished guide request. Returns true when the view changed.
    ///
    /// Waits while a PR open or reload is still in flight: a cached guide can
    /// answer before the PR it belongs to has been parsed and entered, and
    /// checking it against the previous PR would discard it as stale.
    pub fn poll_review_guide_events(&mut self) -> bool {
        if self.pr_open_rx.is_some() || self.pr_reload_rx.is_some() {
            return false;
        }
        let Some(rx) = self.review_guide_rx.as_ref() else {
            return false;
        };
        let event = match rx.try_recv() {
            Ok(event) => event,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            // The fetch failed before the diff arrived, so no request ran.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.review_guide_rx = None;
                return false;
            }
        };
        self.review_guide_rx = None;
        self.apply_review_guide_event(event);
        true
    }

    pub(in crate::app) fn apply_review_guide_event(&mut self, event: ReviewGuideEvent) {
        let ReviewGuideEvent { key, result } = event;
        if !matches!(&self.diff_source, DiffSource::PullRequest(pr) if pr.key == key) {
            return;
        }
        let guide = match result {
            Ok(guide) => guide,
            Err(e) => {
                self.set_warning(format!("Review guide unavailable: {e}"));
                return;
            }
        };
        if self
            .current_review_guide()
            .is_some_and(|active| active.guide == guide)
        {
            return;
        }
        self.reorder_files_preserving_view(|app| {
            app.review_guide = Some(ActiveReviewGuide::new(key, guide));
        });
        // Don't clobber a warning or error the user hasn't seen yet.
        if self.message.is_none() {
            self.set_message("Files ordered by review guide (see PR description)");
        }
    }

    /// Apply the current guide to freshly sorted files: order them by guide
    /// step and prepend the overview to the PR description. Runs at the end
    /// of `sort_files_by_directory`, which every PR load and reload passes
    /// through after installing its `diff_files` and `pr_info`.
    ///
    /// The order stays a valid directory tree: at every level, files and
    /// subdirectories are ordered by the earliest guide step inside them, so
    /// each subtree stays contiguous (`build_visible_items` requires that).
    /// When the guide interleaves directories, the tree order approximates it.
    pub(in crate::app) fn apply_review_guide(&mut self) {
        let Some(active) = self.current_review_guide().cloned() else {
            return;
        };

        if let Some(info) = self.pr_info.as_mut()
            && !info.details.body.starts_with(&active.description)
        {
            let body = if info.details.body.trim().is_empty() {
                "_(no description)_"
            } else {
                info.details.body.as_str()
            };
            info.details.body = format!("{}{body}", active.description);
        }

        // Files the guide did not place go last, in their current order.
        let ranks: Vec<(usize, usize)> = self
            .diff_files
            .iter()
            .enumerate()
            .map(|(idx, file)| {
                active
                    .ranks
                    .get(file.display_path())
                    .copied()
                    .unwrap_or((usize::MAX, idx))
            })
            .collect();
        let mut subtree_rank: HashMap<&Path, (usize, usize)> = HashMap::new();
        for (file, rank) in self.diff_files.iter().zip(&ranks) {
            for dir in file.display_path().ancestors().skip(1) {
                let entry = subtree_rank.entry(dir).or_insert(*rank);
                *entry = (*entry).min(*rank);
            }
        }
        let keys: Vec<(bool, Vec<TreeLevelKey>)> = self
            .diff_files
            .iter()
            .zip(&ranks)
            .map(|(file, rank)| {
                let path = file.display_path();
                let mut key: Vec<TreeLevelKey> = path
                    .parent()
                    .into_iter()
                    .flat_map(Path::ancestors)
                    .filter(|dir| !dir.as_os_str().is_empty())
                    .map(|dir| {
                        let name = dir.file_name().unwrap_or_default().to_os_string();
                        (subtree_rank[dir], name)
                    })
                    .collect();
                key.reverse();
                key.push((*rank, path.file_name().unwrap_or_default().to_os_string()));
                (!file.is_commit_message, key)
            })
            .collect();
        let mut order: Vec<usize> = (0..self.diff_files.len()).collect();
        order.sort_by(|a, b| keys[*a].cmp(&keys[*b]));
        let mut files: Vec<Option<DiffFile>> = self.diff_files.drain(..).map(Some).collect();
        self.diff_files = order
            .into_iter()
            .filter_map(|idx| files[idx].take())
            .collect();
    }

    /// Apply `change` (anything that alters the file order or the PR panel's
    /// height), re-sort `diff_files`, and put the cursor back where it was: the
    /// same overview line, or the same line of the same file at the same
    /// viewport row. The cursor is measured before `change` runs, against the
    /// layout it was placed in.
    fn reorder_files_preserving_view(&mut self, change: impl FnOnce(&mut Self)) {
        if self.diff_files.is_empty() {
            change(self);
            self.rebuild_annotations();
            return;
        }
        let overview_cursor = self
            .is_cursor_in_overview()
            .then_some(self.diff_state.cursor_line);
        let current_path = self.current_file_path().cloned();
        let file_start = self.calculate_file_scroll_offset(self.diff_state.current_file_idx);
        let relative_line = self.diff_state.cursor_line.saturating_sub(file_start);
        let viewport_row = self
            .diff_state
            .cursor_line
            .saturating_sub(self.diff_state.scroll_offset);
        let old_paths: Vec<PathBuf> = self
            .diff_files
            .iter()
            .map(|file| file.display_path().clone())
            .collect();

        change(self);
        self.sort_files_by_directory(true);
        self.remap_file_indices(&old_paths);
        self.expand_all_dirs();
        self.rebuild_annotations();

        if let Some(line) = overview_cursor {
            // The overview went into the PR description panel, which grew;
            // the cursor stays on the same row.
            self.diff_state.cursor_line = line.min(self.max_cursor_line());
            self.diff_state.scroll_offset = self.diff_state.scroll_offset.min(line);
            self.ensure_cursor_visible();
            return;
        }
        let Some(target_idx) = current_path.and_then(|path| {
            self.diff_files
                .iter()
                .position(|file| *file.display_path() == path)
        }) else {
            return;
        };
        self.jump_to_file(target_idx);
        let start = self.calculate_file_scroll_offset(target_idx);
        let height = self.file_render_height(target_idx, &self.diff_files[target_idx]);
        self.diff_state.cursor_line = start + relative_line.min(height.saturating_sub(1));
        self.diff_state.scroll_offset = self
            .diff_state
            .cursor_line
            .saturating_sub(viewport_row)
            .min(self.max_scroll_offset());
        self.ensure_cursor_visible();
        self.update_current_file_from_cursor();
    }

    /// Re-key file-index-addressed state after `diff_files` was permuted.
    /// `old_paths[i]` is the path that sat at index `i` before.
    fn remap_file_indices(&mut self, old_paths: &[PathBuf]) {
        let new_idx: HashMap<&PathBuf, usize> = self
            .diff_files
            .iter()
            .enumerate()
            .map(|(idx, file)| (file.display_path(), idx))
            .collect();
        let remap = |map: &mut HashMap<GapId, Vec<DiffLine>>| {
            *map = std::mem::take(map)
                .into_iter()
                .filter_map(|(gap, lines)| {
                    let file_idx = *new_idx.get(old_paths.get(gap.file_idx)?)?;
                    Some((GapId { file_idx, ..gap }, lines))
                })
                .collect();
        };
        remap(&mut self.expanded_top);
        remap(&mut self.expanded_bottom);
        self.file_line_count_cache.clear();
        self.populate_file_line_count_cache();
    }
}
