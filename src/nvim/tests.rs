use super::*;

#[test]
fn mode_from_raw_covers_neovim_mode_strings() {
    for (raw, mode) in [
        ("n", NvimMode::Normal),
        ("no", NvimMode::OperatorPending),
        ("nov", NvimMode::OperatorPending),
        ("niI", NvimMode::Insert),
        ("niR", NvimMode::Replace),
        ("i", NvimMode::Insert),
        ("ic", NvimMode::Insert),
        ("R", NvimMode::Replace),
        ("v", NvimMode::Visual),
        ("V", NvimMode::VisualLine),
        ("\u{16}", NvimMode::VisualBlock),
        ("s", NvimMode::Select),
        ("c", NvimMode::Command),
        ("r?", NvimMode::Prompt),
        ("t", NvimMode::Terminal),
    ] {
        assert_eq!(NvimMode::from_raw(raw), mode, "{raw:?}");
    }
}

#[test]
fn byte_cursor_round_trips_through_line_col() {
    let text = "héllo\n\n世界 ok";
    for cursor in (0..=text.len()).filter(|&i| text.is_char_boundary(i)) {
        let (lines, row, col) = byte_to_line_col(text, cursor);
        assert_eq!(
            line_col_to_byte(&lines, row, col),
            cursor,
            "cursor={cursor}"
        );
    }
}

#[test]
fn line_col_to_byte_clamps_out_of_range_positions() {
    let lines = ["ab", "cd"];
    assert_eq!(line_col_to_byte(&lines, 0, 99), 2);
    assert_eq!(line_col_to_byte(&lines, 99, 0), 3);
    // Mid-character columns snap back to the character start.
    assert_eq!(line_col_to_byte(&["é"], 0, 1), 0);
}

#[test]
fn selection_ranges_clamp_to_lines_and_char_boundaries() {
    let lines = ["héllo", "", "xyz"];
    // Inclusive-end columns past the line (eol) clamp; empty lines vanish;
    // a span ending mid-character widens to the character end.
    let ranges = selection_ranges(&lines, &[(0, 0, 9), (1, 0, 1), (2, 1, 2), (0, 1, 2)]);
    assert_eq!(ranges, vec![0..6, 9..10, 1..3]);
}

#[test]
fn state_parses_get_state_reply() {
    let value = Value::Map(vec![
        ("lines".into(), Value::Array(vec!["ab".into(), "cd".into()])),
        ("row".into(), 1.into()),
        ("col".into(), 1.into()),
        ("mode".into(), "v".into()),
        ("plain_normal".into(), false.into()),
        ("cmdline".into(), Value::Nil),
        (
            "selection".into(),
            Value::Array(vec![Value::Array(vec![0.into(), 1.into(), 2.into()])]),
        ),
        ("recording".into(), "q".into()),
        ("active".into(), true.into()),
    ]);
    let state = NvimState::from_value(&value).unwrap();
    assert_eq!(state.text, "ab\ncd");
    assert_eq!(state.cursor, 4);
    assert_eq!(state.mode, NvimMode::Visual);
    assert_eq!(state.selection, vec![1..2]);
    assert_eq!(state.recording, Some('q'));
    assert_eq!(state.cmdline, None);
}

// Integration tests against a real `nvim --clean`; skipped without one.

fn host() -> Option<NvimHost> {
    host_with_tab_width(4)
}

fn host_with_tab_width(tab_width: usize) -> Option<NvimHost> {
    let options = NvimOptions {
        args: vec!["--clean".to_string()],
        tab_width,
        ..NvimOptions::default()
    };
    match NvimHost::spawn(&options) {
        Ok(host) => Some(host),
        Err(NvimError::NotFound(_)) => {
            eprintln!("skipping: nvim is not on PATH");
            None
        }
        Err(err) => panic!("{err}"),
    }
}

fn begun(text: &str, cursor: usize) -> Option<NvimHost> {
    let mut host = host()?;
    host.begin(text, cursor).expect("begin");
    settle(&mut host);
    host.take_events();
    Some(host)
}

/// Wait for a state that lands after the call returned. Production code picks
/// it up on the next `poll`; a loaded test run can outlast the sync timeout.
fn settle(host: &mut NvimHost) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.is_syncing() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
        host.poll();
    }
}

fn send(host: &mut NvimHost, keys: &str) {
    host.send_keys(keys);
    settle(host);
}

/// Poll until `done` holds for the collected events, or panic after 2s.
fn wait_for_events(host: &mut NvimHost, done: impl Fn(&[NvimEvent]) -> bool) -> Vec<NvimEvent> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut events = Vec::new();
    loop {
        host.poll();
        events.extend(host.take_events());
        if done(&events) {
            return events;
        }
        assert!(Instant::now() < deadline, "timed out; got {events:?}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn nvim_edits_with_real_vim_commands() {
    let Some(mut host) = begun("", 0) else { return };
    assert_eq!(host.state().mode, NvimMode::Insert);

    send(&mut host, "hello world");
    assert_eq!(host.state().text, "hello world");
    assert_eq!(host.state().cursor, 11);

    send(&mut host, "<Esc>");
    assert_eq!(host.state().mode, NvimMode::Normal);
    assert!(host.state().plain_normal);

    send(&mut host, "0dw");
    assert_eq!(host.state().text, "world");
    send(&mut host, "3x");
    assert_eq!(host.state().text, "ld");

    send(&mut host, "ccfoo(bar, baz)<Esc>0f(ci(x<Esc>");
    assert_eq!(host.state().text, "foo(x)");
    send(&mut host, ":s/o/0/g<CR>");
    assert_eq!(host.state().text, "f00(x)");
    send(&mut host, "u");
    assert_eq!(host.state().text, "foo(x)");
}

#[test]
fn nvim_begin_places_insert_cursor() {
    for (text, cursor, typed, expected) in [
        ("abc", 3, "!", "abc!"),
        ("abc", 1, "X", "aXbc"),
        ("abc", 0, "X", "Xabc"),
        ("ab\ncd", 3, "X", "ab\nXcd"),
        ("ab\ncd", 2, "X", "abX\ncd"),
        ("héllo", 3, "X", "héXllo"),
        ("hé", 3, "X", "héX"),
    ] {
        let Some(mut host) = begun(text, cursor) else {
            return;
        };
        send(&mut host, typed);
        assert_eq!(host.state().text, expected, "{text:?}@{cursor}");
    }
}

#[test]
fn nvim_operator_pending_is_not_plain_normal() {
    let Some(mut host) = begun("abc", 0) else {
        return;
    };
    send(&mut host, "<Esc>d");
    assert_eq!(host.state().mode, NvimMode::OperatorPending);
    assert!(!host.state().plain_normal);
    send(&mut host, "<Esc>");
    assert!(host.state().plain_normal);
}

#[test]
fn nvim_undo_does_not_reach_the_previous_comment() {
    let Some(mut host) = begun("first", 5) else {
        return;
    };
    send(&mut host, " edit<Esc>");
    host.begin("second", 6).unwrap();
    settle(&mut host);
    send(&mut host, "<Esc>uu");
    assert_eq!(host.state().text, "second");
}

#[test]
fn nvim_reports_command_line_and_recording() {
    let Some(mut host) = begun("a\nb", 0) else {
        return;
    };
    send(&mut host, "<Esc>:s/a");
    assert_eq!(host.state().mode, NvimMode::Command);
    assert_eq!(host.state().cmdline.as_deref(), Some(":s/a"));
    send(&mut host, "<Esc>qa");
    assert_eq!(host.state().recording, Some('a'));
    send(&mut host, "A!<Esc>jq");
    assert_eq!(host.state().recording, None);
    send(&mut host, "@a");
    assert_eq!(host.state().text, "a!\nb!");
}

#[test]
fn nvim_reports_visual_selection_as_byte_ranges() {
    let Some(mut host) = begun("ab😀cd\nxyz", 0) else {
        return;
    };
    send(&mut host, "<Esc>gg0v3l");
    assert_eq!(host.state().mode, NvimMode::Visual);
    assert_eq!(host.state().selection, vec![0..7]);
    send(&mut host, "<Esc>V");
    assert_eq!(host.state().selection, vec![0..8]);
    send(&mut host, "<Esc>0<C-v>j");
    assert_eq!(host.state().mode, NvimMode::VisualBlock);
    assert_eq!(host.state().selection, vec![0..1, 9..10]);
    send(&mut host, "<Esc>");
    assert!(host.state().selection.is_empty());
}

#[test]
fn nvim_paste_lands_at_insert_cursor() {
    let Some(mut host) = begun("abc", 1) else {
        return;
    };
    host.paste("XY");
    settle(&mut host);
    assert_eq!(host.state().text, "aXYbc");
    assert_eq!(host.state().cursor, 3);
    host.paste("1\r\n2");
    settle(&mut host);
    assert_eq!(host.state().text, "aXY1\n2bc");
}

#[test]
fn nvim_write_saves_and_quit_exits() {
    let Some(mut host) = begun("", 0) else { return };
    send(&mut host, "draft<Esc>:w<CR>");
    let events = wait_for_events(&mut host, |events| {
        events.iter().any(|e| matches!(e, NvimEvent::Save(_)))
    });
    assert!(events.contains(&NvimEvent::Save("draft".to_string())));

    // Edits after the write must not trip E37 on `:q`.
    send(&mut host, "x:q<CR>");
    let events = wait_for_events(&mut host, |events| {
        events.iter().any(|e| matches!(e, NvimEvent::Exited { .. }))
    });
    assert!(events.contains(&NvimEvent::Quit));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, NvimEvent::Exited { expected: true, .. }))
    );
    assert!(!host.is_alive());
}

#[test]
fn nvim_forwards_messages() {
    let Some(mut host) = begun("", 0) else { return };
    send(&mut host, "<Esc>:echo 'hi'<CR>");
    let events = wait_for_events(&mut host, |events| {
        events
            .iter()
            .any(|e| matches!(e, NvimEvent::Message { .. }))
    });
    assert!(events.contains(&NvimEvent::Message {
        kind: "echo".to_string(),
        text: "hi".to_string()
    }));
}

#[test]
fn nvim_insert_tab_uses_tab_width_spaces() {
    let Some(mut host) = host_with_tab_width(2) else {
        return;
    };
    host.begin("", 0).unwrap();
    settle(&mut host);
    send(&mut host, "<Tab>x");
    assert_eq!(host.state().text, "  x");
}
