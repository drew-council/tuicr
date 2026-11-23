//! The keymap: which key sequence runs which [`Action`] in which context.
//!
//! Each [`KeyContext`] inherits the bindings of its parent, ending at
//! [`KeyContext::Global`]. A binding in a more specific context replaces one
//! for the same sequence in its parent. Text-input states (the `:` line,
//! searches, filters and the comment box) are not contexts; they use the
//! fixed editing keys in [`super::text_input`].

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

/// A binding as written, built in or from `[keys]`: a context, a key
/// sequence in Vim notation, and the action it runs. `Action::None` removes
/// an inherited or default binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub context: KeyContext,
    pub keys: Cow<'static, str>,
    pub action: Action,
}

impl Binding {
    pub const fn new(context: KeyContext, keys: &'static str, action: Action) -> Self {
        Self {
            context,
            keys: Cow::Borrowed(keys),
            action,
        }
    }
}

use Action as A;
use Binding as B;
use KeyContext as K;

/// The built-in key bindings.
pub const DEFAULT_BINDINGS: &[Binding] = &[
    B::new(K::Global, "<C-l>", A::Redraw),
    // Review panes
    B::new(K::Normal, "j", A::CursorDown(1)),
    B::new(K::Normal, "<Down>", A::CursorDown(1)),
    B::new(K::Normal, "k", A::CursorUp(1)),
    B::new(K::Normal, "<Up>", A::CursorUp(1)),
    B::new(K::Normal, "<C-e>", A::ScrollViewDown(1)),
    B::new(K::Normal, "<C-y>", A::ScrollViewUp(1)),
    B::new(K::Normal, "<C-d>", A::HalfPageDown),
    B::new(K::Normal, "<C-u>", A::HalfPageUp),
    B::new(K::Normal, "<C-f>", A::PageDown),
    B::new(K::Normal, "<PageDown>", A::PageDown),
    B::new(K::Normal, "<C-b>", A::PageUp),
    B::new(K::Normal, "<PageUp>", A::PageUp),
    B::new(K::Normal, "h", A::ScrollLeft(4)),
    B::new(K::Normal, "<Left>", A::ScrollLeft(4)),
    B::new(K::Normal, "l", A::ScrollRight(4)),
    B::new(K::Normal, "<Right>", A::ScrollRight(4)),
    B::new(K::Normal, "g", A::GoToTop),
    B::new(K::Normal, "<*-G>", A::GoToBottom),
    B::new(K::Normal, "<*-}>", A::NextFile),
    B::new(K::Normal, "<*-{>", A::PrevFile),
    B::new(K::Normal, "<*-]>", A::NextHunk),
    B::new(K::Normal, "<*-[>", A::PrevHunk),
    B::new(K::Normal, "m", A::NextComment),
    B::new(K::Normal, "<*-M>", A::PrevComment),
    B::new(K::Normal, "<*-)>", A::CycleCommitNext),
    B::new(K::Normal, "<*-(>", A::CycleCommitPrev),
    B::new(K::Normal, "z<*-z>", A::CenterCursor),
    B::new(K::Normal, "z<*-t>", A::CursorToTop),
    B::new(K::Normal, "z<*-b>", A::CursorToBottom),
    B::new(K::Normal, "<Tab>", A::ToggleFocus),
    B::new(K::Normal, "<*-S-Tab>", A::ToggleFocusReverse),
    B::new(K::Normal, "<CR>", A::Select),
    B::new(K::Normal, "<S-CR>", A::SelectFull),
    B::new(K::Normal, "<Space>", A::ToggleExpand),
    B::new(K::Normal, "o", A::ExpandAll),
    B::new(K::Normal, "<*-O>", A::CollapseAll),
    B::new(K::Normal, "r", A::ToggleReviewed),
    B::new(K::Normal, "<*-R>", A::ToggleHunkReviewed),
    B::new(K::Normal, "c", A::AddLineComment),
    B::new(K::Normal, "<*-C>", A::AddFileComment),
    B::new(K::Normal, "i", A::EditComment),
    B::new(K::Normal, "<*-A>", A::EditCommentAtEnd),
    B::new(K::Normal, "dd", A::DeleteComment),
    B::new(K::Normal, "<*-v>", A::EnterVisualMode),
    B::new(K::Normal, "<*-V>", A::EnterVisualMode),
    B::new(K::Normal, "y", A::ExportToClipboard),
    B::new(K::Normal, "<*-Y>", A::CopyCommentAtCursor),
    B::new(K::Normal, "e", A::EditFile),
    B::new(K::Normal, "<*-/>", A::EnterSearchMode),
    B::new(K::Normal, "n", A::SearchNext),
    B::new(K::Normal, "<*-N>", A::SearchPrev),
    B::new(K::Normal, "<Esc>", A::ClearSearchHighlight),
    B::new(K::Normal, "<*-:>", A::EnterCommandMode),
    B::new(K::Normal, "<*-?>", A::ToggleHelp),
    B::new(K::Normal, "q", A::QuitHint),
    B::new(K::Normal, "<*-Z><*-Z>", A::ExportQuit),
    B::new(K::Normal, "<*-Z><*-Q>", A::ForceQuit),
    B::new(K::Normal, "<leader><*-e>", A::ToggleFileList),
    B::new(K::Normal, "<leader><*-h>", A::FocusPaneLeft),
    B::new(K::Normal, "<leader><*-l>", A::FocusPaneRight),
    B::new(K::Normal, "<leader><*-k>", A::FocusPaneUp),
    B::new(K::Normal, "<leader><*-j>", A::FocusPaneDown),
    B::new(K::Normal, "<leader><*-c>", A::AddReviewComment),
    B::new(K::Normal, "<leader><*-s>", A::ToggleCommits),
    B::new(K::Normal, "<leader><*-f>", A::ToggleSingleFileView),
    B::new(K::Normal, "<leader><*-n>", A::ToggleRelativeNumber),
    B::new(K::Normal, "<leader><*-p>", A::FilePicker),
    // The tree claims these; the diff keeps `i` = edit comment, `e` = edit file.
    B::new(K::FileTree, "i", A::FileTreeFilterInclude),
    B::new(K::FileTree, "e", A::FileTreeFilterExclude),
    B::new(K::FileTree, "<*-I>", A::FileTreeClearInclude),
    B::new(K::FileTree, "<*-E>", A::FileTreeClearExclude),
    // Visual selection
    B::new(K::Visual, "j", A::CursorDown(1)),
    B::new(K::Visual, "<Down>", A::CursorDown(1)),
    B::new(K::Visual, "k", A::CursorUp(1)),
    B::new(K::Visual, "<Up>", A::CursorUp(1)),
    B::new(K::Visual, "<C-d>", A::HalfPageDown),
    B::new(K::Visual, "<C-u>", A::HalfPageUp),
    B::new(K::Visual, "<C-f>", A::PageDown),
    B::new(K::Visual, "<PageDown>", A::PageDown),
    B::new(K::Visual, "<C-b>", A::PageUp),
    B::new(K::Visual, "<PageUp>", A::PageUp),
    B::new(K::Visual, "c", A::AddRangeComment),
    B::new(K::Visual, "<CR>", A::AddRangeComment),
    B::new(K::Visual, "s", A::AddRangeSuggestion),
    B::new(K::Visual, "y", A::CopySelection),
    B::new(K::Visual, "<Esc>", A::ExitMode),
    B::new(K::Visual, "<*-v>", A::ExitMode),
    B::new(K::Visual, "<*-V>", A::ExitMode),
    B::new(K::Visual, "q", A::QuitHint),
    // Pending-comment summary
    B::new(K::Summary, "j", A::CursorDown(1)),
    B::new(K::Summary, "<Down>", A::CursorDown(1)),
    B::new(K::Summary, "k", A::CursorUp(1)),
    B::new(K::Summary, "<Up>", A::CursorUp(1)),
    B::new(K::Summary, "<C-d>", A::HalfPageDown),
    B::new(K::Summary, "<C-u>", A::HalfPageUp),
    B::new(K::Summary, "<C-f>", A::PageDown),
    B::new(K::Summary, "<PageDown>", A::PageDown),
    B::new(K::Summary, "<C-b>", A::PageUp),
    B::new(K::Summary, "<PageUp>", A::PageUp),
    B::new(K::Summary, "g", A::GoToTop),
    B::new(K::Summary, "<*-G>", A::GoToBottom),
    B::new(K::Summary, "<CR>", A::Select),
    B::new(K::Summary, "dd", A::DeleteComment),
    B::new(K::Summary, "<Esc>", A::ExitMode),
    B::new(K::Summary, "q", A::ExitMode),
    // Help
    B::new(K::Help, "j", A::CursorDown(1)),
    B::new(K::Help, "<Down>", A::CursorDown(1)),
    B::new(K::Help, "k", A::CursorUp(1)),
    B::new(K::Help, "<Up>", A::CursorUp(1)),
    B::new(K::Help, "h", A::ScrollLeft(4)),
    B::new(K::Help, "<Left>", A::ScrollLeft(4)),
    B::new(K::Help, "l", A::ScrollRight(4)),
    B::new(K::Help, "<Right>", A::ScrollRight(4)),
    B::new(K::Help, "<C-d>", A::HalfPageDown),
    B::new(K::Help, "<C-u>", A::HalfPageUp),
    B::new(K::Help, "<C-f>", A::PageDown),
    B::new(K::Help, "<PageDown>", A::PageDown),
    B::new(K::Help, "<C-b>", A::PageUp),
    B::new(K::Help, "<PageUp>", A::PageUp),
    B::new(K::Help, "g", A::GoToTop),
    B::new(K::Help, "<*-G>", A::GoToBottom),
    B::new(K::Help, "<*-/>", A::EnterSearchMode),
    B::new(K::Help, "n", A::SearchNext),
    B::new(K::Help, "<*-N>", A::SearchPrev),
    B::new(K::Help, "<*-?>", A::ToggleHelp),
    B::new(K::Help, "q", A::ToggleHelp),
    B::new(K::Help, "<Esc>", A::ToggleHelp),
    // Full error message (`:messages`)
    B::new(K::Messages, "j", A::CursorDown(1)),
    B::new(K::Messages, "<Down>", A::CursorDown(1)),
    B::new(K::Messages, "k", A::CursorUp(1)),
    B::new(K::Messages, "<Up>", A::CursorUp(1)),
    B::new(K::Messages, "h", A::ScrollLeft(4)),
    B::new(K::Messages, "<Left>", A::ScrollLeft(4)),
    B::new(K::Messages, "l", A::ScrollRight(4)),
    B::new(K::Messages, "<Right>", A::ScrollRight(4)),
    B::new(K::Messages, "<C-d>", A::HalfPageDown),
    B::new(K::Messages, "<C-u>", A::HalfPageUp),
    B::new(K::Messages, "<C-f>", A::PageDown),
    B::new(K::Messages, "<PageDown>", A::PageDown),
    B::new(K::Messages, "<C-b>", A::PageUp),
    B::new(K::Messages, "<PageUp>", A::PageUp),
    B::new(K::Messages, "g", A::GoToTop),
    B::new(K::Messages, "<*-G>", A::GoToBottom),
    B::new(K::Messages, "<*-?>", A::ToggleHelp),
    B::new(K::Messages, "q", A::ToggleHelp),
    B::new(K::Messages, "<Esc>", A::ToggleHelp),
    // Yes/no dialog
    B::new(K::Confirm, "<*-y>", A::ConfirmYes),
    B::new(K::Confirm, "<*-Y>", A::ConfirmYes),
    B::new(K::Confirm, "<*-CR>", A::ConfirmYes),
    B::new(K::Confirm, "<*-n>", A::ConfirmNo),
    B::new(K::Confirm, "<*-N>", A::ConfirmNo),
    B::new(K::Confirm, "<*-Esc>", A::ConfirmNo),
    // Submit flow
    B::new(K::SubmitResolver, "j", A::CursorDown(1)),
    B::new(K::SubmitResolver, "<Down>", A::CursorDown(1)),
    B::new(K::SubmitResolver, "k", A::CursorUp(1)),
    B::new(K::SubmitResolver, "<Up>", A::CursorUp(1)),
    B::new(K::SubmitResolver, "<CR>", A::Select),
    B::new(K::SubmitResolver, "<Space>", A::ToggleSelect),
    B::new(K::SubmitResolver, "s", A::SubmitResolverAdvance),
    B::new(K::SubmitResolver, "<Esc>", A::ExitMode),
    B::new(K::SubmitConfirm, "<*-y>", A::ConfirmYes),
    B::new(K::SubmitConfirm, "<*-Y>", A::ConfirmYes),
    B::new(K::SubmitConfirm, "<*-CR>", A::ConfirmYes),
    B::new(K::SubmitConfirm, "<*-n>", A::ConfirmNo),
    B::new(K::SubmitConfirm, "<*-N>", A::ConfirmNo),
    B::new(K::SubmitConfirm, "<*-Esc>", A::ConfirmNo),
    B::new(K::SubmitConfirm, "<*-r>", A::SubmitReloadPr),
    B::new(K::SubmitConfirm, "<*-R>", A::SubmitReloadPr),
    B::new(K::SubmitPicker, "j", A::CursorDown(1)),
    B::new(K::SubmitPicker, "<Down>", A::CursorDown(1)),
    B::new(K::SubmitPicker, "k", A::CursorUp(1)),
    B::new(K::SubmitPicker, "<Up>", A::CursorUp(1)),
    B::new(K::SubmitPicker, "<CR>", A::Select),
    B::new(K::SubmitPicker, "<Esc>", A::ExitMode),
    B::new(K::SubmitPicker, "q", A::QuitHint),
    // Review target selector
    B::new(K::Targets, "j", A::CursorDown(1)),
    B::new(K::Targets, "<Down>", A::CursorDown(1)),
    B::new(K::Targets, "k", A::CursorUp(1)),
    B::new(K::Targets, "<Up>", A::CursorUp(1)),
    B::new(K::Targets, "<Space>", A::ToggleSelect),
    B::new(K::Targets, "<CR>", A::Select),
    B::new(K::Targets, "<Tab>", A::TargetSelectorTabNext),
    B::new(K::Targets, "<*-S-Tab>", A::TargetSelectorTabPrev),
    B::new(K::Targets, "<*-/>", A::EnterSearchMode),
    B::new(K::Targets, "r", A::TogglePrReviewRequestedFilter),
    B::new(K::Targets, "a", A::SessionsTabToggleShowEmpty),
    B::new(K::Targets, "dd", A::SessionsTabDelete),
    B::new(K::Targets, "<*-:>", A::EnterCommandMode),
    B::new(K::Targets, "<Esc>", A::ExitMode),
    B::new(K::Targets, "q", A::QuitHint),
    // Theme picker
    B::new(K::ThemePicker, "j", A::CursorDown(1)),
    B::new(K::ThemePicker, "<Down>", A::CursorDown(1)),
    B::new(K::ThemePicker, "k", A::CursorUp(1)),
    B::new(K::ThemePicker, "<Up>", A::CursorUp(1)),
    B::new(K::ThemePicker, "<CR>", A::Select),
    B::new(K::ThemePicker, "/", A::EnterSearchMode),
    B::new(K::ThemePicker, "<Esc>", A::ExitMode),
    // File picker
    B::new(K::FilePicker, "j", A::CursorDown(1)),
    B::new(K::FilePicker, "<Down>", A::CursorDown(1)),
    B::new(K::FilePicker, "k", A::CursorUp(1)),
    B::new(K::FilePicker, "<Up>", A::CursorUp(1)),
    B::new(K::FilePicker, "<CR>", A::Select),
    B::new(K::FilePicker, "/", A::EnterSearchMode),
    B::new(K::FilePicker, "<Esc>", A::ExitMode),
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
    contexts: BTreeMap<KeyContext, Vec<(Vec<KeyChord>, Action)>>,
}

/// A binding with its parsed keys, and whether it came from `[keys]`.
struct Row<'a> {
    binding: &'a Binding,
    chords: Vec<KeyChord>,
    user: bool,
}

/// Whether the bound sequence starts with the pressed `keys`.
fn starts_with(bound: &[KeyChord], keys: &[KeyChord]) -> bool {
    bound.len() >= keys.len() && bound.iter().zip(keys).all(|(b, k)| b.accepts(k))
}

/// Whether two bound sequences are the same keys.
fn same_keys(a: &[KeyChord], b: &[KeyChord]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| a.same_key(b))
}

/// Whether one bound sequence starts the other, so both cannot be bound.
fn conflicts(a: &[KeyChord], b: &[KeyChord]) -> bool {
    a.len() != b.len() && a.iter().zip(b).all(|(a, b)| a.overlaps(b))
}

impl Keymap {
    /// The built-in bindings.
    pub fn with_defaults(leader: char) -> Keymap {
        Self::build(leader, &[]).0
    }

    /// Built-in bindings overlaid with `user` bindings. Returns the keymap
    /// and a warning for each user binding that was rejected or that
    /// displaced another binding.
    ///
    /// A context's map is filled outermost layer first, built-in before user
    /// bindings within a layer, and a later binding removes any it conflicts
    /// with.
    pub fn build(leader: char, user: &[Binding]) -> (Keymap, Vec<String>) {
        let mut warnings = BTreeSet::new();
        let mut rows: Vec<Row> = Vec::new();
        for binding in DEFAULT_BINDINGS {
            let chords = parse_key_sequence(&binding.keys, leader).expect("default binding parses");
            rows.push(Row {
                binding,
                chords,
                user: false,
            });
        }
        let ctrl_c = KeyChord::from(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        for binding in user {
            let parsed = parse_key_sequence(&binding.keys, leader).map(|chords| {
                chords
                    .into_iter()
                    .map(KeyChord::loosened)
                    .collect::<Vec<_>>()
            });
            let rejected = match parsed {
                Err(e) => Some(e),
                Ok(chords) if chords.contains(&ctrl_c) => {
                    Some("Ctrl-c always exits and cannot be bound".into())
                }
                Ok(chords) if binding.context == KeyContext::Global && chords.len() > 1 => {
                    Some("a global binding must be a single key".into())
                }
                Ok(chords) => {
                    rows.push(Row {
                        binding,
                        chords,
                        user: true,
                    });
                    None
                }
            };
            if let Some(reason) = rejected {
                warnings.insert(format!(
                    "Warning: keys.{} '{}': {reason}; ignoring binding",
                    binding.context.name(),
                    binding.keys
                ));
            }
        }

        let mut contexts = BTreeMap::new();
        for &leaf in KeyContext::ALL {
            let layers = leaf.layers();
            let depth = |row: &Row| {
                layers
                    .iter()
                    .position(|layer| *layer == row.binding.context)
            };
            let mut active: Vec<&Row> = rows.iter().filter(|row| depth(row).is_some()).collect();
            // A `<leader>` default wins over a default key that shares the
            // leader's character, whichever layer either is in.
            let is_leader = |row: &Row| !row.user && row.binding.keys.starts_with("<leader>");
            let leader_rows: Vec<&Row> = active
                .iter()
                .copied()
                .filter(|row| is_leader(row))
                .collect();
            active.retain(|row| {
                row.user
                    || is_leader(row)
                    || !leader_rows
                        .iter()
                        .any(|leader| conflicts(&leader.chords, &row.chords))
            });
            active.sort_by_key(|row| (depth(row), row.user));
            let mut map: Vec<(Vec<KeyChord>, &Binding)> = Vec::new();
            for row in &active {
                let displaced = insert(&mut map, &row.chords, row.binding);
                if !row.user {
                    continue;
                }
                for (old_chords, old) in displaced {
                    let unbound_by_user = active.iter().any(|other| {
                        other.user
                            && other.binding.action == Action::None
                            && other.chords == old_chords
                    });
                    if !unbound_by_user {
                        warnings.insert(format!(
                            "Warning: keys.{} '{}' replaces '{}' ({}); bind '{}' = \"none\" to silence this",
                            row.binding.context.name(),
                            row.binding.keys,
                            old.keys,
                            old.action.name().unwrap_or("none"),
                            old.keys,
                        ));
                    }
                }
            }
            let map = map
                .into_iter()
                .map(|(keys, binding)| (keys, binding.action.clone()))
                .collect();
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
                Resolution::Run(action) => {
                    pending.clear();
                    return Resolution::Run(action);
                }
                Resolution::Pending => return Resolution::Pending,
                Resolution::Unbound if pending.len() > 1 => {
                    pending.clear();
                    pending.push(chord);
                }
                Resolution::Unbound => {
                    pending.clear();
                    return Resolution::Unbound;
                }
            }
        }
    }

    /// The action that pressing exactly `keys` runs in `context`.
    pub fn action(&self, context: KeyContext, keys: &[KeyChord]) -> Option<Action> {
        match self.lookup(context, keys) {
            Resolution::Run(action) => Some(action),
            _ => None,
        }
    }

    /// Whether `keys` is bound or starts a binding in `context`.
    pub fn binds(&self, context: KeyContext, keys: &[KeyChord]) -> bool {
        self.lookup(context, keys) != Resolution::Unbound
    }

    /// Every binding in `context`, including inherited ones, in the order
    /// [`Keymap::build`] applied them.
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

    fn lookup(&self, context: KeyContext, keys: &[KeyChord]) -> Resolution {
        let mut pending = false;
        // A binding for the exact modifiers wins over an any-modifier one.
        let mut loose = None;
        for (bound, action) in self.bindings(context) {
            if bound == keys {
                return Resolution::Run(action.clone());
            }
            if !starts_with(bound, keys) {
                continue;
            }
            if bound.len() == keys.len() {
                loose.get_or_insert(action);
            } else {
                pending = true;
            }
        }
        match loose {
            Some(action) => Resolution::Run(action.clone()),
            None if pending => Resolution::Pending,
            None => Resolution::Unbound,
        }
    }
}

/// Insert a binding, removing any it conflicts with: the same sequence, a
/// shorter sequence it extends, or a longer one it starts. Returns the
/// removed bindings other than an exact replacement.
fn insert<'a>(
    map: &mut Vec<(Vec<KeyChord>, &'a Binding)>,
    chords: &[KeyChord],
    binding: &'a Binding,
) -> Vec<(Vec<KeyChord>, &'a Binding)> {
    if binding.action == Action::None {
        // Unbinding a prefix also unbinds the sequences it starts.
        map.retain(|(existing, _)| !starts_with(existing, chords));
        return Vec::new();
    }
    map.retain(|(existing, _)| !same_keys(existing, chords));
    let displaced = map
        .extract_if(.., |(existing, _)| conflicts(existing, chords))
        .collect();
    map.push((chords.to_vec(), binding));
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
        for B { context, keys, .. } in DEFAULT_BINDINGS {
            assert!(
                seen.insert((*context, keys)),
                "duplicate {keys} in {context:?}"
            );
        }
    }

    #[test]
    fn should_bind_only_registered_actions_by_default() {
        for B { keys, action, .. } in DEFAULT_BINDINGS {
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
        assert_eq!(run(&keymap, K::Diff, "dZZ"), Resolution::Run(A::ExportQuit));
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
        for context in [K::Diff, K::Visual, K::Targets, K::SubmitPicker] {
            assert_eq!(run(&keymap, context, "q"), Resolution::Run(A::QuitHint));
        }
        assert_eq!(run(&keymap, K::Help, "q"), Resolution::Run(A::ToggleHelp));
        assert_eq!(run(&keymap, K::Summary, "q"), Resolution::Run(A::ExitMode));
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
    fn should_match_a_user_bound_capital_however_shift_is_reported() {
        let (keymap, warnings) = Keymap::build(
            DEFAULT_LEADER_KEY,
            &[Binding::new(K::Normal, "G", A::GoToTop)],
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        for modifiers in [KeyModifiers::NONE, KeyModifiers::SHIFT] {
            assert_eq!(
                keymap.resolve(
                    K::Diff,
                    &mut Vec::new(),
                    chord(KeyCode::Char('G'), modifiers)
                ),
                Resolution::Run(A::GoToTop)
            );
        }
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
    fn should_let_a_pane_override_a_global_key() {
        let (keymap, _) = Keymap::build(
            DEFAULT_LEADER_KEY,
            &[Binding::new(K::Diff, "<C-l>", A::ToggleHelp)],
        );
        assert_eq!(
            run(&keymap, K::Diff, "<C-l>"),
            Resolution::Run(A::ToggleHelp)
        );
        assert_eq!(
            run(&keymap, K::FileTree, "<C-l>"),
            Resolution::Run(A::Redraw)
        );
    }

    #[test]
    fn should_unbind_a_leader_default_spelled_with_the_leader_character() {
        let (keymap, warnings) = Keymap::build(';', &[Binding::new(K::Normal, ";e", A::None)]);
        assert!(warnings.is_empty(), "{warnings:?}");
        let keys = parse_key_sequence("<leader>e", ';').unwrap();
        assert_eq!(keymap.action(K::Diff, &keys), None);
    }

    #[test]
    fn should_reject_bindings_that_can_never_run() {
        let (keymap, warnings) = Keymap::build(
            DEFAULT_LEADER_KEY,
            &[
                Binding::new(K::Global, "gx", A::ToggleHelp),
                Binding::new(K::Normal, "<C-c>", A::ToggleHelp),
            ],
        );
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert_eq!(run(&keymap, K::Diff, "gx"), Resolution::Unbound);
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
