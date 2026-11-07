use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Position;

use crate::app::{
    self, App, CommandCompletionState, ExpandDirection, FileTreeItem, FileTreePrompt, FocusedPanel,
    GapCursorHit, InputMode, TargetTab, VisualSelection,
};
use crate::error::TuicrError;
use crate::forge::remote_comments::PrCommentsVisibility;
use crate::forge::submit::SubmitEvent;
use crate::input::{
    ACTIONS, Action, ActionSpec, Invocation, KeyChord, KeyContext, Resolution, map_command_key,
    map_comment_key, map_file_picker_filter_key, map_prompt_key,
};
use crate::model::{ClearScope, LineSide};
use crate::output::{
    copy_text_to_clipboard_with, export_to_clipboard_with, export_to_file, generate_export_content,
};
use crate::text_edit::{
    delete_char_before, delete_word_before, next_char_boundary, prev_char_boundary,
};

/// Columns scrolled per horizontal mouse wheel tick. Matches the default
/// step for keyboard arrow scrolling so the two input methods feel
/// interchangeable.
const WHEEL_COLS: usize = 4;

/// Shown when a bare `q` is pressed in a mode that used to quit on it.
/// Transitional — drop this a few releases after the `q` removal has landed.
const QUIT_HINT_MESSAGE: &str = "q no longer quits — use :q to quit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionDirection {
    Forward,
    Reverse,
}

/// Run an invocation in the active context, applying its count.
pub fn dispatch_action(app: &mut App, invocation: impl Into<Invocation>) {
    let (invocation, times) = invocation.into().resolve();
    // A count repeats the action; the last run takes the invocation itself.
    for _ in 1..times {
        dispatch_once(app, invocation.clone());
    }
    dispatch_once(app, invocation);
}

fn dispatch_once(app: &mut App, invocation: Invocation) {
    if invocation.action != Action::ConfirmQuit {
        app.quit_warned = false;
    }
    if handle_mode_independent_action(app, &invocation) {
        return;
    }
    let action = invocation.action;
    match app.input_mode {
        InputMode::Help | InputMode::MessageDetails => handle_help_action(app, action),
        InputMode::Summary => handle_summary_action(app, action),
        InputMode::Command => handle_command_action(app, action),
        InputMode::Search => handle_search_action(app, action),
        InputMode::Comment => handle_comment_action(app, action),
        InputMode::Confirm => handle_confirm_action(app, action),
        InputMode::CommitSelect => handle_commit_select_action(app, action),
        InputMode::VisualSelect => handle_visual_action(app, action),
        InputMode::SubmitResolver => handle_submit_resolver_action(app, action),
        InputMode::SubmitConfirm => handle_submit_confirm_action(app, action),
        InputMode::SubmitActionPicker => handle_submit_action_picker_action(app, action),
        InputMode::ThemePicker => handle_theme_picker_action(app, action),
        InputMode::FilePicker => handle_file_picker_action(app, action),
        InputMode::Normal => handle_normal_action(app, action),
    }
}

/// Handle actions that behave the same in every mode.
/// Returns `true` when `action` was one of them.
fn handle_mode_independent_action(app: &mut App, invocation: &Invocation) -> bool {
    let Invocation { action, count } = invocation;
    match action {
        Action::GoToLine | Action::GoToOldLine => {
            let side = if *action == Action::GoToLine {
                LineSide::New
            } else {
                LineSide::Old
            };
            match count {
                Some(line) => app.go_to_source_line(u32::try_from(*line).unwrap_or(u32::MAX), side),
                None => app.set_message("Type a line number first, e.g. 42G"),
            }
        }
        Action::Quit => quit(app),
        Action::ConfirmQuit => confirm_quit(app),
        Action::ForceQuit => app.should_quit = true,
        Action::QuitHint => app.set_message(QUIT_HINT_MESSAGE),
        Action::Write => match app.save_current_session_merging_external() {
            Ok(path) => app.set_message(format!("Saved to {}", path.display())),
            Err(e) => app.set_error(format!("Save failed: {e}")),
        },
        Action::WriteQuit => write_quit(app),
        Action::ExportQuit => export_quit(app),
        Action::Reload => reload_review(app),
        Action::EditFile => app.queue_editor_for_focused_item(),
        Action::ExportToClipboard => handle_export(app),
        Action::CopyUrl => handle_copy_pr_url(app),
        Action::ClearAll => app.clear_comments(ClearScope::CommentsAndReviewed),
        Action::ClearComments => app.clear_comments(ClearScope::CommentsOnly),
        Action::ToggleHelp => app.toggle_help(),
        Action::MessageDetails => app.open_message_details(),
        Action::Summary => app.enter_summary_mode(),
        Action::ThemePicker => app.enter_theme_picker_mode(),
        Action::FilePicker => app.enter_file_picker_mode(),
        Action::Version => app.set_message(format!("tuicr v{}", env!("CARGO_PKG_VERSION"))),
        Action::CheckUpdate => check_for_updates(app),
        Action::EnterCommandMode => app.enter_command_mode(),
        Action::Redraw => app.request_full_redraw(),
        Action::CenterCursor => app.center_cursor(),
        Action::CursorToTop => app.cursor_to_top(),
        Action::CursorToBottom => app.cursor_to_bottom(),
        Action::ToggleFileList => app.toggle_file_list(),
        Action::FocusPaneLeft => app.focus_pane_left(),
        Action::FocusPaneRight => app.focus_pane_right(),
        Action::FocusPaneUp => app.focus_pane_up(),
        Action::FocusPaneDown => app.focus_pane_down(),
        Action::AddReviewComment => app.enter_review_comment_mode(),
        Action::ToggleDiffView => app.toggle_diff_view_mode(),
        Action::ToggleSingleFileView => app.toggle_single_file_view(),
        Action::Stage => app.stage_reviewed_files(),
        Action::SetWrap => app.set_diff_wrap(true),
        Action::ToggleWrap => app.toggle_diff_wrap(),
        Action::SetRelativeNumber => app.relative_line_numbers = true,
        Action::SetNoRelativeNumber => app.relative_line_numbers = false,
        Action::ToggleRelativeNumber => app.relative_line_numbers = !app.relative_line_numbers,
        Action::SetWordDiff => app.set_word_diff(true),
        Action::SetNoWordDiff => app.set_word_diff(false),
        Action::ToggleWordDiff => app.set_word_diff(!app.word_diff),
        Action::SetVim => app.set_comment_vim(true),
        Action::SetNoVim => app.set_comment_vim(false),
        Action::ToggleVim => app.toggle_comment_vim(),
        Action::ShowCommits => app.set_commit_selector_visible(true),
        Action::HideCommits => app.set_commit_selector_visible(false),
        Action::ToggleCommits => app.toggle_commit_selector(),
        Action::ShowReviewed => app.set_show_reviewed(true),
        Action::HideReviewed => app.set_show_reviewed(false),
        Action::ToggleShowReviewed => app.toggle_show_reviewed(),
        Action::SetMinimalUi => app.set_minimal_ui(true),
        Action::SetNoMinimalUi => app.set_minimal_ui(false),
        Action::ToggleMinimalUi => app.toggle_minimal_ui(),
        Action::CollapseGenerated => app.set_collapse_generated(true),
        Action::ShowGenerated => app.set_collapse_generated(false),
        Action::ToggleGenerated => app.toggle_collapse_generated(),
        Action::OpenCommits => open_target_selector(app, TargetTab::Local),
        Action::OpenPullRequests => open_target_selector(app, TargetTab::PullRequests),
        Action::OpenSessions => open_target_selector(app, TargetTab::Sessions),
        Action::SubmitPicker => app.start_submit_action_picker(),
        Action::SubmitComment => app.start_submit(SubmitEvent::Comment),
        Action::SubmitApprove => app.start_submit(SubmitEvent::Approve),
        Action::SubmitRequestChanges => app.start_submit(SubmitEvent::RequestChanges),
        Action::SubmitDraft => app.start_submit(SubmitEvent::Draft),
        Action::CommentsUnresolved => {
            set_remote_comments_visibility(app, PrCommentsVisibility::Unresolved);
        }
        Action::CommentsAll => set_remote_comments_visibility(app, PrCommentsVisibility::All),
        Action::CommentsHide => set_remote_comments_visibility(app, PrCommentsVisibility::Hide),
        _ => return false,
    }
    true
}

/// `:q`. Refuses while unsaved comments exist; a session dirty only from
/// reviewed marks is discarded instead.
fn quit(app: &mut App) {
    if app.dirty && app.session.has_comments() {
        app.set_error("No write since last change (add ! to override)");
    } else if app.dirty {
        app.discard_session_and_quit();
    } else {
        app.should_quit = true;
    }
}

/// `q` under `q_quits`. In a review pane, unsaved comments take a second
/// press; every other mode quits at once.
fn confirm_quit(app: &mut App) {
    if app.input_mode != InputMode::Normal {
        app.should_quit = true;
    } else if app.dirty && app.session.has_comments() && !app.quit_warned {
        let context = app.key_context().unwrap_or(KeyContext::Normal);
        let key = app
            .keymap
            .label(context, &[Action::ConfirmQuit], app.leader_key);
        app.set_sticky_warning(format!("Unsaved changes. Press {key} again to quit."));
        app.quit_warned = true;
    } else if app.dirty && !app.session.has_comments() {
        app.discard_session_and_quit();
    } else {
        app.should_quit = true;
    }
}

/// `ZZ`. Saves, then copies the review and quits without asking.
/// A failed save reports the error and stays open, like `:wq`.
fn export_quit(app: &mut App) {
    match app.save_current_session_merging_external() {
        Ok(_) if app.session.has_comments() => {
            handle_export(app);
            app.should_quit = true;
        }
        Ok(_) => app.should_quit = true,
        Err(e) => app.set_error(format!("Save failed: {e}")),
    }
}

/// `:x`. Saves, then offers to copy the review before quitting.
fn write_quit(app: &mut App) {
    if let Err(e) = app.save_current_session_merging_external() {
        app.set_error(format!("Save failed: {e}"));
        return;
    }
    if !app.session.has_comments() {
        app.should_quit = true;
    } else if app.output_to_stdout {
        // Skip the confirmation dialog and export directly.
        handle_export(app);
    } else {
        app.enter_confirm_mode(app::ConfirmAction::CopyAndQuit);
    }
}

fn open_target_selector(app: &mut App, tab: TargetTab) {
    if let Err(e) = app.enter_target_selector(tab) {
        let what = match tab {
            TargetTab::Local => "load commits",
            TargetTab::PullRequests => "open PR selector",
            TargetTab::Sessions => "open session selector",
        };
        app.set_error(format!("Failed to {what}: {e}"));
    }
}

/// Handle one key press: resolve it through the keymap (or the text editor
/// when a prompt is open), apply any `{N}` count, and dispatch the result.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    if let Some(bare) = split_escape_prefix(app, key) {
        handle_key(app, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        handle_key(app, bare);
        return;
    }
    let chord = KeyChord::from(key);
    let action = match app.key_context() {
        None if handle_global_key(app, key) => return,
        None => map_text_key(app, key),
        Some(context) => {
            // Vim-style `{N}` count, in review panes only: digits accumulate
            // in `pending_count` until a key runs an action, which applies
            // the count by its `CountMode`. A digit bound here is a key.
            if context.takes_count()
                && app.pending_keys.is_empty()
                && let KeyCode::Char(c @ '0'..='9') = key.code
                && key.modifiers == KeyModifiers::NONE
                && !app.keymap.binds(context, &[chord])
            {
                let n = app.pending_count.unwrap_or(0);
                let digit = c as usize - '0' as usize;
                app.pending_count = Some(n.saturating_mul(10).saturating_add(digit).min(999_999));
                return;
            }
            match app.keymap.resolve(context, &mut app.pending_keys, chord) {
                Resolution::Run(action) => action,
                Resolution::Pending => {
                    app.pending_count = None;
                    return;
                }
                Resolution::Unbound => Action::None,
            }
        }
    };
    // Only review panes take counts; elsewhere a typed count waits.
    let count = if app.input_mode == InputMode::Normal {
        app.pending_count.take()
    } else {
        None
    };
    dispatch_action(app, Invocation::new(action, count));
}

/// Releasing a key bound to a cursor move re-arms the two-press file walk
/// of single-file view (see `App::down_released_since_arm`).
pub fn handle_key_release(app: &mut App, key: KeyEvent) {
    let Some(context) = app.key_context() else {
        return;
    };
    match app.keymap.action(context, &[KeyChord::from(key)]) {
        Some(Action::CursorDown(_)) => app.down_released_since_arm = true,
        Some(Action::CursorUp(_)) => app.up_released_since_arm = true,
        _ => {}
    }
}

/// The fixed editor map for the text input that owns the keyboard.
fn map_text_key(app: &App, key: KeyEvent) -> Action {
    match app.input_mode {
        InputMode::Command => map_command_key(key),
        InputMode::Comment => map_comment_key(key),
        InputMode::FilePicker => map_file_picker_filter_key(key),
        _ => map_prompt_key(key),
    }
}

/// Terminals report Esc followed quickly by a key as that key with Alt
/// (tmux does so within its `escape-time`), so `Esc :` arrives as `Alt-:`.
/// When nothing binds the Alt chord, split it back into Esc and the bare key,
/// as Vim does. Ctrl+Alt is left alone: Windows reports AltGr that way.
fn split_escape_prefix(app: &mut App, key: KeyEvent) -> Option<KeyEvent> {
    if !key.modifiers.contains(KeyModifiers::ALT) || key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    let chord = KeyChord::from(key);
    let bound = match app.key_context() {
        Some(context) => {
            // The chord may also be the next key of a pending sequence.
            app.pending_keys.push(chord);
            let extends = app.keymap.binds(context, &app.pending_keys);
            app.pending_keys.pop();
            extends || app.keymap.binds(context, &[chord])
        }
        None => {
            app.keymap.binds(KeyContext::Global, &[chord]) || map_text_key(app, key) != Action::None
        }
    };
    (!bound).then(|| KeyEvent::new(key.code, key.modifiers - KeyModifiers::ALT))
}

/// Run a `global` binding (`Ctrl-l`) while a text input owns the keyboard.
/// Everywhere else the context's own map already includes the global layer.
/// Returns `true` when the key was consumed.
pub fn handle_global_key(app: &mut App, key: KeyEvent) -> bool {
    let chord = KeyChord::from(key);
    match app.keymap.action(KeyContext::Global, &[chord]).cloned() {
        Some(action) => {
            dispatch_action(app, action);
            true
        }
        None => false,
    }
}

pub fn handle_mouse_event(app: &mut App, event: MouseEvent) {
    let pos = Position::new(event.column, event.row);
    match event.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            let scroll_up = matches!(event.kind, MouseEventKind::ScrollUp);
            let action = if scroll_up {
                Action::MouseScrollUp(app.mouse_scroll_lines)
            } else {
                Action::MouseScrollDown(app.mouse_scroll_lines)
            };
            let over_file_list = app.file_list_area.is_some_and(|r| r.contains(pos));
            let over_diff = app.diff_area.is_some_and(|r| r.contains(pos));
            let over_commit_list = app.commit_list_inner_area.is_some_and(|r| r.contains(pos));
            match app.input_mode {
                InputMode::Help | InputMode::MessageDetails => handle_help_action(app, action),
                InputMode::Summary => handle_summary_action(app, action),
                InputMode::ThemePicker => {
                    app.move_theme_picker_selection(if scroll_up { -1 } else { 1 });
                }
                InputMode::FilePicker => {
                    app.move_file_picker_selection(if scroll_up { -1 } else { 1 });
                }
                InputMode::CommitSelect | InputMode::Normal if over_commit_list => {
                    wheel_commit_list(app, scroll_up);
                }
                InputMode::Normal if over_file_list => handle_file_list_action(app, action),
                InputMode::Normal
                    if app.comment_navigator_area.is_some_and(|r| r.contains(pos)) =>
                {
                    handle_comment_navigator_action(app, action)
                }
                InputMode::Normal if over_diff => handle_diff_action(app, action),
                InputMode::VisualSelect if over_diff => handle_diff_action(app, action),
                _ => {}
            }
            clear_visual_if_cursor_offscreen(app);
        }
        MouseEventKind::ScrollLeft | MouseEventKind::ScrollRight => {
            // Bypass the Action layer: trackpad swipes dispatch directly to
            // the diff viewport so they don't trigger keyboard focus-slide
            // (FEAT-0012) or abort an active VisualSelect.
            let scroll_left = matches!(event.kind, MouseEventKind::ScrollLeft);
            let over_diff = app.diff_area.is_some_and(|r| r.contains(pos));
            if over_diff && matches!(app.input_mode, InputMode::Normal | InputMode::VisualSelect) {
                if scroll_left {
                    app.scroll_left(WHEEL_COLS);
                } else {
                    app.scroll_right(WHEEL_COLS);
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left)
            if matches!(app.input_mode, InputMode::Normal | InputMode::VisualSelect) =>
        {
            // Set focus based on the outer panel area (includes the
            // border) so clicks on dead space still move focus.
            if app.file_list_area.is_some_and(|r| r.contains(pos)) {
                app.focused_panel = FocusedPanel::FileList;
            } else if app.diff_area.is_some_and(|r| r.contains(pos)) {
                app.focused_panel = FocusedPanel::Diff;
            }
            // Reset back to Normal so an Up without drag falls through to handle_left_click.
            if app.input_mode == InputMode::VisualSelect {
                app.exit_visual_mode();
            }
            app.mouse_drag_active = false;
            if app.diff_inner_area.is_some_and(|r| r.contains(pos))
                && let Some(point) = app.cell_to_sel_point(pos.x, pos.y)
            {
                app.visual_selection = Some(VisualSelection::collapsed(point));
                if let Some(idx) = app.diff_annotation_at_screen_row(pos.y) {
                    app.move_cursor_to_annotation(idx);
                }
            } else {
                app.visual_selection = None;
                handle_left_click(app, pos);
            }
        }
        MouseEventKind::Down(MouseButton::Left) if app.input_mode == InputMode::CommitSelect => {
            if let Some(idx) = app.commit_list_idx_at_screen_row(pos.y) {
                app.commit_list_cursor = idx;
                handle_commit_select_action(app, Action::ToggleSelect);
            }
        }
        MouseEventKind::Drag(MouseButton::Left)
            if matches!(app.input_mode, InputMode::Normal | InputMode::VisualSelect) =>
        {
            // An open file-tree prompt owns input; a drag must not start a
            // visual selection (it would strand the draft in VisualSelect).
            if app.file_tree_prompt_editing() {
                return;
            }
            let Some(sel) = app.visual_selection else {
                return;
            };
            let Some(mut head) = app.cell_to_sel_point(pos.x, pos.y) else {
                return;
            };
            // Pin head to the anchor's pane so cross-side drags don't escape.
            head.side = sel.anchor.side;
            if head == sel.head && app.mouse_drag_active {
                return;
            }
            let moved = head != sel.head;
            let promoted_now = moved && !app.mouse_drag_active;
            app.visual_selection = Some(VisualSelection {
                anchor: sel.anchor,
                head,
            });
            if moved {
                app.mouse_drag_active = true;
            }
            if promoted_now && app.input_mode == InputMode::Normal {
                app.input_mode = InputMode::VisualSelect;
            }
            if app.input_mode == InputMode::VisualSelect
                && head.annotation_idx != sel.head.annotation_idx
            {
                app.move_cursor_to_annotation(head.annotation_idx);
            }
        }
        MouseEventKind::Up(MouseButton::Left)
            if matches!(app.input_mode, InputMode::Normal | InputMode::VisualSelect) =>
        {
            if app.visual_selection.is_none() {
                return;
            }
            if !app.mouse_drag_active {
                app.visual_selection = None;
                if app.input_mode == InputMode::VisualSelect {
                    app.exit_visual_mode();
                }
                handle_left_click(app, pos);
            }
            app.mouse_drag_active = false;
        }
        _ => {}
    }
}

/// Helix-style: scrolling the cursor off-viewport drops the selection.
pub fn clear_visual_if_cursor_offscreen(app: &mut App) {
    if app.input_mode == InputMode::VisualSelect && !app.is_cursor_visible() {
        app.exit_visual_mode();
    }
}

/// Wheel scroll inside a commit list (full-screen picker or inline selector).
/// The list is short and selection-oriented, so each tick moves the cursor
/// rather than the viewport, matching how arrow keys behave.
fn wheel_commit_list(app: &mut App, scroll_up: bool) {
    // Match screen direction: the inline pane (Normal mode) reverses its
    // display in ascending order, so wheel-up moves toward newer (lower) data
    // indices there. The full-screen picker (CommitSelect) is always
    // newest-first.
    let scroll_up = if app.input_mode == InputMode::Normal && app.commits_ascending() {
        !scroll_up
    } else {
        scroll_up
    };
    for _ in 0..app.mouse_scroll_lines {
        if scroll_up {
            app.commit_select_up();
        } else {
            app.commit_select_down();
        }
    }
}

fn handle_left_click(app: &mut App, pos: Position) {
    if app.file_list_inner_area.is_some_and(|r| r.contains(pos))
        && let Some(idx) = app.file_list_idx_at_screen_row(pos.y)
    {
        app.focused_panel = FocusedPanel::FileList;
        app.file_list_state.select(idx);
        if let Some(item) = app.build_visible_items().get(idx).cloned() {
            match item {
                FileTreeItem::Directory { path, .. } => app.toggle_directory(&path),
                FileTreeItem::File { file_idx, .. } => {
                    app.jump_to_file(file_idx);
                    app.focused_panel = FocusedPanel::Diff;
                }
            }
        }
        return;
    }

    if app
        .comment_navigator_inner_area
        .is_some_and(|r| r.contains(pos))
        && let Some(idx) = app.comment_navigator_idx_at_screen_row(pos.y)
    {
        app.focused_panel = FocusedPanel::Comments;
        app.comment_navigator_state.select(idx);
        app.jump_to_selected_comment();
        return;
    }

    if app.has_inline_commit_selector()
        && app.commit_list_inner_area.is_some_and(|r| r.contains(pos))
        && let Some(idx) = app.commit_list_idx_at_screen_row(pos.y)
    {
        app.focused_panel = FocusedPanel::CommitSelector;
        app.commit_list_cursor = idx;
        handle_commit_selector_action(app, Action::ToggleExpand);
        return;
    }

    if app.diff_inner_area.is_some_and(|r| r.contains(pos))
        && let Some(idx) = app.diff_annotation_at_screen_row(pos.y)
    {
        app.focused_panel = FocusedPanel::Diff;
        app.move_cursor_to_annotation(idx);
        handle_diff_action(app, Action::Select);
    }
}

fn handle_copy_pr_url(app: &mut App) {
    let app::DiffSource::PullRequest(pr) = &app.diff_source else {
        app.set_warning(":copy-url only applies in PR mode");
        return;
    };
    let url = pr.url.clone();

    match copy_text_to_clipboard_with(
        &url,
        app.clipboard_override.as_deref(),
        app.clipboard_command.as_deref(),
    ) {
        Ok(copy) => app.set_copy_status(&copy, copy.message("PR URL")),
        Err(e) => app.set_warning(format!("Failed to copy PR URL: {e}")),
    }
}

/// Export review: either to clipboard or set pending stdout output based on app.output_to_stdout.
/// When output_to_stdout is true, stores the content and sets should_quit.
fn handle_export(app: &mut App) {
    let slug = app.session_slug();
    if app.output_to_stdout {
        match generate_export_content(
            &app.session,
            &app.diff_source,
            &app.comment_types,
            &app.export,
            &app.forge_review_threads,
            slug.as_deref(),
            &app.export_diff(),
        ) {
            Ok(content) => {
                app.pending_stdout_output = Some(content);
                app.should_quit = true;
            }
            Err(e) => app.set_warning(format!("{e}")),
        }
    } else {
        match export_to_clipboard_with(
            &app.session,
            &app.diff_source,
            &app.comment_types,
            &app.export,
            &app.forge_review_threads,
            slug.as_deref(),
            &app.export_diff(),
            app.clipboard_override.as_deref(),
            app.clipboard_command.as_deref(),
        ) {
            Ok(copy) => app.set_copy_status(&copy, copy.message("Review")),
            Err(e) => app.set_warning(format!("{e}")),
        }
    }
}

/// The path given to `:file <path>`, or `Some("")` for a bare `:file`.
fn file_command_path(cmd: &str) -> Option<&str> {
    let rest = cmd.strip_prefix("file")?;
    (rest.is_empty() || rest.starts_with(' ')).then(|| rest.trim())
}

/// `:file <path>`: write the review to a file. A leading `~/` means home.
fn handle_export_to_file(app: &mut App, path: &str) {
    if path.is_empty() {
        app.set_warning("Usage: :file <path>");
        return;
    }
    let path = match path.strip_prefix("~/") {
        Some(rest) => directories::BaseDirs::new()
            .map(|dirs| dirs.home_dir().join(rest))
            .unwrap_or_else(|| path.into()),
        None => path.into(),
    };
    let slug = app.session_slug();
    match export_to_file(
        &app.session,
        &app.diff_source,
        &app.comment_types,
        &app.export,
        &app.forge_review_threads,
        slug.as_deref(),
        &path,
        &app.export_diff(),
    ) {
        Ok(msg) => app.set_message(msg),
        Err(TuicrError::Io(e)) => {
            app.set_warning(format!("Could not write {}: {e}", path.display()))
        }
        Err(TuicrError::NoComments) => app.set_warning("No comments to write"),
        Err(e) => app.set_warning(format!("{e}")),
    }
}

/// Copy just the comment under the cursor (`Y`). Unlike `y`, the rest of the
/// review stays out of the clipboard, so a single comment can go straight into
/// a chat message or an agent prompt.
fn handle_copy_comment_at_cursor(app: &mut App) {
    let Some(content) = app
        .comment_content_at_cursor()
        .or_else(|| app.remote_comment_content_at_cursor())
    else {
        if app.cursor_on_remote_thread() {
            let forge = app.forge_display_name();
            app.set_message(format!("No copyable comment in this {forge} thread"));
        } else {
            app.set_message("No comment at cursor");
        }
        return;
    };
    match copy_text_to_clipboard_with(
        &content,
        app.clipboard_override.as_deref(),
        app.clipboard_command.as_deref(),
    ) {
        Ok(copy) => app.set_copy_status(&copy, copy.message("Comment")),
        Err(e) => app.set_warning(format!("{e}")),
    }
}

fn comment_line_start(buffer: &str, cursor: usize) -> usize {
    let cursor = cursor.min(buffer.len());
    match buffer[..cursor].rfind('\n') {
        Some(pos) => pos + 1,
        None => 0,
    }
}

fn comment_line_end(buffer: &str, cursor: usize) -> usize {
    let cursor = cursor.min(buffer.len());
    match buffer[cursor..].find('\n') {
        Some(pos) => cursor + pos,
        None => buffer.len(),
    }
}

fn comment_word_left(buffer: &str, cursor: usize) -> usize {
    let cursor = cursor.min(buffer.len());
    if cursor == 0 {
        return 0;
    }
    let before = &buffer[..cursor];
    let mut idx = 0;
    let mut found_word = false;
    for (pos, ch) in before.char_indices().rev() {
        if !ch.is_whitespace() {
            idx = pos;
            found_word = true;
            break;
        }
    }

    if !found_word {
        return 0;
    }

    for (pos, ch) in before[..idx].char_indices().rev() {
        if ch.is_whitespace() {
            return pos + ch.len_utf8();
        }
        idx = pos;
    }

    idx
}

fn comment_word_right(buffer: &str, cursor: usize) -> usize {
    let cursor = cursor.min(buffer.len());
    if cursor >= buffer.len() {
        return buffer.len();
    }

    let mut chars = buffer[cursor..].char_indices();
    if let Some((_, ch)) = chars.next()
        && ch.is_whitespace()
    {
        for (pos, ch) in buffer[cursor..].char_indices() {
            if !ch.is_whitespace() {
                return cursor + pos;
            }
        }
        return buffer.len();
    }

    let mut word_end = buffer.len();
    for (pos, ch) in buffer[cursor..].char_indices() {
        if ch.is_whitespace() {
            word_end = cursor + pos;
            break;
        }
    }

    if word_end >= buffer.len() {
        return buffer.len();
    }

    for (pos, ch) in buffer[word_end..].char_indices() {
        if !ch.is_whitespace() {
            return word_end + pos;
        }
    }

    buffer.len()
}

/// Append a pasted payload into a single-line buffer, dropping embedded
/// newlines so a multi-line paste doesn't smear across the prompt.
fn push_single_line(buffer: &mut String, text: &str) {
    for ch in text.chars() {
        if matches!(ch, '\n' | '\r') {
            continue;
        }
        buffer.push(ch);
    }
}

/// Handle actions in Help mode (scrolling only)
pub fn handle_help_action(app: &mut App, action: Action) {
    match action {
        Action::CursorDown(n) => app.help_scroll_down(n),
        Action::CursorUp(n) => app.help_scroll_up(n),
        Action::ScrollLeft(n) => app.help_scroll_left(n),
        Action::ScrollRight(n) => app.help_scroll_right(n),
        Action::HalfPageDown => app.help_scroll_down(app.help_state.viewport_height / 2),
        Action::HalfPageUp => app.help_scroll_up(app.help_state.viewport_height / 2),
        Action::PageDown => app.help_scroll_down(app.help_state.viewport_height),
        Action::PageUp => app.help_scroll_up(app.help_state.viewport_height),
        Action::GoToTop => app.help_scroll_to_top(),
        Action::GoToBottom => app.help_scroll_to_bottom(),
        Action::MouseScrollDown(n) => app.help_scroll_down(n),
        Action::MouseScrollUp(n) => app.help_scroll_up(n),
        Action::EnterSearchMode => app.enter_search_mode(),
        Action::SearchNext => {
            app.search_next_in_help();
        }
        Action::SearchPrev => {
            app.search_prev_in_help();
        }
        _ => {}
    }
}

/// Handle selection, scrolling, activation, and dismissal in the pending-comments summary view.
pub fn handle_summary_action(app: &mut App, action: Action) {
    match action {
        Action::CursorDown(n) => app.summary_select_down(n),
        Action::CursorUp(n) => app.summary_select_up(n),
        Action::HalfPageDown => {
            app.summary_scroll_down(app.summary_state.viewport_height / 2);
        }
        Action::HalfPageUp => {
            app.summary_scroll_up(app.summary_state.viewport_height / 2);
        }
        Action::PageDown => app.summary_scroll_down(app.summary_state.viewport_height),
        Action::PageUp => app.summary_scroll_up(app.summary_state.viewport_height),
        Action::GoToTop => app.summary_select_first(),
        Action::GoToBottom => app.summary_select_last(),
        Action::MouseScrollDown(n) => app.summary_scroll_down(n),
        Action::MouseScrollUp(n) => app.summary_scroll_up(n),
        Action::Select => {
            let selected_target = app
                .summary_state
                .targets
                .get(app.summary_state.selected_comment)
                .cloned()
                .flatten();
            if let Some(target) = selected_target {
                app.jump_to_summary_comment(target);
            } else if !app.summary_state.targets.is_empty() {
                app.set_warning("That comment is hidden by the current diff or filters");
            }
        }
        Action::DeleteComment => {
            if !app.delete_selected_summary_comment() {
                app.set_warning("Selected comment is no longer editable or available");
            }
        }
        Action::ExitMode => app.exit_summary_mode(),
        _ => {}
    }
}

/// Handle actions in Command mode (text input for :commands)
pub fn handle_command_action(app: &mut App, action: Action) {
    match action {
        Action::InsertChar(c) => {
            app.command_completion = None;
            app.command_buffer.push(c);
        }
        Action::Paste(text) => {
            app.command_completion = None;
            push_single_line(&mut app.command_buffer, &text);
        }
        Action::DeleteChar => {
            app.command_completion = None;
            app.command_buffer.pop();
        }
        Action::DeleteWord => {
            app.command_completion = None;
            let cursor = app.command_buffer.len();
            delete_word_before(&mut app.command_buffer, cursor);
        }
        Action::ClearLine => {
            app.command_completion = None;
            app.command_buffer.clear();
        }
        Action::CompleteCommand => complete_command(app, CompletionDirection::Forward),
        Action::CompleteCommandReverse => complete_command(app, CompletionDirection::Reverse),
        Action::ExitMode => app.exit_command_mode(),
        Action::SubmitInput => {
            let cmd = app.command_buffer.trim().to_string();
            // Leave command mode first so the command runs in the mode it was
            // typed from and can open a mode of its own.
            app.exit_command_mode();
            match parse_command(&cmd) {
                Some(ParsedCommand::Run(invocation)) => dispatch_action(app, invocation),
                Some(ParsedCommand::Theme(name)) => {
                    app.apply_theme(name);
                }
                Some(ParsedCommand::File(path)) => handle_export_to_file(app, path),
                None => app.set_message(format!("Unknown command: {cmd}")),
            }
        }
        _ => {}
    }
}

/// A `:` command line, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedCommand<'a> {
    Run(Invocation),
    /// `:theme <name>`.
    Theme(&'a str),
    /// `:file <path>`. The path is empty for a bare `:file`.
    File(&'a str),
}

fn parse_command(cmd: &str) -> Option<ParsedCommand<'_>> {
    if let Some(action) = Action::from_command(cmd) {
        return Some(ParsedCommand::Run(action.into()));
    }
    // `:N` and `:oN` are `{N}` counts on a line jump, like `NG`.
    let (action, digits) = match cmd.strip_prefix('o') {
        Some(rest) => (Action::GoToOldLine, rest),
        None => (Action::GoToLine, cmd),
    };
    if let Ok(line) = digits.parse::<u32>() {
        return Some(ParsedCommand::Run(Invocation::new(
            action,
            Some(line as usize),
        )));
    }
    if let Some(path) = file_command_path(cmd) {
        return Some(ParsedCommand::File(path));
    }
    let name = cmd.strip_prefix("theme ")?.trim();
    (!name.is_empty()).then_some(ParsedCommand::Theme(name))
}

/// CommandCompleter computes command-buffer replacements without mutating App.
struct CommandCompleter<'a> {
    /// Registry whose command names are exposed as completion candidates.
    specs: &'a [ActionSpec],
}

impl<'a> CommandCompleter<'a> {
    /// Build a completer over the command registry it should expose.
    pub fn new(specs: &'a [ActionSpec]) -> Self {
        Self { specs }
    }

    /// Return the next command-completion state for the current prompt state.
    ///
    /// `buffer` is the command text currently shown to the user. `active` is
    /// the previous completion cycle, if repeated Tab presses are cycling an
    /// existing match set. The result describes the next buffer contents and
    /// active completion cycle without mutating App.
    pub fn complete(
        &self,
        buffer: &str,
        active: Option<&CommandCompletionState>,
        direction: CompletionDirection,
    ) -> CompletionResult {
        if let Some(result) = self.cycle_existing(buffer, active, direction) {
            return result;
        }

        let prefix = buffer.trim_start().to_string();
        let matches: Vec<&'static str> = self
            .command_names()
            .filter(|command| command.starts_with(&prefix))
            .collect();

        match matches.len() {
            0 => CompletionResult::NoMatch,
            1 => CompletionResult::replace(matches[0], None),
            _ => {
                let common_prefix = Self::common_prefix(&matches);
                if common_prefix.len() > prefix.len() {
                    CompletionResult::replace(common_prefix, None)
                } else {
                    self.start_cycle(buffer, prefix, matches, direction)
                }
            }
        }
    }

    fn command_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.specs
            .iter()
            .flat_map(|spec| spec.commands.iter().copied())
    }

    fn cycle_existing(
        &self,
        buffer: &str,
        active: Option<&CommandCompletionState>,
        direction: CompletionDirection,
    ) -> Option<CompletionResult> {
        let completion = active?;
        let current = completion.matches.get(completion.selected)?;
        if buffer != *current {
            return None;
        }

        let selected = Self::next_index(completion.selected, completion.matches.len(), direction);
        Some(CompletionResult::replace(
            completion.matches[selected],
            Some(CommandCompletionState {
                prefix: completion.prefix.clone(),
                matches: completion.matches.clone(),
                selected,
            }),
        ))
    }

    fn start_cycle(
        &self,
        buffer: &str,
        prefix: String,
        matches: Vec<&'static str>,
        direction: CompletionDirection,
    ) -> CompletionResult {
        let selected = matches
            .iter()
            .position(|command| *command == buffer)
            .map(|idx| Self::next_index(idx, matches.len(), direction))
            .unwrap_or_else(|| match direction {
                CompletionDirection::Forward => 0,
                CompletionDirection::Reverse => matches.len() - 1,
            });

        CompletionResult::replace(
            matches[selected],
            Some(CommandCompletionState {
                prefix,
                matches,
                selected,
            }),
        )
    }

    fn next_index(selected: usize, match_count: usize, direction: CompletionDirection) -> usize {
        match direction {
            CompletionDirection::Forward => (selected + 1) % match_count,
            CompletionDirection::Reverse => (selected + match_count - 1) % match_count,
        }
    }

    fn common_prefix(matches: &[&str]) -> String {
        let mut prefix = matches
            .first()
            .map(|command| (*command).to_string())
            .unwrap_or_default();
        for command in &matches[1..] {
            while !command.starts_with(&prefix) {
                prefix.pop();
            }
        }
        prefix
    }
}

/// CompletionResult tells the handler how to update command-mode state.
enum CompletionResult {
    /// Leave the buffer unchanged and surface a no-match message.
    NoMatch,
    /// Replace the buffer and optionally keep a cycling state active.
    Replace {
        /// New command-buffer contents.
        buffer: String,
        /// Completion state used by the next Tab press, if cycling started.
        completion: Option<CommandCompletionState>,
    },
}

impl CompletionResult {
    fn replace(
        buffer: impl Into<String>,
        completion: Option<CommandCompletionState>,
    ) -> CompletionResult {
        CompletionResult::Replace {
            buffer: buffer.into(),
            completion,
        }
    }
}

fn reload_review(app: &mut App) {
    let comment_reload = app.reload_persisted_session_if_changed(true);
    if matches!(app.diff_source, app::DiffSource::PullRequest(_)) {
        if let Err(e) = comment_reload {
            app.set_warning(format!("Comment reload failed: {e}"));
        }
        // Async: shows a spinner in the status bar; result is applied in
        // `poll_pr_reload_events` and the cursor is restored to the captured
        // anchor.
        if let Err(e) = app.spawn_pr_reload() {
            app.set_error(format!("Reload failed: {e}"));
        }
    } else {
        match app.reload_diff_files() {
            Ok((count, invalidated)) => {
                let comment_suffix = match comment_reload {
                    Ok(added) if added > 0 => {
                        format!(", loaded {added} external comments")
                    }
                    Ok(_) => String::new(),
                    Err(e) => format!(", comment reload failed: {e}"),
                };
                if invalidated > 0 {
                    app.set_message(format!(
                        "Reloaded {count} files, {invalidated} changed since last review{comment_suffix}"
                    ));
                } else {
                    app.set_message(format!("Reloaded {count} files{comment_suffix}"));
                }
            }
            // A staged or unstaged review can empty out under it, for example
            // after a commit. That is an empty review, not a failed reload.
            Err(crate::error::TuicrError::NoChanges) if app.reload_can_empty() => {
                app.show_emptied_review();
            }
            Err(e) => app.set_error(format!("Reload failed: {e}")),
        }
    }
}

fn check_for_updates(app: &mut App) {
    match crate::update::check_for_updates() {
        crate::update::UpdateCheckResult::UpdateAvailable(info) => {
            app.set_message(format!(
                "Update available: v{} -> v{}",
                info.current_version, info.latest_version
            ));
        }
        crate::update::UpdateCheckResult::UpToDate(info) => {
            app.set_message(format!("tuicr v{} is up to date", info.current_version));
        }
        crate::update::UpdateCheckResult::AheadOfRelease(info) => {
            app.set_message(format!(
                "You're from the future! v{} > v{}",
                info.current_version, info.latest_version
            ));
        }
        crate::update::UpdateCheckResult::Failed(err) => {
            app.set_warning(format!("Update check failed: {err}"));
        }
    }
}

fn set_remote_comments_visibility(app: &mut App, visibility: PrCommentsVisibility) {
    if !matches!(app.diff_source, app::DiffSource::PullRequest(_)) {
        app.set_warning(":comments only applies in PR mode");
        return;
    }

    let changed = app.set_remote_comments_visibility(visibility);
    let label = visibility.label();
    if changed {
        app.set_message(format!("Remote comments: {label}"));
    } else {
        app.set_message(format!("Remote comments: already {label}"));
    }
}

fn complete_command(app: &mut App, direction: CompletionDirection) {
    let completer = CommandCompleter::new(ACTIONS);
    match completer.complete(
        &app.command_buffer,
        app.command_completion.as_ref(),
        direction,
    ) {
        CompletionResult::NoMatch => {
            app.command_completion = None;
            app.set_message("No command matches");
        }
        CompletionResult::Replace { buffer, completion } => {
            app.command_buffer = buffer;
            app.command_completion = completion;
        }
    }
}

/// Handle actions in Search mode (text input for /pattern)
pub fn handle_search_action(app: &mut App, action: Action) {
    match action {
        Action::InsertChar(c) => app.search_buffer.push(c),
        Action::Paste(text) => push_single_line(&mut app.search_buffer, &text),
        Action::DeleteChar => {
            app.search_buffer.pop();
        }
        Action::DeleteWord if !app.search_buffer.is_empty() => {
            while app
                .search_buffer
                .chars()
                .last()
                .map(|c| c.is_whitespace())
                .unwrap_or(false)
            {
                app.search_buffer.pop();
            }
            while app
                .search_buffer
                .chars()
                .last()
                .map(|c| !c.is_whitespace())
                .unwrap_or(false)
            {
                app.search_buffer.pop();
            }
        }
        Action::ClearLine => {
            app.search_buffer.clear();
        }
        Action::ExitMode => app.exit_search_mode(),
        Action::SubmitInput => {
            if app.searching_help() {
                app.search_in_help_from_scroll();
            } else {
                app.search_in_diff_from_cursor();
            }
            app.exit_search_mode();
        }
        _ => {}
    }
}

/// Handle actions in Comment mode (text input for comments)
pub fn handle_comment_action(app: &mut App, action: Action) {
    if matches!(
        action,
        Action::InsertChar(_)
            | Action::Paste(_)
            | Action::DeleteChar
            | Action::DeleteWord
            | Action::ClearLine
            | Action::TextCursorLeft
            | Action::TextCursorRight
            | Action::TextCursorLineStart
            | Action::TextCursorLineEnd
            | Action::TextCursorWordLeft
            | Action::TextCursorWordRight
    ) {
        app.comment_preferred_column = None;
    }
    match action {
        Action::InsertChar(c) => {
            app.comment_buffer.insert(app.comment_cursor, c);
            app.comment_cursor += c.len_utf8();
        }
        Action::Paste(text) => {
            let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
            app.comment_buffer
                .insert_str(app.comment_cursor, &normalized);
            app.comment_cursor += normalized.len();
        }
        Action::DeleteChar => {
            app.comment_cursor = delete_char_before(&mut app.comment_buffer, app.comment_cursor);
        }
        Action::ExitMode => app.exit_comment_mode(),
        Action::SubmitInput => app.save_comment(),
        Action::CycleCommentType => app.cycle_comment_type(),
        Action::CycleCommentTypeReverse => app.cycle_comment_type_reverse(),
        Action::TextCursorUp | Action::TextCursorDown => {
            app.comment_cursor = crate::ui::comment_panel::move_comment_cursor(
                &app.comment_buffer,
                app.comment_cursor,
                app.diff_state.viewport_width.saturating_sub(1),
                matches!(action, Action::TextCursorDown),
                &mut app.comment_preferred_column,
            );
        }
        Action::TextCursorLeft => {
            app.comment_cursor = prev_char_boundary(&app.comment_buffer, app.comment_cursor);
        }
        Action::TextCursorRight => {
            app.comment_cursor = next_char_boundary(&app.comment_buffer, app.comment_cursor);
        }
        Action::TextCursorLineStart => {
            app.comment_cursor = comment_line_start(&app.comment_buffer, app.comment_cursor);
        }
        Action::TextCursorLineEnd => {
            app.comment_cursor = comment_line_end(&app.comment_buffer, app.comment_cursor);
        }
        Action::TextCursorWordLeft => {
            app.comment_cursor = comment_word_left(&app.comment_buffer, app.comment_cursor);
        }
        Action::TextCursorWordRight => {
            app.comment_cursor = comment_word_right(&app.comment_buffer, app.comment_cursor);
        }
        Action::DeleteWord => {
            app.comment_cursor = delete_word_before(&mut app.comment_buffer, app.comment_cursor);
        }
        Action::ClearLine => {
            app.comment_buffer.clear();
            app.comment_cursor = 0;
        }
        _ => {}
    }
}

/// Handle actions in Confirm mode (Y/N prompts)
pub fn handle_confirm_action(app: &mut App, action: Action) {
    // Only the quit prompts end the session. A Sessions-tab delete answers
    // in place and returns to the selector.
    let quits = !matches!(
        app.pending_confirm,
        Some(app::ConfirmAction::DeleteSession | app::ConfirmAction::DeleteRemoteDraft)
    );
    match action {
        Action::ConfirmYes => {
            if let Some(app::ConfirmAction::DeleteSession) = app.pending_confirm {
                app.sessions_tab_delete_selected();
            }
            if let Some(app::ConfirmAction::DeleteRemoteDraft) = app.pending_confirm {
                app.spawn_remote_draft_delete();
            }
            if let Some(app::ConfirmAction::CopyAndQuit) = app.pending_confirm {
                let slug = app.session_slug();
                if app.output_to_stdout {
                    match generate_export_content(
                        &app.session,
                        &app.diff_source,
                        &app.comment_types,
                        &app.export,
                        &app.forge_review_threads,
                        slug.as_deref(),
                        &app.export_diff(),
                    ) {
                        Ok(content) => app.pending_stdout_output = Some(content),
                        Err(e) => app.set_warning(format!("{e}")),
                    }
                } else {
                    match export_to_clipboard_with(
                        &app.session,
                        &app.diff_source,
                        &app.comment_types,
                        &app.export,
                        &app.forge_review_threads,
                        slug.as_deref(),
                        &app.export_diff(),
                        app.clipboard_override.as_deref(),
                        app.clipboard_command.as_deref(),
                    ) {
                        // tuicr quits next, so the status bar would never show it.
                        Ok(copy) => app.pending_exit_warning = copy.fallback_warning,
                        Err(e) => app.set_warning(format!("{e}")),
                    }
                }
            }
            app.exit_confirm_mode();
            app.should_quit = quits;
        }
        Action::ConfirmNo => {
            app.exit_confirm_mode();
            app.should_quit = quits;
        }
        _ => {}
    }
}

/// Handle actions in CommitSelect mode.
///
/// CommitSelect actually drives the review target selector, which has two
/// tabs (Local and Pull Requests). Tab-shared actions (switch tab, quit) are
/// handled first; per-tab dispatch follows.
pub fn handle_commit_select_action(app: &mut App, action: Action) {
    // Filter-editing sub-state on the PR tab. Routed before tab dispatch
    // because typed characters must go to the filter buffer rather than
    // to local-commit movement.
    if app.pr_filter_editing() {
        handle_pr_filter_action(app, action);
        return;
    }

    match action {
        Action::TargetSelectorTabNext => app.cycle_target_tab(true),
        Action::TargetSelectorTabPrev => app.cycle_target_tab(false),
        Action::ExitMode => {
            // Esc during an in-flight PR open aborts the load and stays
            // in the selector. Takes precedence over the
            // commit-selection-range exit so the user isn't stuck staring
            // at a spinner.
            if app.cancel_pr_open() {
                return;
            }
            if !app.selector_opened_from_review && app.commit_selection_range.is_none() {
                return;
            }
            if let Err(e) = app.exit_commit_select_mode() {
                app.set_error(format!("Failed to reload changes: {e}"));
            }
        }
        other => match app.target_tab {
            TargetTab::Local => handle_local_target_action(app, other),
            TargetTab::PullRequests => handle_pr_target_action(app, other),
            TargetTab::Sessions => handle_sessions_target_action(app, other),
        },
    }
}

fn handle_sessions_target_action(app: &mut App, action: Action) {
    match action {
        Action::CursorUp(_) => app.sessions_tab_cursor_up(),
        Action::CursorDown(_) => app.sessions_tab_cursor_down(),
        Action::Select => {
            if let Err(e) = app.sessions_tab_select() {
                app.set_error(format!("Failed to open session: {e}"));
            }
        }
        Action::SessionsTabToggleShowEmpty => app.sessions_tab_toggle_show_empty(),
        Action::SessionsTabDelete => {
            if app.sessions_tab.cursor_session().is_some() {
                app.enter_confirm_mode(app::ConfirmAction::DeleteSession);
            }
        }
        // Space is a no-op: sessions are picked, not multi-selected.
        Action::ToggleSelect => {}
        _ => {}
    }
}

fn handle_local_target_action(app: &mut App, action: Action) {
    match action {
        Action::CursorUp(_) => app.commit_select_up(),
        Action::CursorDown(_) => app.commit_select_down(),
        Action::ToggleSelect => {
            // If on expand row, expand commits instead of toggling selection
            if app.is_on_expand_row() {
                if let Err(e) = app.expand_commit() {
                    app.set_error(format!("Failed to load commits: {e}"));
                }
            } else {
                // Toggle + auto-advance so repeated Space sweeps a range.
                app.toggle_commit_selection_and_advance();
            }
        }
        Action::Select => {
            // if on expand row, expand commit instead of confirming
            if app.is_on_expand_row() {
                if let Err(e) = app.expand_commit() {
                    app.set_error(format!("Failed to load commits: {e}"));
                }
            } else if let Err(e) = app.confirm_commit_selection() {
                app.set_error(format!("Failed to load commits: {e}"));
            }
        }
        _ => {}
    }
}

fn handle_pr_target_action(app: &mut App, action: Action) {
    match action {
        Action::CursorUp(_) => app.pr_tab_cursor_up(),
        Action::CursorDown(_) => app.pr_tab_cursor_down(),
        Action::Select => {
            app.pr_tab_select();
        }
        Action::ToggleSelect => {
            // Space is a no-op on the PR tab (spec).
        }
        Action::EnterSearchMode => {
            app.begin_pr_filter();
        }
        Action::TogglePrReviewRequestedFilter => {
            app.toggle_pr_review_requested_filter();
        }
        _ => {}
    }
}

fn handle_pr_filter_action(app: &mut App, action: Action) {
    match action {
        Action::InsertChar(c) => app.pr_filter_insert_char(c),
        Action::Paste(text) => {
            for ch in text.chars().filter(|c| !matches!(*c, '\n' | '\r')) {
                app.pr_filter_insert_char(ch);
            }
        }
        Action::DeleteChar => app.pr_filter_delete_char(),
        Action::DeleteWord => {
            // Soft word-delete: collapses to clear-line for the v1 cut.
            app.pr_filter_clear();
        }
        Action::ClearLine => app.pr_filter_clear(),
        Action::SubmitInput => app.commit_pr_filter(),
        Action::ExitMode => app.cancel_pr_filter(),
        _ => {}
    }
}

/// Handle actions when inline commit selector panel is focused
pub fn handle_commit_selector_action(app: &mut App, action: Action) {
    match action {
        // `j`/`k` track screen direction: in ascending (oldest-first) order the
        // display is reversed, so moving down the screen moves toward newer
        // (lower) data indices.
        Action::CursorDown(_) => {
            if app.commits_ascending() {
                app.commit_select_up();
            } else {
                app.commit_select_down();
            }
        }
        Action::CursorUp(_) => {
            if app.commits_ascending() {
                app.commit_select_down();
            } else {
                app.commit_select_up();
            }
        }
        // Space toggles + auto-advances so repeated presses sweep a contiguous
        // run; Enter selects only the cursor commit, unselecting the others.
        Action::ToggleExpand | Action::ToggleSelect | Action::Select => {
            if action == Action::Select {
                app.select_only_cursor_commit();
            } else {
                app.toggle_commit_selection_and_advance();
            }
            // PR mode reloads via the forge `compare` API on a background
            // thread (persisting the new range so it survives a restart);
            // local reviews reload from the VCS.
            if let Err(e) = app.reload_inline_selection_for_source() {
                app.set_error(format!("Failed to load diff: {e}"));
            }
        }
        // Normal mode maps Esc to `ClearSearchHighlight`; in this pane it
        // also returns focus to the diff.
        Action::ClearSearchHighlight => {
            app.clear_search_highlight();
            app.focused_panel = FocusedPanel::Diff;
        }
        _ => handle_shared_normal_action(app, action),
    }
}

/// Handle actions in VisualSelect mode
pub fn handle_visual_action(app: &mut App, action: Action) {
    match action {
        Action::CursorDown(n) => app.visual_cursor_down(n),
        Action::CursorUp(n) => app.visual_cursor_up(n),
        Action::AddRangeComment | Action::AddRangeSuggestion => {
            if action == Action::AddRangeSuggestion {
                app.enter_suggestion_from_visual();
            } else if app.visual_selection_line_range().is_some() {
                app.enter_comment_from_visual();
            } else {
                app.set_warning("Invalid selection - move cursor to a diff line");
                app.exit_visual_mode();
            }
        }
        Action::CopySelection => {
            match app.copy_visual_selection() {
                Ok((0, _)) => app.set_message("Nothing to copy"),
                Ok((_, Some(warning))) => app.set_warning(warning),
                Ok((n, None)) => app.set_message(format!("Copied {n} chars")),
                Err(e) => app.set_warning(format!("{e}")),
            }
            app.exit_visual_mode();
        }
        Action::ExitMode => app.exit_visual_mode(),
        Action::ScrollViewDown(n) | Action::MouseScrollDown(n) => app.scroll_view_down(n),
        Action::ScrollViewUp(n) | Action::MouseScrollUp(n) => app.scroll_view_up(n),
        Action::HalfPageDown => app.page_down(app.diff_state.viewport_height / 2),
        Action::HalfPageUp => app.page_up(app.diff_state.viewport_height / 2),
        Action::PageDown => app.page_down(app.diff_state.viewport_height),
        Action::PageUp => app.page_up(app.diff_state.viewport_height),
        _ => {}
    }
    clear_visual_if_cursor_offscreen(app);
}

/// Route an action in `InputMode::Normal`. An open file-tree prompt owns
/// input regardless of which panel has focus (a mouse click can move focus
/// off the tree while the prompt is still open); otherwise focus decides.
pub fn handle_normal_action(app: &mut App, action: Action) {
    if app.file_tree_prompt_editing() {
        return handle_file_list_action(app, action);
    }
    match app.focused_panel {
        FocusedPanel::FileList => handle_file_list_action(app, action),
        FocusedPanel::Comments => handle_comment_navigator_action(app, action),
        FocusedPanel::Diff => handle_diff_action(app, action),
        FocusedPanel::CommitSelector => handle_commit_selector_action(app, action),
    }
}

/// Route a bracketed-paste payload to whichever input is accepting text.
/// Everything else ignores it, so pasted text never drives normal-mode keys.
pub fn handle_paste(app: &mut App, text: String) {
    let action = Action::Paste(text);
    match app.input_mode {
        InputMode::Comment => handle_comment_action(app, action),
        InputMode::Command => handle_command_action(app, action),
        InputMode::Search => handle_search_action(app, action),
        InputMode::CommitSelect if app.pr_filter_editing() => {
            handle_commit_select_action(app, action)
        }
        InputMode::Normal if app.file_tree_prompt_editing() => handle_file_list_action(app, action),
        InputMode::ThemePicker if app.theme_picker_filtering() => {
            handle_theme_picker_action(app, action)
        }
        InputMode::FilePicker if app.file_picker_filtering() => {
            handle_file_picker_action(app, action)
        }
        _ => {}
    }
}

/// Handle actions when file list panel is focused
pub fn handle_file_list_action(app: &mut App, action: Action) {
    // An open `i`/`e`/`/` prompt owns keyboard input until it is submitted or
    // cancelled. The wheel still scrolls the list underneath it.
    if app.file_tree_prompt_editing()
        && !matches!(
            action,
            Action::MouseScrollDown(_) | Action::MouseScrollUp(_)
        )
    {
        handle_file_tree_prompt_action(app, action);
        return;
    }
    match action {
        Action::FileTreeFilterInclude => app.begin_file_tree_prompt(FileTreePrompt::Include),
        Action::FileTreeFilterExclude => app.begin_file_tree_prompt(FileTreePrompt::Exclude),
        Action::FileTreeClearInclude => app.clear_include_filter(),
        Action::FileTreeClearExclude => app.clear_exclude_filter(),
        Action::EnterSearchMode => app.begin_file_tree_prompt(FileTreePrompt::Search),
        // With a tree search active, n/N step file matches. Otherwise they
        // fall through to the diff search so the shared binding still works.
        Action::SearchNext if app.file_tree_search_active() => app.file_tree_search_next(),
        Action::SearchPrev if app.file_tree_search_active() => app.file_tree_search_prev(),
        Action::CursorDown(n) => app.file_list_down(n),
        Action::CursorUp(n) => app.file_list_up(n),
        Action::ScrollLeft(n) => app.file_list_state.scroll_left(n),
        Action::ScrollRight(n) => app.file_list_state.scroll_right(n),
        Action::MouseScrollDown(n) => app.file_list_viewport_scroll_down(n),
        Action::MouseScrollUp(n) => app.file_list_viewport_scroll_up(n),
        Action::Select | Action::ToggleExpand => {
            if let Some(item) = app.get_selected_tree_item() {
                match item {
                    FileTreeItem::Directory { path, .. } => app.toggle_directory(&path),
                    FileTreeItem::File { file_idx, .. } => {
                        app.jump_to_file(file_idx);
                        app.focused_panel = FocusedPanel::Diff;
                    }
                }
            }
        }
        Action::ToggleReviewed => {
            if let Some(FileTreeItem::File { file_idx, .. }) = app.get_selected_tree_item() {
                app.toggle_reviewed_for_file_idx(file_idx, false);
            } else {
                app.set_warning("Select a file to toggle reviewed");
            }
        }
        _ => handle_shared_normal_action(app, action),
    }
}

/// Handle input while a file-tree prompt (`i` include, `e` exclude, `/`
/// search) is open. Mirrors `handle_pr_filter_action` for the target selector.
fn handle_file_tree_prompt_action(app: &mut App, action: Action) {
    match action {
        Action::InsertChar(c) => app.file_tree_prompt_insert_char(c),
        Action::Paste(text) => app.file_tree_prompt_insert_str(&text),
        Action::DeleteChar => app.file_tree_prompt_delete_char(),
        Action::DeleteWord => app.file_tree_prompt_delete_word(),
        Action::ClearLine => app.file_tree_prompt_clear_line(),
        Action::SubmitInput => app.commit_file_tree_prompt(),
        Action::ExitMode => app.cancel_file_tree_prompt(),
        _ => {}
    }
}

/// Handle actions in `InputMode::FilePicker`, with the same draft gate as
/// `handle_theme_picker_action`.
pub fn handle_file_picker_action(app: &mut App, action: Action) {
    if app.file_picker_filtering() {
        match action {
            Action::InsertChar(c) => app.file_picker_filter_insert_char(c),
            Action::Paste(text) => text
                .chars()
                .filter(|c| !matches!(c, '\n' | '\r'))
                .for_each(|c| app.file_picker_filter_insert_char(c)),
            Action::DeleteChar => app.file_picker_filter_delete_char(),
            Action::DeleteWord => app.file_picker_filter_delete_word(),
            Action::ClearLine => app.file_picker_filter_clear_line(),
            Action::CursorDown(n) => app.move_file_picker_selection(n as isize),
            Action::CursorUp(n) => app.move_file_picker_selection(-(n as isize)),
            Action::SubmitInput => app.submit_file_picker_filter(),
            Action::ExitMode => app.cancel_file_picker_filter(),
            _ => {}
        }
        return;
    }
    match action {
        Action::CursorDown(n) => app.move_file_picker_selection(n as isize),
        Action::CursorUp(n) => app.move_file_picker_selection(-(n as isize)),
        Action::EnterSearchMode => app.begin_file_picker_filter(),
        Action::Select => app.confirm_file_picker(),
        Action::ExitMode => app.cancel_file_picker(),
        _ => {}
    }
}

/// Handle actions in `InputMode::ThemePicker`. Mirrors
/// `handle_file_list_action`'s prompt-editing gate: while the `/` filter
/// draft is open, input goes to `handle_theme_picker_filter_action` instead
/// of list navigation.
pub fn handle_theme_picker_action(app: &mut App, action: Action) {
    if app.theme_picker_filtering() {
        handle_theme_picker_filter_action(app, action);
        return;
    }
    match action {
        Action::CursorDown(n) => app.move_theme_picker_selection(n as isize),
        Action::CursorUp(n) => app.move_theme_picker_selection(-(n as isize)),
        Action::EnterSearchMode => app.begin_theme_picker_filter(),
        Action::Select => app.confirm_theme_picker(),
        Action::ExitMode => app.cancel_theme_picker(),
        _ => {}
    }
}

/// Handle input while the theme picker's `/` filter draft is open. Mirrors
/// `handle_file_tree_prompt_action`.
fn handle_theme_picker_filter_action(app: &mut App, action: Action) {
    match action {
        Action::InsertChar(c) => app.theme_picker_filter_insert_char(c),
        Action::Paste(text) => text
            .chars()
            .filter(|c| !matches!(c, '\n' | '\r'))
            .for_each(|c| app.theme_picker_filter_insert_char(c)),
        Action::DeleteChar => app.theme_picker_filter_delete_char(),
        Action::DeleteWord => app.theme_picker_filter_delete_word(),
        Action::ClearLine => app.theme_picker_filter_clear_line(),
        Action::SubmitInput => app.commit_theme_picker_filter(),
        Action::ExitMode => app.cancel_theme_picker_filter(),
        _ => {}
    }
}

/// Handle actions when the comment navigator panel is focused
pub fn handle_comment_navigator_action(app: &mut App, action: Action) {
    match action {
        Action::CursorDown(n) => {
            app.comment_navigator_down(n);
            // Auto-follow: scroll the diff to the selected comment without
            // stealing focus from the navigator (Enter still jumps + focuses).
            let saved_focus = app.focused_panel;
            app.jump_to_selected_comment();
            app.focused_panel = saved_focus;
        }
        Action::CursorUp(n) => {
            app.comment_navigator_up(n);
            let saved_focus = app.focused_panel;
            app.jump_to_selected_comment();
            app.focused_panel = saved_focus;
        }
        Action::ScrollLeft(n) => app.comment_navigator_state.scroll_left(n),
        Action::ScrollRight(n) => app.comment_navigator_state.scroll_right(n),
        Action::MouseScrollDown(n) => app.comment_navigator_viewport_scroll_down(n),
        Action::MouseScrollUp(n) => app.comment_navigator_viewport_scroll_up(n),
        Action::Select => {
            app.jump_to_selected_comment();
        }
        _ => handle_shared_normal_action(app, action),
    }
}

/// Enter edit mode for the comment under the cursor, placing the text cursor at
/// the end (vim `A` / non-vim default) or beginning (vim `i`). Surfaces the
/// right message when the comment is read-only or absent.
fn edit_comment_at_cursor(app: &mut App, cursor_at_end: bool) {
    if app.cursor_on_locked_comment() {
        let forge = app.forge_display_name();
        app.set_message(format!(
            "Comment already pushed to {forge} — read only in tuicr"
        ));
    } else if !app.enter_edit_mode(cursor_at_end) {
        if app.cursor_on_remote_thread() {
            let forge = app.forge_display_name();
            app.set_message(format!("{forge} comment — read only in tuicr"));
        } else {
            app.set_message("No comment at cursor");
        }
    }
}

fn delete_comment_at_cursor(app: &mut App) {
    if app.cursor_on_locked_comment() {
        let forge = app.forge_display_name();
        app.set_message(format!(
            "Comment already pushed to {forge} — read only in tuicr"
        ));
    } else if !app.delete_comment_at_cursor() {
        if app.begin_remote_draft_delete() {
            // Waiting on the confirmation prompt.
        } else if app.cursor_on_remote_thread() {
            let forge = app.forge_display_name();
            app.set_message(format!("{forge} comment — read only in tuicr"));
        } else {
            app.set_message("No comment at cursor");
        }
    }
}

/// Handle actions when diff panel is focused
pub fn handle_diff_action(app: &mut App, action: Action) {
    match action {
        Action::CursorDown(n) => app.cursor_down(n),
        Action::CursorUp(n) => app.cursor_up(n),
        Action::ScrollViewDown(n) => app.scroll_view_down(n),
        Action::ScrollViewUp(n) => app.scroll_view_up(n),
        Action::ScrollLeft(n) => app.scroll_left(n),
        Action::ScrollRight(n) => app.scroll_right(n),
        Action::MouseScrollDown(n) => app.scroll_view_down(n),
        Action::MouseScrollUp(n) => app.scroll_view_up(n),
        Action::Select => {
            if let Some(hit) = app.get_gap_at_cursor() {
                match hit {
                    GapCursorHit::Expander(gap_id, dir) => {
                        let limit = if dir == ExpandDirection::Both {
                            None
                        } else {
                            Some(20)
                        };
                        if let Err(e) = app.expand_gap(gap_id, dir, limit) {
                            app.set_error(format!("Failed to expand: {e}"));
                        }
                    }
                    GapCursorHit::HiddenLines(gap_id) => {
                        if let Err(e) = app.expand_gap(gap_id, ExpandDirection::Both, None) {
                            app.set_error(format!("Failed to expand: {e}"));
                        }
                    }
                    GapCursorHit::ExpandedContent(gap_id) => {
                        app.collapse_gap(gap_id);
                    }
                }
            }
        }
        // Space already means "expand what's under the cursor" in the file
        // list and the commit selector; in the diff panel it was unbound, so
        // it takes on the same meaning here: toggle whether the current
        // file's diff body is shown, regardless of why it defaulted to
        // collapsed or expanded.
        Action::ToggleExpand => {
            if !app.toggle_file_collapse() {
                handle_shared_normal_action(app, action);
            }
        }
        Action::SelectFull => {
            if let Some(hit) = app.get_gap_at_cursor() {
                match hit {
                    GapCursorHit::Expander(gap_id, _) | GapCursorHit::HiddenLines(gap_id) => {
                        if let Err(e) = app.expand_gap(gap_id, ExpandDirection::Both, None) {
                            app.set_error(format!("Failed to expand: {e}"));
                        }
                    }
                    GapCursorHit::ExpandedContent(gap_id) => {
                        app.collapse_gap(gap_id);
                    }
                }
            }
        }
        _ => handle_shared_normal_action(app, action),
    }
}

/// Handle actions shared between file list and diff panels in Normal mode
fn handle_shared_normal_action(app: &mut App, action: Action) {
    match action {
        Action::ExitMode => {
            app.show_file_list = false;
            app.focused_panel = FocusedPanel::Diff;
        }
        Action::HalfPageDown => app.page_down(app.diff_state.viewport_height / 2),
        Action::HalfPageUp => app.page_up(app.diff_state.viewport_height / 2),
        Action::PageDown => app.page_down(app.diff_state.viewport_height),
        Action::PageUp => app.page_up(app.diff_state.viewport_height),
        Action::GoToTop => app.jump_to_top(),
        Action::GoToBottom => app.jump_to_bottom(),
        Action::NextFile => app.next_file(),
        Action::PrevFile => app.prev_file(),
        Action::NextHunk => app.next_hunk(),
        Action::PrevHunk => app.prev_hunk(),
        Action::NextComment => app.next_comment(),
        Action::PrevComment => app.prev_comment(),
        Action::ToggleReviewed => app.toggle_reviewed(),
        Action::ToggleHunkReviewed => app.toggle_hunk_reviewed(),
        Action::ToggleFocus => {
            let has_selector = app.has_inline_commit_selector();
            let has_comments = app.has_comment_navigator_items();
            // Cycle: FileList -> Diff -> CommitSelector -> Comments -> FileList,
            // skipping panels that aren't present.
            app.focused_panel = match (app.focused_panel, has_selector, has_comments) {
                (FocusedPanel::FileList, _, _) => FocusedPanel::Diff,
                (FocusedPanel::Diff, true, _) => FocusedPanel::CommitSelector,
                (FocusedPanel::Diff, false, true) => FocusedPanel::Comments,
                (FocusedPanel::Diff, false, false) => FocusedPanel::FileList,
                (FocusedPanel::CommitSelector, _, true) => FocusedPanel::Comments,
                (FocusedPanel::CommitSelector, _, false) => FocusedPanel::FileList,
                (FocusedPanel::Comments, _, _) => FocusedPanel::FileList,
            };
            if matches!(
                app.focused_panel,
                FocusedPanel::FileList | FocusedPanel::Comments
            ) {
                app.show_file_list = true;
            }
        }
        Action::ToggleFocusReverse => {
            let has_selector = app.has_inline_commit_selector();
            let has_comments = app.has_comment_navigator_items();
            app.focused_panel = match (app.focused_panel, has_selector, has_comments) {
                (FocusedPanel::FileList, _, true) => FocusedPanel::Comments,
                (FocusedPanel::FileList, true, false) => FocusedPanel::CommitSelector,
                (FocusedPanel::FileList, false, false) => FocusedPanel::Diff,
                (FocusedPanel::Comments, true, _) => FocusedPanel::CommitSelector,
                (FocusedPanel::Comments, false, _) => FocusedPanel::Diff,
                (FocusedPanel::Diff, _, _) => FocusedPanel::FileList,
                (FocusedPanel::CommitSelector, _, _) => FocusedPanel::Diff,
            };
            if matches!(
                app.focused_panel,
                FocusedPanel::FileList | FocusedPanel::Comments
            ) {
                app.show_file_list = true;
            }
        }
        Action::ExpandAll => {
            app.expand_all_dirs();
            app.set_message("All directories expanded");
        }
        Action::CollapseAll => {
            app.collapse_all_dirs();
            app.set_message("All directories collapsed");
        }
        Action::EnterSearchMode => app.enter_search_mode(),
        Action::AddLineComment => {
            let line = app.get_line_at_cursor();
            if line.is_some() {
                app.enter_comment_mode(false, line);
            } else {
                app.set_message("Move cursor to a diff line to add a line comment");
            }
        }
        Action::AddFileComment => app.enter_comment_mode(true, None),
        // `i` edits the comment at cursor. In vim mode the text cursor starts at
        // the beginning; otherwise (and for `A`) it starts at the end.
        Action::EditComment => edit_comment_at_cursor(app, !app.comment_vim_enabled),
        // `A` (vim only) edits with the text cursor at end-of-line.
        Action::EditCommentAtEnd if app.comment_vim_enabled => edit_comment_at_cursor(app, true),
        Action::CopyCommentAtCursor => handle_copy_comment_at_cursor(app),
        Action::SearchNext => {
            app.search_next_in_diff();
        }
        Action::SearchPrev => {
            app.search_prev_in_diff();
        }
        Action::ClearSearchHighlight => app.clear_search_highlight(),
        Action::EnterVisualMode => {
            if app.get_line_at_cursor().is_some() {
                app.enter_visual_mode_at_cursor();
            } else {
                app.set_message("Move cursor to a diff line to start visual selection");
            }
        }
        // `(` moves toward the top row, `)` toward the bottom row. The cycle
        // methods walk data indices (newest-first), so ascending display order
        // swaps which one runs to keep the on-screen direction stable.
        Action::CycleCommitNext if app.has_review_commits() => {
            if app.commits_ascending() {
                app.cycle_commit_prev();
            } else {
                app.cycle_commit_next();
            }
            if let Err(e) = app.reload_inline_selection_for_source() {
                app.set_error(format!("Failed to load diff: {e}"));
            }
        }
        Action::CycleCommitPrev if app.has_review_commits() => {
            if app.commits_ascending() {
                app.cycle_commit_next();
            } else {
                app.cycle_commit_prev();
            }
            if let Err(e) = app.reload_inline_selection_for_source() {
                app.set_error(format!("Failed to load diff: {e}"));
            }
        }
        Action::DeleteComment => delete_comment_at_cursor(app),
        _ => {}
    }
}

/// Handle actions in the submit resolver modal: pick what to do with each
/// comment that did not map to an inline GitHub review comment.
pub fn handle_submit_resolver_action(app: &mut App, action: Action) {
    match action {
        Action::CursorDown(_) => app.submit_resolver_cursor_down(),
        Action::CursorUp(_) => app.submit_resolver_cursor_up(),
        Action::Select | Action::ToggleSelect => app.submit_resolver_toggle(),
        Action::SubmitResolverAdvance => app.submit_resolver_advance(),
        Action::ExitMode => app.cancel_submit(),
        _ => {}
    }
}

/// Handle actions in the bare-`:submit` action picker. Up/down move the
/// cursor through Comment/Approve/Request changes/Draft; Enter dispatches
/// preflight with the picked event (skipping the confirmation modal); Esc
/// cancels back to normal.
pub fn handle_submit_action_picker_action(app: &mut App, action: Action) {
    match action {
        Action::CursorDown(_) => app.submit_picker_cursor_down(),
        Action::CursorUp(_) => app.submit_picker_cursor_up(),
        Action::Select => app.submit_picker_confirm(),
        Action::ExitMode => app.cancel_submit_action_picker(),
        _ => {}
    }
}

/// Handle actions in the final submit confirmation modal.
pub fn handle_submit_confirm_action(app: &mut App, action: Action) {
    match action {
        Action::ConfirmYes => app.confirm_submit(),
        Action::ConfirmNo => app.cancel_submit(),
        // Only meaningful when the stale-head warning is visible.
        Action::SubmitReloadPr
            if app.submit_head_is_stale()
                && matches!(app.diff_source, app::DiffSource::PullRequest(_)) =>
        {
            app.cancel_submit();
            if let Err(e) = app.spawn_pr_reload() {
                app.set_error(format!("Reload failed: {e}"));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod command_tests {
    use super::{ParsedCommand, parse_command};
    use crate::input::{Action, Invocation};

    fn runs(cmd: &str) -> Option<Invocation> {
        match parse_command(cmd)? {
            ParsedCommand::Run(invocation) => Some(invocation),
            ParsedCommand::Theme(_) | ParsedCommand::File(_) => None,
        }
    }

    #[test]
    fn parses_relative_line_number_commands() {
        assert_eq!(
            runs("set relativenumber"),
            Some(Action::SetRelativeNumber.into())
        );
        assert_eq!(
            runs("set norelativenumber"),
            Some(Action::SetNoRelativeNumber.into())
        );
        assert_eq!(
            runs("set relativenumber!"),
            Some(Action::ToggleRelativeNumber.into())
        );
    }

    #[test]
    fn parses_word_diff_commands() {
        assert_eq!(runs("set worddiff"), Some(Action::SetWordDiff.into()));
        assert_eq!(runs("set noworddiff"), Some(Action::SetNoWordDiff.into()));
        assert_eq!(runs("set worddiff!"), Some(Action::ToggleWordDiff.into()));
    }

    #[test]
    fn parses_generated_commands() {
        assert_eq!(
            runs("set generated"),
            Some(Action::CollapseGenerated.into())
        );
        assert_eq!(runs("set nogenerated"), Some(Action::ShowGenerated.into()));
        assert_eq!(runs("set generated!"), Some(Action::ToggleGenerated.into()));
        assert_eq!(runs("generated"), Some(Action::ToggleGenerated.into()));
    }

    #[test]
    fn parses_minimal_ui_commands() {
        assert_eq!(runs("set minimal"), Some(Action::SetMinimalUi.into()));
        assert_eq!(runs("set nominimal"), Some(Action::SetNoMinimalUi.into()));
        assert_eq!(runs("set minimal!"), Some(Action::ToggleMinimalUi.into()));
    }

    #[test]
    fn parses_file_command_paths() {
        assert_eq!(
            super::file_command_path("file review.md"),
            Some("review.md")
        );
        assert_eq!(super::file_command_path("file   a b.md "), Some("a b.md"));
        assert_eq!(super::file_command_path("file"), Some(""));
        assert_eq!(super::file_command_path("files"), None);
        assert_eq!(super::file_command_path("focus"), None);
        assert_eq!(
            parse_command("file review.md"),
            Some(ParsedCommand::File("review.md"))
        );
    }

    #[test]
    fn parses_copy_url_command() {
        assert_eq!(runs("copy-url"), Some(Action::CopyUrl.into()));
    }

    #[test]
    fn parses_every_reviewed_visibility_command_form() {
        // Mirrors `:set commits` / `:set nocommits` / `:set commits!`, plus a
        // bare toggle alias in the shape of `:wrap`.
        assert_eq!(runs("set reviewed"), Some(Action::ShowReviewed.into()));
        assert_eq!(runs("set noreviewed"), Some(Action::HideReviewed.into()));
        assert_eq!(
            runs("set reviewed!"),
            Some(Action::ToggleShowReviewed.into())
        );
        assert_eq!(runs("reviewed"), Some(Action::ToggleShowReviewed.into()));
    }

    #[test]
    fn parses_line_numbers_as_counted_line_jumps() {
        assert_eq!(
            runs("42"),
            Some(Invocation::new(Action::GoToLine, Some(42)))
        );
        assert_eq!(
            runs("o7"),
            Some(Invocation::new(Action::GoToOldLine, Some(7)))
        );
        assert_eq!(parse_command("4294967297"), None);
    }

    #[test]
    fn parses_theme_names_and_rejects_a_bare_theme_prefix() {
        assert_eq!(
            parse_command("theme nord"),
            Some(ParsedCommand::Theme("nord"))
        );
        assert_eq!(runs("theme"), Some(Action::ThemePicker.into()));
        assert_eq!(parse_command("theme "), None);
        assert_eq!(parse_command("nope"), None);
    }
}
