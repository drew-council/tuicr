//! Fixed editing keys for text input: the `:` line, `/` searches, list
//! filters and the comment box. Bindable keys live in [`super::keymap`].

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::input::Action;

/// True when a `KeyCode::Char` event carries the modifier pair Windows uses to
/// report AltGr.
///
/// The Windows console sets `RIGHT_ALT_PRESSED` together with
/// `LEFT_CTRL_PRESSED` while AltGr is held, so crossterm reports
/// `CONTROL | ALT` and hands back the already-composed character. On German,
/// Nordic, Polish and Portuguese layouts AltGr is the only way to reach `@`,
/// `\`, `|`, `[`, `]`, `{`, `}`, `~` and several accented letters, which the
/// `:` command line, the `/` searches and the `i`/`e` regex filters all need.
/// Every Ctrl-only chord is matched ahead of this, so `Ctrl-w` and `Ctrl-u`
/// keep their meaning.
fn is_altgr_text(modifiers: KeyModifiers) -> bool {
    modifiers.contains(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

/// Line editor shared by the `/` search, the file-tree prompts and the PR filter.
pub fn map_prompt_key(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, KeyModifiers::NONE) => Action::ExitMode,
        (KeyCode::Enter, KeyModifiers::NONE) => Action::SubmitInput,
        (KeyCode::Backspace, mods) if mods.contains(KeyModifiers::ALT) => Action::DeleteWord,
        (KeyCode::Backspace, KeyModifiers::NONE) => Action::DeleteChar,
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => Action::DeleteWord,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => Action::ClearLine,
        (KeyCode::Char(c), mods) if is_altgr_text(mods) => Action::InsertChar(c),
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => Action::InsertChar(c),
        _ => Action::None,
    }
}

/// Line editor of the theme picker's `/` filter. It has no word delete.
pub fn map_picker_filter_key(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Esc, KeyModifiers::NONE) => Action::ExitMode,
        (KeyCode::Enter, KeyModifiers::NONE) => Action::SubmitInput,
        (KeyCode::Backspace, KeyModifiers::NONE) => Action::DeleteChar,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => Action::ClearLine,
        (KeyCode::Char(c), mods) if is_altgr_text(mods) => Action::InsertChar(c),
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => Action::InsertChar(c),
        _ => Action::None,
    }
}

/// The file picker's `/` filter: the theme picker's editor plus `↑`/`↓` to move
/// the selection, since letters are text here.
pub fn map_file_picker_filter_key(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Down, KeyModifiers::NONE) => Action::CursorDown(1),
        (KeyCode::Up, KeyModifiers::NONE) => Action::CursorUp(1),
        _ => map_picker_filter_key(key),
    }
}

/// The `:` line: the prompt editor plus Tab completion.
pub fn map_command_key(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        (KeyCode::Tab, KeyModifiers::NONE) => Action::CompleteCommand,
        (KeyCode::BackTab, _) => Action::CompleteCommandReverse,
        _ => map_prompt_key(key),
    }
}

pub fn map_comment_key(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        // Cancel: Esc, Ctrl+C
        (KeyCode::Esc, KeyModifiers::NONE) => Action::ExitMode,
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::ExitMode,
        // Submit: Enter without shift (Ctrl+Enter and Ctrl+S also work)
        (KeyCode::Enter, KeyModifiers::NONE) => Action::SubmitInput,
        (KeyCode::Enter, KeyModifiers::CONTROL) => Action::SubmitInput,
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::SubmitInput,
        // Newline insertion: multiple aliases to survive different terminal/tmux environments.
        // - Shift+Enter: works when Kitty keyboard protocol is active (modern terminals)
        // - Alt+Enter:   works in tmux even with vim-tmux-navigator (ESC CR, not intercepted)
        // - Ctrl+J:      works in terminals without tmux (0x0A → Char('j')+CONTROL)
        // - Ctrl+K:      works without vim-tmux-navigator, intercepted when navigator is active
        // vim-tmux-navigator binds C-j (down) and C-k (up), so Alt-Enter is the reliable tmux key.
        (KeyCode::Enter, mods) if mods.contains(KeyModifiers::SHIFT) => Action::InsertChar('\n'),
        (KeyCode::Enter, mods) if mods.contains(KeyModifiers::ALT) => Action::InsertChar('\n'),
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => Action::InsertChar('\n'),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Action::InsertChar('\n'),
        // Comment type: Tab to cycle (match any modifier so terminals that
        // report Tab with unexpected flags still work; Shift+Tab comes in as
        // BackTab via the second arm).
        // Also match Char('\t'): some terminal multiplexers (e.g. tmux without
        // extended-keys) send Tab as the raw byte 0x09 which crossterm maps to
        // Char('\t') instead of KeyCode::Tab.
        (KeyCode::Tab, _) | (KeyCode::Char('\t'), _) => Action::CycleCommentType,
        (KeyCode::BackTab, _) => Action::CycleCommentTypeReverse,
        // Cursor movement
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => Action::TextCursorLineStart,
        (KeyCode::Char('e'), KeyModifiers::CONTROL) => Action::TextCursorLineEnd,
        (KeyCode::Left, mods)
            if mods.contains(KeyModifiers::ALT) || mods.contains(KeyModifiers::CONTROL) =>
        {
            Action::TextCursorWordLeft
        }
        (KeyCode::Right, mods)
            if mods.contains(KeyModifiers::ALT) || mods.contains(KeyModifiers::CONTROL) =>
        {
            Action::TextCursorWordRight
        }
        (KeyCode::Home, _) => Action::TextCursorLineStart,
        (KeyCode::End, _) => Action::TextCursorLineEnd,
        (KeyCode::Left, mods)
            if mods.contains(KeyModifiers::SUPER) || mods.contains(KeyModifiers::META) =>
        {
            Action::TextCursorLineStart
        }
        (KeyCode::Right, mods)
            if mods.contains(KeyModifiers::SUPER) || mods.contains(KeyModifiers::META) =>
        {
            Action::TextCursorLineEnd
        }
        (KeyCode::Up, KeyModifiers::NONE) => Action::TextCursorUp,
        (KeyCode::Down, KeyModifiers::NONE) => Action::TextCursorDown,
        (KeyCode::Left, KeyModifiers::NONE) => Action::TextCursorLeft,
        (KeyCode::Right, KeyModifiers::NONE) => Action::TextCursorRight,
        // Editing
        (KeyCode::Backspace, mods)
            if mods.contains(KeyModifiers::ALT)
                || mods.contains(KeyModifiers::SUPER)
                || mods.contains(KeyModifiers::META) =>
        {
            Action::DeleteWord
        }
        (KeyCode::Backspace, KeyModifiers::NONE) => Action::DeleteChar,
        (KeyCode::Char('w'), KeyModifiers::CONTROL) => Action::DeleteWord,
        (KeyCode::Char('u'), KeyModifiers::CONTROL) => Action::ClearLine,
        (KeyCode::Char('b'), KeyModifiers::ALT) => Action::TextCursorWordLeft,
        (KeyCode::Char('f'), KeyModifiers::ALT) => Action::TextCursorWordRight,
        (KeyCode::Char(c), _) => Action::InsertChar(c),
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn should_type_altgr_composed_characters_in_every_text_prompt() {
        // Windows reports AltGr as CONTROL | ALT and hands back the composed
        // character, so on German, Nordic, Polish and Portuguese layouts these
        // are the only way to type `@`, `\`, `[`, `|` and the accented
        // letters. Every prompt has to take them as text.
        let altgr =
            |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL | KeyModifiers::ALT);
        for c in ['@', '\\', '[', ']', '{', '}', '|', '~', 'ą'] {
            assert_eq!(map_command_key(altgr(c)), Action::InsertChar(c), "`:` {c}");
            assert_eq!(
                map_prompt_key(altgr(c)),
                Action::InsertChar(c),
                "prompt {c}"
            );
        }
    }

    #[test]
    fn should_keep_ctrl_editing_chords_out_of_the_altgr_arm() {
        // The AltGr arm must not swallow the Ctrl-only editing chords, which
        // arrive without ALT set.
        let ctrl = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert_eq!(map_command_key(ctrl('w')), Action::DeleteWord);
        assert_eq!(map_command_key(ctrl('u')), Action::ClearLine);
        assert_eq!(map_prompt_key(ctrl('w')), Action::DeleteWord);
        assert_eq!(map_prompt_key(ctrl('u')), Action::ClearLine);
    }

    #[test]
    fn should_map_alt_backspace_to_delete_word_in_text_input() {
        let alt_bs = KeyEvent::new(KeyCode::Backspace, KeyModifiers::ALT);
        assert_eq!(map_prompt_key(alt_bs), Action::DeleteWord);
        assert_eq!(map_command_key(alt_bs), Action::DeleteWord);
        assert_eq!(map_comment_key(alt_bs), Action::DeleteWord);
    }

    #[test]
    fn should_type_digits_in_prompts() {
        assert_eq!(
            map_command_key(key(KeyCode::Char('4'))),
            Action::InsertChar('4')
        );
        assert_eq!(
            map_prompt_key(key(KeyCode::Char('4'))),
            Action::InsertChar('4')
        );
    }

    #[test]
    fn should_type_letters_but_move_with_arrows_in_the_file_picker_filter() {
        let map = map_file_picker_filter_key;
        assert_eq!(map(key(KeyCode::Char('j'))), Action::InsertChar('j'));
        assert_eq!(map(key(KeyCode::Char('k'))), Action::InsertChar('k'));
        assert_eq!(map(key(KeyCode::Down)), Action::CursorDown(1));
        assert_eq!(map(key(KeyCode::Up)), Action::CursorUp(1));
        assert_eq!(map(key(KeyCode::Enter)), Action::SubmitInput);
        assert_eq!(map(key(KeyCode::Esc)), Action::ExitMode);
    }

    #[test]
    fn should_complete_commands_with_tab() {
        assert_eq!(map_command_key(key(KeyCode::Tab)), Action::CompleteCommand);
        assert_eq!(
            map_command_key(key(KeyCode::BackTab)),
            Action::CompleteCommandReverse
        );
        assert_eq!(map_prompt_key(key(KeyCode::Tab)), Action::None);
    }

    #[test]
    fn should_submit_and_cancel_prompts() {
        assert_eq!(map_prompt_key(key(KeyCode::Enter)), Action::SubmitInput);
        assert_eq!(map_prompt_key(key(KeyCode::Esc)), Action::ExitMode);
    }

    #[test]
    fn should_cycle_comment_type_with_tab_and_backtab() {
        assert_eq!(map_comment_key(key(KeyCode::Tab)), Action::CycleCommentType);
        assert_eq!(
            map_comment_key(key(KeyCode::Char('\t'))),
            Action::CycleCommentType
        );
        assert_eq!(
            map_comment_key(key(KeyCode::BackTab)),
            Action::CycleCommentTypeReverse
        );
    }

    #[test]
    fn should_insert_newlines_with_every_comment_alias() {
        for event in [
            KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
            KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL),
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        ] {
            assert_eq!(map_comment_key(event), Action::InsertChar('\n'));
        }
        assert_eq!(map_comment_key(key(KeyCode::Enter)), Action::SubmitInput);
    }
}
