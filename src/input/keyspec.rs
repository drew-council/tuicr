//! Key chords and the Vim-style notation used to write them (`<C-d>`, `zz`,
//! `<leader>e`).

use std::fmt;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// One key press, normalized so that equivalent terminal reports compare
/// equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    code: KeyCode,
    modifiers: KeyModifiers,
}

const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const ALT: KeyModifiers = KeyModifiers::ALT;
const SHIFT: KeyModifiers = KeyModifiers::SHIFT;

impl KeyChord {
    pub fn char(c: char) -> Self {
        Self::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }.normalized()
    }

    /// The plain character this chord types, if it has no modifiers.
    pub fn plain_char(&self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) if self.modifiers.is_empty() => Some(c),
            _ => None,
        }
    }

    /// Shift is part of a character (`G`, `{`), not a separate modifier,
    /// and Shift-Tab is the distinct `BackTab` key. Ctrl+Alt on a character is
    /// how Windows reports AltGr, so it is the bare composed character.
    fn normalized(mut self) -> Self {
        // The notation has no name for Super, Hyper or Meta.
        self.modifiers &= CTRL | ALT | SHIFT;
        match self.code {
            KeyCode::Char('\t') => self.code = KeyCode::Tab,
            KeyCode::Char(c) => {
                if self.modifiers.contains(CTRL | ALT) {
                    self.modifiers = KeyModifiers::NONE;
                }
                // Shift+j is not `j`; only drop Shift once it is in the character.
                if !c.is_lowercase() {
                    self.modifiers.remove(SHIFT);
                }
            }
            KeyCode::Tab if self.modifiers.contains(SHIFT) => {
                self.code = KeyCode::BackTab;
                self.modifiers.remove(SHIFT);
            }
            KeyCode::BackTab => self.modifiers.remove(SHIFT),
            _ => {}
        }
        self
    }
}

impl From<KeyEvent> for KeyChord {
    fn from(key: KeyEvent) -> Self {
        Self::new(key.code, key.modifiers)
    }
}

/// A key the notation names inside `<…>`.
struct NamedKey {
    code: KeyCode,
    /// Shown in help. Also accepted as a name, in any case.
    label: &'static str,
    /// Other accepted names, lowercase.
    aliases: &'static [&'static str],
}

const fn named(code: KeyCode, label: &'static str, aliases: &'static [&'static str]) -> NamedKey {
    NamedKey {
        code,
        label,
        aliases,
    }
}

const NAMED_KEYS: &[NamedKey] = &[
    named(KeyCode::Esc, "Esc", &[]),
    named(KeyCode::Enter, "Enter", &["cr", "return"]),
    named(KeyCode::Tab, "Tab", &[]),
    named(KeyCode::BackTab, "Shift-Tab", &[]),
    named(KeyCode::Backspace, "Backspace", &["bs"]),
    named(KeyCode::Delete, "Delete", &["del"]),
    named(KeyCode::Insert, "Insert", &[]),
    named(KeyCode::Up, "Up", &[]),
    named(KeyCode::Down, "Down", &[]),
    named(KeyCode::Left, "Left", &[]),
    named(KeyCode::Right, "Right", &[]),
    named(KeyCode::Home, "Home", &[]),
    named(KeyCode::End, "End", &[]),
    named(KeyCode::PageUp, "PageUp", &[]),
    named(KeyCode::PageDown, "PageDown", &[]),
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
    let mut modifiers = KeyModifiers::NONE;
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
        KeyCode::Char(leader)
    } else if lower == "space" {
        KeyCode::Char(' ')
    } else if lower == "lt" {
        KeyCode::Char('<')
    } else if let Some(key) = NAMED_KEYS
        .iter()
        .find(|key| key.label.eq_ignore_ascii_case(name) || key.aliases.contains(&lower.as_str()))
    {
        key.code
    } else if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok())
        && (1..=24).contains(&n)
    {
        KeyCode::F(n)
    } else {
        let mut chars = name.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if modifiers.contains(SHIFT) => {
                modifiers.remove(SHIFT);
                KeyCode::Char(c.to_uppercase().next().unwrap_or(c))
            }
            (Some(c), None) => KeyCode::Char(c),
            _ => return Err(format!("unknown key <{inner}>")),
        }
    };
    Ok(KeyChord::new(code, modifiers))
}

impl fmt::Display for KeyChord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (modifier, prefix) in [(CTRL, "Ctrl-"), (ALT, "Alt-"), (SHIFT, "Shift-")] {
            if self.modifiers.contains(modifier) {
                f.write_str(prefix)?;
            }
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("Space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::F(n) => write!(f, "F{n}"),
            code => match NAMED_KEYS.iter().find(|key| key.code == code) {
                Some(key) => f.write_str(key.label),
                None => write!(f, "{code:?}"),
            },
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
