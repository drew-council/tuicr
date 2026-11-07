//! The action vocabulary shared by key bindings and `:` commands.
//!
//! Every user-facing behavior is an [`Action`]. Keys and commands both
//! resolve to one, and a handler for the active context performs it. Actions
//! listed in [`ACTIONS`] carry a stable kebab-case name, which is what the
//! keymap config refers to. Variants without a spec are internal input
//! events (typed text, paste, mouse) and cannot be bound.

/// A single user intent, independent of the key or command that produced it.
///
/// Motion variants carry a step size. A count typed before the key multiplies
/// it (see [`CountMode::Scale`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Navigation
    CursorDown(usize),
    CursorUp(usize),
    HalfPageDown,
    HalfPageUp,
    PageDown,
    PageUp,
    GoToTop,
    GoToBottom,
    /// Jump to the new-side source line given by the count (`{N}G`, `:N`).
    GoToLine,
    /// Jump to the old-side source line given by the count (`:oN`).
    GoToOldLine,
    Digit(u8),
    NextFile,
    PrevFile,
    NextHunk,
    PrevHunk,
    NextComment,
    PrevComment,
    PendingZCommand,
    PendingShiftZCommand,
    PendingLeaderCommand,
    ScrollLeft(usize),
    ScrollRight(usize),
    ScrollViewDown(usize),
    ScrollViewUp(usize),
    MouseScrollUp(usize),
    MouseScrollDown(usize),

    // Panel focus
    ToggleFocus,
    ToggleFocusReverse,
    /// Activate the item under the cursor (Enter).
    Select,
    /// Activate the item under the cursor fully (Shift-Enter).
    SelectFull,

    // Review actions
    ToggleReviewed,
    ToggleHunkReviewed,
    AddLineComment,
    AddFileComment,
    EditComment,
    /// Edit the comment at cursor with the text cursor at end (vim `A`).
    EditCommentAtEnd,
    PendingDCommand,
    EditFile,
    SearchNext,
    SearchPrev,
    ClearSearchHighlight,

    // Visual selection mode
    EnterVisualMode,
    AddRangeComment,
    AddRangeSuggestion,
    /// Copy the visual selection's text.
    CopySelection,

    // Session
    Quit,
    /// Transitional hint for the removed `q`-quits binding: shows a message
    /// pointing at `:q` instead of quitting. Drop this a few releases after
    /// the `q` removal has had time to land.
    QuitHint,
    ExportToClipboard,
    /// Copy just the comment under the cursor (`Y`), not the whole review.
    CopyCommentAtCursor,

    // Mode changes
    EnterCommandMode,
    /// Open the `/` prompt. Lists interpret it as their filter prompt.
    EnterSearchMode,
    ExitMode,
    ToggleHelp,

    // Screen
    /// Clear the terminal and repaint every cell on the next frame (`Ctrl-l`).
    Redraw,

    // Text input
    InsertChar(char),
    /// Bracketed-paste payload from the terminal. Carries the full pasted
    /// string so multi-line / shifted input doesn't get re-interpreted as
    /// keystrokes (Enter submitting, ':' opening command mode, etc.).
    Paste(String),
    DeleteChar,
    DeleteWord,
    ClearLine,
    SubmitInput,
    CompleteCommand,
    CompleteCommandReverse,
    TextCursorLeft,
    TextCursorRight,
    TextCursorUp,
    TextCursorDown,
    TextCursorLineStart,
    TextCursorLineEnd,
    TextCursorWordLeft,
    TextCursorWordRight,

    // Comment type
    CycleCommentType,
    CycleCommentTypeReverse,

    // Confirm dialog
    ConfirmYes,
    ConfirmNo,

    // Lists
    /// Toggle the list item under the cursor (Space).
    ToggleSelect,
    /// Cycle inline commit selector to next individual commit (`)`)
    CycleCommitNext,
    /// Cycle inline commit selector to previous individual commit (`(`)
    CycleCommitPrev,

    // Review target selector
    TargetSelectorTabNext,
    TargetSelectorTabPrev,
    /// Toggle PR tab between all open PRs and PRs awaiting your review.
    TogglePrReviewRequestedFilter,
    /// Sessions tab: show or hide sessions holding no review progress.
    SessionsTabToggleShowEmpty,
    /// Sessions tab: delete the session under the cursor, after a confirm.
    SessionsTabDelete,

    // Submit flow
    /// Advance from the resolver to final confirmation.
    SubmitResolverAdvance,
    /// Reload the PR from the submit confirmation (stale-head warning only).
    SubmitReloadPr,

    ToggleExpand,
    ExpandAll,
    CollapseAll,

    // File tree filters
    FileTreeFilterInclude,
    FileTreeFilterExclude,
    FileTreeClearInclude,
    FileTreeClearExclude,

    // No-op
    None,
}

/// How a count typed before a key (`5j`) applies to the action it selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountMode {
    /// The count is dropped.
    Ignore,
    /// The count multiplies the action's step size.
    Scale,
    /// The action runs once per count.
    Repeat,
    /// With a count, the action becomes [`Action::GoToLine`].
    Line,
    /// The handler reads the count itself.
    Pass,
}

/// An action paired with the `{N}` count typed before it, if any.
///
/// Keys and `:` commands both produce one. `:42` is
/// `Invocation { action: GoToLine, count: Some(42) }`, the same as `42G`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub action: Action,
    pub count: Option<usize>,
}

impl Invocation {
    pub fn new(action: Action, count: Option<usize>) -> Self {
        Self { action, count }
    }

    /// Apply the count through the action's [`CountMode`]. Returns the
    /// invocation to run and how many times to run it. The returned count is
    /// kept only for actions that read it themselves.
    pub fn resolve(self) -> (Invocation, usize) {
        let Some(count) = self.count else {
            return (self, 1);
        };
        let count = count.max(1);
        let action = self.action;
        match action.count_mode() {
            CountMode::Ignore => (Invocation::from(action), 1),
            CountMode::Scale => (Invocation::from(action.scaled(count)), 1),
            CountMode::Repeat => (Invocation::from(action), count),
            CountMode::Line => (Invocation::new(Action::GoToLine, Some(count)), 1),
            CountMode::Pass => (Invocation::new(action, Some(count)), 1),
        }
    }
}

impl From<Action> for Invocation {
    fn from(action: Action) -> Self {
        Self::new(action, None)
    }
}

/// Registry entry for a bindable action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    /// The action, with its default step size where it has one.
    pub action: Action,
    /// Stable kebab-case identifier used by the keymap config.
    pub name: &'static str,
    pub count: CountMode,
    /// One-line description for the help popup.
    pub help: &'static str,
}

const fn spec(
    action: Action,
    name: &'static str,
    count: CountMode,
    help: &'static str,
) -> ActionSpec {
    ActionSpec {
        action,
        name,
        count,
        help,
    }
}

use CountMode::{Ignore, Line, Pass, Repeat, Scale};

/// Every bindable action. Order is the help popup's order within a context.
pub const ACTIONS: &[ActionSpec] = &[
    // Navigation
    spec(Action::CursorDown(1), "cursor-down", Scale, "Move down"),
    spec(Action::CursorUp(1), "cursor-up", Scale, "Move up"),
    spec(
        Action::ScrollViewDown(1),
        "scroll-view-down",
        Scale,
        "Scroll view down",
    ),
    spec(
        Action::ScrollViewUp(1),
        "scroll-view-up",
        Scale,
        "Scroll view up",
    ),
    spec(
        Action::HalfPageDown,
        "half-page-down",
        Ignore,
        "Half page down",
    ),
    spec(Action::HalfPageUp, "half-page-up", Ignore, "Half page up"),
    spec(Action::PageDown, "page-down", Ignore, "Full page down"),
    spec(Action::PageUp, "page-up", Ignore, "Full page up"),
    spec(Action::ScrollLeft(4), "scroll-left", Scale, "Scroll left"),
    spec(
        Action::ScrollRight(4),
        "scroll-right",
        Scale,
        "Scroll right",
    ),
    spec(Action::GoToTop, "go-to-top", Ignore, "Go to top"),
    spec(
        Action::GoToBottom,
        "go-to-bottom",
        Line,
        "Go to bottom ({N} jumps to line N)",
    ),
    spec(
        Action::GoToLine,
        "go-to-line",
        Pass,
        "Go to new-side line {N}",
    ),
    spec(
        Action::GoToOldLine,
        "go-to-old-line",
        Pass,
        "Go to old-side line {N}",
    ),
    spec(Action::NextFile, "next-file", Repeat, "Next file"),
    spec(Action::PrevFile, "prev-file", Repeat, "Previous file"),
    spec(Action::NextHunk, "next-hunk", Repeat, "Next hunk"),
    spec(Action::PrevHunk, "prev-hunk", Repeat, "Previous hunk"),
    spec(Action::NextComment, "next-comment", Ignore, "Next comment"),
    spec(
        Action::PrevComment,
        "prev-comment",
        Ignore,
        "Previous comment",
    ),
    spec(
        Action::CycleCommitNext,
        "next-commit",
        Ignore,
        "Show next commit",
    ),
    spec(
        Action::CycleCommitPrev,
        "prev-commit",
        Ignore,
        "Show previous commit",
    ),
    // Panels and lists
    spec(Action::ToggleFocus, "focus-next", Ignore, "Focus next pane"),
    spec(
        Action::ToggleFocusReverse,
        "focus-prev",
        Ignore,
        "Focus previous pane",
    ),
    spec(
        Action::Select,
        "select",
        Ignore,
        "Open or activate the item under the cursor",
    ),
    spec(
        Action::SelectFull,
        "select-full",
        Ignore,
        "Expand the whole gap under the cursor",
    ),
    spec(
        Action::ToggleSelect,
        "toggle-select",
        Ignore,
        "Toggle the item under the cursor",
    ),
    spec(
        Action::ToggleExpand,
        "toggle-expand",
        Ignore,
        "Toggle directory or gap",
    ),
    spec(
        Action::ExpandAll,
        "expand-all",
        Ignore,
        "Expand all directories",
    ),
    spec(
        Action::CollapseAll,
        "collapse-all",
        Ignore,
        "Collapse all directories",
    ),
    spec(
        Action::FileTreeFilterInclude,
        "filter-include",
        Ignore,
        "Include files matching a regex",
    ),
    spec(
        Action::FileTreeFilterExclude,
        "filter-exclude",
        Ignore,
        "Exclude files matching a regex",
    ),
    spec(
        Action::FileTreeClearInclude,
        "clear-include",
        Ignore,
        "Clear the include filter",
    ),
    spec(
        Action::FileTreeClearExclude,
        "clear-exclude",
        Ignore,
        "Clear the exclude filter",
    ),
    // Review
    spec(
        Action::ToggleReviewed,
        "toggle-reviewed",
        Ignore,
        "Toggle file reviewed",
    ),
    spec(
        Action::ToggleHunkReviewed,
        "toggle-hunk-reviewed",
        Ignore,
        "Toggle hunk reviewed",
    ),
    spec(
        Action::AddLineComment,
        "comment-line",
        Ignore,
        "Comment on the cursor line",
    ),
    spec(
        Action::AddFileComment,
        "comment-file",
        Ignore,
        "Comment on the file",
    ),
    spec(
        Action::EditComment,
        "edit-comment",
        Ignore,
        "Edit the comment at cursor",
    ),
    spec(
        Action::EditCommentAtEnd,
        "edit-comment-at-end",
        Ignore,
        "Edit the comment at cursor, text cursor at end",
    ),
    spec(
        Action::EditFile,
        "edit-file",
        Ignore,
        "Open the focused file in $EDITOR",
    ),
    spec(
        Action::EnterVisualMode,
        "visual-mode",
        Ignore,
        "Start a visual selection",
    ),
    spec(
        Action::AddRangeComment,
        "comment-range",
        Ignore,
        "Comment on the selection",
    ),
    spec(
        Action::AddRangeSuggestion,
        "suggest-range",
        Ignore,
        "Suggest a change to the selection",
    ),
    spec(
        Action::CopySelection,
        "copy-selection",
        Ignore,
        "Copy the selected text",
    ),
    spec(
        Action::ExportToClipboard,
        "export",
        Ignore,
        "Copy the review to the clipboard",
    ),
    spec(
        Action::CopyCommentAtCursor,
        "copy-comment",
        Ignore,
        "Copy the comment at cursor",
    ),
    // Search
    spec(
        Action::EnterSearchMode,
        "search",
        Ignore,
        "Search (filter in lists)",
    ),
    spec(
        Action::SearchNext,
        "search-next",
        Ignore,
        "Next search match",
    ),
    spec(
        Action::SearchPrev,
        "search-prev",
        Ignore,
        "Previous search match",
    ),
    spec(
        Action::ClearSearchHighlight,
        "clear-search-highlight",
        Ignore,
        "Clear search highlight",
    ),
    // Modes and session
    spec(
        Action::EnterCommandMode,
        "command-mode",
        Ignore,
        "Enter command mode",
    ),
    spec(Action::ToggleHelp, "help", Ignore, "Toggle help"),
    spec(Action::ExitMode, "exit", Ignore, "Close or cancel"),
    spec(Action::Quit, "quit", Ignore, "Quit"),
    spec(Action::Redraw, "redraw", Ignore, "Redraw the screen"),
    // Dialogs
    spec(Action::ConfirmYes, "yes", Ignore, "Confirm"),
    spec(Action::ConfirmNo, "no", Ignore, "Decline"),
    // Review target selector
    spec(
        Action::TargetSelectorTabNext,
        "next-tab",
        Ignore,
        "Next tab",
    ),
    spec(
        Action::TargetSelectorTabPrev,
        "prev-tab",
        Ignore,
        "Previous tab",
    ),
    spec(
        Action::TogglePrReviewRequestedFilter,
        "toggle-review-requested",
        Ignore,
        "Toggle PRs awaiting your review",
    ),
    spec(
        Action::SessionsTabToggleShowEmpty,
        "toggle-empty-sessions",
        Ignore,
        "Show or hide empty sessions",
    ),
    spec(
        Action::SessionsTabDelete,
        "delete-session",
        Ignore,
        "Delete the session under the cursor",
    ),
    // Submit flow
    spec(
        Action::SubmitResolverAdvance,
        "submit-advance",
        Ignore,
        "Continue to submit confirmation",
    ),
    spec(
        Action::SubmitReloadPr,
        "reload-pr",
        Ignore,
        "Reload the pull request",
    ),
];

impl Action {
    /// The registry entry for this action, or `None` for internal events.
    pub fn spec(&self) -> Option<&'static ActionSpec> {
        let id = std::mem::discriminant(self);
        ACTIONS
            .iter()
            .find(|spec| std::mem::discriminant(&spec.action) == id)
    }

    /// Look up a bindable action by its config name.
    pub fn from_name(name: &str) -> Option<Action> {
        ACTIONS
            .iter()
            .find(|spec| spec.name == name)
            .map(|spec| spec.action.clone())
    }

    pub fn name(&self) -> Option<&'static str> {
        self.spec().map(|spec| spec.name)
    }

    pub fn count_mode(&self) -> CountMode {
        self.spec().map_or(Ignore, |spec| spec.count)
    }

    /// Multiply a motion's step size by `count`. Other actions are unchanged.
    pub fn scaled(mut self, count: usize) -> Action {
        match &mut self {
            Action::CursorDown(n)
            | Action::CursorUp(n)
            | Action::ScrollLeft(n)
            | Action::ScrollRight(n)
            | Action::ScrollViewDown(n)
            | Action::ScrollViewUp(n) => *n = n.saturating_mul(count),
            _ => {}
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn should_give_every_action_a_unique_name() {
        let mut names = HashSet::new();
        for spec in ACTIONS {
            assert!(names.insert(spec.name), "duplicate name {}", spec.name);
        }
    }

    #[test]
    fn should_register_each_action_once() {
        let mut seen = HashSet::new();
        for spec in ACTIONS {
            assert!(
                seen.insert(std::mem::discriminant(&spec.action)),
                "{} registered twice",
                spec.name
            );
        }
    }

    #[test]
    fn should_use_kebab_case_names() {
        for spec in ACTIONS {
            assert!(
                spec.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{} is not kebab-case",
                spec.name
            );
        }
    }

    #[test]
    fn should_round_trip_names() {
        for spec in ACTIONS {
            let action = Action::from_name(spec.name).expect("registered name");
            assert_eq!(action.name(), Some(spec.name));
        }
    }

    #[test]
    fn should_leave_internal_events_unnamed() {
        assert_eq!(Action::InsertChar('x').name(), None);
        assert_eq!(Action::Paste("x".into()).name(), None);
        assert_eq!(Action::MouseScrollDown(3).name(), None);
        assert_eq!(Action::None.name(), None);
    }

    #[test]
    fn should_run_once_without_a_count() {
        let (run, times) = Invocation::from(Action::NextFile).resolve();
        assert_eq!((run, times), (Invocation::from(Action::NextFile), 1));
    }

    #[test]
    fn should_resolve_each_count_mode() {
        let resolve = |action, count| Invocation::new(action, Some(count)).resolve();
        assert_eq!(
            resolve(Action::CursorDown(1), 5),
            (Invocation::from(Action::CursorDown(5)), 1)
        );
        assert_eq!(
            resolve(Action::NextHunk, 3),
            (Invocation::from(Action::NextHunk), 3)
        );
        assert_eq!(
            resolve(Action::GoToBottom, 42),
            (Invocation::new(Action::GoToLine, Some(42)), 1)
        );
        assert_eq!(
            resolve(Action::GoToOldLine, 7),
            (Invocation::new(Action::GoToOldLine, Some(7)), 1)
        );
        assert_eq!(
            resolve(Action::NextComment, 4),
            (Invocation::from(Action::NextComment), 1)
        );
    }

    #[test]
    fn should_treat_a_zero_count_as_one() {
        assert_eq!(
            Invocation::new(Action::NextFile, Some(0)).resolve(),
            (Invocation::from(Action::NextFile), 1)
        );
    }

    #[test]
    fn should_scale_motion_step_by_count() {
        assert_eq!(Action::CursorDown(1).scaled(5), Action::CursorDown(5));
        assert_eq!(Action::ScrollLeft(4).scaled(3), Action::ScrollLeft(12));
        assert_eq!(Action::NextFile.scaled(3), Action::NextFile);
    }
}
