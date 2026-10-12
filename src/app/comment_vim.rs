use super::*;
use crate::nvim::{NvimError, NvimEvent, NvimOptions, NvimState};
use crate::ui::comment_panel::CommentVimView;

impl App {
    /// Lazily build the vim overlay from the buffer on first key in comment mode
    /// (no-op unless `comment_vim` is on). Seeding from the buffer covers both
    /// fresh and existing-comment edits.
    pub fn ensure_comment_vim_editor(&mut self) {
        if !self.comment_vim_enabled || self.input_mode != InputMode::Comment {
            return;
        }
        if self.comment_vim_editor.is_some() {
            return;
        }
        if self.comment_vim_backend == CommentVimBackend::Nvim && !self.comment_nvim_unavailable {
            match self.begin_comment_nvim() {
                Ok(()) => return,
                Err(err) => {
                    self.comment_nvim = None;
                    self.comment_nvim_unavailable = true;
                    self.set_error(format!(
                        "Embedded Neovim unavailable, using the built-in vim editor: {err}"
                    ));
                }
            }
        }
        self.start_builtin_comment_vim();
    }

    fn start_builtin_comment_vim(&mut self) {
        self.comment_vim_editor = Some(CommentVimEngine::Builtin(Box::new(
            CommentVimEditor::from_buffer(&self.comment_buffer, self.comment_cursor),
        )));
    }

    fn begin_comment_nvim(&mut self) -> std::result::Result<(), NvimError> {
        if self
            .comment_nvim
            .as_ref()
            .is_some_and(|host| !host.is_alive())
        {
            self.comment_nvim = None;
        }
        let host = match self.comment_nvim.as_mut() {
            Some(host) => host,
            None => self
                .comment_nvim
                .insert(NvimHost::spawn(&self.comment_nvim_options())?),
        };
        host.begin(&self.comment_buffer, self.comment_cursor)?;
        self.comment_vim_editor = Some(CommentVimEngine::Nvim);
        self.apply_comment_nvim_events();
        Ok(())
    }

    fn comment_nvim_options(&self) -> NvimOptions {
        NvimOptions {
            tab_width: self.comment_tab_width,
            ..NvimOptions::default()
        }
    }

    /// Start the embedded Neovim ahead of the first comment, so its startup
    /// (including the user's config) overlaps with reviewing instead of
    /// delaying the first keystroke. No-op unless the Neovim backend is in use.
    pub fn prewarm_comment_nvim(&mut self) -> std::result::Result<(), NvimError> {
        if !self.comment_vim_enabled
            || self.comment_vim_backend != CommentVimBackend::Nvim
            || self.comment_nvim_unavailable
            || self.comment_nvim.is_some()
        {
            return Ok(());
        }
        match NvimHost::spawn(&self.comment_nvim_options()) {
            Ok(host) => {
                self.comment_nvim = Some(host);
                Ok(())
            }
            Err(err) => {
                self.comment_nvim_unavailable = true;
                Err(err)
            }
        }
    }

    /// True when the current (or next) comment is edited by the embedded
    /// Neovim rather than the built-in editor.
    pub fn comment_vim_uses_nvim(&self) -> bool {
        match &self.comment_vim_editor {
            Some(CommentVimEngine::Nvim) => true,
            Some(CommentVimEngine::Builtin(_)) => false,
            None => {
                self.comment_vim_backend == CommentVimBackend::Nvim
                    && !self.comment_nvim_unavailable
            }
        }
    }

    /// Drain asynchronous Neovim traffic (state changed by a mapping timeout,
    /// messages, exits). Called every tick; returns true when a redraw is due.
    pub fn poll_comment_nvim_events(&mut self) -> bool {
        let Some(host) = self.comment_nvim.as_mut() else {
            return false;
        };
        host.poll();
        self.apply_comment_nvim_events()
    }

    fn apply_comment_nvim_events(&mut self) -> bool {
        let Some(host) = self.comment_nvim.as_mut() else {
            return false;
        };
        let events = host.take_events();
        let changed = !events.is_empty();
        for event in events {
            self.apply_comment_nvim_event(event);
        }
        changed
    }

    fn apply_comment_nvim_event(&mut self, event: NvimEvent) {
        let nvim_session = self.input_mode == InputMode::Comment
            && matches!(self.comment_vim_editor, Some(CommentVimEngine::Nvim));
        match event {
            NvimEvent::State => {
                if nvim_session && let Some(host) = &self.comment_nvim {
                    let state = host.state();
                    self.comment_buffer = state.text.clone();
                    self.comment_cursor = state.cursor;
                }
            }
            NvimEvent::Save(text) => {
                if nvim_session {
                    self.comment_buffer = text;
                    self.comment_vim_pending = CommentVimPending::None;
                    self.save_comment();
                }
            }
            // `:wq` arrives as Save then Quit; the save already left comment
            // mode, so only a bare `:q` cancels here.
            NvimEvent::Quit => {
                if nvim_session {
                    self.exit_comment_mode();
                }
            }
            // Vim errors (`E486: Pattern not found`, ...) are editing feedback,
            // not app failures: show them as expiring warnings, not sticky
            // errors.
            NvimEvent::Message { kind, text } => match kind.as_str() {
                "emsg" | "echoerr" | "lua_error" | "rpc_error" | "wmsg" => self.set_warning(text),
                _ => self.set_message(text),
            },
            NvimEvent::Exited { expected, detail } => {
                self.comment_nvim = None;
                if expected {
                    // `:q` quits Neovim itself; load the next one in the
                    // background so the next comment does not wait for it.
                    if let Err(err) = self.prewarm_comment_nvim() {
                        self.set_error(format!("Failed to restart embedded Neovim: {err}"));
                    }
                } else {
                    self.set_error(format!("Embedded Neovim exited unexpectedly{detail}"));
                    // Keep the comment being typed editable.
                    if nvim_session {
                        self.start_builtin_comment_vim();
                    }
                }
            }
        }
    }

    /// Header indicator for the comment box as `(text, warn)`, or `None` when
    /// vim is off. `warn` is true for the cancel-confirm hint so the renderer
    /// can paint it red. Shows the in-progress `:` command-line when active,
    /// otherwise the pending-confirm hint, otherwise the mode label.
    pub fn comment_vim_mode_label(&self) -> Option<(String, bool)> {
        if !self.comment_vim_enabled || self.input_mode != InputMode::Comment {
            return None;
        }
        if let Some(cmd) = &self.comment_vim_command {
            return Some((format!(":{cmd}"), false));
        }
        match self.comment_vim_pending {
            CommentVimPending::Save => return Some(("Enter again to save".to_string(), false)),
            CommentVimPending::Cancel => {
                // `q` records macros in Neovim, so only Esc cancels there.
                let hint = if self.comment_vim_uses_nvim() {
                    "Esc again to cancel"
                } else {
                    "Esc/q again to cancel"
                };
                return Some((hint.to_string(), true));
            }
            CommentVimPending::None => {}
        }
        let label = match &self.comment_vim_editor {
            Some(CommentVimEngine::Builtin(editor)) => editor.label().to_string(),
            Some(CommentVimEngine::Nvim) => self
                .comment_nvim
                .as_ref()
                .map_or_else(|| "INSERT".to_string(), |host| nvim_label(host.state())),
            None => "INSERT".to_string(),
        };
        Some((label, false))
    }

    /// Everything the renderer needs from the vim engine: the header badge and,
    /// with Neovim, the visual selection.
    pub fn comment_vim_view(&self) -> Option<CommentVimView> {
        let (label, warn) = self.comment_vim_mode_label()?;
        let selection = match (&self.comment_vim_editor, &self.comment_nvim) {
            // Only meaningful against the text it was computed for.
            (Some(CommentVimEngine::Nvim), Some(host))
                if host.state().text == self.comment_buffer =>
            {
                host.state().selection.clone()
            }
            _ => Vec::new(),
        };
        Some(CommentVimView {
            label,
            warn,
            selection,
        })
    }

    /// True while a `:` command-line is being entered in the comment box.
    pub fn comment_vim_command_active(&self) -> bool {
        self.comment_vim_command.is_some()
    }

    /// Plain Enter in vim Normal mode: arm Save, or save on the second
    /// consecutive press (like `:w`).
    pub fn comment_vim_enter_normal(&mut self) {
        if self.comment_vim_pending == CommentVimPending::Save {
            self.comment_vim_pending = CommentVimPending::None;
            self.save_comment();
        } else {
            self.comment_vim_pending = CommentVimPending::Save;
        }
    }

    /// Esc in vim Normal mode: arm Cancel, or cancel on the second consecutive
    /// press (like `:q`). A lone Esc does nothing but show the hint.
    pub fn comment_vim_esc_normal(&mut self) {
        if self.comment_vim_pending == CommentVimPending::Cancel {
            self.comment_vim_pending = CommentVimPending::None;
            self.exit_comment_mode();
        } else {
            self.comment_vim_pending = CommentVimPending::Cancel;
        }
    }

    /// Reset the pending double-press state; called when any other key
    /// interrupts the sequence.
    pub fn comment_vim_reset_pending(&mut self) {
        self.comment_vim_pending = CommentVimPending::None;
    }

    /// Open the `:` command-line (vim Normal mode).
    pub fn start_comment_vim_command(&mut self) {
        self.comment_vim_command = Some(String::new());
    }

    /// Append a typed character to the `:` command-line.
    pub fn comment_vim_command_push(&mut self, c: char) {
        if let Some(cmd) = self.comment_vim_command.as_mut() {
            cmd.push(c);
        }
    }

    /// Backspace in the `:` command-line; backspacing past `:` closes it.
    pub fn comment_vim_command_backspace(&mut self) {
        if let Some(cmd) = self.comment_vim_command.as_mut() {
            if cmd.is_empty() {
                self.comment_vim_command = None;
            } else {
                cmd.pop();
            }
        }
    }

    /// Abandon the `:` command-line without running anything.
    pub fn comment_vim_command_cancel(&mut self) {
        self.comment_vim_command = None;
    }

    /// Execute the typed `:` command: `w`/`wq`/`x` save, `q`/`q!` cancel.
    pub fn run_comment_vim_command(&mut self) {
        let cmd = self.comment_vim_command.take().unwrap_or_default();
        match cmd.trim() {
            "w" | "wq" | "x" => self.save_comment(),
            "q" | "q!" => self.exit_comment_mode(),
            "" => {}
            other => self.set_warning(format!("Not a comment command: :{other}")),
        }
    }

    /// True when the vim engine is in Normal mode with nothing pending, so
    /// plain Enter/Esc can be claimed for save/cancel.
    pub fn comment_vim_in_normal_mode(&self) -> bool {
        match &self.comment_vim_editor {
            Some(CommentVimEngine::Builtin(editor)) => editor.is_normal_mode(),
            Some(CommentVimEngine::Nvim) => self
                .comment_nvim
                .as_ref()
                .is_some_and(|host| host.state().plain_normal),
            None => false,
        }
    }

    /// Feed a key to the vim overlay and sync the result into the canonical
    /// `comment_buffer`/`comment_cursor`.
    pub fn comment_vim_feed_key(&mut self, key: crossterm::event::KeyEvent) {
        match self.comment_vim_editor.as_mut() {
            Some(CommentVimEngine::Builtin(editor)) => {
                let (text, cursor) = editor.feed_key(key);
                self.comment_buffer = text;
                self.comment_cursor = cursor;
            }
            Some(CommentVimEngine::Nvim) => {
                if let Some(host) = self.comment_nvim.as_mut() {
                    host.send_key(key);
                }
                self.apply_comment_nvim_events();
            }
            None => {}
        }
    }

    /// Insert a soft tab (`comment_tab_width` spaces) into the vim overlay,
    /// used for Tab while typing in Insert mode.
    pub fn comment_vim_insert_soft_tab(&mut self) {
        let space = crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char(' '),
            crossterm::event::KeyModifiers::NONE,
        );
        for _ in 0..self.comment_tab_width {
            self.comment_vim_feed_key(space);
        }
    }

    /// Feed a bracketed-paste payload to the vim overlay and sync the result.
    pub fn comment_vim_feed_paste(&mut self, text: String) {
        match self.comment_vim_editor.as_mut() {
            Some(CommentVimEngine::Builtin(editor)) => {
                let (text, cursor) = editor.feed_paste(text);
                self.comment_buffer = text;
                self.comment_cursor = cursor;
            }
            Some(CommentVimEngine::Nvim) => {
                if let Some(host) = self.comment_nvim.as_mut() {
                    host.paste(&text);
                }
                self.apply_comment_nvim_events();
            }
            None => {}
        }
    }

    /// Enable/disable vim editing at runtime (e.g. `:vim`); takes effect on the
    /// next comment session.
    pub fn set_comment_vim(&mut self, enabled: bool) {
        self.comment_vim_enabled = enabled;
        if enabled {
            // Retry Neovim: the user may have fixed what kept it from starting.
            self.comment_nvim_unavailable = false;
            if let Err(err) = self.prewarm_comment_nvim() {
                self.set_error(format!(
                    "Embedded Neovim unavailable, using the built-in vim editor: {err}"
                ));
                return;
            }
        } else {
            self.comment_vim_editor = None;
            self.comment_nvim = None;
        }
        self.set_message(if enabled {
            "Vim mode enabled for the comment box"
        } else {
            "Vim mode disabled for the comment box"
        });
    }

    /// Toggle vim modal editing for the comment box.
    pub fn toggle_comment_vim(&mut self) {
        self.set_comment_vim(!self.comment_vim_enabled);
    }
}

/// Header badge for the Neovim engine: the command line while one is open,
/// otherwise the mode, plus the register being recorded into.
fn nvim_label(state: &NvimState) -> String {
    let mut label = state
        .cmdline
        .clone()
        .unwrap_or_else(|| state.mode.label().to_string());
    if let Some(register) = state.recording {
        label.push_str(&format!(" recording @{register}"));
    }
    label
}
