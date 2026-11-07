//! Key chords and the Vim-style notation used to write them (`<C-d>`, `zz`,
//! `<leader>e`).

use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// One key press, normalized so that equivalent terminal reports compare
/// equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct KeyChord {
    code: KeyCodeKey,
    modifiers: u8,
}

/// `KeyCode` subset that is `Ord`, so chord sequences can key a `BTreeMap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum KeyCodeKey {
    Char(char),
    F(u8),
    Named(NamedKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum NamedKey {
    Esc,
    Enter,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

const CTRL: u8 = 1;
const ALT: u8 = 2;
const SHIFT: u8 = 4;

impl KeyChord {
    pub fn char(c: char) -> Self {
        Self::new(KeyCodeKey::Char(c), 0)
    }

    fn new(code: KeyCodeKey, modifiers: u8) -> Self {
        Self { code, modifiers }.normalized()
    }

    /// The plain character this chord types, if it has no modifiers.
    pub fn plain_char(&self) -> Option<char> {
        match self.code {
            KeyCodeKey::Char(c) if self.modifiers == 0 => Some(c),
            _ => None,
        }
    }

    /// Shift is part of a character (`G`, `{`), not a separate modifier,
    /// and Shift-Tab is the distinct `BackTab` key. Ctrl+Alt on a character is
    /// how Windows reports AltGr, so it is the bare composed character.
    fn normalized(mut self) -> Self {
        match self.code {
            KeyCodeKey::Char('\t') => {
                self.code = KeyCodeKey::Named(NamedKey::Tab);
            }
            KeyCodeKey::Char(c) => {
                if self.modifiers & (CTRL | ALT) == CTRL | ALT {
                    self.modifiers = 0;
                }
                // Shift+j is not `j`; only drop Shift once it is in the character.
                if !c.is_lowercase() {
                    self.modifiers &= !SHIFT;
                }
            }
            KeyCodeKey::Named(NamedKey::Tab) if self.modifiers & SHIFT != 0 => {
                self.code = KeyCodeKey::Named(NamedKey::BackTab);
                self.modifiers &= !SHIFT;
            }
            KeyCodeKey::Named(NamedKey::BackTab) => self.modifiers &= !SHIFT,
            _ => {}
        }
        self
    }
}

impl From<KeyEvent> for KeyChord {
    fn from(key: KeyEvent) -> Self {
        let code = match key.code {
            KeyCode::Char(c) => KeyCodeKey::Char(c),
            KeyCode::F(n) => KeyCodeKey::F(n),
            KeyCode::Esc => KeyCodeKey::Named(NamedKey::Esc),
            KeyCode::Enter => KeyCodeKey::Named(NamedKey::Enter),
            KeyCode::Tab => KeyCodeKey::Named(NamedKey::Tab),
            KeyCode::BackTab => KeyCodeKey::Named(NamedKey::BackTab),
            KeyCode::Backspace => KeyCodeKey::Named(NamedKey::Backspace),
            KeyCode::Delete => KeyCodeKey::Named(NamedKey::Delete),
            KeyCode::Insert => KeyCodeKey::Named(NamedKey::Insert),
            KeyCode::Up => KeyCodeKey::Named(NamedKey::Up),
            KeyCode::Down => KeyCodeKey::Named(NamedKey::Down),
            KeyCode::Left => KeyCodeKey::Named(NamedKey::Left),
            KeyCode::Right => KeyCodeKey::Named(NamedKey::Right),
            KeyCode::Home => KeyCodeKey::Named(NamedKey::Home),
            KeyCode::End => KeyCodeKey::Named(NamedKey::End),
            KeyCode::PageUp => KeyCodeKey::Named(NamedKey::PageUp),
            KeyCode::PageDown => KeyCodeKey::Named(NamedKey::PageDown),
            // Keys the notation cannot name match nothing.
            _ => KeyCodeKey::F(0),
        };
        let mut modifiers = 0;
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            modifiers |= CTRL;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            modifiers |= ALT;
        }
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            modifiers |= SHIFT;
        }
        Self::new(code, modifiers)
    }
}

const NAMED_KEYS: &[(&str, NamedKey)] = &[
    ("esc", NamedKey::Esc),
    ("cr", NamedKey::Enter),
    ("enter", NamedKey::Enter),
    ("return", NamedKey::Enter),
    ("tab", NamedKey::Tab),
    ("bs", NamedKey::Backspace),
    ("backspace", NamedKey::Backspace),
    ("del", NamedKey::Delete),
    ("delete", NamedKey::Delete),
    ("insert", NamedKey::Insert),
    ("up", NamedKey::Up),
    ("down", NamedKey::Down),
    ("left", NamedKey::Left),
    ("right", NamedKey::Right),
    ("home", NamedKey::Home),
    ("end", NamedKey::End),
    ("pageup", NamedKey::PageUp),
    ("pagedown", NamedKey::PageDown),
];

/// Parse a key sequence such as `zz`, `<C-d>`, `<S-Tab>` or `<leader>e`.
///
/// Inside `<…>`, `C-`, `A-` (or `M-`) and `S-` prefix a key name or a single
/// character. `<lt>` is a literal `<` and `<Space>` a space.
pub fn parse_key_sequence(spec: &str, leader: char) -> Result<Vec<KeyChord>, String> {
    let mut chords = Vec::new();
    let mut rest = spec;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && let Some(end) = rest.find('>')
            && end > 1
        {
            chords.push(parse_bracketed(&rest[1..end], leader)?);
            rest = &rest[end + 1..];
        } else {
            chords.push(KeyChord::char(c));
            rest = &rest[c.len_utf8()..];
        }
    }
    if chords.is_empty() {
        return Err("empty key sequence".into());
    }
    Ok(chords)
}

fn parse_bracketed(inner: &str, leader: char) -> Result<KeyChord, String> {
    let mut modifiers = 0;
    let mut name = inner;
    loop {
        let lower = name.to_ascii_lowercase();
        let bit = match lower.get(..2) {
            Some("c-") => CTRL,
            Some("a-") | Some("m-") => ALT,
            Some("s-") => SHIFT,
            _ => break,
        };
        if name.len() == 2 {
            break;
        }
        modifiers |= bit;
        name = &name[2..];
    }
    let lower = name.to_ascii_lowercase();
    let code = if lower == "leader" {
        KeyCodeKey::Char(leader)
    } else if lower == "space" {
        KeyCodeKey::Char(' ')
    } else if lower == "lt" {
        KeyCodeKey::Char('<')
    } else if let Some((_, key)) = NAMED_KEYS.iter().find(|(n, _)| *n == lower) {
        KeyCodeKey::Named(*key)
    } else if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        KeyCodeKey::F(n)
    } else {
        let mut chars = name.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if modifiers & SHIFT != 0 => {
                modifiers &= !SHIFT;
                KeyCodeKey::Char(c.to_uppercase().next().unwrap_or(c))
            }
            (Some(c), None) => KeyCodeKey::Char(c),
            _ => return Err(format!("unknown key <{inner}>")),
        }
    };
    Ok(KeyChord::new(code, modifiers))
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers & CTRL != 0 {
            f.write_str("Ctrl-")?;
        }
        if self.modifiers & ALT != 0 {
            f.write_str("Alt-")?;
        }
        if self.modifiers & SHIFT != 0 {
            f.write_str("Shift-")?;
        }
        match self.code {
            KeyCodeKey::Char(' ') => f.write_str("Space"),
            KeyCodeKey::Char(c) => write!(f, "{c}"),
            KeyCodeKey::F(n) => write!(f, "F{n}"),
            KeyCodeKey::Named(key) => f.write_str(match key {
                NamedKey::Esc => "Esc",
                NamedKey::Enter => "Enter",
                NamedKey::Tab => "Tab",
                NamedKey::BackTab => "Shift-Tab",
                NamedKey::Backspace => "Backspace",
                NamedKey::Delete => "Delete",
                NamedKey::Insert => "Insert",
                NamedKey::Up => "Up",
                NamedKey::Down => "Down",
                NamedKey::Left => "Left",
                NamedKey::Right => "Right",
                NamedKey::Home => "Home",
                NamedKey::End => "End",
                NamedKey::PageUp => "PageUp",
                NamedKey::PageDown => "PageDown",
            }),
        }
    }
}

/// Render a sequence for the help popup and hints: `zz`, `;e`, `Ctrl-d`.
pub fn display_sequence(chords: &[KeyChord]) -> String {
    let parts: Vec<String> = chords.iter().map(ToString::to_string).collect();
    if parts.iter().all(|part| part.chars().count() == 1) {
        parts.concat()
    } else {
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(code: KeyCode, modifiers: KeyModifiers) -> KeyChord {
        KeyEvent::new(code, modifiers).into()
    }

    fn parse(spec: &str) -> Vec<KeyChord> {
        parse_key_sequence(spec, ';').expect(spec)
    }

    #[test]
    fn should_parse_plain_characters_as_a_sequence() {
        assert_eq!(parse("zz"), vec![KeyChord::char('z'), KeyChord::char('z')]);
        assert_eq!(
            parse("G"),
            vec![event(KeyCode::Char('G'), KeyModifiers::SHIFT)]
        );
    }

    #[test]
    fn should_parse_bracketed_keys_and_modifiers() {
        assert_eq!(
            parse("<C-d>"),
            vec![event(KeyCode::Char('d'), KeyModifiers::CONTROL)]
        );
        assert_eq!(
            parse("<A-BS>"),
            vec![event(KeyCode::Backspace, KeyModifiers::ALT)]
        );
        assert_eq!(
            parse("<S-CR>"),
            vec![event(KeyCode::Enter, KeyModifiers::SHIFT)]
        );
        assert_eq!(
            parse("<PageDown>"),
            vec![event(KeyCode::PageDown, KeyModifiers::NONE)]
        );
        assert_eq!(
            parse("<F5>"),
            vec![event(KeyCode::F(5), KeyModifiers::NONE)]
        );
    }

    #[test]
    fn should_expand_the_leader() {
        assert_eq!(
            parse_key_sequence("<leader>e", ',').unwrap(),
            vec![KeyChord::char(','), KeyChord::char('e')]
        );
    }

    #[test]
    fn should_treat_shift_tab_and_backtab_alike() {
        let backtab = event(KeyCode::BackTab, KeyModifiers::SHIFT);
        assert_eq!(parse("<S-Tab>"), vec![backtab]);
        assert_eq!(event(KeyCode::Tab, KeyModifiers::SHIFT), backtab);
    }

    #[test]
    fn should_treat_a_raw_tab_byte_as_tab() {
        assert_eq!(
            event(KeyCode::Char('\t'), KeyModifiers::NONE),
            event(KeyCode::Tab, KeyModifiers::NONE)
        );
    }

    #[test]
    fn should_ignore_shift_on_characters() {
        assert_eq!(
            event(KeyCode::Char('}'), KeyModifiers::SHIFT),
            KeyChord::char('}')
        );
    }

    #[test]
    fn should_read_altgr_characters_as_plain_characters() {
        assert_eq!(
            event(
                KeyCode::Char('{'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            ),
            KeyChord::char('{')
        );
    }

    #[test]
    fn should_parse_shifted_letters_as_capitals() {
        assert_eq!(parse("<S-a>"), parse("A"));
    }

    #[test]
    fn should_parse_literal_angle_brackets() {
        assert_eq!(parse("<lt>"), vec![KeyChord::char('<')]);
        assert_eq!(parse("<"), vec![KeyChord::char('<')]);
        assert_eq!(parse("<>"), vec![KeyChord::char('<'), KeyChord::char('>')]);
    }

    #[test]
    fn should_reject_unknown_key_names() {
        assert!(parse_key_sequence("<Nope>", ';').is_err());
        assert!(parse_key_sequence("", ';').is_err());
    }

    #[test]
    fn should_display_sequences_compactly() {
        assert_eq!(display_sequence(&parse("<leader>e")), ";e");
        assert_eq!(display_sequence(&parse("<C-d>")), "Ctrl-d");
        assert_eq!(display_sequence(&parse("<S-Tab>")), "Shift-Tab");
        assert_eq!(display_sequence(&parse("<Space>")), "Space");
        assert_eq!(display_sequence(&parse("g<CR>")), "g Enter");
    }
}
