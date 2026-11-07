//! Key bindings: which key sequence runs which [`Action`] in which context.
//!
//! Each [`KeyContext`] inherits the bindings of its parent, ending at
//! [`KeyContext::Global`]. A binding in a more specific context replaces one
//! for the same sequence in its parent. Text-input states (the `:` line,
//! searches, filters and the comment box) are not contexts; they use the
//! fixed editing keys in [`super::keybindings`].

use std::collections::{BTreeMap, BTreeSet};

use super::Action;
use super::keyspec::{KeyChord, display_sequence, parse_key_sequence};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyContext {
    /// Every mode, including text input.
    Global,
    /// Shared by the four review panes.
    Normal,
    Diff,
    FileTree,
    Comments,
    Commits,
    Visual,
    Summary,
    Help,
    Messages,
    Confirm,
    SubmitResolver,
    SubmitConfirm,
    SubmitPicker,
    Targets,
    ThemePicker,
    FilePicker,
}

impl KeyContext {
    pub const ALL: &[KeyContext] = &[
        KeyContext::Global,
        KeyContext::Normal,
        KeyContext::Diff,
        KeyContext::FileTree,
        KeyContext::Comments,
        KeyContext::Commits,
        KeyContext::Visual,
        KeyContext::Summary,
        KeyContext::Help,
        KeyContext::Messages,
        KeyContext::Confirm,
        KeyContext::SubmitResolver,
        KeyContext::SubmitConfirm,
        KeyContext::SubmitPicker,
        KeyContext::Targets,
        KeyContext::ThemePicker,
        KeyContext::FilePicker,
    ];

    /// The name used in config tables (`[keys.file-tree]`).
    pub fn name(self) -> &'static str {
        match self {
            KeyContext::Global => "global",
            KeyContext::Normal => "normal",
            KeyContext::Diff => "diff",
            KeyContext::FileTree => "file-tree",
            KeyContext::Comments => "comments",
            KeyContext::Commits => "commits",
            KeyContext::Visual => "visual",
            KeyContext::Summary => "summary",
            KeyContext::Help => "help",
            KeyContext::Messages => "messages",
            KeyContext::Confirm => "confirm",
            KeyContext::SubmitResolver => "submit-resolver",
            KeyContext::SubmitConfirm => "submit-confirm",
            KeyContext::SubmitPicker => "submit-picker",
            KeyContext::Targets => "targets",
            KeyContext::ThemePicker => "theme-picker",
            KeyContext::FilePicker => "file-picker",
        }
    }

    pub fn from_name(name: &str) -> Option<KeyContext> {
        Self::ALL.iter().copied().find(|ctx| ctx.name() == name)
    }

    pub fn parent(self) -> Option<KeyContext> {
        match self {
            KeyContext::Global => None,
            KeyContext::Diff
            | KeyContext::FileTree
            | KeyContext::Comments
            | KeyContext::Commits => Some(KeyContext::Normal),
            _ => Some(KeyContext::Global),
        }
    }

    /// Whether a `{N}` count may be typed before a key here.
    pub fn takes_count(self) -> bool {
        self.parent() == Some(KeyContext::Normal)
    }

    /// This context and its ancestors, outermost first.
    fn layers(self) -> Vec<KeyContext> {
        let mut layers = vec![self];
        while let Some(parent) = layers.last().and_then(|ctx| ctx.parent()) {
            layers.push(parent);
        }
        layers.reverse();
        layers
    }
}

/// A binding as written: a context, a key sequence in Vim notation, and the
/// action it runs. `Action::None` removes an inherited or default binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub context: KeyContext,
    pub keys: String,
    pub action: Action,
}

impl Binding {
    pub fn new(context: KeyContext, keys: impl Into<String>, action: Action) -> Self {
        Self {
            context,
            keys: keys.into(),
            action,
        }
    }
}

use Action as A;
use KeyContext as K;

/// The built-in key bindings.
pub const DEFAULT_BINDINGS: &[(KeyContext, &str, Action)] = &[
    (K::Global, "<C-l>", A::Redraw),
    // Review panes
    (K::Normal, "j", A::CursorDown(1)),
    (K::Normal, "<Down>", A::CursorDown(1)),
    (K::Normal, "k", A::CursorUp(1)),
    (K::Normal, "<Up>", A::CursorUp(1)),
    (K::Normal, "<C-e>", A::ScrollViewDown(1)),
    (K::Normal, "<C-y>", A::ScrollViewUp(1)),
    (K::Normal, "<C-d>", A::HalfPageDown),
    (K::Normal, "<C-u>", A::HalfPageUp),
    (K::Normal, "<C-f>", A::PageDown),
    (K::Normal, "<PageDown>", A::PageDown),
    (K::Normal, "<C-b>", A::PageUp),
    (K::Normal, "<PageUp>", A::PageUp),
    (K::Normal, "h", A::ScrollLeft(4)),
    (K::Normal, "<Left>", A::ScrollLeft(4)),
    (K::Normal, "l", A::ScrollRight(4)),
    (K::Normal, "<Right>", A::ScrollRight(4)),
    (K::Normal, "g", A::GoToTop),
    (K::Normal, "G", A::GoToBottom),
    (K::Normal, "}", A::NextFile),
    (K::Normal, "{", A::PrevFile),
    (K::Normal, "]", A::NextHunk),
    (K::Normal, "[", A::PrevHunk),
    (K::Normal, "m", A::NextComment),
    (K::Normal, "M", A::PrevComment),
    (K::Normal, ")", A::CycleCommitNext),
    (K::Normal, "(", A::CycleCommitPrev),
    (K::Normal, "zz", A::CenterCursor),
    (K::Normal, "zt", A::CursorToTop),
    (K::Normal, "zb", A::CursorToBottom),
    (K::Normal, "<Tab>", A::ToggleFocus),
    (K::Normal, "<S-Tab>", A::ToggleFocusReverse),
    (K::Normal, "<CR>", A::Select),
    (K::Normal, "<S-CR>", A::SelectFull),
    (K::Normal, "<Space>", A::ToggleExpand),
    (K::Normal, "o", A::ExpandAll),
    (K::Normal, "O", A::CollapseAll),
    (K::Normal, "r", A::ToggleReviewed),
    (K::Normal, "R", A::ToggleHunkReviewed),
    (K::Normal, "c", A::AddLineComment),
    (K::Normal, "C", A::AddFileComment),
    (K::Normal, "i", A::EditComment),
    (K::Normal, "A", A::EditCommentAtEnd),
    (K::Normal, "dd", A::DeleteComment),
    (K::Normal, "v", A::EnterVisualMode),
    (K::Normal, "V", A::EnterVisualMode),
    (K::Normal, "y", A::ExportToClipboard),
    (K::Normal, "Y", A::CopyCommentAtCursor),
    (K::Normal, "e", A::EditFile),
    (K::Normal, "/", A::EnterSearchMode),
    (K::Normal, "n", A::SearchNext),
    (K::Normal, "N", A::SearchPrev),
    (K::Normal, "<Esc>", A::ClearSearchHighlight),
    (K::Normal, ":", A::EnterCommandMode),
    (K::Normal, "?", A::ToggleHelp),
    (K::Normal, "q", A::QuitHint),
    (K::Normal, "ZZ", A::WriteQuit),
    (K::Normal, "ZQ", A::ForceQuit),
    (K::Normal, "<leader>e", A::ToggleFileList),
    (K::Normal, "<leader>h", A::FocusPaneLeft),
    (K::Normal, "<leader>l", A::FocusPaneRight),
    (K::Normal, "<leader>k", A::FocusPaneUp),
    (K::Normal, "<leader>j", A::FocusPaneDown),
    (K::Normal, "<leader>c", A::AddReviewComment),
    (K::Normal, "<leader>s", A::ToggleCommits),
    (K::Normal, "<leader>f", A::ToggleSingleFileView),
    (K::Normal, "<leader>n", A::ToggleRelativeNumber),
    (K::Normal, "<leader>p", A::FilePicker),
    // The tree claims these; the diff keeps `i` = edit comment, `e` = edit file.
    (K::FileTree, "i", A::FileTreeFilterInclude),
    (K::FileTree, "e", A::FileTreeFilterExclude),
    (K::FileTree, "I", A::FileTreeClearInclude),
    (K::FileTree, "E", A::FileTreeClearExclude),
    (K::Commits, "<Esc>", A::ExitMode),
    // Visual selection
    (K::Visual, "j", A::CursorDown(1)),
    (K::Visual, "<Down>", A::CursorDown(1)),
    (K::Visual, "k", A::CursorUp(1)),
    (K::Visual, "<Up>", A::CursorUp(1)),
    (K::Visual, "c", A::AddRangeComment),
    (K::Visual, "<CR>", A::AddRangeComment),
    (K::Visual, "s", A::AddRangeSuggestion),
    (K::Visual, "y", A::CopySelection),
    (K::Visual, "<Esc>", A::ExitMode),
    (K::Visual, "v", A::ExitMode),
    (K::Visual, "V", A::ExitMode),
    (K::Visual, "q", A::QuitHint),
    // Pending-comment summary
    (K::Summary, "j", A::CursorDown(1)),
    (K::Summary, "<Down>", A::CursorDown(1)),
    (K::Summary, "k", A::CursorUp(1)),
    (K::Summary, "<Up>", A::CursorUp(1)),
    (K::Summary, "<C-d>", A::HalfPageDown),
    (K::Summary, "<C-u>", A::HalfPageUp),
    (K::Summary, "<C-f>", A::PageDown),
    (K::Summary, "<PageDown>", A::PageDown),
    (K::Summary, "<C-b>", A::PageUp),
    (K::Summary, "<PageUp>", A::PageUp),
    (K::Summary, "g", A::GoToTop),
    (K::Summary, "G", A::GoToBottom),
    (K::Summary, "<CR>", A::Select),
    (K::Summary, "dd", A::DeleteComment),
    (K::Summary, "<Esc>", A::ExitMode),
    (K::Summary, "q", A::ExitMode),
    // Help
    (K::Help, "j", A::CursorDown(1)),
    (K::Help, "<Down>", A::CursorDown(1)),
    (K::Help, "k", A::CursorUp(1)),
    (K::Help, "<Up>", A::CursorUp(1)),
    (K::Help, "h", A::ScrollLeft(4)),
    (K::Help, "<Left>", A::ScrollLeft(4)),
    (K::Help, "l", A::ScrollRight(4)),
    (K::Help, "<Right>", A::ScrollRight(4)),
    (K::Help, "<C-d>", A::HalfPageDown),
    (K::Help, "<C-u>", A::HalfPageUp),
    (K::Help, "<C-f>", A::PageDown),
    (K::Help, "<PageDown>", A::PageDown),
    (K::Help, "<C-b>", A::PageUp),
    (K::Help, "<PageUp>", A::PageUp),
    (K::Help, "g", A::GoToTop),
    (K::Help, "G", A::GoToBottom),
    (K::Help, "/", A::EnterSearchMode),
    (K::Help, "n", A::SearchNext),
    (K::Help, "N", A::SearchPrev),
    (K::Help, "?", A::ToggleHelp),
    (K::Help, "q", A::ToggleHelp),
    (K::Help, "<Esc>", A::ToggleHelp),
    // Full error message (`:messages`)
    (K::Messages, "j", A::CursorDown(1)),
    (K::Messages, "<Down>", A::CursorDown(1)),
    (K::Messages, "k", A::CursorUp(1)),
    (K::Messages, "<Up>", A::CursorUp(1)),
    (K::Messages, "h", A::ScrollLeft(4)),
    (K::Messages, "<Left>", A::ScrollLeft(4)),
    (K::Messages, "l", A::ScrollRight(4)),
    (K::Messages, "<Right>", A::ScrollRight(4)),
    (K::Messages, "<C-d>", A::HalfPageDown),
    (K::Messages, "<C-u>", A::HalfPageUp),
    (K::Messages, "<C-f>", A::PageDown),
    (K::Messages, "<PageDown>", A::PageDown),
    (K::Messages, "<C-b>", A::PageUp),
    (K::Messages, "<PageUp>", A::PageUp),
    (K::Messages, "g", A::GoToTop),
    (K::Messages, "G", A::GoToBottom),
    (K::Messages, "?", A::ToggleHelp),
    (K::Messages, "q", A::ToggleHelp),
    (K::Messages, "<Esc>", A::ToggleHelp),
    // Yes/no dialog
    (K::Confirm, "y", A::ConfirmYes),
    (K::Confirm, "Y", A::ConfirmYes),
    (K::Confirm, "<CR>", A::ConfirmYes),
    (K::Confirm, "n", A::ConfirmNo),
    (K::Confirm, "N", A::ConfirmNo),
    (K::Confirm, "<Esc>", A::ConfirmNo),
    // Submit flow
    (K::SubmitResolver, "j", A::CursorDown(1)),
    (K::SubmitResolver, "<Down>", A::CursorDown(1)),
    (K::SubmitResolver, "k", A::CursorUp(1)),
    (K::SubmitResolver, "<Up>", A::CursorUp(1)),
    (K::SubmitResolver, "<CR>", A::Select),
    (K::SubmitResolver, "<Space>", A::ToggleSelect),
    (K::SubmitResolver, "s", A::SubmitResolverAdvance),
    (K::SubmitResolver, "<Esc>", A::ExitMode),
    (K::SubmitConfirm, "y", A::ConfirmYes),
    (K::SubmitConfirm, "Y", A::ConfirmYes),
    (K::SubmitConfirm, "<CR>", A::ConfirmYes),
    (K::SubmitConfirm, "n", A::ConfirmNo),
    (K::SubmitConfirm, "N", A::ConfirmNo),
    (K::SubmitConfirm, "<Esc>", A::ConfirmNo),
    (K::SubmitConfirm, "r", A::SubmitReloadPr),
    (K::SubmitConfirm, "R", A::SubmitReloadPr),
    (K::SubmitPicker, "j", A::CursorDown(1)),
    (K::SubmitPicker, "<Down>", A::CursorDown(1)),
    (K::SubmitPicker, "k", A::CursorUp(1)),
    (K::SubmitPicker, "<Up>", A::CursorUp(1)),
    (K::SubmitPicker, "<CR>", A::Select),
    (K::SubmitPicker, "<Esc>", A::ExitMode),
    (K::SubmitPicker, "q", A::QuitHint),
    // Review target selector
    (K::Targets, "j", A::CursorDown(1)),
    (K::Targets, "<Down>", A::CursorDown(1)),
    (K::Targets, "k", A::CursorUp(1)),
    (K::Targets, "<Up>", A::CursorUp(1)),
    (K::Targets, "<Space>", A::ToggleSelect),
    (K::Targets, "<CR>", A::Select),
    (K::Targets, "<Tab>", A::TargetSelectorTabNext),
    (K::Targets, "<S-Tab>", A::TargetSelectorTabPrev),
    (K::Targets, "/", A::EnterSearchMode),
    (K::Targets, "r", A::TogglePrReviewRequestedFilter),
    (K::Targets, "a", A::SessionsTabToggleShowEmpty),
    (K::Targets, "dd", A::SessionsTabDelete),
    (K::Targets, ":", A::EnterCommandMode),
    (K::Targets, "<Esc>", A::ExitMode),
    (K::Targets, "q", A::QuitHint),
    // Theme picker
    (K::ThemePicker, "j", A::CursorDown(1)),
    (K::ThemePicker, "<Down>", A::CursorDown(1)),
    (K::ThemePicker, "k", A::CursorUp(1)),
    (K::ThemePicker, "<Up>", A::CursorUp(1)),
    (K::ThemePicker, "<CR>", A::Select),
    (K::ThemePicker, "/", A::EnterSearchMode),
    (K::ThemePicker, "<Esc>", A::ExitMode),
    // File picker
    (K::FilePicker, "j", A::CursorDown(1)),
    (K::FilePicker, "<Down>", A::CursorDown(1)),
    (K::FilePicker, "k", A::CursorUp(1)),
    (K::FilePicker, "<Up>", A::CursorUp(1)),
    (K::FilePicker, "<CR>", A::Select),
    (K::FilePicker, "/", A::EnterSearchMode),
    (K::FilePicker, "<Esc>", A::ExitMode),
];

/// Contexts where `q_quits = true` turns the `q` hint back into `:q`.
pub const Q_QUITS_CONTEXTS: &[KeyContext] = &[
    KeyContext::Normal,
    KeyContext::Visual,
    KeyContext::Targets,
    KeyContext::SubmitPicker,
];

/// Outcome of feeding one key into [`Keymap::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A binding completed.
    Run(Action),
    /// The keys so far start a longer binding; wait for the next one.
    Pending,
    /// Nothing is bound to these keys.
    Unbound,
}

/// Effective bindings for every context, after inheritance and overrides.
#[derive(Debug, Clone)]
pub struct Keymap {
    contexts: BTreeMap<KeyContext, BTreeMap<Vec<KeyChord>, Action>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Default,
    User,
}

impl Keymap {
    /// The built-in bindings.
    pub fn with_defaults(leader: char) -> Keymap {
        Self::build(leader, &[]).0
    }

    /// Built-in bindings overlaid with `user` bindings. Returns the keymap
    /// and a warning for each user binding that was rejected or that
    /// displaced another binding.
    pub fn build(leader: char, user: &[Binding]) -> (Keymap, Vec<String>) {
        let mut warnings = BTreeSet::new();
        let mut parsed: Vec<(KeyContext, Vec<KeyChord>, Action, Source, String)> = Vec::new();
        for (context, keys, action) in DEFAULT_BINDINGS {
            let chords = parse_key_sequence(keys, leader).expect("default binding parses");
            parsed.push((
                *context,
                chords,
                action.clone(),
                Source::Default,
                (*keys).into(),
            ));
        }
        for binding in user {
            match parse_key_sequence(&binding.keys, leader) {
                Ok(chords) => parsed.push((
                    binding.context,
                    chords,
                    binding.action.clone(),
                    Source::User,
                    binding.keys.clone(),
                )),
                Err(e) => {
                    warnings.insert(format!(
                        "Warning: keys.{} '{}': {e}; ignoring binding",
                        binding.context.name(),
                        binding.keys
                    ));
                }
            }
        }

        let mut contexts = BTreeMap::new();
        for &leaf in KeyContext::ALL {
            let mut map: BTreeMap<Vec<KeyChord>, (Action, String)> = BTreeMap::new();
            // Leader sequences go last so they win over a single key that
            // shares the leader's character.
            for leader_pass in [false, true] {
                for layer in leaf.layers() {
                    for source in [Source::Default, Source::User] {
                        for (context, chords, action, row_source, keys) in &parsed {
                            let is_leader = keys.to_ascii_lowercase().starts_with("<leader>");
                            if *context != layer
                                || *row_source != source
                                || is_leader != leader_pass
                            {
                                continue;
                            }
                            let displaced = insert(&mut map, chords, action.clone(), keys.clone());
                            if source == Source::User {
                                for (old_chords, old_keys, old_action) in displaced {
                                    let unbound_by_user = parsed.iter().any(|row| {
                                        row.3 == Source::User
                                            && row.2 == Action::None
                                            && row.1 == old_chords
                                            && leaf.layers().contains(&row.0)
                                    });
                                    if unbound_by_user {
                                        continue;
                                    }
                                    warnings.insert(format!(
                                        "Warning: keys.{} '{keys}' replaces '{old_keys}' ({}); bind '{old_keys}' = \"none\" to silence this",
                                        layer.name(),
                                        old_action.name().unwrap_or("none"),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            let map = map.into_iter().map(|(k, (a, _))| (k, a)).collect();
            contexts.insert(leaf, map);
        }
        (Keymap { contexts }, warnings.into_iter().collect())
    }

    /// Feed one key. `pending` holds the keys of an unfinished sequence and
    /// is updated in place. A key that cannot extend `pending` starts over on
    /// its own, so `z` then `j` still moves down.
    pub fn resolve(
        &self,
        context: KeyContext,
        pending: &mut Vec<KeyChord>,
        chord: KeyChord,
    ) -> Resolution {
        pending.push(chord);
        loop {
            match self.lookup(context, pending) {
                Lookup::Action(action) => {
                    pending.clear();
                    return Resolution::Run(action);
                }
                Lookup::Prefix => return Resolution::Pending,
                Lookup::None if pending.len() > 1 => {
                    pending.clear();
                    pending.push(chord);
                }
                Lookup::None => {
                    pending.clear();
                    return Resolution::Unbound;
                }
            }
        }
    }

    /// The action bound to exactly `keys` in `context`.
    pub fn action(&self, context: KeyContext, keys: &[KeyChord]) -> Option<&Action> {
        self.contexts.get(&context)?.get(keys)
    }

    /// Whether `chord` alone is bound or starts a binding in `context`.
    pub fn binds(&self, context: KeyContext, chord: KeyChord) -> bool {
        !matches!(self.lookup(context, &[chord]), Lookup::None)
    }

    /// Every binding in `context`, including inherited ones.
    pub fn bindings(&self, context: KeyContext) -> impl Iterator<Item = (&[KeyChord], &Action)> {
        self.contexts
            .get(&context)
            .into_iter()
            .flatten()
            .map(|(keys, action)| (keys.as_slice(), action))
    }

    /// Display strings of the keys bound to `action` in `context`.
    pub fn keys_for(&self, context: KeyContext, action: &Action) -> Vec<String> {
        self.bindings(context)
            .filter(|(_, bound)| *bound == action)
            .map(|(keys, _)| display_sequence(keys))
            .collect()
    }

    fn lookup(&self, context: KeyContext, keys: &[KeyChord]) -> Lookup {
        let Some(map) = self.contexts.get(&context) else {
            return Lookup::None;
        };
        if let Some(action) = map.get(keys) {
            return Lookup::Action(action.clone());
        }
        match map.range(keys.to_vec()..).next() {
            Some((next, _)) if next.starts_with(keys) => Lookup::Prefix,
            _ => Lookup::None,
        }
    }
}

enum Lookup {
    Action(Action),
    Prefix,
    None,
}

/// Insert a binding, removing any it conflicts with: the same sequence, a
/// shorter sequence it extends, or a longer one it starts. Returns the
/// removed bindings other than an exact replacement.
fn insert(
    map: &mut BTreeMap<Vec<KeyChord>, (Action, String)>,
    chords: &[KeyChord],
    action: Action,
    keys: String,
) -> Vec<(Vec<KeyChord>, String, Action)> {
    let mut displaced = Vec::new();
    map.remove(chords);
    if action == Action::None {
        // Unbinding a prefix also unbinds the sequences it starts.
        map.retain(|existing, _| !existing.starts_with(chords));
        return displaced;
    }
    let conflicts: Vec<Vec<KeyChord>> = map
        .keys()
        .filter(|existing| existing.starts_with(chords) || chords.starts_with(existing))
        .cloned()
        .collect();
    for existing in conflicts {
        if let Some((old_action, old_keys)) = map.remove(&existing) {
            displaced.push((existing, old_keys, old_action));
        }
    }
    map.insert(chords.to_vec(), (action, keys));
    displaced
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DEFAULT_LEADER_KEY;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn chord(code: KeyCode, modifiers: KeyModifiers) -> KeyChord {
        KeyEvent::new(code, modifiers).into()
    }

    fn ch(c: char) -> KeyChord {
        KeyChord::char(c)
    }

    fn run(keymap: &Keymap, context: KeyContext, keys: &str) -> Resolution {
        let mut pending = Vec::new();
        let mut last = Resolution::Unbound;
        for chord in parse_key_sequence(keys, DEFAULT_LEADER_KEY).unwrap() {
            last = keymap.resolve(context, &mut pending, chord);
        }
        last
    }

    fn defaults() -> Keymap {
        Keymap::with_defaults(DEFAULT_LEADER_KEY)
    }

    #[test]
    fn should_map_file_picker_navigation_keys() {
        let keymap = defaults();
        let picker = |keys| run(&keymap, K::FilePicker, keys);
        assert_eq!(picker("j"), Resolution::Run(A::CursorDown(1)));
        assert_eq!(picker("<Down>"), Resolution::Run(A::CursorDown(1)));
        assert_eq!(picker("k"), Resolution::Run(A::CursorUp(1)));
        assert_eq!(picker("<Up>"), Resolution::Run(A::CursorUp(1)));
        assert_eq!(picker("/"), Resolution::Run(A::EnterSearchMode));
        assert_eq!(picker("<CR>"), Resolution::Run(A::Select));
        assert_eq!(picker("<Esc>"), Resolution::Run(A::ExitMode));
        // Other letters are not text until `/` opens the filter draft.
        assert_eq!(picker("x"), Resolution::Unbound);
        assert_eq!(
            run(&keymap, K::Diff, "<leader>p"),
            Resolution::Run(A::FilePicker)
        );
    }

    #[test]
    fn should_build_defaults_without_conflicts() {
        let (_, warnings) = Keymap::build(DEFAULT_LEADER_KEY, &[]);
        assert!(warnings.is_empty(), "{warnings:?}");
        let mut seen = BTreeSet::new();
        for (context, keys, _) in DEFAULT_BINDINGS {
            assert!(
                seen.insert((*context, *keys)),
                "duplicate {keys} in {context:?}"
            );
        }
    }

    #[test]
    fn should_bind_only_registered_actions_by_default() {
        for (_, keys, action) in DEFAULT_BINDINGS {
            assert!(
                action.name().is_some() || *action == Action::QuitHint,
                "{keys} binds unregistered {action:?}"
            );
        }
    }

    #[test]
    fn should_resolve_single_keys() {
        let keymap = defaults();
        assert_eq!(
            run(&keymap, K::Diff, "j"),
            Resolution::Run(A::CursorDown(1))
        );
        assert_eq!(run(&keymap, K::Diff, "G"), Resolution::Run(A::GoToBottom));
        assert_eq!(
            run(&keymap, K::Diff, "<C-e>"),
            Resolution::Run(A::ScrollViewDown(1))
        );
        assert_eq!(run(&keymap, K::Diff, "x"), Resolution::Unbound);
    }

    #[test]
    fn should_wait_for_the_rest_of_a_sequence() {
        let keymap = defaults();
        let mut pending = Vec::new();
        assert_eq!(
            keymap.resolve(K::Diff, &mut pending, ch('z')),
            Resolution::Pending
        );
        assert_eq!(
            keymap.resolve(K::Diff, &mut pending, ch('z')),
            Resolution::Run(A::CenterCursor)
        );
        assert!(pending.is_empty());
    }

    #[test]
    fn should_restart_with_a_key_that_breaks_a_sequence() {
        let keymap = defaults();
        assert_eq!(
            run(&keymap, K::Diff, "zj"),
            Resolution::Run(A::CursorDown(1))
        );
        assert_eq!(run(&keymap, K::Diff, "dZZ"), Resolution::Run(A::WriteQuit));
    }

    #[test]
    fn should_inherit_normal_bindings_in_every_pane() {
        let keymap = defaults();
        for pane in [K::Diff, K::FileTree, K::Comments, K::Commits] {
            assert_eq!(
                run(&keymap, pane, "<leader>e"),
                Resolution::Run(A::ToggleFileList)
            );
            assert_eq!(run(&keymap, pane, "<C-l>"), Resolution::Run(A::Redraw));
        }
    }

    #[test]
    fn should_let_the_file_tree_claim_its_filter_keys() {
        let keymap = defaults();
        assert_eq!(
            run(&keymap, K::FileTree, "i"),
            Resolution::Run(A::FileTreeFilterInclude)
        );
        assert_eq!(
            run(&keymap, K::FileTree, "e"),
            Resolution::Run(A::FileTreeFilterExclude)
        );
        assert_eq!(
            run(&keymap, K::FileTree, "I"),
            Resolution::Run(A::FileTreeClearInclude)
        );
        assert_eq!(
            run(&keymap, K::FileTree, "E"),
            Resolution::Run(A::FileTreeClearExclude)
        );
        assert_eq!(run(&keymap, K::Diff, "i"), Resolution::Run(A::EditComment));
        assert_eq!(run(&keymap, K::Diff, "e"), Resolution::Run(A::EditFile));
    }

    #[test]
    fn should_return_from_the_commit_pane_on_escape() {
        let keymap = defaults();
        assert_eq!(
            run(&keymap, K::Commits, "<Esc>"),
            Resolution::Run(A::ExitMode)
        );
        assert_eq!(
            run(&keymap, K::Diff, "<Esc>"),
            Resolution::Run(A::ClearSearchHighlight)
        );
    }

    #[test]
    fn should_leave_shift_h_unbound_for_vim_motions() {
        // Reviewed-file visibility is command-only (`:set reviewed!`). `H` is a
        // vim motion, and this project deliberately does not spend new
        // single-stroke keys, so nothing may claim it.
        let keymap = defaults();
        assert_eq!(run(&keymap, K::FileTree, "H"), Resolution::Unbound);
        assert_eq!(
            run(&keymap, K::FileTree, "h"),
            Resolution::Run(A::ScrollLeft(4))
        );
    }

    #[test]
    fn should_let_the_leader_win_over_a_key_with_the_same_character() {
        // `leader = "e"` would otherwise be swallowed by the exclude filter.
        let keymap = Keymap::with_defaults('e');
        let mut pending = Vec::new();
        assert_eq!(
            keymap.resolve(K::FileTree, &mut pending, ch('e')),
            Resolution::Pending
        );
        assert_eq!(
            keymap.resolve(
                K::Diff,
                &mut Vec::new(),
                chord(KeyCode::Char('e'), KeyModifiers::CONTROL)
            ),
            Resolution::Run(A::ScrollViewDown(1))
        );
    }

    #[test]
    fn should_leave_semicolon_unbound_when_another_leader_is_configured() {
        let keymap = Keymap::with_defaults(',');
        assert_eq!(run(&keymap, K::Diff, ";"), Resolution::Unbound);
        assert_eq!(
            run(&keymap, K::Diff, ",e"),
            Resolution::Run(A::ToggleFileList)
        );
    }

    #[test]
    fn should_bind_help_search_but_not_message_search() {
        let keymap = defaults();
        assert_eq!(
            run(&keymap, K::Help, "/"),
            Resolution::Run(A::EnterSearchMode)
        );
        assert_eq!(run(&keymap, K::Messages, "/"), Resolution::Unbound);
        assert_eq!(run(&keymap, K::Messages, "n"), Resolution::Unbound);
        assert_eq!(
            run(&keymap, K::Messages, "j"),
            Resolution::Run(A::CursorDown(1))
        );
    }

    #[test]
    fn should_map_backtab_by_either_report() {
        let keymap = defaults();
        for report in [
            chord(KeyCode::BackTab, KeyModifiers::NONE),
            chord(KeyCode::BackTab, KeyModifiers::SHIFT),
            chord(KeyCode::Tab, KeyModifiers::SHIFT),
        ] {
            assert_eq!(
                keymap.resolve(K::Diff, &mut Vec::new(), report),
                Resolution::Run(A::ToggleFocusReverse)
            );
        }
    }

    #[test]
    fn should_delete_sessions_with_dd_in_the_target_selector() {
        let keymap = defaults();
        assert_eq!(
            run(&keymap, K::Targets, "dd"),
            Resolution::Run(A::SessionsTabDelete)
        );
        assert_eq!(
            run(&keymap, K::Summary, "dd"),
            Resolution::Run(A::DeleteComment)
        );
        assert_eq!(
            run(&keymap, K::Diff, "dd"),
            Resolution::Run(A::DeleteComment)
        );
    }

    #[test]
    fn should_map_q_to_the_quit_hint() {
        let keymap = defaults();
        for context in Q_QUITS_CONTEXTS {
            let context = if *context == K::Normal {
                K::Diff
            } else {
                *context
            };
            assert_eq!(run(&keymap, context, "q"), Resolution::Run(A::QuitHint));
        }
        assert_eq!(run(&keymap, K::Help, "q"), Resolution::Run(A::ToggleHelp));
        assert_eq!(run(&keymap, K::Summary, "q"), Resolution::Run(A::ExitMode));
    }

    #[test]
    fn should_restore_q_as_quit_with_overrides() {
        let bindings: Vec<Binding> = Q_QUITS_CONTEXTS
            .iter()
            .map(|context| Binding::new(*context, "q", A::Quit))
            .collect();
        let (keymap, warnings) = Keymap::build(DEFAULT_LEADER_KEY, &bindings);
        assert!(warnings.is_empty(), "{warnings:?}");
        for context in [K::Diff, K::FileTree, K::Visual, K::Targets, K::SubmitPicker] {
            assert_eq!(run(&keymap, context, "q"), Resolution::Run(A::Quit));
        }
        assert_eq!(run(&keymap, K::Help, "q"), Resolution::Run(A::ToggleHelp));
    }

    #[test]
    fn should_let_user_bindings_override_defaults() {
        let (keymap, warnings) = Keymap::build(
            DEFAULT_LEADER_KEY,
            &[
                Binding::new(K::Normal, "r", A::ToggleHunkReviewed),
                Binding::new(K::Normal, "R", A::ToggleReviewed),
                Binding::new(K::Normal, "<C-s>", A::SubmitPicker),
            ],
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(
            run(&keymap, K::Diff, "r"),
            Resolution::Run(A::ToggleHunkReviewed)
        );
        assert_eq!(
            run(&keymap, K::Diff, "R"),
            Resolution::Run(A::ToggleReviewed)
        );
        assert_eq!(
            run(&keymap, K::FileTree, "<C-s>"),
            Resolution::Run(A::SubmitPicker)
        );
    }

    #[test]
    fn should_unbind_with_none() {
        let (keymap, _) =
            Keymap::build(DEFAULT_LEADER_KEY, &[Binding::new(K::Normal, "Y", A::None)]);
        assert_eq!(run(&keymap, K::Diff, "Y"), Resolution::Unbound);
    }

    #[test]
    fn should_warn_when_a_user_binding_displaces_a_sequence() {
        let (keymap, warnings) = Keymap::build(
            DEFAULT_LEADER_KEY,
            &[Binding::new(K::Normal, "d", A::HalfPageDown)],
        );
        assert_eq!(run(&keymap, K::Diff, "d"), Resolution::Run(A::HalfPageDown));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].contains("'dd' (delete-comment)"),
            "{warnings:?}"
        );

        for bindings in [
            [
                Binding::new(K::Normal, "dd", A::None),
                Binding::new(K::Normal, "d", A::HalfPageDown),
            ],
            [
                Binding::new(K::Normal, "d", A::HalfPageDown),
                Binding::new(K::Normal, "dd", A::None),
            ],
        ] {
            let (keymap, warnings) = Keymap::build(DEFAULT_LEADER_KEY, &bindings);
            assert!(warnings.is_empty(), "{warnings:?}");
            assert_eq!(run(&keymap, K::Diff, "d"), Resolution::Run(A::HalfPageDown));
        }
    }

    #[test]
    fn should_prefer_the_more_specific_context() {
        let (keymap, _) = Keymap::build(
            DEFAULT_LEADER_KEY,
            &[Binding::new(K::Normal, "e", A::ToggleWrap)],
        );
        assert_eq!(run(&keymap, K::Diff, "e"), Resolution::Run(A::ToggleWrap));
        assert_eq!(
            run(&keymap, K::FileTree, "e"),
            Resolution::Run(A::FileTreeFilterExclude)
        );
    }

    #[test]
    fn should_warn_about_unparsable_keys() {
        let (_, warnings) = Keymap::build(
            DEFAULT_LEADER_KEY,
            &[Binding::new(K::Normal, "<Nope>", A::Quit)],
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown key <Nope>"), "{warnings:?}");
    }

    #[test]
    fn should_list_keys_bound_to_an_action() {
        let keymap = defaults();
        assert_eq!(
            keymap.keys_for(K::Diff, &A::CursorDown(1)),
            vec!["j".to_string(), "Down".to_string()]
        );
    }
}
