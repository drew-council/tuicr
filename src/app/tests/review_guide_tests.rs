use std::path::PathBuf;

use crate::app::{App, DiffSource, FileTreeItem, InputMode, PullRequestDiffSource};
use crate::forge::traits::{ForgeRepository, PrSessionKey, PullRequestDetails, PullRequestInfo};
use crate::model::{
    DiffFile, DiffHunk, DiffLine, FileStatus, LineOrigin, ReviewSession, SessionDiffSource,
};
use crate::review_guide::{ReviewGuide, ReviewGuideEvent, ReviewStep};
use crate::theme::Theme;
use crate::vcs::traits::VcsType;
use crate::vcs::{PrNoopVcs, VcsInfo};

const HEAD: &str = "abc1234567890";

fn key(head: &str) -> PrSessionKey {
    PrSessionKey::new(ForgeRepository::github("github.com", "o", "r"), 7, head)
}

fn hunk() -> DiffHunk {
    let lines: Vec<DiffLine> = (1..=4)
        .map(|n| DiffLine {
            origin: LineOrigin::Addition,
            content: format!("line {n}"),
            old_lineno: None,
            new_lineno: Some(n),
            raw_content: None,
            highlighted_spans: None,
        })
        .collect();
    DiffHunk {
        header: "@@ -0,0 +1,4 @@".to_string(),
        lines,
        old_start: 0,
        old_count: 0,
        new_start: 1,
        new_count: 4,
    }
}

fn file(path: &str) -> DiffFile {
    DiffFile {
        old_path: None,
        new_path: Some(path.into()),
        status: FileStatus::Added,
        hunks: vec![hunk()],
        is_binary: false,
        is_too_large: false,
        is_commit_message: false,
        content_hash: 0,
    }
}

fn pr_info() -> PullRequestInfo {
    PullRequestInfo::from_details(PullRequestDetails {
        repository: ForgeRepository::github("github.com", "o", "r"),
        number: 7,
        title: "Add users".to_string(),
        url: "https://github.com/o/r/pull/7".to_string(),
        state: "OPEN".to_string(),
        is_draft: false,
        author: None,
        head_ref_name: "users".to_string(),
        base_ref_name: "main".to_string(),
        head_sha: HEAD.to_string(),
        base_sha: "base".to_string(),
        body: "Adds a user service.".to_string(),
        updated_at: None,
        closed: false,
        merged_at: None,
        diff_start_sha: None,
    })
}

/// A PR app whose directory order is
/// api/user.proto, src/user.rs, src/user_service.rs, tests/user_test.rs
/// (directories sorted, forge order kept within a directory).
fn build_app() -> App {
    let pr = PullRequestDiffSource {
        key: key(HEAD),
        base_sha: "base".to_string(),
        title: "Add users".to_string(),
        url: "https://github.com/o/r/pull/7".to_string(),
        head_ref_name: "users".to_string(),
        base_ref_name: "main".to_string(),
        state: "OPEN".to_string(),
        closed: false,
        merged: false,
    };
    let vcs_info = VcsInfo {
        root_path: PathBuf::from("forge:github.com/o/r"),
        head_commit: HEAD.to_string(),
        branch_name: Some("users".to_string()),
        vcs_type: VcsType::File,
    };
    let mut session = ReviewSession::new(
        vcs_info.root_path.clone(),
        HEAD.to_string(),
        Some("users".to_string()),
        SessionDiffSource::PullRequest,
    );
    session.pr_session_key = Some(pr.key.clone());
    let mut app = App::build(
        Box::new(PrNoopVcs::new(vcs_info.clone())),
        vcs_info,
        Theme::dark(),
        None,
        false,
        vec![
            file("tests/user_test.rs"),
            file("src/user.rs"),
            file("api/user.proto"),
            file("src/user_service.rs"),
        ],
        session,
        DiffSource::PullRequest(Box::new(pr)),
        InputMode::Normal,
        Vec::new(),
        None,
        None,
    )
    .expect("build pr app");
    app.pr_info = Some(pr_info());
    app.diff_state.viewport_height = 20;
    app.diff_state.viewport_width = 100;
    app.rebuild_annotations();
    app
}

fn guide() -> ReviewGuide {
    let step = |title: &str, files: &[&str]| ReviewStep {
        title: title.to_string(),
        summary: format!("Read the {title}."),
        files: files.iter().map(PathBuf::from).collect(),
    };
    ReviewGuide {
        overview: "Adds a user service behind a protobuf API.".to_string(),
        steps: vec![
            step("API contract", &["api/user.proto"]),
            step("Service interface", &["src/user_service.rs"]),
            step("Implementation", &["src/user.rs"]),
            step("Tests", &["tests/user_test.rs"]),
        ],
    }
}

fn paths(app: &App) -> Vec<String> {
    app.diff_files
        .iter()
        .map(|f| f.display_path().to_string_lossy().into_owned())
        .collect()
}

/// The file and new-side line number under the cursor.
fn cursor_position(app: &App) -> Option<(PathBuf, Option<u32>)> {
    match app.line_annotations.get(app.diff_state.cursor_line)? {
        crate::app::AnnotatedLine::DiffLine {
            file_idx,
            new_lineno,
            ..
        } => Some((
            app.diff_files[*file_idx].display_path().clone(),
            *new_lineno,
        )),
        _ => None,
    }
}

fn start_request(app: &mut App) -> std::sync::mpsc::Sender<ReviewGuideEvent> {
    let (tx, pending) = crate::review_guide::channel();
    app.review_guide_pending = Some(pending);
    tx
}

fn file_idx_of(app: &App, path: &str) -> usize {
    app.diff_files
        .iter()
        .position(|f| f.display_path() == &PathBuf::from(path))
        .unwrap()
}

fn pr_info_text(app: &App) -> Vec<String> {
    crate::ui::pr_info_panel::pr_info_display_lines(app)
        .iter()
        .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
        .collect()
}

fn deliver(app: &mut App, head: &str, result: Result<ReviewGuide, String>) {
    let tx = start_request(app);
    tx.send(ReviewGuideEvent {
        key: key(head),
        result,
    })
    .unwrap();
    assert!(app.poll_review_guide_events());
}

#[test]
fn should_reorder_files_by_guide_steps_when_guide_lands() {
    let mut app = build_app();
    assert_eq!(
        paths(&app),
        [
            "api/user.proto",
            "src/user.rs",
            "src/user_service.rs",
            "tests/user_test.rs"
        ]
    );

    deliver(&mut app, HEAD, Ok(guide()));

    assert_eq!(
        paths(&app),
        [
            "api/user.proto",
            "src/user_service.rs",
            "src/user.rs",
            "tests/user_test.rs"
        ]
    );
    assert!(app.review_guide_pending.is_none());
    assert!(!app.review_guide_in_flight());
}

#[test]
fn should_keep_directory_subtrees_contiguous_when_the_guide_interleaves_them() {
    let mut app = build_app();
    // Interleaves src/ and tests/.
    let guide = ReviewGuide {
        overview: String::new(),
        steps: vec![ReviewStep {
            title: "All".to_string(),
            summary: String::new(),
            files: [
                "src/user_service.rs",
                "tests/user_test.rs",
                "api/user.proto",
                "src/user.rs",
            ]
            .iter()
            .map(PathBuf::from)
            .collect(),
        }],
    };

    deliver(&mut app, HEAD, Ok(guide));

    // Each directory sorts by its earliest file in the guide.
    assert_eq!(
        paths(&app),
        [
            "src/user_service.rs",
            "src/user.rs",
            "tests/user_test.rs",
            "api/user.proto"
        ]
    );
    let dirs: Vec<String> = app
        .build_visible_items()
        .into_iter()
        .filter_map(|item| match item {
            FileTreeItem::Directory { path, .. } => Some(path),
            FileTreeItem::File { .. } => None,
        })
        .collect();
    assert_eq!(dirs, ["src", "tests", "api"]);
}

#[test]
fn should_show_the_guide_as_its_own_section_after_the_description() {
    let mut app = build_app();
    deliver(&mut app, HEAD, Ok(guide()));

    let rows = pr_info_text(&app);
    let at = |needle: &str| {
        rows.iter()
            .position(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle:?} in {rows:#?}"))
    };
    assert!(at("Adds a user service.") < at("═══ Review guide"));
    assert!(at("═══ Review guide") < at("Adds a user service behind a protobuf API."));
    assert!(at("4. Tests") < at("PR #7 Status"));
    // Titles are styled, not marked up, and each step lists its files.
    assert_eq!(rows[at("1. ")], "1. API contract · Read the API contract.");
    assert_eq!(rows[at("1. ") + 1], "   api/user.proto");
    assert!(rows.iter().all(|row| !row.contains("**")), "{rows:#?}");
    let guide_rows = crate::ui::pr_info_panel::pr_info_guide_rows(&app);
    assert_eq!(guide_rows.end, at("4. Tests") + 2);
    assert_eq!(guide_rows.start + 1, at("═══ Review guide"));

    // The forge's description is left as fetched, and re-sorting or a
    // same-head reload's fresh PR info keeps showing the guide once.
    assert_eq!(
        app.pr_info.as_ref().unwrap().details.body,
        "Adds a user service."
    );
    app.pr_info = Some(pr_info());
    app.sort_files_by_directory(false);
    assert_eq!(pr_info_text(&app), rows);
}

#[test]
fn should_hide_the_guide_with_a_reviewed_description() {
    let mut app = build_app();
    deliver(&mut app, HEAD, Ok(guide()));
    app.session.pr_description_reviewed = true;

    assert_eq!(pr_info_text(&app).len(), 1);
    assert!(crate::ui::pr_info_panel::pr_info_guide_rows(&app).is_empty());
}

#[test]
fn should_keep_cursor_on_the_same_file_line_across_the_reorder() {
    let mut app = build_app();
    let tx = start_request(&mut app);
    let idx = app
        .diff_files
        .iter()
        .position(|f| f.display_path() == &PathBuf::from("src/user_service.rs"))
        .unwrap();
    app.jump_to_file(idx);
    app.diff_state.cursor_line += 2;
    let before = cursor_position(&app);
    assert!(matches!(&before, Some((path, Some(_))) if path.ends_with("user_service.rs")));
    let viewport_row = app.diff_state.cursor_line - app.diff_state.scroll_offset;

    tx.send(ReviewGuideEvent {
        key: key(HEAD),
        result: Ok(guide()),
    })
    .unwrap();
    assert!(app.poll_review_guide_events());

    assert_eq!(
        app.current_file_path(),
        Some(&PathBuf::from("src/user_service.rs"))
    );
    assert_eq!(cursor_position(&app), before);
    assert_eq!(
        app.diff_state.cursor_line - app.diff_state.scroll_offset,
        viewport_row
    );
}

#[test]
fn should_keep_cursor_on_its_overview_row_when_guide_lands() {
    // Above the guide section nothing moves.
    let mut app = build_app();
    app.diff_state.cursor_line = 1;
    deliver(&mut app, HEAD, Ok(guide()));
    assert_eq!(app.diff_state.cursor_line, 1);
    assert!(app.is_cursor_in_overview());

    // Below it, the cursor follows its row down.
    let mut app = build_app();
    let status = pr_info_text(&app)
        .iter()
        .position(|row| row.starts_with("Status:"))
        .unwrap();
    app.diff_state.cursor_line = status;
    deliver(&mut app, HEAD, Ok(guide()));
    assert!(app.diff_state.cursor_line > status);
    assert!(pr_info_text(&app)[app.diff_state.cursor_line].starts_with("Status:"));
}

#[test]
fn should_wait_for_normal_mode_before_reordering() {
    let mut app = build_app();
    let tx = start_request(&mut app);
    tx.send(ReviewGuideEvent {
        key: key(HEAD),
        result: Ok(guide()),
    })
    .unwrap();
    let before = paths(&app);

    // A comment being written holds a file index and a row from this layout.
    app.input_mode = InputMode::Comment;
    assert!(!app.poll_review_guide_events());
    assert_eq!(paths(&app), before);
    assert!(app.review_guide_in_flight());

    app.input_mode = InputMode::Normal;
    assert!(app.poll_review_guide_events());
    assert_ne!(paths(&app), before);
}

#[test]
fn should_keep_collapsed_directories_and_the_tree_selection() {
    let mut app = build_app();
    app.toggle_directory("tests");
    let selected = app
        .build_visible_items()
        .iter()
        .position(|item| matches!(item, FileTreeItem::File { file_idx, .. } if *file_idx == file_idx_of(&app, "src/user.rs")))
        .unwrap();
    app.file_list_state.select(selected);

    deliver(&mut app, HEAD, Ok(guide()));

    assert!(!app.expanded_dirs.contains("tests"));
    assert!(matches!(
        app.get_selected_tree_item(),
        Some(FileTreeItem::File { file_idx, .. }) if file_idx == file_idx_of(&app, "src/user.rs")
    ));
}

#[test]
fn should_carry_line_counts_and_expanded_gaps_to_the_moved_files() {
    let mut app = build_app();
    // Stand-ins for what the forge reported per file.
    let counts = [
        ("api/user.proto", 11),
        ("src/user.rs", 22),
        ("src/user_service.rs", 33),
        ("tests/user_test.rs", 44),
    ];
    app.file_line_count_cache = counts
        .iter()
        .map(|(path, count)| (file_idx_of(&app, path), *count))
        .collect();
    let gap = crate::app::GapId {
        file_idx: file_idx_of(&app, "src/user.rs"),
        hunk_idx: 0,
    };
    app.expanded_top.insert(gap, Vec::new());

    deliver(&mut app, HEAD, Ok(guide()));

    // `src/user.rs` and `src/user_service.rs` swapped places.
    for (path, count) in counts {
        assert_eq!(
            app.file_line_count_cache.get(&file_idx_of(&app, path)),
            Some(&count),
            "{path}"
        );
    }
    let gaps: Vec<usize> = app.expanded_top.keys().map(|gap| gap.file_idx).collect();
    assert_eq!(gaps, [file_idx_of(&app, "src/user.rs")]);
}

#[test]
fn should_ignore_guides_for_another_head() {
    let mut app = build_app();
    let tx = start_request(&mut app);
    tx.send(ReviewGuideEvent {
        key: key("someotherhead"),
        result: Ok(guide()),
    })
    .unwrap();

    assert!(!app.poll_review_guide_events());
    assert!(app.current_review_guide().is_none());
    assert_eq!(paths(&app)[1], "src/user.rs");

    // A fetch retried after the head moved sends the current head's guide
    // on the same channel; it still lands.
    tx.send(ReviewGuideEvent {
        key: key(HEAD),
        result: Ok(guide()),
    })
    .unwrap();
    assert!(app.poll_review_guide_events());
    assert!(app.current_review_guide().is_some());
}

#[test]
fn should_warn_and_keep_directory_order_when_guide_fails() {
    let mut app = build_app();
    deliver(&mut app, HEAD, Err("quota exceeded".to_string()));

    assert!(app.current_review_guide().is_none());
    assert_eq!(paths(&app)[1], "src/user.rs");
    let message = app.message.as_ref().map(|m| m.content.clone());
    assert!(message.is_some_and(|m| m.contains("quota exceeded")));
    assert!(
        pr_info_text(&app)
            .iter()
            .all(|row| !row.contains("Review guide"))
    );
}

#[test]
fn should_keep_annotations_in_sync_with_the_taller_description() {
    let mut app = build_app();
    let before = crate::ui::pr_info_panel::pr_info_render_height(&app);

    deliver(&mut app, HEAD, Ok(guide()));

    let height = crate::ui::pr_info_panel::pr_info_render_height(&app);
    assert!(height > before);
    let pr_info_rows = app
        .line_annotations
        .iter()
        .filter(|line| matches!(line, crate::app::AnnotatedLine::PrInfoLine { .. }))
        .count();
    assert_eq!(pr_info_rows, height);
}

#[test]
fn should_wait_for_an_in_flight_pr_open_before_applying_a_guide() {
    let mut app = build_app();
    let (_open_tx, open_rx) = std::sync::mpsc::channel();
    app.pr_open_rx = Some(open_rx);
    let tx = start_request(&mut app);
    tx.send(ReviewGuideEvent {
        key: key(HEAD),
        result: Ok(guide()),
    })
    .unwrap();

    assert!(!app.poll_review_guide_events());
    assert!(app.review_guide_pending.is_some());

    app.pr_open_rx = None;
    assert!(app.poll_review_guide_events());
    assert!(app.current_review_guide().is_some());
}
