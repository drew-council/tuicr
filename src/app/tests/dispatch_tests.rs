use crate::app::*;
use crate::handler::dispatch_action;
use crate::input::{Action, Invocation};
use crate::model::LineSide;
use crate::model::{DiffFile, DiffHunk, DiffLine, FileStatus, LineOrigin};
use crate::vcs::traits::{VcsBackend, VcsInfo, VcsType};
use std::path::PathBuf;

struct StubVcs(VcsInfo);
impl VcsBackend for StubVcs {
    fn info(&self) -> &VcsInfo {
        &self.0
    }
    fn get_working_tree_diff(
        &self,
        _hl: &crate::syntax::SyntaxHighlighter,
    ) -> crate::error::Result<Vec<DiffFile>> {
        Ok(Vec::new())
    }
    fn fetch_context_lines(
        &self,
        _path: &std::path::Path,
        _status: FileStatus,
        _ref_commit: Option<&str>,
        _start: u32,
        _end: u32,
    ) -> crate::error::Result<Vec<DiffLine>> {
        Ok(Vec::new())
    }
    fn file_line_count(
        &self,
        _path: &std::path::Path,
        _status: FileStatus,
        _ref_commit: Option<&str>,
    ) -> crate::error::Result<u32> {
        Ok(0)
    }
}

fn line(origin: LineOrigin, content: &str, old: Option<u32>, new: Option<u32>) -> DiffLine {
    DiffLine {
        origin,
        content: content.to_string(),
        old_lineno: old,
        new_lineno: new,
        raw_content: None,
        highlighted_spans: None,
        stage: None,
    }
}

fn file(path: &str, contents: &[&str]) -> DiffFile {
    let lines = contents
        .iter()
        .enumerate()
        .map(|(idx, content)| line(LineOrigin::Addition, content, None, Some(idx as u32 + 1)))
        .collect::<Vec<_>>();
    let hunks = vec![DiffHunk {
        header: "@@ -0,0 +1 @@".to_string(),
        lines,
        old_start: 0,
        old_count: 0,
        new_start: 1,
        new_count: contents.len() as u32,
    }];
    let content_hash = DiffFile::compute_content_hash(&hunks);
    DiffFile {
        old_path: None,
        new_path: Some(PathBuf::from(path)),
        status: FileStatus::Modified,
        hunks,
        is_binary: false,
        is_too_large: false,
        is_commit_message: false,
        content_hash,
    }
}

fn app_with(files: Vec<DiffFile>) -> App {
    let vcs_info = VcsInfo {
        root_path: PathBuf::from("/tmp"),
        head_commit: "head".into(),
        branch_name: Some("main".into()),
        vcs_type: VcsType::Git,
    };
    let session = ReviewSession::new(
        vcs_info.root_path.clone(),
        vcs_info.head_commit.clone(),
        vcs_info.branch_name.clone(),
        SessionDiffSource::WorkingTree,
    );
    App::build(
        Box::new(StubVcs(vcs_info.clone())),
        vcs_info,
        crate::theme::Theme::dark(),
        None,
        false,
        files,
        session,
        DiffSource::WorkingTree,
        InputMode::Normal,
        Vec::new(),
        None,
        None,
    )
    .expect("build app")
}

fn three_files() -> App {
    let lines: Vec<String> = (1..=30).map(|n| format!("line {n}")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut app = app_with(vec![
        file("a.rs", &lines),
        file("b.rs", &lines),
        file("c.rs", &lines),
    ]);
    app.diff_state.viewport_height = 10;
    app.focused_panel = FocusedPanel::Diff;
    app
}

#[test]
fn should_scale_motion_steps_by_the_count() {
    let mut counted = three_files();
    let mut manual = three_files();

    dispatch_action(
        &mut counted,
        Invocation::new(Action::CursorDown(1), Some(5)),
    );
    manual.cursor_down(5);

    assert_eq!(
        counted.diff_state.cursor_line,
        manual.diff_state.cursor_line
    );
    assert_ne!(
        counted.diff_state.cursor_line,
        three_files().diff_state.cursor_line
    );
}

#[test]
fn should_repeat_file_jumps_by_the_count() {
    let mut counted = three_files();
    let mut manual = three_files();

    dispatch_action(&mut counted, Invocation::new(Action::NextFile, Some(2)));
    manual.next_file();
    manual.next_file();

    assert_eq!(
        counted.diff_state.cursor_line,
        manual.diff_state.cursor_line
    );
}

#[test]
fn should_turn_a_counted_go_to_bottom_into_a_line_jump() {
    let mut counted = three_files();
    let mut manual = three_files();

    dispatch_action(&mut counted, Invocation::new(Action::GoToBottom, Some(7)));
    manual.go_to_source_line(7, LineSide::New);

    assert_eq!(
        counted.diff_state.cursor_line,
        manual.diff_state.cursor_line
    );
}

#[test]
fn should_drop_the_count_for_actions_that_ignore_it() {
    let mut counted = three_files();
    let mut manual = three_files();

    let start = counted.diff_state.cursor_line;

    dispatch_action(&mut counted, Invocation::new(Action::HalfPageDown, Some(4)));
    dispatch_action(&mut manual, Action::HalfPageDown);

    assert_ne!(counted.diff_state.cursor_line, start);
    assert_eq!(
        counted.diff_state.cursor_line,
        manual.diff_state.cursor_line
    );
}

#[test]
fn should_quit_a_clean_session() {
    let mut app = three_files();

    dispatch_action(&mut app, Action::Quit);

    assert!(app.should_quit);
}

fn with_unsaved_comment() -> App {
    let mut app = three_files();
    let path = PathBuf::from("a.rs");
    app.session.add_file(path.clone(), FileStatus::Modified, 0);
    app.session
        .get_file_mut(&path)
        .unwrap()
        .add_file_comment(crate::model::Comment::new(
            "note".to_string(),
            crate::model::CommentType::from_id("note"),
            None,
        ));
    app.dirty = true;
    app
}

#[test]
fn should_take_a_second_press_to_confirm_quit_with_unsaved_comments() {
    let mut app = with_unsaved_comment();

    dispatch_action(&mut app, Action::ConfirmQuit);
    assert!(!app.should_quit);

    dispatch_action(&mut app, Action::ConfirmQuit);
    assert!(app.should_quit);
}

#[test]
fn should_ask_again_when_another_action_follows_the_quit_warning() {
    let mut app = with_unsaved_comment();

    dispatch_action(&mut app, Action::ConfirmQuit);
    dispatch_action(&mut app, Action::ToggleReviewed);
    dispatch_action(&mut app, Action::ConfirmQuit);

    assert!(!app.should_quit);
}

#[test]
fn should_keep_the_quit_warning_across_a_cursor_motion() {
    let mut app = with_unsaved_comment();

    dispatch_action(&mut app, Action::ConfirmQuit);
    dispatch_action(&mut app, Action::CursorDown(1));
    dispatch_action(&mut app, Action::ConfirmQuit);

    assert!(app.should_quit);
}

#[test]
fn should_confirm_quit_at_once_outside_the_review_panes() {
    let mut app = with_unsaved_comment();
    app.input_mode = InputMode::VisualSelect;

    dispatch_action(&mut app, Action::ConfirmQuit);

    assert!(app.should_quit);
}

#[test]
fn should_force_quit_a_dirty_session() {
    let mut app = three_files();
    app.dirty = true;

    dispatch_action(&mut app, Action::ForceQuit);

    assert!(app.should_quit);
}

#[test]
fn should_run_the_same_action_for_a_command_and_a_key() {
    let mut by_command = three_files();
    by_command.enter_command_mode();
    by_command.command_buffer = "set relativenumber!".into();
    crate::handler::handle_command_action(&mut by_command, Action::SubmitInput);

    let mut by_key = three_files();
    dispatch_action(&mut by_key, Action::ToggleRelativeNumber);

    assert_eq!(by_command.input_mode, InputMode::Normal);
    assert!(by_command.relative_line_numbers);
    assert!(by_key.relative_line_numbers);
}

#[test]
fn should_run_a_line_number_command_as_a_counted_jump() {
    let mut by_command = three_files();
    by_command.enter_command_mode();
    by_command.command_buffer = "12".into();
    crate::handler::handle_command_action(&mut by_command, Action::SubmitInput);

    let mut by_key = three_files();
    dispatch_action(&mut by_key, Invocation::new(Action::GoToBottom, Some(12)));

    assert_eq!(
        by_command.diff_state.cursor_line,
        by_key.diff_state.cursor_line
    );
}
