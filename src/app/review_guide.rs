//! App state for the AI review guide (`src/review_guide`).
//!
//! The guide lands asynchronously, after the PR has opened in directory
//! order. It then reorders `diff_files` within the existing directory tree,
//! and the PR info panel grows a "Review guide" section. File indices are
//! absolute, so the reorder remaps everything keyed by `file_idx` and puts
//! the cursor back on the same file line.

use std::ffi::OsString;

use super::*;
use crate::forge::traits::PrSessionKey;
use crate::review_guide::{ReviewGuide, ReviewGuideEvent, ReviewGuideLauncher};

/// One tree level of a file's sort key: the earliest guide rank inside that
/// directory (or the file's own rank), then its name to break ties.
type TreeLevelKey = ((usize, usize), OsString);

/// A guide bound to the PR head it describes.
#[derive(Debug, Clone)]
pub struct ActiveReviewGuide {
    pub key: PrSessionKey,
    pub guide: ReviewGuide,
    /// `(step, position)` per file path.
    ranks: HashMap<PathBuf, (usize, usize)>,
}

impl ActiveReviewGuide {
    pub fn new(key: PrSessionKey, guide: ReviewGuide) -> Self {
        let ranks = guide.ranks();
        Self { key, guide, ranks }
    }
}

/// The file list's selected row, by what it shows rather than where it sits.
enum TreeSelection {
    File(PathBuf),
    Directory(String),
}

impl App {
    /// Build a launcher for the PR about to be fetched, replacing any
    /// in-flight request. `None` when the guide is disabled.
    pub(in crate::app) fn review_guide_launcher(&mut self) -> Option<ReviewGuideLauncher> {
        let config = self.review_guide_config.clone()?;
        let (launcher, pending) =
            ReviewGuideLauncher::new(config, crate::review_guide::default_cache_dir());
        self.review_guide_pending = Some(pending);
        Some(launcher)
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

    /// True while the guide command for the open PR has not answered yet.
    pub fn review_guide_in_flight(&self) -> bool {
        self.review_guide_pending.is_some()
            && matches!(self.diff_source, DiffSource::PullRequest(_))
    }

    /// Drain a finished guide request. Returns true when the view changed.
    ///
    /// Waits while a PR open or reload is still in flight: a cached guide can
    /// answer before the PR it belongs to has been parsed and entered, and
    /// checking it against the previous PR would discard it as stale.
    ///
    /// Also waits for plain Normal mode. The guide permutes `diff_files` and
    /// grows the PR panel, and a comment being written, a visual selection,
    /// a prompt, or a popup all hold rows or file indices from before that.
    pub fn poll_review_guide_events(&mut self) -> bool {
        if self.pr_open_rx.is_some() || self.pr_reload_rx.is_some() {
            return false;
        }
        if self.input_mode != InputMode::Normal || self.file_filter.draft.is_some() {
            return false;
        }
        let Some(pending) = self.review_guide_pending.as_ref() else {
            return false;
        };
        let event = match pending.try_recv() {
            Ok(event) => event,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            // The fetch failed before the diff arrived, so no request ran.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.review_guide_pending = None;
                return false;
            }
        };
        // A fetch retried after the head moved launched a second request;
        // keep the channel open for the one matching the PR that was entered.
        if !matches!(&self.diff_source, DiffSource::PullRequest(pr) if pr.key == event.key) {
            return false;
        }
        self.review_guide_pending = None;
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
            self.set_message("Files ordered by review guide");
        }
    }

    /// Order freshly sorted files by the current guide's steps. Runs at the
    /// end of `sort_files_by_directory`, which every PR load and reload passes
    /// through after installing its `diff_files`.
    ///
    /// The order stays a valid directory tree: at every level, files and
    /// subdirectories are ordered by the earliest guide step inside them, so
    /// each subtree stays contiguous (`build_visible_items` requires that).
    /// When the guide interleaves directories, the tree order approximates it.
    pub(in crate::app) fn apply_review_guide(&mut self) {
        let Some(active) = self.current_review_guide() else {
            return;
        };

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
    /// height), re-sort `diff_files`, and put the view back where it was: the
    /// same overview row, or the same line of the same file at the same
    /// viewport row. Everything is measured before `change` runs, against the
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
        let old_guide_rows = crate::ui::pr_info_panel::pr_info_guide_rows(self);
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
        let tree_selection = self.get_selected_tree_item().map(|item| match item {
            FileTreeItem::File { file_idx, .. } => TreeSelection::File(old_paths[file_idx].clone()),
            FileTreeItem::Directory { path, .. } => TreeSelection::Directory(path),
        });
        // `sort_files_by_directory` clears the cache, and refilling it costs
        // a forge round trip on this thread. A permutation changes no count.
        let line_counts = std::mem::take(&mut self.file_line_count_cache);

        change(self);
        self.sort_files_by_directory(true);
        self.remap_file_indices(&old_paths, line_counts);
        // An overview cursor has no file to follow; the sort left it on file 0.
        let target_idx = current_path
            .filter(|_| overview_cursor.is_none())
            .and_then(|path| {
                self.diff_files
                    .iter()
                    .position(|file| *file.display_path() == path)
            });
        // Single-file view builds annotations for `current_file_idx` only.
        if let Some(idx) = target_idx {
            self.diff_state.current_file_idx = idx;
        }
        self.rebuild_annotations();

        let cursor_line = match (overview_cursor, target_idx) {
            (Some(line), _) => {
                // The guide section sits inside the PR panel: rows above it
                // stay put, rows below it move by however much it grew.
                let old = old_guide_rows;
                let new = crate::ui::pr_info_panel::pr_info_guide_rows(self);
                if line >= old.end {
                    line - old.len() + new.len()
                } else if line >= old.start {
                    new.start + (line - old.start).min(new.len().saturating_sub(1))
                } else {
                    line
                }
            }
            (None, Some(idx)) => {
                let start = self.calculate_file_scroll_offset(idx);
                let height = self.file_render_height(idx, &self.diff_files[idx]);
                start + relative_line.min(height.saturating_sub(1))
            }
            (None, None) => 0,
        };
        self.diff_state.cursor_line = cursor_line.min(self.max_cursor_line());
        self.diff_state.scroll_offset = self
            .diff_state
            .cursor_line
            .saturating_sub(viewport_row)
            .min(self.max_scroll_offset());
        self.ensure_cursor_visible();
        // Last: the cursor helpers above move the file list selection too.
        self.restore_tree_selection(tree_selection);
    }

    fn restore_tree_selection(&mut self, selection: Option<TreeSelection>) {
        let Some(selection) = selection else {
            return;
        };
        let tree_idx =
            self.build_visible_items()
                .iter()
                .position(|item| match (item, &selection) {
                    (FileTreeItem::File { file_idx, .. }, TreeSelection::File(path)) => {
                        self.diff_files[*file_idx].display_path() == path
                    }
                    (FileTreeItem::Directory { path, .. }, TreeSelection::Directory(dir)) => {
                        path == dir
                    }
                    _ => false,
                });
        if let Some(tree_idx) = tree_idx {
            self.file_list_state.select(tree_idx);
        }
    }

    /// Re-key file-index-addressed state after `diff_files` was permuted.
    /// `old_paths[i]` is the path that sat at index `i` before, and
    /// `line_counts` is the line count cache from before the permutation.
    fn remap_file_indices(&mut self, old_paths: &[PathBuf], line_counts: HashMap<usize, u32>) {
        let new_idx: HashMap<&PathBuf, usize> = self
            .diff_files
            .iter()
            .enumerate()
            .map(|(idx, file)| (file.display_path(), idx))
            .collect();
        let moved = |old_idx: usize| new_idx.get(old_paths.get(old_idx)?).copied();
        let remap = |map: &mut HashMap<GapId, Vec<DiffLine>>| {
            *map = std::mem::take(map)
                .into_iter()
                .filter_map(|(gap, lines)| {
                    let file_idx = moved(gap.file_idx)?;
                    Some((GapId { file_idx, ..gap }, lines))
                })
                .collect();
        };
        remap(&mut self.expanded_top);
        remap(&mut self.expanded_bottom);
        self.file_line_count_cache = line_counts
            .into_iter()
            .filter_map(|(old_idx, count)| Some((moved(old_idx)?, count)))
            .collect();
    }
}
