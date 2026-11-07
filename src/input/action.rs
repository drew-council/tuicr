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
    NextFile,
    PrevFile,
    NextHunk,
    PrevHunk,
    NextComment,
    PrevComment,
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
    /// Quit without saving (`:q!`, `ZQ`).
    ForceQuit,
    Write,
    /// Save, offer to copy the review, then quit (`:x`, `ZZ`).
    WriteQuit,
    Reload,
    CopyUrl,
    /// Open the pull request in the browser.
    OpenUrl,
    /// Copy the review's branch name.
    CopyBranch,
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
    SetNoWrap,
    ToggleWrap,
    SetRelativeNumber,
    SetNoRelativeNumber,
    ToggleRelativeNumber,
    SetVim,
    SetNoVim,
    ToggleVim,
    ShowCommits,
    HideCommits,
    ToggleCommits,
    ShowReviewed,
    HideReviewed,
    ToggleShowReviewed,
    ShowGenerated,
    HideGenerated,
    ToggleShowGenerated,
    ShowVendored,
    HideVendored,
    ToggleShowVendored,
    ShowBotReviews,
    HideBotReviews,
    ToggleShowBotReviews,

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
    OpenCommits,
    OpenPullRequests,
    OpenSessions,
    TargetSelectorTabNext,
    TargetSelectorTabPrev,
    /// Cycle the PR tab scope: all open PRs, review requested, requested by
    /// name (GitHub).
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

use CountMode::{Ignore, Line, Pass, Repeat, Scale};

/// Every bindable action. Command completion cycles in this order.
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
    spec(
        Action::CenterCursor,
        "center-cursor",
        Ignore,
        "Center the cursor line",
    ),
    spec(
        Action::CursorToTop,
        "cursor-to-top",
        Ignore,
        "Scroll the cursor line to the top",
    ),
    spec(
        Action::CursorToBottom,
        "cursor-to-bottom",
        Ignore,
        "Scroll the cursor line to the bottom",
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
        Action::FocusPaneLeft,
        "focus-left",
        Ignore,
        "Focus the pane to the left",
    ),
    spec(
        Action::FocusPaneRight,
        "focus-right",
        Ignore,
        "Focus the pane to the right",
    ),
    spec(
        Action::FocusPaneUp,
        "focus-up",
        Ignore,
        "Focus the pane above",
    ),
    spec(
        Action::FocusPaneDown,
        "focus-down",
        Ignore,
        "Focus the pane below",
    ),
    spec(
        Action::ToggleFileList,
        "toggle-file-list",
        Ignore,
        "Show or hide the file list",
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
        Action::AddReviewComment,
        "comment-review",
        Ignore,
        "Comment on the whole review",
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
        Action::DeleteComment,
        "delete-comment",
        Ignore,
        "Delete the comment at cursor",
    ),
    spec(
        Action::EditFile,
        "edit-file",
        Ignore,
        "Open the focused file in $EDITOR",
    )
    .cmd(&["edit"]),
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
    )
    .cmd(&["clip", "export"]),
    spec(
        Action::CopyCommentAtCursor,
        "copy-comment",
        Ignore,
        "Copy the comment at cursor",
    ),
    spec(
        Action::CopyUrl,
        "copy-url",
        Ignore,
        "Copy the pull request URL",
    )
    .cmd(&["copy-url"]),
    spec(
        Action::OpenUrl,
        "open-url",
        Ignore,
        "Open the pull request in the browser",
    )
    .cmd(&["open-url", "browse"]),
    spec(
        Action::CopyBranch,
        "copy-branch",
        Ignore,
        "Copy the review's branch name",
    )
    .cmd(&["branch"]),
    spec(Action::Summary, "summary", Ignore, "List pending comments").cmd(&["summary"]),
    spec(
        Action::ClearAll,
        "clear",
        Ignore,
        "Clear comments and reviewed marks",
    )
    .cmd(&["clear"]),
    spec(
        Action::ClearComments,
        "clear-comments",
        Ignore,
        "Clear comments",
    )
    .cmd(&["clearc"]),
    spec(Action::Stage, "stage", Ignore, "Stage reviewed files").cmd(&["stage"]),
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
    // Session
    spec(Action::Quit, "quit", Ignore, "Quit").cmd(&["q", "quit"]),
    spec(
        Action::ForceQuit,
        "force-quit",
        Ignore,
        "Quit without saving",
    )
    .cmd(&["q!", "quit!"]),
    spec(Action::Write, "write", Ignore, "Save the session").cmd(&["w", "write"]),
    spec(
        Action::WriteQuit,
        "write-quit",
        Ignore,
        "Save, offer to copy the review, and quit",
    )
    .cmd(&["x", "wq"]),
    spec(
        Action::Reload,
        "reload",
        Ignore,
        "Reload the diff and comments",
    )
    .cmd(&["e", "reload"]),
    // Modes and screens
    spec(
        Action::EnterCommandMode,
        "command-mode",
        Ignore,
        "Enter command mode",
    ),
    spec(Action::ToggleHelp, "help", Ignore, "Toggle help").cmd(&["help", "h"]),
    spec(
        Action::MessageDetails,
        "messages",
        Ignore,
        "Show the full current error",
    )
    .cmd(&["messages"]),
    spec(Action::Version, "version", Ignore, "Show the version").cmd(&["version"]),
    spec(
        Action::CheckUpdate,
        "check-update",
        Ignore,
        "Check for updates",
    )
    .cmd(&["update"]),
    spec(Action::ExitMode, "exit", Ignore, "Close or cancel"),
    spec(Action::Redraw, "redraw", Ignore, "Redraw the screen"),
    // Options
    spec(Action::SetWrap, "set-wrap", Ignore, "Wrap long lines").cmd(&["set wrap"]),
    spec(
        Action::SetNoWrap,
        "set-nowrap",
        Ignore,
        "Do not wrap long lines",
    )
    .cmd(&["set nowrap"]),
    spec(
        Action::ToggleWrap,
        "toggle-wrap",
        Ignore,
        "Toggle line wrap",
    )
    .cmd(&["set wrap!", "wrap"]),
    spec(
        Action::SetRelativeNumber,
        "set-relativenumber",
        Ignore,
        "Show relative line numbers",
    )
    .cmd(&["set relativenumber"]),
    spec(
        Action::SetNoRelativeNumber,
        "set-norelativenumber",
        Ignore,
        "Show absolute line numbers",
    )
    .cmd(&["set norelativenumber"]),
    spec(
        Action::ToggleRelativeNumber,
        "toggle-relativenumber",
        Ignore,
        "Toggle relative line numbers",
    )
    .cmd(&["set relativenumber!"]),
    spec(
        Action::ToggleVim,
        "toggle-vim",
        Ignore,
        "Toggle vim editing in comments",
    )
    .cmd(&["vim", "set vim!"]),
    spec(
        Action::SetVim,
        "set-vim",
        Ignore,
        "Use vim editing in comments",
    )
    .cmd(&["set vim"]),
    spec(
        Action::SetNoVim,
        "set-novim",
        Ignore,
        "Use plain editing in comments",
    )
    .cmd(&["novim", "set novim"]),
    spec(
        Action::ShowCommits,
        "show-commits",
        Ignore,
        "Show the commit selector",
    )
    .cmd(&["set commits"]),
    spec(
        Action::HideCommits,
        "hide-commits",
        Ignore,
        "Hide the commit selector",
    )
    .cmd(&["set nocommits"]),
    spec(
        Action::ToggleCommits,
        "toggle-commits",
        Ignore,
        "Show or hide the commit selector",
    )
    .cmd(&["set commits!"]),
    spec(
        Action::ShowReviewed,
        "show-reviewed",
        Ignore,
        "Show reviewed files",
    )
    .cmd(&["set reviewed"]),
    spec(
        Action::HideReviewed,
        "hide-reviewed",
        Ignore,
        "Hide reviewed files",
    )
    .cmd(&["set noreviewed"]),
    spec(
        Action::ToggleShowReviewed,
        "toggle-show-reviewed",
        Ignore,
        "Show or hide reviewed files",
    )
    .cmd(&["reviewed", "set reviewed!"]),
    spec(
        Action::ShowGenerated,
        "show-generated",
        Ignore,
        "Show .gitattributes generated files",
    )
    .cmd(&["set generated"]),
    spec(
        Action::HideGenerated,
        "hide-generated",
        Ignore,
        "Hide .gitattributes generated files",
    )
    .cmd(&["set nogenerated"]),
    spec(
        Action::ToggleShowGenerated,
        "toggle-show-generated",
        Ignore,
        "Show or hide .gitattributes generated files",
    )
    .cmd(&["generated", "set generated!"]),
    spec(
        Action::ShowVendored,
        "show-vendored",
        Ignore,
        "Show .gitattributes vendored files",
    )
    .cmd(&["set vendored"]),
    spec(
        Action::HideVendored,
        "hide-vendored",
        Ignore,
        "Hide .gitattributes vendored files",
    )
    .cmd(&["set novendored"]),
    spec(
        Action::ToggleShowVendored,
        "toggle-show-vendored",
        Ignore,
        "Show or hide .gitattributes vendored files",
    )
    .cmd(&["vendored", "set vendored!"]),
    spec(
        Action::ShowBotReviews,
        "show-bot-reviews",
        Ignore,
        "Show bot review summaries",
    )
    .cmd(&["set botreviews"]),
    spec(
        Action::HideBotReviews,
        "hide-bot-reviews",
        Ignore,
        "Hide bot review summaries",
    )
    .cmd(&["set nobotreviews"]),
    spec(
        Action::ToggleShowBotReviews,
        "toggle-show-bot-reviews",
        Ignore,
        "Show or hide bot review summaries",
    )
    .cmd(&["botreviews", "set botreviews!"]),
    spec(
        Action::ToggleDiffView,
        "toggle-diff-view",
        Ignore,
        "Switch unified and side-by-side diff",
    )
    .cmd(&["diff"]),
    spec(
        Action::ToggleSingleFileView,
        "toggle-single-file-view",
        Ignore,
        "Toggle single-file view",
    )
    .cmd(&["focus", "f"]),
    // Review target selector
    spec(
        Action::OpenCommits,
        "open-commits",
        Ignore,
        "Open the review target selector",
    )
    .cmd(&["commits", "targets"]),
    spec(
        Action::OpenPullRequests,
        "open-prs",
        Ignore,
        "Open the pull request selector",
    )
    .cmd(&["prs"]),
    spec(
        Action::OpenSessions,
        "open-sessions",
        Ignore,
        "Open the saved review selector",
    )
    .cmd(&["sessions", "reviews"]),
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
        "Cycle the PR scope: all, review requested, requested by name",
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
    // Pull request review
    spec(
        Action::SubmitPicker,
        "submit",
        Ignore,
        "Pick how to submit the review",
    )
    .cmd(&["submit"]),
    spec(
        Action::SubmitComment,
        "submit-comment",
        Ignore,
        "Submit the review as comments",
    )
    .cmd(&["submit comment"]),
    spec(
        Action::SubmitApprove,
        "submit-approve",
        Ignore,
        "Submit the review as an approval",
    )
    .cmd(&["submit approve"]),
    spec(
        Action::SubmitRequestChanges,
        "submit-request-changes",
        Ignore,
        "Submit the review requesting changes",
    )
    .cmd(&["submit request-changes"]),
    spec(
        Action::SubmitDraft,
        "submit-draft",
        Ignore,
        "Push the review as a draft",
    )
    .cmd(&["submit draft"]),
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
    spec(
        Action::CommentsUnresolved,
        "comments-unresolved",
        Ignore,
        "Show unresolved forge threads",
    )
    .cmd(&["comments unresolved"]),
    spec(
        Action::CommentsAll,
        "comments-all",
        Ignore,
        "Show all forge threads",
    )
    .cmd(&["comments all"]),
    spec(
        Action::CommentsHide,
        "comments-hide",
        Ignore,
        "Hide forge threads",
    )
    .cmd(&["comments hide"]),
    spec(Action::ThemePicker, "theme-picker", Ignore, "Pick a theme").cmd(&["theme"]),
    spec(
        Action::FilePicker,
        "file-picker",
        Ignore,
        "Jump to a file by path",
    ),
    // Dialogs
    spec(Action::ConfirmYes, "yes", Ignore, "Confirm"),
    spec(Action::ConfirmNo, "no", Ignore, "Decline"),
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
    fn should_document_every_action_in_the_config_reference() {
        let docs = include_str!("../../docs/CONFIG.md");
        for spec in ACTIONS {
            assert!(
                docs.contains(&format!("| `{}` |", spec.name)),
                "docs/CONFIG.md is missing action `{}`",
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
