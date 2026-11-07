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
    AddReviewComment,
    /// Delete the comment under the cursor (`dd`).
    DeleteComment,
    EditFile,
    Stage,
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
    /// Quit, refusing while unsaved comments exist (`:q`).
    Quit,
    /// Quit from a key (`q` under `q_quits`): unsaved comments in a review
    /// pane take a second press.
    ConfirmQuit,
    /// Quit without saving (`:q!`, `ZQ`).
    ForceQuit,
    Write,
    /// Save, offer to copy the review, then quit (`:x`).
    WriteQuit,
    /// Save, copy the review without asking, then quit (`ZZ`).
    ExportQuit,
    Reload,
    CopyUrl,
    /// Clear comments and reviewed marks.
    ClearAll,
    ClearComments,
    MessageDetails,
    Summary,
    ThemePicker,
    /// Open the file picker popup (`<leader>p`).
    FilePicker,
    Version,
    CheckUpdate,
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
    /// Put the cursor line at the middle of the view (`zz`).
    CenterCursor,
    /// Put the cursor line at the top of the view (`zt`).
    CursorToTop,
    /// Put the cursor line at the bottom of the view (`zb`).
    CursorToBottom,
    ToggleFileList,
    FocusPaneLeft,
    FocusPaneRight,
    FocusPaneUp,
    FocusPaneDown,
    ToggleDiffView,
    ToggleSingleFileView,

    // Options
    SetWrap,
    ToggleWrap,
    SetRelativeNumber,
    SetNoRelativeNumber,
    ToggleRelativeNumber,
    SetWordDiff,
    SetNoWordDiff,
    ToggleWordDiff,
    SetVim,
    SetNoVim,
    ToggleVim,
    ShowCommits,
    HideCommits,
    ToggleCommits,
    ShowReviewed,
    HideReviewed,
    ToggleShowReviewed,
    SetMinimalUi,
    SetNoMinimalUi,
    ToggleMinimalUi,
    CollapseGenerated,
    ShowGenerated,
    ToggleGenerated,

    // Text input and the comment box. These come from the fixed maps in
    // `text_input.rs` and from paste events. They have no `ACTIONS` entry, so
    // `[keys]` cannot bind them.
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
    OpenCommits,
    OpenPullRequests,
    OpenSessions,
    TargetSelectorTabNext,
    TargetSelectorTabPrev,
    /// Toggle PR tab between all open PRs and PRs awaiting your review.
    TogglePrReviewRequestedFilter,
    /// Sessions tab: show or hide sessions holding no review progress.
    SessionsTabToggleShowEmpty,
    /// Sessions tab: delete the session under the cursor, after a confirm.
    SessionsTabDelete,

    // Submit flow
    /// Open the submit action picker (`:submit`).
    SubmitPicker,
    SubmitComment,
    SubmitApprove,
    SubmitRequestChanges,
    SubmitDraft,
    CommentsUnresolved,
    CommentsAll,
    CommentsHide,
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
    /// `:` command names that run this action. Often empty.
    pub commands: &'static [&'static str],
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
        commands: &[],
    }
}

impl ActionSpec {
    const fn cmd(mut self, commands: &'static [&'static str]) -> Self {
        self.commands = commands;
        self
    }
}

use Action as A;
use CountMode::{Ignore, Line, Pass, Repeat, Scale};

/// Every bindable action. Command completion cycles in this order.
#[rustfmt::skip]
pub const ACTIONS: &[ActionSpec] = &[
    // Navigation
    spec(A::CursorDown(1), "cursor-down", Scale, "Move down"),
    spec(A::CursorUp(1), "cursor-up", Scale, "Move up"),
    spec(A::ScrollViewDown(1), "scroll-view-down", Scale, "Scroll view down"),
    spec(A::ScrollViewUp(1), "scroll-view-up", Scale, "Scroll view up"),
    spec(A::HalfPageDown, "half-page-down", Ignore, "Half page down"),
    spec(A::HalfPageUp, "half-page-up", Ignore, "Half page up"),
    spec(A::PageDown, "page-down", Ignore, "Full page down"),
    spec(A::PageUp, "page-up", Ignore, "Full page up"),
    spec(A::ScrollLeft(4), "scroll-left", Scale, "Scroll left"),
    spec(A::ScrollRight(4), "scroll-right", Scale, "Scroll right"),
    spec(A::GoToTop, "go-to-top", Ignore, "Go to top"),
    spec(A::GoToBottom, "go-to-bottom", Line, "Go to bottom ({N} jumps to line N)"),
    spec(A::GoToLine, "go-to-line", Pass, "Go to new-side line {N}"),
    spec(A::GoToOldLine, "go-to-old-line", Pass, "Go to old-side line {N}"),
    spec(A::NextFile, "next-file", Repeat, "Next file"),
    spec(A::PrevFile, "prev-file", Repeat, "Previous file"),
    spec(A::NextHunk, "next-hunk", Repeat, "Next hunk"),
    spec(A::PrevHunk, "prev-hunk", Repeat, "Previous hunk"),
    spec(A::NextComment, "next-comment", Ignore, "Next comment"),
    spec(A::PrevComment, "prev-comment", Ignore, "Previous comment"),
    spec(A::CycleCommitNext, "next-commit", Ignore, "Show next commit"),
    spec(A::CycleCommitPrev, "prev-commit", Ignore, "Show previous commit"),
    spec(A::CenterCursor, "center-cursor", Ignore, "Center the cursor line"),
    spec(A::CursorToTop, "cursor-to-top", Ignore, "Scroll the cursor line to the top"),
    spec(A::CursorToBottom, "cursor-to-bottom", Ignore, "Scroll the cursor line to the bottom"),
    // Panels and lists
    spec(A::ToggleFocus, "focus-next", Ignore, "Focus next pane"),
    spec(A::ToggleFocusReverse, "focus-prev", Ignore, "Focus previous pane"),
    spec(A::FocusPaneLeft, "focus-left", Ignore, "Focus the pane to the left"),
    spec(A::FocusPaneRight, "focus-right", Ignore, "Focus the pane to the right"),
    spec(A::FocusPaneUp, "focus-up", Ignore, "Focus the pane above"),
    spec(A::FocusPaneDown, "focus-down", Ignore, "Focus the pane below"),
    spec(A::ToggleFileList, "toggle-file-list", Ignore, "Show or hide the file list"),
    spec(A::Select, "select", Ignore, "Open or activate the item under the cursor"),
    spec(A::SelectFull, "select-full", Ignore, "Expand the whole gap under the cursor"),
    spec(A::ToggleSelect, "toggle-select", Ignore, "Toggle the item under the cursor"),
    spec(A::ToggleExpand, "toggle-expand", Ignore, "Expand or collapse the file, directory or commit"),
    spec(A::ExpandAll, "expand-all", Ignore, "Expand all directories"),
    spec(A::CollapseAll, "collapse-all", Ignore, "Collapse all directories"),
    spec(A::FileTreeFilterInclude, "filter-include", Ignore, "Include files matching a regex"),
    spec(A::FileTreeFilterExclude, "filter-exclude", Ignore, "Exclude files matching a regex"),
    spec(A::FileTreeClearInclude, "clear-include", Ignore, "Clear the include filter"),
    spec(A::FileTreeClearExclude, "clear-exclude", Ignore, "Clear the exclude filter"),
    // Review
    spec(A::ToggleReviewed, "toggle-reviewed", Ignore, "Toggle file reviewed"),
    spec(A::ToggleHunkReviewed, "toggle-hunk-reviewed", Ignore, "Toggle hunk reviewed"),
    spec(A::AddLineComment, "comment-line", Ignore, "Comment on the cursor line"),
    spec(A::AddFileComment, "comment-file", Ignore, "Comment on the file"),
    spec(A::AddReviewComment, "comment-review", Ignore, "Comment on the whole review"),
    spec(A::EditComment, "edit-comment", Ignore, "Edit the comment at cursor"),
    spec(A::EditCommentAtEnd, "edit-comment-at-end", Ignore, "Edit the comment at cursor, text cursor at end"),
    spec(A::DeleteComment, "delete-comment", Ignore, "Delete the comment at cursor"),
    spec(A::EditFile, "edit-file", Ignore, "Open the focused file in $EDITOR").cmd(&["edit"]),
    spec(A::EnterVisualMode, "visual-mode", Ignore, "Start a visual selection"),
    spec(A::AddRangeComment, "comment-range", Ignore, "Comment on the selection"),
    spec(A::AddRangeSuggestion, "suggest-range", Ignore, "Suggest a change to the selection"),
    spec(A::CopySelection, "copy-selection", Ignore, "Copy the selected text"),
    spec(A::ExportToClipboard, "export", Ignore, "Copy the review to the clipboard").cmd(&["clip", "export"]),
    spec(A::CopyCommentAtCursor, "copy-comment", Ignore, "Copy the comment at cursor"),
    spec(A::CopyUrl, "copy-url", Ignore, "Copy the pull request URL (PR mode)").cmd(&["copy-url"]),
    spec(A::Summary, "summary", Ignore, "List pending comments").cmd(&["summary"]),
    spec(A::ClearAll, "clear", Ignore, "Clear comments and reviewed marks").cmd(&["clear"]),
    spec(A::ClearComments, "clear-comments", Ignore, "Clear comments").cmd(&["clearc"]),
    spec(A::Stage, "stage", Ignore, "Stage reviewed files").cmd(&["stage"]),
    // Search
    spec(A::EnterSearchMode, "search", Ignore, "Search (filter in lists)"),
    spec(A::SearchNext, "search-next", Ignore, "Next search match"),
    spec(A::SearchPrev, "search-prev", Ignore, "Previous search match"),
    spec(A::ClearSearchHighlight, "clear-search-highlight", Ignore, "Clear search highlight"),
    // Session
    spec(A::Quit, "quit", Ignore, "Quit").cmd(&["q", "quit"]),
    spec(A::ConfirmQuit, "confirm-quit", Ignore, "Quit; unsaved comments take a second press"),
    spec(A::ForceQuit, "force-quit", Ignore, "Quit without saving").cmd(&["q!", "quit!"]),
    spec(A::Write, "write", Ignore, "Save the session").cmd(&["w", "write"]),
    spec(A::WriteQuit, "write-quit", Ignore, "Save and quit, asking to copy the review if comments exist").cmd(&["x", "wq"]),
    spec(A::ExportQuit, "export-quit", Ignore, "Save, copy the review if comments exist, and quit"),
    spec(A::Reload, "reload", Ignore, "Reload the diff and comments (PR mode: refetch the PR)").cmd(&["e", "reload"]),
    // Modes and screens
    spec(A::EnterCommandMode, "command-mode", Ignore, "Enter command mode"),
    spec(A::ToggleHelp, "help", Ignore, "Toggle help").cmd(&["help", "h"]),
    spec(A::MessageDetails, "messages", Ignore, "Show full details for the current error").cmd(&["messages"]),
    spec(A::Version, "version", Ignore, "Show the version").cmd(&["version"]),
    spec(A::CheckUpdate, "check-update", Ignore, "Check for updates").cmd(&["update"]),
    spec(A::ExitMode, "exit", Ignore, "Close or cancel"),
    spec(A::Redraw, "redraw", Ignore, "Redraw the screen"),
    // Options
    spec(A::SetWrap, "set-wrap", Ignore, "Wrap long lines").cmd(&["set wrap"]),
    spec(A::ToggleWrap, "toggle-wrap", Ignore, "Toggle line wrap").cmd(&["set wrap!", "wrap"]),
    spec(A::SetRelativeNumber, "set-relativenumber", Ignore, "Show relative line numbers").cmd(&["set relativenumber"]),
    spec(A::SetNoRelativeNumber, "set-norelativenumber", Ignore, "Show absolute line numbers").cmd(&["set norelativenumber"]),
    spec(A::ToggleRelativeNumber, "toggle-relativenumber", Ignore, "Toggle relative line numbers").cmd(&["set relativenumber!"]),
    spec(A::SetWordDiff, "set-worddiff", Ignore, "Emphasize the changed words of modified lines").cmd(&["set worddiff"]),
    spec(A::SetNoWordDiff, "set-noworddiff", Ignore, "Stop emphasizing changed words").cmd(&["set noworddiff"]),
    spec(A::ToggleWordDiff, "toggle-worddiff", Ignore, "Toggle changed-word emphasis").cmd(&["set worddiff!"]),
    spec(A::ToggleVim, "toggle-vim", Ignore, "Toggle vim editing in comments").cmd(&["vim", "set vim!"]),
    spec(A::SetVim, "set-vim", Ignore, "Use vim editing in comments").cmd(&["set vim"]),
    spec(A::SetNoVim, "set-novim", Ignore, "Use plain editing in comments").cmd(&["novim", "set novim"]),
    spec(A::ShowCommits, "show-commits", Ignore, "Show the commit selector").cmd(&["set commits"]),
    spec(A::HideCommits, "hide-commits", Ignore, "Hide the commit selector").cmd(&["set nocommits"]),
    spec(A::ToggleCommits, "toggle-commits", Ignore, "Show or hide the commit selector").cmd(&["set commits!"]),
    spec(A::ShowReviewed, "show-reviewed", Ignore, "Show reviewed files").cmd(&["set reviewed"]),
    spec(A::HideReviewed, "hide-reviewed", Ignore, "Hide reviewed files").cmd(&["set noreviewed"]),
    spec(A::ToggleShowReviewed, "toggle-show-reviewed", Ignore, "Show or hide reviewed files").cmd(&["reviewed", "set reviewed!"]),
    spec(A::SetMinimalUi, "set-minimal", Ignore, "Use the minimal UI").cmd(&["set minimal"]),
    spec(A::SetNoMinimalUi, "set-nominimal", Ignore, "Use the full UI").cmd(&["set nominimal"]),
    spec(A::ToggleMinimalUi, "toggle-minimal", Ignore, "Toggle the minimal UI").cmd(&["set minimal!"]),
    spec(A::CollapseGenerated, "collapse-generated", Ignore, "Collapse files marked generated in .gitattributes").cmd(&["set generated"]),
    spec(A::ShowGenerated, "show-generated", Ignore, "Show generated files").cmd(&["set nogenerated"]),
    spec(A::ToggleGenerated, "toggle-generated", Ignore, "Toggle generated file collapse").cmd(&["generated", "set generated!"]),
    spec(A::ToggleDiffView, "toggle-diff-view", Ignore, "Switch unified and side-by-side diff").cmd(&["diff"]),
    spec(A::ToggleSingleFileView, "toggle-single-file-view", Ignore, "Toggle single-file view").cmd(&["focus", "f"]),
    // Review target selector
    spec(A::OpenCommits, "open-commits", Ignore, "Open the review target selector on Local").cmd(&["commits", "targets"]),
    spec(A::OpenPullRequests, "open-prs", Ignore, "Open the review target selector on Pull Requests").cmd(&["prs"]),
    spec(A::OpenSessions, "open-sessions", Ignore, "Open the review target selector on Sessions").cmd(&["sessions", "reviews"]),
    spec(A::TargetSelectorTabNext, "next-tab", Ignore, "Next tab"),
    spec(A::TargetSelectorTabPrev, "prev-tab", Ignore, "Previous tab"),
    spec(A::TogglePrReviewRequestedFilter, "toggle-review-requested", Ignore, "Toggle PRs awaiting your review"),
    spec(A::SessionsTabToggleShowEmpty, "toggle-empty-sessions", Ignore, "Show or hide empty sessions"),
    spec(A::SessionsTabDelete, "delete-session", Ignore, "Delete the session under the cursor"),
    // Pull request review
    spec(A::SubmitPicker, "submit", Ignore, "Pick a review event to submit (PR mode)").cmd(&["submit"]),
    spec(A::SubmitComment, "submit-comment", Ignore, "Submit a COMMENT review").cmd(&["submit comment"]),
    spec(A::SubmitApprove, "submit-approve", Ignore, "Submit an APPROVE review").cmd(&["submit approve"]),
    spec(A::SubmitRequestChanges, "submit-request-changes", Ignore, "Submit a REQUEST_CHANGES review").cmd(&["submit request-changes"]),
    spec(A::SubmitDraft, "submit-draft", Ignore, "Push a pending (draft) review").cmd(&["submit draft"]),
    spec(A::SubmitResolverAdvance, "submit-advance", Ignore, "Continue to submit confirmation"),
    spec(A::SubmitReloadPr, "reload-pr", Ignore, "Reload the pull request"),
    spec(A::CommentsUnresolved, "comments-unresolved", Ignore, "Show unresolved remote comments (PR mode)").cmd(&["comments unresolved"]),
    spec(A::CommentsAll, "comments-all", Ignore, "Show all remote comments, resolved and outdated included (PR mode)").cmd(&["comments all"]),
    spec(A::CommentsHide, "comments-hide", Ignore, "Hide remote comments (PR mode)").cmd(&["comments hide"]),
    spec(A::ThemePicker, "theme-picker", Ignore, "Pick a theme").cmd(&["theme"]),
    spec(A::FilePicker, "file-picker", Ignore, "Jump to a file by path"),
    // Dialogs
    spec(A::ConfirmYes, "yes", Ignore, "Confirm"),
    spec(A::ConfirmNo, "no", Ignore, "Decline"),
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

    /// Look up the action a `:` command name runs.
    pub fn from_command(command: &str) -> Option<Action> {
        ACTIONS
            .iter()
            .find(|spec| spec.commands.contains(&command))
            .map(|spec| spec.action.clone())
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
    fn should_give_every_command_name_one_action() {
        let mut commands = HashSet::new();
        for spec in ACTIONS {
            for command in spec.commands {
                assert!(commands.insert(*command), "duplicate command {command}");
            }
        }
    }

    #[test]
    fn should_look_up_actions_by_command_name() {
        assert_eq!(Action::from_command("wq"), Some(Action::WriteQuit));
        assert_eq!(
            Action::from_command("set relativenumber!"),
            Some(Action::ToggleRelativeNumber)
        );
        assert_eq!(
            Action::from_command("reviewed"),
            Some(Action::ToggleShowReviewed)
        );
        assert_eq!(Action::from_command("nope"), None);
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
        assert_eq!(Action::TextCursorLeft.name(), None);
        assert_eq!(Action::CycleCommentType.name(), None);
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
