//! The action vocabulary shared by key bindings and `:` commands.
//!
//! Every user-facing behavior is an [`Action`]. Keys and commands both
//! resolve to one, and a handler for the active context performs it. Actions
//! listed in [`ACTIONS`] carry a stable kebab-case name, which is what the
//! keymap config refers to. The `internal` variants are input events (typed
//! text, paste, mouse) and cannot be bound.

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

/// Declares [`Action`] and [`ACTIONS`] from one list, so a bindable action
/// cannot exist without its registry entry. `internal` variants are input
/// events (typed text, paste, mouse) that have no entry and cannot be bound.
macro_rules! actions {
    (
        bindable { $(
            $(#[$doc:meta])*
            $variant:ident $(($ty:ty = $step:expr))? => $name:literal, $count:ident, $help:literal $(, [$($command:literal),+])?;
        )* }
        internal { $( $(#[$idoc:meta])* $internal:ident $(($ity:ty))? ),* $(,)? }
    ) => {
        /// A single user intent, independent of the key or command that produced it.
        ///
        /// Motion variants carry a step size. A count typed before the key multiplies
        /// it (see [`CountMode::Scale`]).
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum Action {
            $( $(#[$doc])* $variant $(($ty))?, )*
            $( $(#[$idoc])* $internal $(($ity))?, )*
        }

        /// Every bindable action. Command completion cycles in this order.
        pub const ACTIONS: &[ActionSpec] = &[
            $(
                ActionSpec {
                    action: Action::$variant $(($step))?,
                    name: $name,
                    count: CountMode::$count,
                    help: $help,
                    commands: &[$($($command),+)?],
                },
            )*
        ];
    };
}

// Each row: variant, config name, count mode, help text, then any `:` command
// names. A step in parentheses is the default the config name binds.
#[rustfmt::skip]
actions! {
    bindable {
        // Navigation
        CursorDown(usize = 1) => "cursor-down", Scale, "Move down";
        CursorUp(usize = 1) => "cursor-up", Scale, "Move up";
        ScrollViewDown(usize = 1) => "scroll-view-down", Scale, "Scroll view down";
        ScrollViewUp(usize = 1) => "scroll-view-up", Scale, "Scroll view up";
        HalfPageDown => "half-page-down", Ignore, "Half page down";
        HalfPageUp => "half-page-up", Ignore, "Half page up";
        PageDown => "page-down", Ignore, "Full page down";
        PageUp => "page-up", Ignore, "Full page up";
        ScrollLeft(usize = 4) => "scroll-left", Scale, "Scroll left";
        ScrollRight(usize = 4) => "scroll-right", Scale, "Scroll right";
        GoToTop => "go-to-top", Ignore, "Go to top";
        GoToBottom => "go-to-bottom", Line, "Go to bottom ({N} jumps to line N)";
        /// Jump to the new-side source line given by the count (`{N}G`, `:N`).
        GoToLine => "go-to-line", Pass, "Go to new-side line {N}";
        /// Jump to the old-side source line given by the count (`:oN`).
        GoToOldLine => "go-to-old-line", Pass, "Go to old-side line {N}";
        NextFile => "next-file", Repeat, "Next file";
        PrevFile => "prev-file", Repeat, "Previous file";
        NextHunk => "next-hunk", Repeat, "Next hunk";
        PrevHunk => "prev-hunk", Repeat, "Previous hunk";
        NextComment => "next-comment", Ignore, "Next comment";
        PrevComment => "prev-comment", Ignore, "Previous comment";
        /// Cycle inline commit selector to next individual commit (`)`)
        CycleCommitNext => "next-commit", Ignore, "Show next commit";
        /// Cycle inline commit selector to previous individual commit (`(`)
        CycleCommitPrev => "prev-commit", Ignore, "Show previous commit";
        /// Put the cursor line at the middle of the view (`zz`).
        CenterCursor => "center-cursor", Ignore, "Center the cursor line";
        /// Put the cursor line at the top of the view (`zt`).
        CursorToTop => "cursor-to-top", Ignore, "Scroll the cursor line to the top";
        /// Put the cursor line at the bottom of the view (`zb`).
        CursorToBottom => "cursor-to-bottom", Ignore, "Scroll the cursor line to the bottom";
        // Panels and lists
        ToggleFocus => "focus-next", Ignore, "Focus next pane";
        ToggleFocusReverse => "focus-prev", Ignore, "Focus previous pane";
        FocusPaneLeft => "focus-left", Ignore, "Focus the pane to the left";
        FocusPaneRight => "focus-right", Ignore, "Focus the pane to the right";
        FocusPaneUp => "focus-up", Ignore, "Focus the pane above";
        FocusPaneDown => "focus-down", Ignore, "Focus the pane below";
        ToggleFileList => "toggle-file-list", Ignore, "Show or hide the file list";
        /// Activate the item under the cursor (Enter).
        Select => "select", Ignore, "Open or activate the item under the cursor";
        /// Activate the item under the cursor fully (Shift-Enter).
        SelectFull => "select-full", Ignore, "Expand the whole gap under the cursor";
        /// Toggle the list item under the cursor (Space).
        ToggleSelect => "toggle-select", Ignore, "Toggle the item under the cursor";
        ToggleExpand => "toggle-expand", Ignore, "Toggle directory or gap";
        ExpandAll => "expand-all", Ignore, "Expand all directories";
        CollapseAll => "collapse-all", Ignore, "Collapse all directories";
        FileTreeFilterInclude => "filter-include", Ignore, "Include files matching a regex";
        FileTreeFilterExclude => "filter-exclude", Ignore, "Exclude files matching a regex";
        FileTreeClearInclude => "clear-include", Ignore, "Clear the include filter";
        FileTreeClearExclude => "clear-exclude", Ignore, "Clear the exclude filter";
        // Review
        ToggleReviewed => "toggle-reviewed", Ignore, "Toggle file reviewed";
        ToggleHunkReviewed => "toggle-hunk-reviewed", Ignore, "Toggle hunk reviewed";
        AddLineComment => "comment-line", Ignore, "Comment on the cursor line";
        AddFileComment => "comment-file", Ignore, "Comment on the file";
        AddReviewComment => "comment-review", Ignore, "Comment on the whole review";
        EditComment => "edit-comment", Ignore, "Edit the comment at cursor";
        /// Edit the comment at cursor with the text cursor at end (vim `A`).
        EditCommentAtEnd => "edit-comment-at-end", Ignore, "Edit the comment at cursor, text cursor at end";
        /// Delete the comment under the cursor (`dd`).
        DeleteComment => "delete-comment", Ignore, "Delete the comment at cursor";
        EditFile => "edit-file", Ignore, "Open the focused file in $EDITOR", ["edit"];
        EnterVisualMode => "visual-mode", Ignore, "Start a visual selection";
        AddRangeComment => "comment-range", Ignore, "Comment on the selection";
        AddRangeSuggestion => "suggest-range", Ignore, "Suggest a change to the selection";
        /// Copy the visual selection's text.
        CopySelection => "copy-selection", Ignore, "Copy the selected text";
        ExportToClipboard => "export", Ignore, "Copy the review to the clipboard", ["clip", "export"];
        /// Copy just the comment under the cursor (`Y`), not the whole review.
        CopyCommentAtCursor => "copy-comment", Ignore, "Copy the comment at cursor";
        CopyUrl => "copy-url", Ignore, "Copy the pull request URL (PR mode)", ["copy-url"];
        Summary => "summary", Ignore, "List pending comments", ["summary"];
        /// Clear comments and reviewed marks.
        ClearAll => "clear", Ignore, "Clear comments and reviewed marks", ["clear"];
        ClearComments => "clear-comments", Ignore, "Clear comments", ["clearc"];
        Stage => "stage", Ignore, "Stage reviewed files", ["stage"];
        // Search
        /// Open the `/` prompt. Lists interpret it as their filter prompt.
        EnterSearchMode => "search", Ignore, "Search (filter in lists)";
        SearchNext => "search-next", Ignore, "Next search match";
        SearchPrev => "search-prev", Ignore, "Previous search match";
        ClearSearchHighlight => "clear-search-highlight", Ignore, "Clear search highlight";
        // Session
        /// Quit, refusing while unsaved comments exist (`:q`).
        Quit => "quit", Ignore, "Quit", ["q", "quit"];
        /// Quit from a key (`q` under `q_quits`): unsaved comments in a review
        /// pane take a second press.
        ConfirmQuit => "confirm-quit", Ignore, "Quit; unsaved comments take a second press";
        /// Quit without saving (`:q!`, `ZQ`).
        ForceQuit => "force-quit", Ignore, "Quit without saving", ["q!", "quit!"];
        Write => "write", Ignore, "Save the session", ["w", "write"];
        /// Save, offer to copy the review, then quit (`:x`).
        WriteQuit => "write-quit", Ignore, "Save and quit, asking to copy the review if comments exist", ["x", "wq"];
        /// Save, copy the review without asking, then quit (`ZZ`).
        ExportQuit => "export-quit", Ignore, "Save, copy the review if comments exist, and quit";
        Reload => "reload", Ignore, "Reload the diff and comments (PR mode: refetch the PR)", ["e", "reload"];
        // Modes and screens
        EnterCommandMode => "command-mode", Ignore, "Enter command mode";
        ToggleHelp => "help", Ignore, "Toggle help", ["help", "h"];
        MessageDetails => "messages", Ignore, "Show full details for the current error", ["messages"];
        Version => "version", Ignore, "Show the version", ["version"];
        CheckUpdate => "check-update", Ignore, "Check for updates", ["update"];
        ExitMode => "exit", Ignore, "Close or cancel";
        /// Clear the terminal and repaint every cell on the next frame (`Ctrl-l`).
        Redraw => "redraw", Ignore, "Redraw the screen";
        // Options
        SetWrap => "set-wrap", Ignore, "Wrap long lines", ["set wrap"];
        SetNoWrap => "set-nowrap", Ignore, "Do not wrap long lines", ["set nowrap"];
        ToggleWrap => "toggle-wrap", Ignore, "Toggle line wrap", ["set wrap!", "wrap"];
        SetRelativeNumber => "set-relativenumber", Ignore, "Show relative line numbers", ["set relativenumber"];
        SetNoRelativeNumber => "set-norelativenumber", Ignore, "Show absolute line numbers", ["set norelativenumber"];
        ToggleRelativeNumber => "toggle-relativenumber", Ignore, "Toggle relative line numbers", ["set relativenumber!"];
        ToggleVim => "toggle-vim", Ignore, "Toggle vim editing in comments", ["vim", "set vim!"];
        SetVim => "set-vim", Ignore, "Use vim editing in comments", ["set vim"];
        SetNoVim => "set-novim", Ignore, "Use plain editing in comments", ["novim", "set novim"];
        ShowCommits => "show-commits", Ignore, "Show the commit selector", ["set commits"];
        HideCommits => "hide-commits", Ignore, "Hide the commit selector", ["set nocommits"];
        ToggleCommits => "toggle-commits", Ignore, "Show or hide the commit selector", ["set commits!"];
        ShowReviewed => "show-reviewed", Ignore, "Show reviewed files", ["set reviewed"];
        HideReviewed => "hide-reviewed", Ignore, "Hide reviewed files", ["set noreviewed"];
        ToggleShowReviewed => "toggle-show-reviewed", Ignore, "Show or hide reviewed files", ["reviewed", "set reviewed!"];
        SetMinimalUi => "set-minimal", Ignore, "Use the minimal UI", ["set minimal"];
        SetNoMinimalUi => "set-nominimal", Ignore, "Use the full UI", ["set nominimal"];
        ToggleMinimalUi => "toggle-minimal", Ignore, "Toggle the minimal UI", ["set minimal!"];
        ToggleDiffView => "toggle-diff-view", Ignore, "Switch unified and side-by-side diff", ["diff"];
        ToggleSingleFileView => "toggle-single-file-view", Ignore, "Toggle single-file view", ["focus", "f"];
        // Review target selector
        OpenCommits => "open-commits", Ignore, "Open the review target selector on Local", ["commits", "targets"];
        OpenPullRequests => "open-prs", Ignore, "Open the review target selector on Pull Requests", ["prs"];
        OpenSessions => "open-sessions", Ignore, "Open the review target selector on Sessions", ["sessions", "reviews"];
        TargetSelectorTabNext => "next-tab", Ignore, "Next tab";
        TargetSelectorTabPrev => "prev-tab", Ignore, "Previous tab";
        /// Cycle the PR tab scope: all open PRs, review requested, requested by
        /// name (GitHub).
        TogglePrReviewRequestedFilter => "toggle-review-requested", Ignore, "Cycle the PR scope: all, review requested, requested by name";
        /// Sessions tab: show or hide sessions holding no review progress.
        SessionsTabToggleShowEmpty => "toggle-empty-sessions", Ignore, "Show or hide empty sessions";
        /// Sessions tab: delete the session under the cursor, after a confirm.
        SessionsTabDelete => "delete-session", Ignore, "Delete the session under the cursor";
        // Pull request review
        /// Open the submit action picker (`:submit`).
        SubmitPicker => "submit", Ignore, "Pick a review event to submit (PR mode)", ["submit"];
        SubmitComment => "submit-comment", Ignore, "Submit a COMMENT review", ["submit comment"];
        SubmitApprove => "submit-approve", Ignore, "Submit an APPROVE review", ["submit approve"];
        SubmitRequestChanges => "submit-request-changes", Ignore, "Submit a REQUEST_CHANGES review", ["submit request-changes"];
        SubmitDraft => "submit-draft", Ignore, "Push a pending (draft) review", ["submit draft"];
        /// Advance from the resolver to final confirmation.
        SubmitResolverAdvance => "submit-advance", Ignore, "Continue to submit confirmation";
        /// Reload the PR from the submit confirmation (stale-head warning only).
        SubmitReloadPr => "reload-pr", Ignore, "Reload the pull request";
        CommentsUnresolved => "comments-unresolved", Ignore, "Show unresolved remote comments (PR mode)", ["comments unresolved"];
        CommentsAll => "comments-all", Ignore, "Show all remote comments, resolved and outdated included (PR mode)", ["comments all"];
        CommentsHide => "comments-hide", Ignore, "Hide remote comments (PR mode)", ["comments hide"];
        ThemePicker => "theme-picker", Ignore, "Pick a theme", ["theme"];
        /// Open the file picker popup (`<leader>p`).
        FilePicker => "file-picker", Ignore, "Jump to a file by path";
        // Dialogs
        ConfirmYes => "yes", Ignore, "Confirm";
        ConfirmNo => "no", Ignore, "Decline";
    }
    internal {
        MouseScrollUp(usize),
        MouseScrollDown(usize),
        /// Transitional hint for the removed `q`-quits binding: shows a message
        /// pointing at `:q` instead of quitting. Drop this a few releases after
        /// the `q` removal has had time to land.
        QuitHint,
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
        None,
    }
}

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
        self.spec().map_or(CountMode::Ignore, |spec| spec.count)
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
            let commands: Vec<String> = spec.commands.iter().map(|c| format!("`:{c}`")).collect();
            let row = format!(
                "| `{}` | {} | {} |",
                spec.name,
                spec.help,
                commands.join(", ")
            );
            assert!(
                docs.contains(&row),
                "docs/CONFIG.md is missing the row {row}"
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
