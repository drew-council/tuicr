//! Translate crossterm key events into `nvim_input()` key notation.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Key notation for `key`, or `None` for keys Neovim has no name for.
pub fn to_nvim_input(key: KeyEvent) -> Option<String> {
    let mods = key.modifiers;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    let alt = mods.contains(KeyModifiers::ALT);
    let super_ = mods.contains(KeyModifiers::SUPER);
    let shift = mods.contains(KeyModifiers::SHIFT);

    let name = match key.code {
        KeyCode::Char(c) => {
            // Shift is already folded into the character (`A`, `?`), so an
            // unmodified char is sent literally; only `<` needs escaping.
            if !ctrl && !alt && !super_ {
                return Some(if c == '<' {
                    "<lt>".into()
                } else {
                    c.to_string()
                });
            }
            let name = match c {
                '<' => "lt".to_string(),
                ' ' => "Space".to_string(),
                '\\' => "Bslash".to_string(),
                '|' => "Bar".to_string(),
                c if ctrl && shift && c.is_ascii_alphabetic() => {
                    // `<C-a>` and `<C-A>` are the same key to Neovim; the
                    // shifted form needs an explicit `S-`.
                    return Some(notation(
                        &c.to_ascii_lowercase().to_string(),
                        true,
                        alt,
                        true,
                        super_,
                    ));
                }
                c => c.to_string(),
            };
            return Some(notation(&name, ctrl, alt, false, super_));
        }
        KeyCode::Enter => "CR".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Backspace => "BS".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => {
            return Some(notation("Tab", ctrl, alt, true, super_));
        }
        KeyCode::Delete => "Del".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        _ => return None,
    };
    Some(notation(&name, ctrl, alt, shift, super_))
}

fn notation(name: &str, ctrl: bool, alt: bool, shift: bool, super_: bool) -> String {
    let mut prefix = String::new();
    if ctrl {
        prefix.push_str("C-");
    }
    if alt {
        prefix.push_str("M-");
    }
    if shift {
        prefix.push_str("S-");
    }
    if super_ {
        prefix.push_str("D-");
    }
    format!("<{prefix}{name}>")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Option<String> {
        to_nvim_input(KeyEvent::new(code, modifiers))
    }

    #[test]
    fn plain_and_shifted_chars_are_literal() {
        assert_eq!(key(KeyCode::Char('a'), KeyModifiers::NONE).unwrap(), "a");
        assert_eq!(key(KeyCode::Char('A'), KeyModifiers::SHIFT).unwrap(), "A");
        assert_eq!(key(KeyCode::Char('?'), KeyModifiers::SHIFT).unwrap(), "?");
        assert_eq!(key(KeyCode::Char('é'), KeyModifiers::NONE).unwrap(), "é");
        assert_eq!(key(KeyCode::Char(' '), KeyModifiers::NONE).unwrap(), " ");
    }

    #[test]
    fn less_than_is_escaped() {
        assert_eq!(key(KeyCode::Char('<'), KeyModifiers::NONE).unwrap(), "<lt>");
        assert_eq!(
            key(KeyCode::Char('<'), KeyModifiers::CONTROL).unwrap(),
            "<C-lt>"
        );
    }

    #[test]
    fn modified_chars_use_angle_notation() {
        assert_eq!(
            key(KeyCode::Char('r'), KeyModifiers::CONTROL).unwrap(),
            "<C-r>"
        );
        assert_eq!(key(KeyCode::Char('b'), KeyModifiers::ALT).unwrap(), "<M-b>");
        assert_eq!(
            key(
                KeyCode::Char('A'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )
            .unwrap(),
            "<C-S-a>"
        );
        assert_eq!(
            key(KeyCode::Char(' '), KeyModifiers::CONTROL).unwrap(),
            "<C-Space>"
        );
        assert_eq!(
            key(KeyCode::Char('\\'), KeyModifiers::CONTROL).unwrap(),
            "<C-Bslash>"
        );
    }

    #[test]
    fn special_keys_keep_their_modifiers() {
        assert_eq!(key(KeyCode::Enter, KeyModifiers::NONE).unwrap(), "<CR>");
        assert_eq!(key(KeyCode::Esc, KeyModifiers::NONE).unwrap(), "<Esc>");
        assert_eq!(key(KeyCode::Backspace, KeyModifiers::NONE).unwrap(), "<BS>");
        assert_eq!(
            key(KeyCode::BackTab, KeyModifiers::SHIFT).unwrap(),
            "<S-Tab>"
        );
        assert_eq!(
            key(KeyCode::Right, KeyModifiers::CONTROL).unwrap(),
            "<C-Right>"
        );
        assert_eq!(key(KeyCode::Enter, KeyModifiers::SHIFT).unwrap(), "<S-CR>");
        assert_eq!(key(KeyCode::F(5), KeyModifiers::NONE).unwrap(), "<F5>");
    }

    #[test]
    fn unnamed_keys_are_dropped() {
        assert_eq!(key(KeyCode::CapsLock, KeyModifiers::NONE), None);
    }
}
