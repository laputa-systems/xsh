//! The interactive read loop: raw-mode line editing, completion, history
//! search, and the directory picker. One call to [`read_line`] owns the
//! terminal from prompt to submitted line.

use super::alias::AliasMap;
use super::builtin;
use super::complete::{self, CompletionState, Completions};
use super::history::FuzzyMatch;
use super::input::{InputEvent, InputReader, Key, KeyEvent};
use super::line::{self, LineBuffer};
use super::render;
use super::session::Session;
use super::term::{self, TermWriter};
use std::fmt::Write as _;
use std::os::fd::RawFd;
use std::path::Path;

/// Terminal-facing state that survives between prompts.
pub(super) struct Ui {
    pub(super) rows: u16,
    pub(super) cols: u16,
    /// Reusable buffer for the rendered prompt string.
    pub(super) prompt_buf: String,
    /// Reusable completion arena, avoiding an allocation per Tab press.
    pub(super) completion_buffer: Completions,
    /// Reusable fuzzy match buffer, avoiding an allocation per Ctrl+R key.
    pub(super) history_match_buffer: Vec<FuzzyMatch>,
    /// The previous command may have left the terminal mid-line, so the next
    /// prompt must begin on a fresh row for its layout to start at column 0.
    pub(super) prompt_needs_line_start: bool,
    pub(super) exit_warned: bool,
    pub(super) signal_fd: RawFd,
    /// Every submitted line, for `copy-scrollback`.
    pub(super) session_log: String,
    /// Layout captured for `xshi-dump` and Ctrl+P.
    pub(super) layout_dump: String,
    pub(super) path_cache: super::path::PathCache,
    /// A command line was submitted and its completion marker is still due.
    pub(super) osc133_pending_d: bool,
}

impl Ui {
    pub(super) fn new(signal_fd: RawFd) -> Self {
        let (rows, cols) = term::term_size();
        Self {
            rows,
            cols,
            prompt_buf: String::with_capacity(128),
            completion_buffer: Completions::with_capacity(2048, 64),
            history_match_buffer: Vec::with_capacity(200),
            prompt_needs_line_start: false,
            exit_warned: false,
            signal_fd,
            session_log: String::new(),
            layout_dump: String::new(),
            path_cache: super::path::PathCache::new(),
            osc133_pending_d: false,
        }
    }
}

pub(super) enum ReadResult {
    Line(String),
    Exit,
    Empty,
}

enum Mode {
    Normal,
    Completion {
        state: CompletionState,
        base_line: LineBuffer,
    },
    HistorySearch {
        query: LineBuffer,
        matches: Vec<FuzzyMatch>,
        candidates: Vec<usize>,
        scratch: Vec<usize>,
        candidate_stack: Vec<(usize, Vec<usize>)>,
        selected: usize,
        saved_line: String,
    },
    DirPicker {
        entries: Vec<String>,
        selected: usize,
    },
}

fn env_str<'a>(session: &'a Session, name: &[u8]) -> &'a str {
    session
        .env
        .get(name)
        .and_then(|value| std::str::from_utf8(value).ok())
        .unwrap_or("")
}

fn path_env(session: &Session) -> &[u8] {
    session
        .env
        .get(b"PATH".as_slice())
        .map_or(&[], Vec::as_slice)
}

fn home_string(session: &Session) -> String {
    env_str(session, b"HOME").to_owned()
}

fn capture_layout_dump(
    session: &Session,
    ui: &mut Ui,
    mode: &Mode,
    line: &LineBuffer,
    prompt_display_len: usize,
    region: render::RenderedRegion,
) {
    let suggestion =
        if line.text().len() >= 3 && !line.has_newlines() && line.cursor() == line.text().len() {
            session
                .history
                .session_prefix_search(line.text(), 0)
                .and_then(|entry| entry.strip_prefix(line.text()))
        } else {
            None
        };
    let suggestion_display_len = suggestion.map(line::str_width).unwrap_or(0);

    ui.layout_dump.clear();
    let (rows, cols) = (ui.rows, ui.cols);
    let out = &mut ui.layout_dump;
    let _ = writeln!(out, "xshi layout dump");
    let _ = writeln!(out, "version: {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "terminal.rows: {rows}");
    let _ = writeln!(out, "terminal.cols: {cols}");
    let _ = writeln!(out, "prompt.display_len: {prompt_display_len}");
    let _ = writeln!(out, "line.bytes: {}", line.text().len());
    let _ = writeln!(out, "line.cursor_byte: {}", line.cursor());
    let _ = writeln!(
        out,
        "line.cursor_display_col: {}",
        line.display_cursor_pos()
    );
    let _ = writeln!(out, "line.display_len: {}", line.display_len());
    let _ = writeln!(out, "suggestion.present: {}", suggestion.is_some());
    let _ = writeln!(out, "suggestion.display_len: {suggestion_display_len}");
    let _ = writeln!(out, "line.has_newlines: {}", line.has_newlines());
    let _ = writeln!(out, "rendered_region: {region:?}");
    let _ = writeln!(out, "mode: {}", mode_name(mode));
    render::debug_dump_prompt_layout(out, prompt_display_len, line, suggestion_display_len, cols);

    match mode {
        Mode::Normal => {}
        Mode::Completion { state, base_line } => {
            let _ = writeln!(
                out,
                "completion.base_line.cursor_byte: {}",
                base_line.cursor()
            );
            let _ = writeln!(out, "completion.selected: {}", state.selected);
            let _ = writeln!(out, "completion.cols: {}", state.cols);
            let _ = writeln!(out, "completion.rows: {}", state.rows);
            let _ = writeln!(out, "completion.scroll: {}", state.scroll);
            let _ = writeln!(out, "completion.term_cols: {}", state.term_cols);
            let _ = writeln!(out, "completion.term_rows: {rows}");
            let _ = writeln!(
                out,
                "completion.dir_prefix_bytes: {}",
                state.dir_prefix.len()
            );
            let _ = writeln!(out, "completion.in_quote: {}", state.in_quote);
            let _ = writeln!(out, "completion.entries: {:?}", state.comp.entries);
            render::debug_dump_completion_layout(out, state, rows);
        }
        Mode::HistorySearch {
            query,
            matches,
            candidates,
            scratch,
            candidate_stack,
            selected,
            saved_line,
        } => {
            let _ = writeln!(out, "history.query.cursor_byte: {}", query.cursor());
            let _ = writeln!(
                out,
                "history.query.cursor_display_col: {}",
                query.display_cursor_pos()
            );
            let _ = writeln!(out, "history.matches: {matches:?}");
            let _ = writeln!(out, "history.candidates: {candidates:?}");
            let _ = writeln!(out, "history.scratch: {scratch:?}");
            let _ = writeln!(out, "history.candidate_stack: {candidate_stack:?}");
            let _ = writeln!(out, "history.selected: {selected}");
            let _ = writeln!(out, "history.saved_line_bytes: {}", saved_line.len());
            render::debug_dump_pager_layout(
                out,
                line::str_width("search: "),
                line::str_width(query.text()),
                0,
                query.display_cursor_pos(),
                matches.len(),
                *selected,
                rows,
                cols,
            );
        }
        Mode::DirPicker { entries, selected } => {
            let _ = writeln!(out, "dir_picker.entry_count: {}", entries.len());
            let _ = writeln!(out, "dir_picker.selected: {selected}");
            render::debug_dump_pager_layout(
                out,
                line::str_width("dirs:"),
                0,
                0,
                0,
                entries.len(),
                *selected,
                rows,
                cols,
            );
        }
    }
}

fn mode_name(mode: &Mode) -> &'static str {
    match mode {
        Mode::Normal => "normal",
        Mode::Completion { .. } => "completion",
        Mode::HistorySearch { .. } => "history_search",
        Mode::DirPicker { .. } => "dir_picker",
    }
}

/// Writes the captured layout dump to a fresh `~/.cache/xshi/dump-<hex>`.
/// Deliberately does not touch the terminal, so Ctrl+P can capture a corrupted
/// screen without changing it.
fn write_layout_dump_to_file(session: &Session, ui: &Ui) -> Result<String, String> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let home = home_string(session);
    if home.is_empty() {
        return Err("xshi-dump: HOME is not set".to_string());
    }

    let dir = Path::new(&home).join(".cache/xshi");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("xshi-dump: could not create {}: {error}", dir.display()))?;
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))
        .map_err(|error| format!("xshi-dump: could not secure {}: {error}", dir.display()))?;

    for _ in 0..8 {
        let path = dir.join(format!("dump-{}", random_dump_hex()));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                std::io::Write::write_all(&mut file, ui.layout_dump.as_bytes()).map_err(
                    |error| format!("xshi-dump: could not write {}: {error}", path.display()),
                )?;
                return Ok(path.display().to_string());
            }
            Err(_) => continue,
        }
    }

    Err(format!(
        "xshi-dump: could not choose a unique dump filename in {}",
        dir.display()
    ))
}

/// Implements the `xshi-dump` builtin: writes the last captured layout.
pub(super) fn write_layout_dump(session: &Session, ui: &Ui) -> (i32, String, String) {
    match write_layout_dump_to_file(session, ui) {
        Ok(path) => (0, format!("xshi-dump: wrote {path}\n"), String::new()),
        Err(message) => (1, String::new(), format!("{message}\n")),
    }
}

fn random_dump_hex() -> String {
    let mut bytes = [0u8; 16];
    let random_read = std::fs::File::open("/dev/urandom")
        .and_then(|mut file| std::io::Read::read_exact(&mut file, &mut bytes));
    if random_read.is_err() {
        let mut value = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            ^ (rustix::process::getpid().as_raw_pid() as u128);
        for byte in &mut bytes {
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            *byte = value as u8;
        }
    }

    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// OSC 7 lets terminal emulators track the working directory; it also marks
/// the start of every prompt cycle for terminal tests.
fn write_osc7(session: &Session) {
    let pwd = env_str(session, b"PWD");
    if pwd.is_empty() {
        return;
    }
    let hostname = env_str(session, b"HOSTNAME");
    let mut osc = format!("\x1b]7;file://{hostname}");
    for byte in pwd.bytes() {
        if byte == b' ' {
            osc.push_str("%20");
        } else {
            osc.push(byte as char);
        }
    }
    osc.push('\x07');
    let _ = std::io::Write::write_all(&mut std::io::stdout(), osc.as_bytes());
}

pub(super) fn read_line(session: &mut Session, ui: &mut Ui) -> ReadResult {
    let _raw = match term::RawMode::enable() {
        Ok(raw) => raw,
        Err(error) => {
            eprintln!("xshi: raw mode: {error}");
            return ReadResult::Exit;
        }
    };

    let mut tw = TermWriter::new();
    let mut reader = InputReader::new(ui.signal_fd);
    let mut line = LineBuffer::new();
    let mut mode = Mode::Normal;
    let mut history_idx: Option<usize> = None;
    let mut saved_line = String::new();
    let mut grid_column: Option<usize> = None;
    let mut undo = UndoState::new();
    if ui.osc133_pending_d {
        term::write_osc133_d(session.last_status);
        ui.osc133_pending_d = false;
    }
    write_osc7(session);

    if ui.prompt_needs_line_start {
        // Commands write directly to the terminal, so the shell cannot observe
        // whether their output ended with a newline. Start the prompt on a
        // fresh row to keep prompt geometry independent of child output.
        tw.write_str("\r\n");
        ui.prompt_needs_line_start = false;
    }
    term::write_osc133_a();

    let pwd = env_str(session, b"PWD").to_owned();
    let denv_dirty = session.denv_dirty();
    let last_status = session.last_status;
    session
        .prompt
        .render_into(&mut ui.prompt_buf, last_status, &pwd, denv_dirty);
    let prompt_str = std::mem::take(&mut ui.prompt_buf);
    let prompt_display_len = session.prompt.display_len(&prompt_str);

    let mut region = render_prompt_region(
        &mut tw,
        session,
        ui,
        &line,
        &prompt_str,
        prompt_display_len,
        render::RenderedRegion::default(),
    );
    let mut history_cache = render::HistoryPagerCache::default();
    let _ = tw.flush_to_stdout();
    term::write_osc133_b();

    loop {
        let event = reader.read_event();
        match event {
            InputEvent::Signal(sig) => {
                if sig == libc::SIGINT {
                    tw.write_str("^C\r\n");
                    let _ = tw.flush_to_stdout();
                    term::write_osc133_d(130);
                    ui.osc133_pending_d = false;
                    ui.prompt_buf = prompt_str;
                    return ReadResult::Empty;
                }
                if sig == rustix::process::Signal::WINCH.as_raw() {
                    let (rows, cols) = term::term_size();
                    ui.rows = rows;
                    ui.cols = cols;
                    grid_column = None;
                    update_mode_layout_for_resize(&mut mode, ui);

                    // Terminal emulators reflow the live cursor and their DEC
                    // saved cursor differently on resize, so the saved
                    // RenderedRegion anchor is no longer a safe basis for an
                    // incremental repaint. Start from a known screen position
                    // and rebuild the active region.
                    tw.clear_screen();
                    region = render::RenderedRegion::default();
                    history_cache.clear();
                }
                region = render_active_mode(
                    &mut tw,
                    &mode,
                    session,
                    ui,
                    &line,
                    &prompt_str,
                    prompt_display_len,
                    region,
                    &mut history_cache,
                );
                let _ = tw.flush_to_stdout();
                continue;
            }
            InputEvent::Paste(text) => {
                // Insert the entire paste in one shot, with no per-char render.
                grid_column = None;
                if let Mode::HistorySearch {
                    query,
                    matches,
                    candidates,
                    scratch,
                    candidate_stack,
                    selected,
                    ..
                } = &mut mode
                {
                    query.insert_str(&text);
                    candidate_stack.clear();
                    session.history.visible_entry_indices_into(scratch);
                    session.history.fuzzy_search_subset_into_in_dir(
                        query.text(),
                        scratch,
                        candidates,
                        matches,
                        200,
                        &session.cwd,
                    );
                    std::mem::swap(candidates, scratch);
                    *selected = 0;
                    region = render_history_mode(
                        &mut tw,
                        &mode,
                        session,
                        ui,
                        region,
                        &mut history_cache,
                    );
                    let _ = tw.flush_to_stdout();
                    continue;
                }
                undo.before_edit(&line, EditGroup::Other);
                line.insert_str(&text);
                region = render_active_mode(
                    &mut tw,
                    &mode,
                    session,
                    ui,
                    &line,
                    &prompt_str,
                    prompt_display_len,
                    region,
                    &mut history_cache,
                );
                let _ = tw.flush_to_stdout();
                continue;
            }
            InputEvent::PasteRejected => {
                grid_column = None;
                tw.write_str("\x07");
                let _ = tw.flush_to_stdout();
                continue;
            }
            InputEvent::Key(key) => {
                // Ctrl+P writes a layout dump in any mode and does not repaint,
                // so it captures the screen exactly as it is. Intercept before
                // mode dispatch so every mode is covered uniformly.
                if key.key == Key::Char('p') && key.mods.ctrl {
                    capture_layout_dump(
                        session,
                        ui,
                        &mode,
                        &line,
                        prompt_display_len,
                        region,
                    );
                    if let Err(message) = write_layout_dump_to_file(session, ui) {
                        eprintln!("{message}");
                    }
                    continue;
                }
                match &mut mode {
                    Mode::Normal => {
                        match handle_normal_key(
                            key,
                            &mut line,
                            &mut history_idx,
                            &mut saved_line,
                            &mut grid_column,
                            prompt_display_len,
                            session,
                            ui,
                            &mut undo,
                        ) {
                            KeyAction::Continue => {}
                            KeyAction::Execute(text) => {
                                capture_layout_dump(
                                    session,
                                    ui,
                                    &mode,
                                    &line,
                                    prompt_display_len,
                                    region,
                                );
                                // Clear autosuggestion ghost text before freezing the line.
                                if line.cursor() == line.text().len() {
                                    tw.write_str("\x1b[J");
                                }
                                tw.write_str("\r\n");
                                let _ = tw.flush_to_stdout();
                                ui.prompt_buf = prompt_str;
                                let submitted = submit_text(&text);
                                ui.osc133_pending_d = true;
                                if submitted.is_empty() {
                                    return ReadResult::Empty;
                                }
                                if parsed_line_accepts(&submitted, &session.aliases) {
                                    term::write_osc133_c();
                                }
                                if submitted_line_clears_screen(&submitted) {
                                    ui.prompt_needs_line_start = true;
                                }
                                return ReadResult::Line(submitted);
                            }
                            KeyAction::Continuation => {
                                line.insert_char('\n');
                                history_idx = None;
                            }
                            KeyAction::Cancel => {
                                tw.write_str("^C\r\n");
                                let _ = tw.flush_to_stdout();
                                term::write_osc133_d(130);
                                ui.osc133_pending_d = false;
                                ui.prompt_buf = prompt_str;
                                return ReadResult::Empty;
                            }
                            KeyAction::Exit => {
                                tw.write_str("\r\n");
                                let _ = tw.flush_to_stdout();
                                ui.prompt_buf = prompt_str;
                                let result = handle_exit(session, ui);
                                let status = if matches!(result, ReadResult::Empty) {
                                    130
                                } else {
                                    session.last_status
                                };
                                term::write_osc133_d(status);
                                return result;
                            }
                            KeyAction::ClearScreen => {
                                tw.clear_screen();
                                region = render::RenderedRegion::default();
                            }
                            KeyAction::StartHistorySearch => {
                                session.history.sync();
                                saved_line = line.text().to_string();
                                let mut candidates = Vec::new();
                                session.history.visible_entry_indices_into(&mut candidates);
                                let mut scratch = Vec::new();
                                let mut matches = std::mem::take(&mut ui.history_match_buffer);
                                session.history.fuzzy_search_subset_into_in_dir(
                                    "",
                                    &candidates,
                                    &mut scratch,
                                    &mut matches,
                                    200,
                                    &session.cwd,
                                );
                                std::mem::swap(&mut candidates, &mut scratch);
                                region.clear(&mut tw);
                                mode = Mode::HistorySearch {
                                    query: LineBuffer::new(),
                                    matches,
                                    candidates,
                                    scratch,
                                    candidate_stack: Vec::new(),
                                    selected: 0,
                                    saved_line: saved_line.clone(),
                                };
                                history_cache.clear();
                                region = render_history_mode(
                                    &mut tw,
                                    &mode,
                                    session,
                                    ui,
                                    render::RenderedRegion::default(),
                                    &mut history_cache,
                                );
                                let _ = tw.flush_to_stdout();
                                continue;
                            }
                            KeyAction::StartDirPicker => {
                                // Show the dir stack most recent first, excluding
                                // the current directory.
                                let pwd = std::env::current_dir()
                                    .map(|p| p.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                let entries: Vec<String> = session
                                    .dir_stack
                                    .iter()
                                    .rev()
                                    .filter(|d| *d != &pwd)
                                    .cloned()
                                    .collect();
                                if !entries.is_empty() {
                                    region.clear(&mut tw);
                                    mode = Mode::DirPicker {
                                        entries,
                                        selected: 0,
                                    };
                                    region =
                                        render_dir_picker_mode(&mut tw, &mode, session, ui, region);
                                    let _ = tw.flush_to_stdout();
                                    continue;
                                }
                            }
                            KeyAction::StartCompletion => {
                                let comp = std::mem::take(&mut ui.completion_buffer);
                                let base_line = line.clone();
                                let mut cs = start_completion(&base_line, ui.cols, session, comp);
                                if cs.comp.len() == 1 {
                                    preview_completion(&mut line, &cs, &base_line);
                                    ui.completion_buffer = cs.comp;
                                } else if !cs.comp.is_empty() {
                                    cs.selected = usize::MAX;
                                    preview_completion(&mut line, &cs, &base_line);
                                    mode = Mode::Completion {
                                        state: cs,
                                        base_line,
                                    };
                                } else {
                                    ui.completion_buffer = cs.comp;
                                }
                            }
                        }

                        region = render_active_mode(
                            &mut tw,
                            &mode,
                            session,
                            ui,
                            &line,
                            &prompt_str,
                            prompt_display_len,
                            region,
                            &mut history_cache,
                        );
                        let _ = tw.flush_to_stdout();
                    }

                    Mode::Completion { state, base_line } => {
                        match handle_completion_key(key, state, base_line) {
                            CompletionAction::Navigate => {
                                preview_completion(&mut line, state, base_line);
                                // Cursor is on the prompt line: repaint the grid in place.
                                let info = render::render_line(
                                    &mut tw,
                                    &prompt_str,
                                    prompt_display_len,
                                    &line,
                                    ui.cols,
                                    region,
                                    &render::RenderOpts::default(),
                                );
                                region = info;
                                render::render_completions(&mut tw, state, info, false, ui.rows);
                                let _ = tw.flush_to_stdout();
                                continue;
                            }
                            CompletionAction::Refilter => {
                                // Reclaim the buffer from the current state and re-run completion.
                                let selected = state.selected;
                                let comp = std::mem::take(&mut state.comp);
                                let mut cs = start_completion(base_line, ui.cols, session, comp);
                                if !cs.comp.is_empty() {
                                    cs.selected = if selected == usize::MAX {
                                        usize::MAX
                                    } else {
                                        selected.min(cs.comp.len() - 1)
                                    };
                                    preview_completion(&mut line, &cs, base_line);
                                    let info = render::render_line(
                                        &mut tw,
                                        &prompt_str,
                                        prompt_display_len,
                                        &line,
                                        ui.cols,
                                        region,
                                        &render::RenderOpts::default(),
                                    );
                                    region = info;
                                    render::render_completions(&mut tw, &cs, info, true, ui.rows);
                                    mode = Mode::Completion {
                                        state: cs,
                                        base_line: base_line.clone(),
                                    };
                                    let _ = tw.flush_to_stdout();
                                    continue;
                                } else {
                                    line = base_line.clone();
                                    ui.completion_buffer = cs.comp;
                                    mode = Mode::Normal;
                                }
                            }
                            CompletionAction::Accept => {
                                preview_completion(&mut line, state, base_line);
                                ui.completion_buffer = std::mem::take(&mut state.comp);
                                mode = Mode::Normal;
                            }
                            CompletionAction::Cancel => {
                                line = base_line.clone();
                                ui.completion_buffer = std::mem::take(&mut state.comp);
                                mode = Mode::Normal;
                            }
                        }

                        // Cursor is on the prompt line: render_line's \r plus
                        // clear-to-end-of-screen clears the grid below.
                        let info = render::render_line(
                            &mut tw,
                            &prompt_str,
                            prompt_display_len,
                            &line,
                            ui.cols,
                            region,
                            &render::RenderOpts::default(),
                        );
                        region = info;
                        let _ = tw.flush_to_stdout();
                    }

                    Mode::HistorySearch {
                        query,
                        matches,
                        candidates,
                        scratch,
                        candidate_stack,
                        selected,
                        saved_line,
                    } => match handle_history_search_key(
                        key,
                        query,
                        matches,
                        candidates,
                        scratch,
                        candidate_stack,
                        selected,
                        session,
                    ) {
                        HistoryAction::Continue => {
                            region = render_history_mode(
                                &mut tw,
                                &mode,
                                session,
                                ui,
                                region,
                                &mut history_cache,
                            );
                            let _ = tw.flush_to_stdout();
                        }
                        HistoryAction::Accept(text) => {
                            line.set(&text);
                            ui.history_match_buffer = std::mem::take(matches);
                            history_cache.clear();
                            mode = Mode::Normal;
                            region.clear_from_cursor(&mut tw);
                            let info = render::render_line(
                                &mut tw,
                                &prompt_str,
                                prompt_display_len,
                                &line,
                                ui.cols,
                                render::RenderedRegion::default(),
                                &render::RenderOpts::default(),
                            );
                            region = info;
                            let _ = tw.flush_to_stdout();
                        }
                        HistoryAction::Cancel => {
                            line.set(saved_line);
                            ui.history_match_buffer = std::mem::take(matches);
                            history_cache.clear();
                            mode = Mode::Normal;
                            region.clear_from_cursor(&mut tw);
                            let info = render::render_line(
                                &mut tw,
                                &prompt_str,
                                prompt_display_len,
                                &line,
                                ui.cols,
                                render::RenderedRegion::default(),
                                &render::RenderOpts::default(),
                            );
                            region = info;
                            let _ = tw.flush_to_stdout();
                        }
                    },

                    Mode::DirPicker { entries, selected } => match (key.key, key.mods.ctrl) {
                        (Key::Escape, _) | (Key::Char('c'), true) => {
                            mode = Mode::Normal;
                            region.clear(&mut tw);
                            let info = render::render_line(
                                &mut tw,
                                &prompt_str,
                                prompt_display_len,
                                &line,
                                ui.cols,
                                render::RenderedRegion::default(),
                                &render::RenderOpts::default(),
                            );
                            region = info;
                            let _ = tw.flush_to_stdout();
                        }
                        (Key::Enter, _) => {
                            if let Some(dir) = entries.get(*selected) {
                                let dir = dir.clone();
                                eprintln!("{dir}");
                                super::app::change_directory(session, &dir);
                            }
                            mode = Mode::Normal;
                            region.clear(&mut tw);
                            let info = render::render_line(
                                &mut tw,
                                &prompt_str,
                                prompt_display_len,
                                &line,
                                ui.cols,
                                render::RenderedRegion::default(),
                                &render::RenderOpts::default(),
                            );
                            region = info;
                            let _ = tw.flush_to_stdout();
                        }
                        (Key::Up, _) if *selected > 0 => {
                            *selected -= 1;
                            region = render_dir_picker_mode(&mut tw, &mode, session, ui, region);
                            let _ = tw.flush_to_stdout();
                        }
                        (Key::Down, _) if *selected + 1 < entries.len() => {
                            *selected += 1;
                            region = render_dir_picker_mode(&mut tw, &mode, session, ui, region);
                            let _ = tw.flush_to_stdout();
                        }
                        _ => {}
                    },
                }
            }
        }
    }
}

fn render_prompt_region(
    tw: &mut TermWriter,
    session: &mut Session,
    ui: &mut Ui,
    line: &LineBuffer,
    prompt_str: &str,
    prompt_display_len: usize,
    region: render::RenderedRegion,
) -> render::RenderedRegion {
    let cmd_color = if !line.has_newlines() {
        let first = line.text().split_whitespace().next().unwrap_or("");
        if first.is_empty() {
            None
        } else if builtin::is_builtin(first)
            || session.aliases.get(first).is_some()
            || first.contains('/')
        {
            Some(true)
        } else {
            Some(ui.path_cache.contains(first, path_env(session)))
        }
    } else {
        None
    };

    let text = line.text();
    let suggestion = if text.len() >= 3 && !line.has_newlines() && line.cursor() == text.len() {
        session
            .history
            .session_prefix_search(text, 0)
            .and_then(|entry| entry.strip_prefix(text))
            .unwrap_or("")
    } else {
        ""
    };

    let opts = render::RenderOpts {
        cmd_color,
        suggestion,
    };
    render::render_line(
        tw,
        prompt_str,
        prompt_display_len,
        line,
        ui.cols,
        region,
        &opts,
    )
}

#[allow(clippy::too_many_arguments)]
fn render_active_mode(
    tw: &mut TermWriter,
    mode: &Mode,
    session: &mut Session,
    ui: &mut Ui,
    line: &LineBuffer,
    prompt_str: &str,
    prompt_display_len: usize,
    region: render::RenderedRegion,
    history_cache: &mut render::HistoryPagerCache,
) -> render::RenderedRegion {
    match mode {
        Mode::Normal => render_prompt_region(
            tw,
            session,
            ui,
            line,
            prompt_str,
            prompt_display_len,
            region,
        ),
        Mode::Completion { state, .. } => {
            let info = render::render_line(
                tw,
                prompt_str,
                prompt_display_len,
                line,
                ui.cols,
                region,
                &render::RenderOpts::default(),
            );
            render::render_completions(tw, state, info, true, ui.rows);
            info
        }
        Mode::HistorySearch { .. } => {
            render_history_mode(tw, mode, session, ui, region, history_cache)
        }
        Mode::DirPicker { .. } => render_dir_picker_mode(tw, mode, session, ui, region),
    }
}

fn update_mode_layout_for_resize(mode: &mut Mode, ui: &Ui) {
    if let Mode::Completion { state, .. } = mode {
        let (cols, rows) = complete::compute_grid(&state.comp.entries, ui.cols);
        state.cols = cols;
        state.rows = rows;
        state.term_cols = ui.cols;
        state.scroll = state.scroll.min(rows.saturating_sub(1));
    }
}

enum KeyAction {
    Continue,
    Execute(String),
    Continuation,
    Cancel,
    Exit,
    ClearScreen,
    StartHistorySearch,
    StartDirPicker,
    StartCompletion,
}

/// Prepares an edited buffer for execution. A backslash-newline is removed,
/// a newline inside quotes stays data, a newline after an operator continues
/// the command, and any other newline separates commands.
fn submit_text(input: &str) -> String {
    if !input.contains('\n') && !input.contains("\\\n") {
        return input.to_string();
    }
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_single = false;
    let mut in_double = false;
    while let Some(ch) = chars.next() {
        if in_single {
            result.push(ch);
            if ch == '\'' {
                in_single = false;
            }
            continue;
        }
        if ch == '\\' && chars.peek() == Some(&'\n') {
            chars.next();
            continue;
        }
        if in_double {
            result.push(ch);
            if ch == '"' {
                in_double = false;
            }
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                result.push(ch);
            }
            '"' => {
                in_double = true;
                result.push(ch);
            }
            '\n' => {
                let before = result.trim_end();
                let continues = before.is_empty() || before.ends_with(['|', '&', ';']);
                if !continues {
                    result.push(';');
                } else if !before.is_empty() && !result.ends_with(' ') {
                    result.push(' ');
                }
            }
            _ => result.push(ch),
        }
    }
    result
}

fn parsed_line_accepts(input: &str, aliases: &super::alias::AliasMap) -> bool {
    if super::shell::ShellParser::new(input).parse_line().is_err() {
        return false;
    }
    let first = input.split_whitespace().next().unwrap_or_default();
    aliases.get(first).is_some()
        || !matches!(first, "eval" | "exec" | "source" | "." | "trap" | "bg")
}

fn submitted_line_clears_screen(input: &str) -> bool {
    input.split_whitespace().next() == Some("c")
}

#[derive(Clone, PartialEq, Eq)]
struct UndoEntry {
    text: String,
    cursor: usize,
}

impl UndoEntry {
    fn from_line(line: &LineBuffer) -> Self {
        Self {
            text: line.text().to_string(),
            cursor: line.cursor(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditGroup {
    Typing,
    Other,
}

struct UndoState {
    undo: Vec<UndoEntry>,
    redo: Vec<UndoEntry>,
    group: Option<EditGroup>,
}

impl UndoState {
    fn new() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            group: None,
        }
    }

    fn break_group(&mut self) {
        self.group = None;
    }

    fn before_edit(&mut self, line: &LineBuffer, group: EditGroup) {
        if self.group == Some(group) && group == EditGroup::Typing {
            return;
        }
        let entry = UndoEntry::from_line(line);
        if self.undo.last() != Some(&entry) {
            self.undo.push(entry);
            if self.undo.len() > 256 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.group = Some(group);
    }

    fn undo(&mut self, line: &mut LineBuffer) -> bool {
        let Some(entry) = self.undo.pop() else {
            return false;
        };
        let current = UndoEntry::from_line(line);
        if self.redo.last() != Some(&current) {
            self.redo.push(current);
            if self.redo.len() > 256 {
                self.redo.remove(0);
            }
        }
        line.set_with_cursor(&entry.text, entry.cursor);
        self.group = None;
        true
    }

    fn redo(&mut self, line: &mut LineBuffer) -> bool {
        let Some(entry) = self.redo.pop() else {
            return false;
        };
        let current = UndoEntry::from_line(line);
        if self.undo.last() != Some(&current) {
            self.undo.push(current);
            if self.undo.len() > 256 {
                self.undo.remove(0);
            }
        }
        line.set_with_cursor(&entry.text, entry.cursor);
        self.group = None;
        true
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_normal_key(
    key: KeyEvent,
    line: &mut LineBuffer,
    history_idx: &mut Option<usize>,
    saved_line: &mut String,
    grid_column: &mut Option<usize>,
    prompt_display_len: usize,
    session: &Session,
    ui: &Ui,
    undo: &mut UndoState,
) -> KeyAction {
    if !matches!(key.key, Key::Up | Key::Down) || key.mods.ctrl {
        *grid_column = None;
    }

    match (key.key, key.mods.ctrl, key.mods.alt) {
        (Key::Char('_'), true, _) => {
            undo.undo(line);
            *history_idx = None;
            return KeyAction::Continue;
        }
        (Key::Char('r'), false, true) => {
            undo.redo(line);
            *history_idx = None;
            return KeyAction::Continue;
        }
        (Key::Char('c'), true, _) => return KeyAction::Cancel,
        (Key::Char('d'), true, _) => {
            if line.is_empty() {
                return KeyAction::Exit;
            }
            undo.before_edit(line, EditGroup::Other);
            line.delete_forward();
        }
        (Key::Char('a'), true, _) => {
            undo.break_group();
            line.move_home();
        }
        (Key::Char('e'), true, _) => {
            undo.break_group();
            line.move_end();
        }
        (Key::Char('k'), true, _) => {
            undo.before_edit(line, EditGroup::Other);
            line.kill_to_end();
        }
        (Key::Char('u'), true, _) => {
            undo.before_edit(line, EditGroup::Other);
            line.kill_to_start();
        }
        (Key::Char('w'), true, _) => {
            undo.before_edit(line, EditGroup::Other);
            line.kill_word_back();
        }
        (Key::Char('d'), false, true) => {
            undo.before_edit(line, EditGroup::Other);
            line.kill_word_forward();
        }
        (Key::Char('y'), true, _) => {
            undo.before_edit(line, EditGroup::Other);
            line.yank();
        }
        (Key::Char('l'), true, _) => return KeyAction::ClearScreen,
        (Key::Char('r'), true, _) => return KeyAction::StartHistorySearch,

        (Key::Char('b'), false, true) => {
            undo.break_group();
            line.move_word_left();
        }
        (Key::Char('f'), false, true) => {
            undo.break_group();
            line.move_word_right();
        }

        (Key::Up, _, _) if !key.mods.ctrl => {
            if line.has_newlines() && !line.on_first_line() {
                undo.break_group();
                line.move_line_up();
                *grid_column = None;
            } else if !line.has_newlines() {
                let col = *grid_column.get_or_insert_with(|| {
                    line.grid_cursor_position(prompt_display_len, ui.cols).1
                });
                if line.move_grid_up(prompt_display_len, ui.cols, col) {
                    undo.break_group();
                } else {
                    *grid_column = None;
                    navigate_history(line, history_idx, saved_line, session, true, undo);
                }
            } else {
                navigate_history(line, history_idx, saved_line, session, true, undo);
            }
        }
        (Key::Down, _, _) if !key.mods.ctrl => {
            if line.has_newlines() && !line.on_last_line() {
                undo.break_group();
                line.move_line_down();
                *grid_column = None;
            } else if !line.has_newlines() {
                let col = *grid_column.get_or_insert_with(|| {
                    line.grid_cursor_position(prompt_display_len, ui.cols).1
                });
                if line.move_grid_down(prompt_display_len, ui.cols, col) {
                    undo.break_group();
                } else {
                    *grid_column = None;
                    navigate_history(line, history_idx, saved_line, session, false, undo);
                }
            } else {
                navigate_history(line, history_idx, saved_line, session, false, undo);
            }
        }
        (Key::Left, _, _) if key.mods.ctrl || key.mods.alt => {
            undo.break_group();
            line.move_word_left();
        }
        (Key::Right, _, _) if key.mods.ctrl || key.mods.alt => {
            if line.cursor() >= line.text().len() && !line.has_newlines() {
                if let Some(entry) = session.history.session_prefix_search(line.text(), 0)
                    && let Some(suffix) = entry.strip_prefix(line.text())
                {
                    let take = suggestion_word_accept_len(suffix);
                    if take > 0 {
                        undo.before_edit(line, EditGroup::Other);
                        let mut owned = line.text().to_string();
                        owned.push_str(&suffix[..take]);
                        line.set(&owned);
                    }
                }
            } else {
                undo.break_group();
                line.move_word_right();
            }
        }
        (Key::Left, _, _) => {
            undo.break_group();
            line.move_left();
        }
        (Key::Right, _, _) => {
            if line.cursor() >= line.text().len() && !line.has_newlines() {
                // At end of line: accept the autosuggestion from history.
                if let Some(entry) = session.history.session_prefix_search(line.text(), 0) {
                    undo.before_edit(line, EditGroup::Other);
                    let owned = entry.to_string();
                    line.set(&owned);
                } else {
                    undo.break_group();
                    line.move_right();
                }
            } else {
                undo.break_group();
                line.move_right();
            }
        }
        (Key::Home, _, _) => {
            undo.break_group();
            line.move_home();
        }
        (Key::End, _, _) => {
            undo.break_group();
            line.move_end();
        }

        (Key::Backspace, true, _) => return KeyAction::StartDirPicker,
        (Key::Backspace, _, _) => {
            undo.before_edit(line, EditGroup::Other);
            line.delete_back();
            *history_idx = None;
        }
        (Key::Delete, true, _) => {
            undo.before_edit(line, EditGroup::Other);
            line.kill_word_back();
            *history_idx = None;
        }
        (Key::Delete, _, _) => {
            undo.before_edit(line, EditGroup::Other);
            line.delete_forward();
        }
        (Key::Tab, _, _) => {
            if line.is_empty() {
                undo.before_edit(line, EditGroup::Other);
                line.insert_str("cd ");
            }
            return KeyAction::StartCompletion;
        }
        (Key::Enter, _, _) => {
            if needs_continuation(line.text()) {
                return KeyAction::Continuation;
            }
            return KeyAction::Execute(line.text().to_string());
        }

        (Key::Char(c), false, false) => {
            undo.before_edit(line, EditGroup::Typing);
            let before = line.text().to_string();
            line.insert_char(c);
            if c == ' ' {
                try_alias_expand(line, &session.aliases);
            }
            if line.text() != before {
                *history_idx = None;
            }
        }

        _ => {}
    }

    KeyAction::Continue
}

fn suggestion_word_accept_len(suffix: &str) -> usize {
    if suffix.is_empty() {
        return 0;
    }
    let mut chars = suffix.char_indices();
    let Some((_, first)) = chars.next() else {
        return 0;
    };
    if first.is_whitespace() {
        let mut end = first.len_utf8();
        for (i, c) in chars {
            if c.is_whitespace() {
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        for (i, c) in suffix[end..].char_indices() {
            if c.is_whitespace() {
                return end + i;
            }
        }
        suffix.len()
    } else {
        for (i, c) in chars {
            if c.is_whitespace() {
                return i;
            }
        }
        suffix.len()
    }
}

fn navigate_history(
    line: &mut LineBuffer,
    history_idx: &mut Option<usize>,
    saved_line: &mut String,
    session: &Session,
    up: bool,
    undo: &mut UndoState,
) {
    let hist = &session.history;
    if hist.is_empty() {
        return;
    }

    if history_idx.is_none() {
        *saved_line = line.text().to_string();
    }

    let new_text = if up {
        let skip = history_idx.map(|i| i + 1).unwrap_or(0);
        let entry = if saved_line.is_empty() {
            hist.session_get(skip)
        } else {
            hist.session_prefix_search(saved_line, skip)
        };
        if let Some(e) = entry {
            *history_idx = Some(skip);
            Some(e.to_string())
        } else {
            None
        }
    } else {
        match history_idx {
            Some(0) | None => {
                *history_idx = None;
                Some(saved_line.clone())
            }
            Some(idx) => {
                *idx -= 1;
                let skip = *idx;
                let entry = if saved_line.is_empty() {
                    hist.session_get(skip)
                } else {
                    hist.session_prefix_search(saved_line, skip)
                };
                entry.map(|e| e.to_string())
            }
        }
    };
    if let Some(text) = new_text
        && text != line.text()
    {
        undo.before_edit(line, EditGroup::Other);
        line.set(&text);
    }
}

fn try_alias_expand(line: &mut LineBuffer, aliases: &AliasMap) {
    let text = line.text();
    // Only expand on the first space: if the trimmed text already contains a
    // space, the alias was already expanded or the user typed arguments.
    let trimmed = text.trim_end();
    if trimmed.contains(' ') {
        return;
    }

    let expanded = aliases.expand_line(text);
    if let std::borrow::Cow::Owned(new_text) = expanded {
        line.set(&new_text);
    }
}

enum CompletionAction {
    Navigate,
    Accept,
    Cancel,
    Refilter,
}

/// Byte offset of the start of the current shell command segment: the text
/// after the last unquoted `|`, `;`, or `&&` in `s`.
fn current_cmd_start(s: &str) -> usize {
    let mut in_single = false;
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                in_single = !in_single;
                i += 1;
            }
            b'|' | b';' if !in_single => {
                i += 1;
                start = i;
            }
            b'&' if !in_single && i + 1 < bytes.len() && bytes[i + 1] == b'&' => {
                i += 2;
                start = i;
            }
            _ => i += 1,
        }
    }
    start
}

/// Splits a shell command segment into tokens, stripping single-quote delimiters.
fn shell_tokens(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    for c in s.chars() {
        match c {
            '\'' => in_single = !in_single,
            ' ' | '\t' if !in_single => {
                if !cur.is_empty() {
                    tokens.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

/// Arguments already present in the current command before `word_start`,
/// skipping the command name. Used to filter duplicates from completions.
fn existing_cmd_args(before_cursor: &str, word_start: usize) -> Vec<String> {
    let prefix = &before_cursor[..word_start];
    let cmd_start = current_cmd_start(prefix);
    let tokens = shell_tokens(&prefix[cmd_start..]);
    if tokens.len() > 1 {
        tokens[1..].to_vec()
    } else {
        Vec::new()
    }
}

/// Removes completion entries whose full form (dir_prefix + name) is already
/// present in `existing`. Leaves the names arena intact; orphaned bytes are
/// harmless.
fn filter_existing_args(comp: &mut Completions, existing: &[String], dir_prefix: &str) {
    if existing.is_empty() {
        return;
    }
    let to_remove: Vec<bool> = comp
        .entries
        .iter()
        .map(|e| {
            let full = format!("{}{}", dir_prefix, comp.entry_name(e));
            // Directories are inserted with a trailing '/', so check both forms.
            existing
                .iter()
                .any(|a| *a == full || (e.is_dir() && a.strip_suffix('/') == Some(full.as_str())))
        })
        .collect();
    let mut idx = 0;
    comp.entries.retain(|_| {
        let remove = to_remove[idx];
        idx += 1;
        !remove
    });
}

/// Whether the cursor is at a command position: first word, after `|`, `&&`,
/// `;`, or immediately after sudo/doas/su.
fn is_command_position(before_cursor: &str, word_start: usize) -> bool {
    if word_start == 0 {
        return true;
    }
    let before_word = before_cursor[..word_start].trim_end();
    if before_word.ends_with('|') || before_word.ends_with(';') || before_word.ends_with("&&") {
        return true;
    }
    // The argument after sudo/doas/su is also a command.
    let prev_word = before_word
        .rsplit_once(|c: char| c.is_whitespace() || c == '|' || c == ';')
        .map(|(_, w)| w)
        .unwrap_or(before_word);
    matches!(prev_word, "sudo" | "doas" | "su")
}

/// Start of the completion word, respecting quotes. Returns
/// `(byte_offset_of_word_start, currently_inside_single_quote)`.
pub(super) fn find_comp_word_start(s: &str) -> (usize, bool) {
    let mut in_single = false;
    let mut in_double = false;
    let mut word_start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if !in_double => {
                in_single = !in_single;
                i += 1;
            }
            b'"' if !in_single => {
                in_double = !in_double;
                i += 1;
            }
            b'\\' if !in_single && i + 1 < bytes.len() => {
                i += 2;
            }
            b' ' | b'\t' | b'\n' if !in_single && !in_double => {
                i += 1;
                word_start = i;
            }
            _ => {
                i += 1;
            }
        }
    }
    (word_start, in_single)
}

/// Whether a completion string needs quoting for the shell.
fn needs_quoting(s: &str) -> bool {
    s.bytes().any(|b| {
        matches!(
            b,
            b' ' | b'\t'
                | b'('
                | b')'
                | b'$'
                | b'*'
                | b'?'
                | b'['
                | b']'
                | b'|'
                | b'&'
                | b'>'
                | b'<'
                | b';'
                | b'#'
                | b'\\'
                | b'"'
                | b'`'
        )
    })
}

fn completion_state(
    comp: Completions,
    term_cols: u16,
    dir_prefix: String,
    in_quote: bool,
) -> CompletionState {
    let (cols, rows) = complete::compute_grid(&comp.entries, term_cols);
    CompletionState {
        comp,
        selected: 0,
        cols,
        rows,
        scroll: 0,
        term_cols,
        dir_prefix,
        in_quote,
    }
}

/// If `word` ends in `$NAME` (a possibly empty run of identifier characters
/// after an unescaped `$`), the offset just past that `$`.
fn variable_prefix_end(word: &str) -> Option<usize> {
    let bytes = word.as_bytes();
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    if end == 0 || bytes[end - 1] != b'$' {
        return None;
    }
    let backslashes = bytes[..end - 1]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    (backslashes % 2 == 0).then_some(end)
}

pub(super) fn start_completion(
    line: &LineBuffer,
    term_cols: u16,
    session: &Session,
    mut comp: Completions,
) -> CompletionState {
    comp.clear();

    let home = home_string(session);
    let text = line.text();
    let before_cursor = &text[..line.cursor()];
    let (word_start, in_single) = find_comp_word_start(before_cursor);
    let existing = existing_cmd_args(before_cursor, word_start);
    let raw_word = &before_cursor[word_start..];

    // Strip quotes for filesystem lookup and track whether the word was
    // quoted. Handles an open quote ('path), a balanced quote ('path'), and a
    // tilde outside the quote (~/'path').
    let (partial, in_quote): (String, bool) = if in_single {
        (raw_word.strip_prefix('\'').unwrap_or(raw_word).into(), true)
    } else if let Some(rest) = raw_word.strip_prefix("~/'") {
        let unquoted = rest.strip_suffix('\'').unwrap_or(rest);
        (format!("~/{unquoted}"), true)
    } else if let Some(inner) = raw_word.strip_prefix('\'') {
        (inner.strip_suffix('\'').unwrap_or(inner).into(), true)
    } else {
        (raw_word.into(), false)
    };

    // `$NAME`: shell variables, exported or not.
    if !in_single && let Some(dollar) = variable_prefix_end(raw_word) {
        let name_prefix = &raw_word[dollar..];
        let mut names: Vec<&Vec<u8>> = session.env.keys().chain(session.vars.keys()).collect();
        names.sort_unstable();
        names.dedup();
        for name in names {
            if let Ok(name) = std::str::from_utf8(name)
                && name.starts_with(name_prefix)
            {
                comp.push(name, false, false, false);
            }
        }
        if !comp.is_empty() {
            return completion_state(comp, term_cols, raw_word[..dollar].to_string(), false);
        }
    }

    // Command position: first word or after |, &&, ;
    if !partial.is_empty()
        && !partial.contains('/')
        && is_command_position(before_cursor, word_start)
    {
        for b in builtin::all_builtin_names() {
            if b.starts_with(partial.as_str()) {
                comp.push(b, false, false, false);
            }
        }
        for (name, _) in session.aliases.iter() {
            if name.starts_with(partial.as_str()) {
                comp.push(name, false, false, false);
            }
        }
        super::path::complete_commands(&partial, &mut comp, path_env(session));
        // Directories are valid commands (implicit cd).
        complete::complete_path_into(&partial, true, &mut comp);
        comp.sort_entries();
        comp.dedup_sorted();
        return completion_state(comp, term_cols, String::new(), false);
    }

    // `cd` completes only directories.
    let first_command_word = text.split_whitespace().next().unwrap_or("");
    let dirs_only = first_command_word == "cd" && word_start > 0;

    // SSH-aware completion: hostname and remote path.
    const SSH_CMDS: &[&str] = &["ssh", "scp", "rsync", "sftp", "mosh"];
    if word_start > 0 && SSH_CMDS.contains(&first_command_word) {
        if let Some(colon_pos) = partial.find(':') {
            let host = &partial[..colon_pos];
            let remote_path = &partial[colon_pos + 1..];
            complete::complete_remote_path(host, remote_path, &mut comp, &session.env);
            if !comp.is_empty() {
                let dir_prefix = if let Some(slash) = remote_path.rfind('/') {
                    format!("{}:{}", host, &remote_path[..=slash])
                } else {
                    format!("{host}:")
                };
                return completion_state(comp, term_cols, dir_prefix, in_quote);
            }
        } else if !partial.contains('/') {
            // No colon, no slash: hostname completion plus local files.
            complete::complete_hostnames(&partial, &home, &mut comp);
            complete::complete_path_into(&partial, false, &mut comp);
            comp.sort_entries();
            comp.dedup_sorted();
            filter_existing_args(&mut comp, &existing, "");
            if !comp.is_empty() {
                return completion_state(comp, term_cols, String::new(), in_quote);
            }
        }
    }

    // Expand tilde for filesystem lookup.
    let expanded = if partial == "~" {
        format!("{home}/")
    } else if let Some(rest) = partial.strip_prefix("~/") {
        format!("{home}/{rest}")
    } else {
        partial.clone()
    };

    // dir_prefix keeps the original (unexpanded) form for accept_completion.
    let dir_prefix = if let Some(slash_pos) = partial.rfind('/') {
        partial[..=slash_pos].to_string()
    } else {
        String::new()
    };

    complete::complete_path_into(&expanded, dirs_only, &mut comp);
    filter_existing_args(&mut comp, &existing, &dir_prefix);
    if !comp.is_empty() {
        return completion_state(comp, term_cols, dir_prefix, in_quote);
    }

    // Fish-style partial path completion: each intermediate component is a
    // prefix, e.g. "~/de/s" resolves "de" to "dev" and completes "s" in ~/dev/.
    let (partial_comp, groups) = complete::complete_partial_path(&expanded, dirs_only);
    if !groups.is_empty() {
        // The user-facing root prefix, for tilde re-contraction.
        let (user_root, expanded_root) = if partial.starts_with("~/") || partial == "~" {
            ("~/".to_string(), format!("{home}/"))
        } else if partial.starts_with('/') {
            ("/".to_string(), "/".to_string())
        } else {
            (String::new(), String::new())
        };

        // Build entries with the resolved intermediate components in the
        // name. comp is already empty, so reuse its arena for the combined
        // "rel_dir/name" strings.
        for (resolved_dir, start, count) in &groups {
            let rel_dir = if expanded_root.is_empty() {
                resolved_dir.as_str()
            } else {
                resolved_dir
                    .strip_prefix(&expanded_root)
                    .unwrap_or(resolved_dir)
            };
            for i in *start..*start + *count {
                let entry = &partial_comp.entries[i];
                let orig_name = partial_comp.entry_name(entry);
                let mark = comp.begin_entry();
                comp.names.push_str(rel_dir);
                comp.names.push_str(orig_name);
                comp.finish_entry(mark, entry.is_dir(), entry.is_link(), entry.is_exec());
            }
        }

        filter_existing_args(&mut comp, &existing, &user_root);
        return completion_state(comp, term_cols, user_root, in_quote);
    }

    // Nothing found: return an empty state, preserving the buffer for reuse.
    CompletionState {
        comp,
        selected: 0,
        cols: 0,
        rows: 0,
        scroll: 0,
        term_cols,
        dir_prefix: String::new(),
        in_quote,
    }
}

fn handle_completion_key(
    key: KeyEvent,
    state: &mut CompletionState,
    line: &mut LineBuffer,
) -> CompletionAction {
    match key.key {
        Key::Up => {
            state.move_up();
            CompletionAction::Navigate
        }
        Key::Down | Key::Tab => {
            state.move_down();
            CompletionAction::Navigate
        }
        Key::Left => {
            state.move_left();
            CompletionAction::Navigate
        }
        Key::Right => {
            state.move_right();
            CompletionAction::Navigate
        }
        Key::Enter => CompletionAction::Accept,
        Key::Escape => CompletionAction::Cancel,
        Key::Char('c') if key.mods.ctrl => CompletionAction::Cancel,
        Key::Char(c) if !key.mods.ctrl && !key.mods.alt => {
            line.insert_char(c);
            CompletionAction::Refilter
        }
        Key::Backspace => {
            if line.cursor() > 0 {
                line.delete_back();
                CompletionAction::Refilter
            } else {
                CompletionAction::Cancel
            }
        }
        _ => CompletionAction::Cancel,
    }
}

fn preview_completion(line: &mut LineBuffer, state: &CompletionState, base_line: &LineBuffer) {
    let Some(entry) = state.selected_entry() else {
        *line = base_line.clone();
        return;
    };
    let Some(name) = state.selected_name() else {
        *line = base_line.clone();
        return;
    };

    let text = base_line.text().to_string();
    let before_cursor = &text[..base_line.cursor()];
    let (word_start, _) = find_comp_word_start(before_cursor);
    let after_cursor = &text[base_line.cursor()..];

    let mut inner = state.dir_prefix.clone();
    inner.push_str(name);
    if entry.is_dir() {
        inner.push('/');
    } else if entry.is_host() {
        inner.push(':');
    }

    // Single-quote the completion if it contains special characters. Always
    // close the quote so the line is valid for immediate execution. On the
    // next Tab press start_completion strips the outer quotes for lookup.
    // Keep ~/ outside the quotes so tilde expansion still works. Embedded
    // single quotes become '\'' (end quote, escaped quote, reopen).
    let variable = state.dir_prefix.ends_with('$');
    let replacement = if variable {
        inner
    } else if state.in_quote || needs_quoting(&inner) || inner.contains('\'') {
        let escaped = inner.replace('\'', "'\\''");
        if let Some(rest) = escaped.strip_prefix("~/") {
            format!("~/'{rest}'")
        } else {
            format!("'{escaped}'")
        }
    } else {
        inner
    };

    let new_text = format!("{}{}{}", &text[..word_start], replacement, after_cursor);
    let new_cursor = word_start + replacement.len();
    line.set_with_cursor(&new_text, new_cursor);
}

enum HistoryAction {
    Continue,
    Accept(String),
    Cancel,
}

#[allow(clippy::too_many_arguments)]
fn handle_history_search_key(
    key: KeyEvent,
    query: &mut LineBuffer,
    matches: &mut Vec<FuzzyMatch>,
    candidates: &mut Vec<usize>,
    scratch: &mut Vec<usize>,
    candidate_stack: &mut Vec<(usize, Vec<usize>)>,
    selected: &mut usize,
    session: &Session,
) -> HistoryAction {
    let mut re_search = false;
    let prev_text = query.text().to_string();
    let prev_cursor = query.cursor();
    match (key.key, key.mods.ctrl, key.mods.alt) {
        (Key::Escape, _, _) => return HistoryAction::Cancel,
        (Key::Char('c'), true, _) => return HistoryAction::Cancel,
        (Key::Enter, _, _) => {
            return if let Some(m) = matches.get(*selected) {
                HistoryAction::Accept(session.history.get(m.entry_idx).to_string())
            } else {
                HistoryAction::Cancel
            };
        }
        (Key::Up, _, _) | (Key::Char('p'), true, _) if *selected > 0 => {
            *selected -= 1;
        }
        (Key::Down, _, _) | (Key::Char('n'), true, _) if *selected + 1 < matches.len() => {
            *selected += 1;
        }

        // Query editing: cursor movement.
        (Key::Left, _, false) if key.mods.ctrl => {
            query.move_word_left();
        }
        (Key::Right, _, false) if key.mods.ctrl => {
            query.move_word_right();
        }
        (Key::Left, _, _) => {
            query.move_left();
        }
        (Key::Right, _, _) => {
            query.move_right();
        }
        (Key::Home, _, _) | (Key::Char('a'), true, _) => query.move_home(),
        (Key::End, _, _) | (Key::Char('e'), true, _) => query.move_end(),
        (Key::Char('b'), _, true) => query.move_word_left(),
        (Key::Char('f'), _, true) => query.move_word_right(),

        // Query editing: text modification.
        (Key::Backspace, _, _) => {
            query.delete_back();
            re_search = true;
        }
        (Key::Delete, true, _) | (Key::Char('d'), false, true) => {
            query.kill_word_back();
            re_search = true;
        }
        (Key::Delete, _, _) | (Key::Char('d'), true, _) => {
            query.delete_forward();
            re_search = true;
        }
        (Key::Char('u'), true, _) => {
            query.kill_to_start();
            re_search = true;
        }
        (Key::Char('k'), true, _) => {
            query.kill_to_end();
            re_search = true;
        }
        (Key::Char('w'), true, _) => {
            query.kill_word_back();
            re_search = true;
        }
        (Key::Char('y'), true, _) => {
            query.yank();
            re_search = true;
        }
        (Key::Char(c), false, false) => {
            query.insert_char(c);
            re_search = true;
        }

        _ => {}
    }
    if re_search {
        let new_text = query.text();
        let append_at_end = prev_cursor == prev_text.len()
            && query.cursor() == new_text.len()
            && new_text.len() > prev_text.len()
            && new_text.starts_with(&prev_text);
        let delete_at_end = prev_cursor == prev_text.len()
            && query.cursor() == new_text.len()
            && new_text.len() < prev_text.len()
            && prev_text.starts_with(new_text);
        if append_at_end {
            candidate_stack.push((prev_text.len(), std::mem::take(candidates)));
            session.history.fuzzy_search_subset_into_in_dir(
                new_text,
                &candidate_stack.last().unwrap().1,
                scratch,
                matches,
                200,
                &session.cwd,
            );
            std::mem::swap(candidates, scratch);
        } else if delete_at_end {
            while candidate_stack
                .last()
                .is_some_and(|(len, _)| *len > new_text.len())
            {
                if let Some((_, old_candidates)) = candidate_stack.pop() {
                    *scratch = old_candidates;
                }
            }
            if let Some((len, _)) = candidate_stack.last()
                && *len == new_text.len()
            {
                let (_, old_candidates) = candidate_stack.pop().unwrap();
                *candidates = old_candidates;
                session.history.fuzzy_search_subset_into_in_dir(
                    new_text,
                    candidates,
                    scratch,
                    matches,
                    200,
                    &session.cwd,
                );
            } else {
                candidate_stack.clear();
                session.history.visible_entry_indices_into(scratch);
                session.history.fuzzy_search_subset_into_in_dir(
                    new_text,
                    scratch,
                    candidates,
                    matches,
                    200,
                    &session.cwd,
                );
                std::mem::swap(candidates, scratch);
            }
        } else {
            candidate_stack.clear();
            session.history.visible_entry_indices_into(scratch);
            session.history.fuzzy_search_subset_into_in_dir(
                new_text,
                scratch,
                candidates,
                matches,
                200,
                &session.cwd,
            );
            std::mem::swap(candidates, scratch);
        }
        *selected = 0;
    }
    HistoryAction::Continue
}

fn render_history_mode(
    tw: &mut TermWriter,
    mode: &Mode,
    session: &Session,
    ui: &Ui,
    prev: render::RenderedRegion,
    cache: &mut render::HistoryPagerCache,
) -> render::RenderedRegion {
    if let Mode::HistorySearch {
        query,
        matches,
        selected,
        ..
    } = mode
    {
        render::render_history_pager_cached(
            tw,
            query.text(),
            matches,
            &session.history,
            *selected,
            ui.rows,
            ui.cols,
            query.display_cursor_pos(),
            prev,
            cache,
        )
    } else {
        render::RenderedRegion::default()
    }
}

fn render_dir_picker_mode(
    tw: &mut TermWriter,
    mode: &Mode,
    session: &Session,
    ui: &Ui,
    prev: render::RenderedRegion,
) -> render::RenderedRegion {
    if let Mode::DirPicker { entries, selected } = mode {
        render::render_dir_picker(
            tw,
            entries,
            *selected,
            &home_string(session),
            ui.rows,
            ui.cols,
            prev,
        )
    } else {
        render::RenderedRegion::default()
    }
}

/// Whether the input needs a continuation line (open quotes, trailing
/// operator, and so on).
fn needs_continuation(input: &str) -> bool {
    let trimmed = input.trim_end();
    if trimmed.is_empty() {
        return false;
    }
    let (in_single, in_double, escape) = scan_quote_state(trimmed.as_bytes());
    if in_single || in_double || escape {
        return true;
    }
    trimmed.ends_with('|') || trimmed.ends_with("&&") || trimmed.ends_with("||")
}

fn scan_quote_state(input: &[u8]) -> (bool, bool, bool) {
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    for &b in input {
        if escape {
            escape = false;
            continue;
        }
        match b {
            b'\\' if !in_single => escape = true,
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            _ => {}
        }
    }
    (in_single, in_double, escape)
}

/// Handles Ctrl+D on an empty line. A stopped job makes the first attempt a
/// warning; the second force-quits.
fn handle_exit(session: &mut Session, ui: &mut Ui) -> ReadResult {
    if session.job.is_some() {
        if ui.exit_warned {
            super::app::terminate_job(session);
            ReadResult::Exit
        } else {
            // Raw mode does not translate newlines, so return the carriage explicitly.
            eprint!("xshi: there is a suspended job. Press Ctrl+D again to force quit.\r\n");
            ui.exit_warned = true;
            ReadResult::Empty
        }
    } else {
        ReadResult::Exit
    }
}

#[cfg(test)]
mod tests {
    use super::{
        EditGroup, LineBuffer, UndoState, submit_text, suggestion_word_accept_len,
        variable_prefix_end,
    };

    #[test]
    fn variable_prefix_ends_just_after_an_unescaped_dollar() {
        assert_eq!(variable_prefix_end("$"), Some(1));
        assert_eq!(variable_prefix_end("$HO"), Some(1));
        assert_eq!(variable_prefix_end("\"a=$HO"), Some(4));
        assert_eq!(
            variable_prefix_end("\\\\$HO"),
            Some(3),
            "an escaped backslash does not escape the dollar"
        );
        assert_eq!(variable_prefix_end("\\$HO"), None);
        assert_eq!(variable_prefix_end("HOME"), None);
        assert_eq!(variable_prefix_end("$HOME/"), None);
        assert_eq!(variable_prefix_end("$HOME$"), Some(6));
    }

    #[test]
    fn suggestion_word_accept_len_splits_words_and_paths() {
        assert_eq!(suggestion_word_accept_len(""), 0);
        assert_eq!(
            suggestion_word_accept_len("checkout feature"),
            "checkout".len()
        );
        assert_eq!(
            suggestion_word_accept_len(" checkout"),
            " checkout".len()
        );
        assert_eq!(suggestion_word_accept_len("  --flag"), "  --flag".len());
        assert_eq!(
            suggestion_word_accept_len("src/main.rs"),
            "src/main.rs".len()
        );
    }

    #[test]
    fn typing_group_coalesces_into_one_undo_step() {
        let mut line = LineBuffer::new();
        let mut undo = UndoState::new();
        for c in "echo".chars() {
            undo.before_edit(&line, EditGroup::Typing);
            line.insert_char(c);
        }
        assert_eq!(undo.undo.len(), 1);
        assert!(undo.undo(&mut line));
        assert!(line.text().is_empty());
        assert!(undo.redo(&mut line));
        assert_eq!(line.text(), "echo");
    }

    #[test]
    fn undo_and_redo_are_bounded_to_256_states() {
        let mut line = LineBuffer::new();
        let mut undo = UndoState::new();
        for _ in 0..300 {
            undo.before_edit(&line, EditGroup::Other);
            line.insert_char('x');
        }
        assert_eq!(undo.undo.len(), 256);
        for _ in 0..300 {
            undo.undo(&mut line);
        }
        assert_eq!(line.text(), "x".repeat(44));
        assert_eq!(undo.redo.len(), 256);
        for _ in 0..300 {
            undo.redo(&mut line);
        }
        assert_eq!(line.text(), "x".repeat(300));
    }

    #[test]
    fn submit_text_follows_shell_line_continuation_rules() {
        assert_eq!(
            submit_text("echo first\necho second"),
            "echo first;echo second"
        );
        assert_eq!(submit_text("echo one \\\ntwo"), "echo one two");
        assert_eq!(submit_text("true &&\necho ok"), "true && echo ok");
        assert_eq!(
            submit_text("echo \"open\nclose\""),
            "echo \"open\nclose\""
        );
        assert_eq!(submit_text("printf '%s' 'a\nb'"), "printf '%s' 'a\nb'");
    }
}
