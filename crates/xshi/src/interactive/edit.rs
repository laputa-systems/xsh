#![allow(clippy::single_call_fn)]

use super::complete::{self, CompletionRequest, CompletionState};
use super::history::FuzzyMatch;
use super::prompt::prompt;
use super::render::{self, RenderOpts, RenderedRegion};
use super::session::Session;
use super::shell::lex_shell;
use rustix::termios::{self as rtermios, OptionalActions, OutputModes, Termios};
use rustix::{event as revent, io as rio, stdio as rstdio};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::{self, Read, Write};

const HISTORY_SEARCH_LIMIT: usize = 200;
const ESCAPE_SEQUENCE_TIMEOUT_MS: i32 = 50;

pub(super) struct RawMode {
    original: Termios,
    raw: Termios,
    enabled: bool,
}

impl RawMode {
    pub(super) fn enter() -> io::Result<Self> {
        let original = rtermios::tcgetattr(rstdio::stdin()).map_err(io::Error::from)?;
        let mut raw = original.clone();
        raw.make_raw();
        raw.output_modes |= OutputModes::OPOST;
        rtermios::tcsetattr(rstdio::stdin(), OptionalActions::Flush, &raw)
            .map_err(io::Error::from)?;
        let mut stdout = io::stdout();
        stdout.write_all(b"\x1b[?2004h")?;
        stdout.flush()?;
        Ok(Self {
            original,
            raw,
            enabled: true,
        })
    }

    pub(super) fn suspend(&mut self) -> io::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        let mut stdout = io::stdout();
        stdout.write_all(b"\x1b[?2004l")?;
        stdout.flush()?;
        rtermios::tcsetattr(rstdio::stdin(), OptionalActions::Now, &self.original)
            .map_err(io::Error::from)?;
        self.enabled = false;
        Ok(())
    }

    pub(super) fn resume(&mut self) -> io::Result<()> {
        if self.enabled {
            return Ok(());
        }
        rtermios::tcsetattr(rstdio::stdin(), OptionalActions::Now, &self.raw)
            .map_err(io::Error::from)?;
        let mut stdout = io::stdout();
        stdout.write_all(b"\x1b[?2004h")?;
        stdout.flush()?;
        self.enabled = true;
        Ok(())
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = io::stdout().write_all(b"\x1b[?2004l");
        let _ = io::stdout().flush();
        let _ = rtermios::tcsetattr(rstdio::stdin(), OptionalActions::Now, &self.original);
    }
}

pub(super) enum EditorEvent {
    Submit(String),
    Cancel,
    Eof,
    Error(io::Error),
}

#[derive(Debug, Eq, PartialEq)]
enum EditorMode {
    Normal,
    Completion,
    HistorySearch,
}

struct HistorySearch {
    saved_line: LineBuffer,
    query: LineBuffer,
    matches: Vec<FuzzyMatch>,
    candidates: Vec<usize>,
    scratch: Vec<usize>,
    candidate_stack: Vec<(usize, Vec<usize>)>,
    selected: usize,
}

struct HistoryNavigation {
    saved_line: LineBuffer,
    prefix: String,
    index: usize,
}

enum HistoryAction {
    Continue,
    Accept(String),
    Cancel,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct LineBuffer {
    pub(super) text: String,
    pub(super) cursor: usize,
    yank: String,
    preferred_column: Option<usize>,
}

impl LineBuffer {
    pub(super) fn from_text(text: &str) -> Self {
        Self {
            text: text.to_string(),
            cursor: text.len(),
            ..Self::default()
        }
    }

    pub(super) fn insert(&mut self, text: &str) {
        self.text.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.preferred_column = None;
    }

    fn set_with_cursor(&mut self, text: &str, cursor: usize) {
        self.text.clear();
        self.text.push_str(text);
        self.cursor = cursor.min(self.text.len());
        self.preferred_column = None;
    }

    pub(super) fn display_cursor_pos(&self) -> usize {
        complete::str_width(&self.text[..self.cursor])
    }

    pub(super) fn display_len(&self) -> usize {
        complete::str_width(&self.text)
    }

    pub(super) fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_boundary(self.cursor);
        self.text.drain(prev..self.cursor);
        self.cursor = prev;
        self.preferred_column = None;
    }

    fn delete_forward(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.next_boundary(self.cursor);
        self.text.drain(self.cursor..next);
        self.preferred_column = None;
    }

    pub(super) fn move_left(&mut self) {
        self.cursor = self.prev_boundary(self.cursor);
        self.preferred_column = None;
    }

    fn move_right(&mut self) {
        self.cursor = self.next_boundary(self.cursor);
        self.preferred_column = None;
    }

    fn move_word_left(&mut self) {
        let mut cursor = self.cursor;
        while cursor > 0 {
            let prev = self.prev_boundary(cursor);
            if !self.text[prev..cursor].chars().all(char::is_whitespace) {
                break;
            }
            cursor = prev;
        }
        while cursor > 0 {
            let prev = self.prev_boundary(cursor);
            if self.text[prev..cursor].chars().all(char::is_whitespace) {
                break;
            }
            cursor = prev;
        }
        self.cursor = cursor;
        self.preferred_column = None;
    }

    fn move_word_right(&mut self) {
        let mut cursor = self.cursor;
        while cursor < self.text.len() {
            let next = self.next_boundary(cursor);
            if !self.text[cursor..next].chars().all(char::is_whitespace) {
                break;
            }
            cursor = next;
        }
        while cursor < self.text.len() {
            let next = self.next_boundary(cursor);
            if self.text[cursor..next].chars().all(char::is_whitespace) {
                break;
            }
            cursor = next;
        }
        self.cursor = cursor;
        self.preferred_column = None;
    }

    fn kill_to_end(&mut self) {
        self.yank = self.text[self.cursor..].to_string();
        self.text.truncate(self.cursor);
    }

    fn kill_to_start(&mut self) {
        self.yank = self.text[..self.cursor].to_string();
        self.text.drain(..self.cursor);
        self.cursor = 0;
    }

    fn kill_word_backward(&mut self) {
        let end = self.cursor;
        self.move_word_left();
        self.yank = self.text[self.cursor..end].to_string();
        self.text.drain(self.cursor..end);
    }

    fn yank(&mut self) {
        let text = self.yank.clone();
        self.insert(&text);
    }

    fn prev_boundary(&self, cursor: usize) -> usize {
        self.text[..cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn next_boundary(&self, cursor: usize) -> usize {
        self.text[cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| cursor + index)
            .unwrap_or(self.text.len())
    }
}

pub(super) fn read_interactive_command(
    session: &mut Session,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> EditorEvent {
    let mut prompt_text = prompt(session);
    let mut reader = io::stdin().lock();
    let mut command = match read_editor_line(
        session,
        stdout,
        &prompt_text,
        LineBuffer::default(),
        &mut reader,
    ) {
        EditorEvent::Submit(line) => line,
        other => return other,
    };
    loop {
        let Some(next) = continuation_prompt(&command) else {
            let _ = stderr.flush();
            return EditorEvent::Submit(command);
        };
        prompt_text = next.to_string();
        match read_editor_line(
            session,
            stdout,
            &prompt_text,
            LineBuffer::default(),
            &mut reader,
        ) {
            EditorEvent::Submit(line) => {
                command = join_continuation(command, line);
            }
            other => return other,
        }
    }
}

fn read_editor_line(
    session: &mut Session,
    stdout: &mut dyn Write,
    prompt_text: &str,
    mut buffer: LineBuffer,
    reader: &mut dyn Read,
) -> EditorEvent {
    let mut history_nav = None;
    let mut active_completion: Option<(LineBuffer, CompletionState)> = None;
    let mut active_history: Option<HistorySearch> = None;
    let mut mode = EditorMode::Normal;
    let mut completion_rows = 0_usize;
    let mut region = RenderedRegion::default();
    let (_, mut cols) = render::term_size();
    if let Ok(next) = render_editor(session, stdout, prompt_text, &buffer, region, cols) {
        region = next;
    } else {
        return EditorEvent::Error(io::Error::last_os_error());
    }
    loop {
        let byte = match read_stdin_byte_blocking() {
            Ok(Some(byte)) => byte,
            Ok(None) => return EditorEvent::Eof,
            Err(err) => return EditorEvent::Error(err),
        };
        let (rows, next_cols) = render::term_size();
        cols = next_cols;
        if mode == EditorMode::HistorySearch {
            let Some(search) = active_history.as_mut() else {
                mode = EditorMode::Normal;
                continue;
            };
            match handle_history_search_key(session, reader, byte, search) {
                HistoryAction::Continue => {
                    match render_history_search(
                        stdout,
                        &search.query,
                        search,
                        session,
                        region,
                        rows,
                        cols,
                    ) {
                        Ok(next) => region = next,
                        Err(_) => return EditorEvent::Error(io::Error::last_os_error()),
                    }
                    continue;
                }
                HistoryAction::Accept(text) => {
                    buffer = LineBuffer::from_text(&text);
                    active_history = None;
                    mode = EditorMode::Normal;
                    match render_editor(session, stdout, prompt_text, &buffer, region, cols) {
                        Ok(next) => region = next,
                        Err(_) => return EditorEvent::Error(io::Error::last_os_error()),
                    }
                    continue;
                }
                HistoryAction::Cancel => {
                    buffer = search.saved_line.clone();
                    active_history = None;
                    mode = EditorMode::Normal;
                    match render_editor(session, stdout, prompt_text, &buffer, region, cols) {
                        Ok(next) => region = next,
                        Err(_) => return EditorEvent::Error(io::Error::last_os_error()),
                    }
                    continue;
                }
            }
        }
        if mode == EditorMode::Completion {
            match handle_active_completion_key(
                session,
                stdout,
                prompt_text,
                cols,
                byte,
                reader,
                &mut buffer,
                &mut active_completion,
                &mut completion_rows,
                &mut region,
            ) {
                Ok(true) => {
                    if active_completion.is_none() {
                        mode = EditorMode::Normal;
                    }
                    continue;
                }
                Ok(false) => mode = EditorMode::Normal,
                Err(event) => return event,
            }
        }
        match byte {
            b'\r' | b'\n' => {
                let _ = write!(stdout, "\r\n");
                return EditorEvent::Submit(buffer.text);
            }
            0x03 => {
                let _ = write!(stdout, "^C\r\n");
                return EditorEvent::Cancel;
            }
            0x04 if buffer.text.is_empty() => return EditorEvent::Eof,
            0x04 => buffer.delete_forward(),
            0x01 => buffer.cursor = 0,
            0x05 => buffer.cursor = buffer.text.len(),
            0x0b => buffer.kill_to_end(),
            0x15 => buffer.kill_to_start(),
            0x17 => buffer.kill_word_backward(),
            0x19 => buffer.yank(),
            0x0c => {
                let _ = write!(stdout, "\x1b[H\x1b[2J");
            }
            b'\t' => {
                if let CompletionAction::Display(state) =
                    complete_buffer(session, &mut buffer, cols)
                {
                    completion_rows =
                        render::render_completions(stdout, &state, region, true, 0).unwrap_or(0);
                    active_completion = Some((buffer.clone(), state));
                    mode = EditorMode::Completion;
                    continue;
                }
            }
            0x7f | 0x08 => buffer.backspace(),
            0x12 => {
                let search = HistorySearch::new(session, buffer.clone());
                match render_history_search(
                    stdout,
                    &search.query,
                    &search,
                    session,
                    region,
                    rows,
                    cols,
                ) {
                    Ok(next) => {
                        region = next;
                        active_history = Some(search);
                        mode = EditorMode::HistorySearch;
                        continue;
                    }
                    Err(_) => return EditorEvent::Error(io::Error::last_os_error()),
                }
            }
            0x1b => handle_escape(session, reader, &mut buffer, &mut history_nav),
            byte if byte >= 0x20 => match read_utf8_text(reader, byte) {
                Ok(text) => {
                    buffer.insert(&text);
                    if text == " " {
                        try_alias_expand(&mut buffer, &session.aliases);
                    }
                    history_nav = None;
                }
                Err(event) => return event,
            },
            _ => {}
        }
        if let Ok(next) = render_editor(session, stdout, prompt_text, &buffer, region, cols) {
            region = next;
        } else {
            return EditorEvent::Error(io::Error::last_os_error());
        }
    }
}

fn try_alias_expand(buffer: &mut LineBuffer, aliases: &BTreeMap<String, String>) {
    let trimmed = buffer.text.trim_end();
    if trimmed.contains(char::is_whitespace) {
        return;
    }

    if let Cow::Owned(text) = expand_alias_line(&buffer.text, aliases) {
        buffer.set_with_cursor(&text, text.len());
    }
}

fn expand_alias_line<'a>(line: &'a str, aliases: &BTreeMap<String, String>) -> Cow<'a, str> {
    let trimmed = line.trim_start();
    let Some(first_word) = trimmed.split_whitespace().next() else {
        return Cow::Borrowed(line);
    };
    let Some(expansion) = aliases.get(first_word) else {
        return Cow::Borrowed(line);
    };
    let leading_ws = &line[..line.len() - trimmed.len()];
    let rest = &trimmed[first_word.len()..];
    Cow::Owned(format!("{leading_ws}{expansion}{rest}"))
}

#[allow(clippy::too_many_arguments)]
fn handle_active_completion_key(
    session: &Session,
    stdout: &mut dyn Write,
    prompt_text: &str,
    cols: u16,
    byte: u8,
    reader: &mut dyn Read,
    buffer: &mut LineBuffer,
    active_completion: &mut Option<(LineBuffer, CompletionState)>,
    completion_rows: &mut usize,
    region: &mut RenderedRegion,
) -> Result<bool, EditorEvent> {
    let Some((mut base, mut state)) = active_completion.take() else {
        return Ok(false);
    };
    match byte {
        b'\t' => {
            state.move_down();
            render_completion_preview(
                stdout,
                prompt_text,
                cols,
                buffer,
                &base,
                &state,
                completion_rows,
                region,
            )?;
            *active_completion = Some((base, state));
        }
        b'\r' | b'\n' => {
            preview_completion(buffer, &state, &base);
            let _ = render::clear_completions(stdout, *region, *completion_rows);
            *completion_rows = 0;
            *region = render_editor(session, stdout, prompt_text, buffer, *region, cols)
                .map_err(|_| EditorEvent::Error(io::Error::last_os_error()))?;
        }
        0x1b => {
            if handle_completion_escape(reader, &mut state) {
                render_completion_preview(
                    stdout,
                    prompt_text,
                    cols,
                    buffer,
                    &base,
                    &state,
                    completion_rows,
                    region,
                )?;
                *active_completion = Some((base, state));
            } else {
                cancel_completion(
                    session,
                    stdout,
                    prompt_text,
                    cols,
                    buffer,
                    base,
                    completion_rows,
                    region,
                )?;
            }
        }
        0x03 => {
            cancel_completion(
                session,
                stdout,
                prompt_text,
                cols,
                buffer,
                base,
                completion_rows,
                region,
            )?;
        }
        0x7f | 0x08 => {
            if base.cursor > 0 {
                base.backspace();
                refilter_completion(
                    session,
                    stdout,
                    prompt_text,
                    cols,
                    buffer,
                    base,
                    state,
                    active_completion,
                    completion_rows,
                    region,
                )?;
            } else {
                cancel_completion(
                    session,
                    stdout,
                    prompt_text,
                    cols,
                    buffer,
                    base,
                    completion_rows,
                    region,
                )?;
            }
        }
        byte if byte >= 0x20 => {
            let text = read_utf8_text(reader, byte)?;
            base.insert(&text);
            refilter_completion(
                session,
                stdout,
                prompt_text,
                cols,
                buffer,
                base,
                state,
                active_completion,
                completion_rows,
                region,
            )?;
        }
        _ => {
            *buffer = base;
            let _ = render::clear_completions(stdout, *region, *completion_rows);
            *completion_rows = 0;
            return Ok(false);
        }
    }
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn render_completion_preview(
    stdout: &mut dyn Write,
    prompt_text: &str,
    cols: u16,
    buffer: &mut LineBuffer,
    base: &LineBuffer,
    state: &CompletionState,
    completion_rows: &mut usize,
    region: &mut RenderedRegion,
) -> Result<(), EditorEvent> {
    *buffer = base.clone();
    preview_completion(buffer, state, base);
    *region = render_editor_without_suggestion(stdout, prompt_text, buffer, *region, cols)
        .map_err(|_| EditorEvent::Error(io::Error::last_os_error()))?;
    *completion_rows = render::render_completions(stdout, state, *region, false, *completion_rows)
        .unwrap_or(*completion_rows);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cancel_completion(
    session: &Session,
    stdout: &mut dyn Write,
    prompt_text: &str,
    cols: u16,
    buffer: &mut LineBuffer,
    base: LineBuffer,
    completion_rows: &mut usize,
    region: &mut RenderedRegion,
) -> Result<(), EditorEvent> {
    *buffer = base;
    let _ = render::clear_completions(stdout, *region, *completion_rows);
    *completion_rows = 0;
    *region = render_editor(session, stdout, prompt_text, buffer, *region, cols)
        .map_err(|_| EditorEvent::Error(io::Error::last_os_error()))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn refilter_completion(
    session: &Session,
    stdout: &mut dyn Write,
    prompt_text: &str,
    cols: u16,
    buffer: &mut LineBuffer,
    base: LineBuffer,
    state: CompletionState,
    active_completion: &mut Option<(LineBuffer, CompletionState)>,
    completion_rows: &mut usize,
    region: &mut RenderedRegion,
) -> Result<(), EditorEvent> {
    let mut next = complete::start_completion(
        session,
        CompletionRequest {
            text: &base.text,
            cursor: base.cursor,
            term_cols: cols,
        },
    );
    if next.comp.is_empty() {
        cancel_completion(
            session,
            stdout,
            prompt_text,
            cols,
            buffer,
            base,
            completion_rows,
            region,
        )?;
        return Ok(());
    }
    next.selected = if state.selected == usize::MAX {
        usize::MAX
    } else {
        state.selected.min(next.comp.len() - 1)
    };
    render_completion_preview(
        stdout,
        prompt_text,
        cols,
        buffer,
        &base,
        &next,
        completion_rows,
        region,
    )?;
    *active_completion = Some((base, next));
    Ok(())
}

fn read_utf8_text(reader: &mut dyn Read, byte: u8) -> Result<String, EditorEvent> {
    let _ = reader;
    let mut bytes = vec![byte];
    while std::str::from_utf8(&bytes).is_err() {
        let next = match read_stdin_byte_blocking() {
            Ok(Some(next)) => next,
            Ok(None) => {
                return Err(EditorEvent::Error(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "unexpected EOF while reading UTF-8 input",
                )));
            }
            Err(err) => {
                return Err(EditorEvent::Error(err));
            }
        };
        bytes.push(next);
    }
    std::str::from_utf8(&bytes)
        .map(str::to_string)
        .map_err(|err| EditorEvent::Error(io::Error::new(io::ErrorKind::InvalidData, err)))
}

impl HistorySearch {
    fn new(session: &mut Session, saved_line: LineBuffer) -> Self {
        session.history.sync();
        let mut candidates = Vec::new();
        session.history.visible_entry_indices_into(&mut candidates);
        let mut scratch = Vec::new();
        let mut matches = Vec::new();
        session.history.fuzzy_search_subset_into(
            "",
            &candidates,
            &mut scratch,
            &mut matches,
            HISTORY_SEARCH_LIMIT,
        );
        std::mem::swap(&mut candidates, &mut scratch);
        Self {
            saved_line,
            query: LineBuffer::default(),
            matches,
            candidates,
            scratch,
            candidate_stack: Vec::new(),
            selected: 0,
        }
    }

    fn rerun(&mut self, session: &Session, prev_text: &str, prev_cursor: usize) {
        let new_text = self.query.text.as_str();
        let append_at_end = prev_cursor == prev_text.len()
            && self.query.cursor == new_text.len()
            && new_text.len() > prev_text.len()
            && new_text.starts_with(prev_text);
        let delete_at_end = prev_cursor == prev_text.len()
            && self.query.cursor == new_text.len()
            && new_text.len() < prev_text.len()
            && prev_text.starts_with(new_text);

        if append_at_end {
            self.candidate_stack
                .push((prev_text.len(), std::mem::take(&mut self.candidates)));
            let source = &self.candidate_stack.last().expect("candidate stack").1;
            session.history.fuzzy_search_subset_into(
                new_text,
                source,
                &mut self.scratch,
                &mut self.matches,
                HISTORY_SEARCH_LIMIT,
            );
            std::mem::swap(&mut self.candidates, &mut self.scratch);
        } else if delete_at_end {
            while self
                .candidate_stack
                .last()
                .is_some_and(|(len, _)| *len > new_text.len())
            {
                if let Some((_, old_candidates)) = self.candidate_stack.pop() {
                    self.scratch = old_candidates;
                }
            }
            if self
                .candidate_stack
                .last()
                .is_some_and(|(len, _)| *len == new_text.len())
            {
                let (_, old_candidates) = self.candidate_stack.pop().expect("candidate stack");
                self.candidates = old_candidates;
                session.history.fuzzy_search_subset_into(
                    new_text,
                    &self.candidates,
                    &mut self.scratch,
                    &mut self.matches,
                    HISTORY_SEARCH_LIMIT,
                );
                std::mem::swap(&mut self.candidates, &mut self.scratch);
            } else {
                self.candidate_stack.clear();
                session
                    .history
                    .visible_entry_indices_into(&mut self.scratch);
                session.history.fuzzy_search_subset_into(
                    new_text,
                    &self.scratch,
                    &mut self.candidates,
                    &mut self.matches,
                    HISTORY_SEARCH_LIMIT,
                );
            }
        } else {
            self.candidate_stack.clear();
            session
                .history
                .visible_entry_indices_into(&mut self.scratch);
            session.history.fuzzy_search_subset_into(
                new_text,
                &self.scratch,
                &mut self.candidates,
                &mut self.matches,
                HISTORY_SEARCH_LIMIT,
            );
        }
        self.selected = 0;
    }
}

fn render_history_search(
    stdout: &mut dyn Write,
    query: &LineBuffer,
    search: &HistorySearch,
    session: &Session,
    prev: RenderedRegion,
    rows: u16,
    cols: u16,
) -> io::Result<RenderedRegion> {
    render::render_history_search(
        stdout,
        query,
        &search.matches,
        &session.history,
        search.selected,
        (rows, cols),
        prev,
    )
}

fn handle_history_search_key(
    session: &Session,
    reader: &mut dyn Read,
    byte: u8,
    search: &mut HistorySearch,
) -> HistoryAction {
    let prev_text = search.query.text.clone();
    let prev_cursor = search.query.cursor;
    let mut re_search = false;

    match byte {
        b'\r' | b'\n' => {
            return search
                .matches
                .get(search.selected)
                .map(|m| HistoryAction::Accept(session.history.get(m.entry_idx).to_string()))
                .unwrap_or(HistoryAction::Cancel);
        }
        0x03 => return HistoryAction::Cancel,
        0x10 if search.selected > 0 => search.selected -= 1,
        0x0e if search.selected + 1 < search.matches.len() => search.selected += 1,
        0x01 => search.query.cursor = 0,
        0x05 => search.query.cursor = search.query.text.len(),
        0x0b => {
            search.query.kill_to_end();
            re_search = true;
        }
        0x15 => {
            search.query.kill_to_start();
            re_search = true;
        }
        0x17 => {
            search.query.kill_word_backward();
            re_search = true;
        }
        0x19 => {
            search.query.yank();
            re_search = true;
        }
        0x04 => {
            search.query.delete_forward();
            re_search = true;
        }
        0x7f | 0x08 => {
            search.query.backspace();
            re_search = true;
        }
        0x1b => match handle_history_escape(reader, search) {
            Some(true) => re_search = true,
            Some(false) => {}
            None => return HistoryAction::Cancel,
        },
        byte if byte >= 0x20 => {
            if let Ok(text) = read_utf8_text(reader, byte) {
                search.query.insert(&text);
                re_search = true;
            }
        }
        _ => {}
    }

    if re_search && (search.query.text != prev_text || search.query.cursor != prev_cursor) {
        search.rerun(session, &prev_text, prev_cursor);
    }
    HistoryAction::Continue
}

fn handle_history_escape(reader: &mut dyn Read, search: &mut HistorySearch) -> Option<bool> {
    let _ = reader;
    let seq = read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS)?;
    match seq[0] {
        b'b' => {
            search.query.move_word_left();
            Some(false)
        }
        b'f' => {
            search.query.move_word_right();
            Some(false)
        }
        b'[' => handle_history_csi(reader, search),
        _ => None,
    }
}

fn handle_history_csi(reader: &mut dyn Read, search: &mut HistorySearch) -> Option<bool> {
    let _ = reader;
    let code = read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS)?;
    match code[0] {
        b'A' => {
            if search.selected > 0 {
                search.selected -= 1;
            }
            Some(false)
        }
        b'B' => {
            if search.selected + 1 < search.matches.len() {
                search.selected += 1;
            }
            Some(false)
        }
        b'D' => {
            search.query.move_left();
            Some(false)
        }
        b'C' => {
            search.query.move_right();
            Some(false)
        }
        b'H' => {
            search.query.cursor = 0;
            Some(false)
        }
        b'F' => {
            search.query.cursor = search.query.text.len();
            Some(false)
        }
        b'3' => {
            if read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| b[0] == b'~') {
                search.query.delete_forward();
                Some(true)
            } else {
                Some(false)
            }
        }
        b'1' => {
            let mut rest = [0_u8; 3];
            if read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| b[0] == b';')
                && read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| {
                    rest[1] = b[0];
                    true
                })
                && read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| {
                    rest[2] = b[0];
                    true
                })
            {
                rest[0] = b';';
                match rest {
                    [b';', b'5', b'D'] => search.query.move_word_left(),
                    [b';', b'5', b'C'] => search.query.move_word_right(),
                    _ => {}
                }
            }
            Some(false)
        }
        _ => Some(false),
    }
}

fn read_stdin_byte_timeout(timeout_ms: i32) -> Option<[u8; 1]> {
    if !stdin_ready_within(timeout_ms) {
        return None;
    }
    let mut byte = [0_u8; 1];
    if rio::read(rstdio::stdin(), &mut byte).ok() != Some(1) {
        return None;
    }
    Some(byte)
}

fn read_stdin_byte_blocking() -> io::Result<Option<u8>> {
    loop {
        let mut byte = [0_u8; 1];
        match rio::read(rstdio::stdin(), &mut byte) {
            Ok(1) => return Ok(Some(byte[0])),
            Ok(0) => return Ok(None),
            Ok(_) => continue,
            Err(err) => {
                let err = io::Error::from(err);
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err);
            }
        }
    }
}

fn stdin_ready_within(timeout_ms: i32) -> bool {
    let stdin = rstdio::stdin();
    let mut fd = [revent::PollFd::new(&stdin, revent::PollFlags::IN)];
    let timeout = if timeout_ms < 0 {
        None
    } else {
        Some(revent::Timespec {
            tv_sec: (timeout_ms / 1000) as _,
            tv_nsec: ((timeout_ms % 1000) * 1_000_000) as _,
        })
    };
    revent::poll(&mut fd, timeout.as_ref())
        .is_ok_and(|ready| ready > 0 && fd[0].revents().contains(revent::PollFlags::IN))
}

fn render_editor(
    session: &Session,
    stdout: &mut dyn Write,
    prompt_text: &str,
    buffer: &LineBuffer,
    prev: RenderedRegion,
    cols: u16,
) -> io::Result<RenderedRegion> {
    let suggestion = autosuggestion(session, buffer);
    render::render_line(
        stdout,
        prompt_text,
        buffer,
        cols,
        prev,
        &RenderOpts { suggestion },
    )
}

fn render_editor_without_suggestion(
    stdout: &mut dyn Write,
    prompt_text: &str,
    buffer: &LineBuffer,
    prev: RenderedRegion,
    cols: u16,
) -> io::Result<RenderedRegion> {
    render::render_line(
        stdout,
        prompt_text,
        buffer,
        cols,
        prev,
        &RenderOpts { suggestion: "" },
    )
}

fn handle_escape(
    session: &Session,
    reader: &mut dyn Read,
    buffer: &mut LineBuffer,
    history_nav: &mut Option<HistoryNavigation>,
) {
    let _ = reader;
    let Some(seq) = read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS) else {
        return;
    };
    if seq[0] == b'b' {
        buffer.move_word_left();
        return;
    }
    if seq[0] == b'f' {
        buffer.move_word_right();
        return;
    }
    if seq[0] != b'[' {
        return;
    }
    let Some(code) = read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS) else {
        return;
    };
    match code[0] {
        b'D' => buffer.move_left(),
        b'C' => {
            if buffer.cursor >= buffer.text.len()
                && !buffer.text.contains('\n')
                && let Some(entry) = history_prefix_match(session, &buffer.text)
            {
                buffer.text = entry.to_string();
                buffer.cursor = buffer.text.len();
            } else {
                buffer.move_right();
            }
        }
        b'H' => buffer.cursor = 0,
        b'F' => buffer.cursor = buffer.text.len(),
        b'A' => history_prev(session, buffer, history_nav),
        b'B' => history_next(session, buffer, history_nav),
        b'3' if read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS)
            .is_some_and(|b| b[0] == b'~') =>
        {
            buffer.delete_forward();
        }
        b'1' => {
            let mut rest = [0_u8; 3];
            if read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| b[0] == b';')
                && read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| {
                    rest[1] = b[0];
                    true
                })
                && read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| {
                    rest[2] = b[0];
                    true
                })
            {
                rest[0] = b';';
                match rest {
                    [b';', b'5', b'D'] => buffer.move_word_left(),
                    [b';', b'5', b'C'] => buffer.move_word_right(),
                    _ => {}
                }
            }
        }
        b'2' => {
            let mut rest = [0_u8; 3];
            if read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| b[0] == b'0')
                && read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| b[0] == b'0')
                && read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS).is_some_and(|b| b[0] == b'~')
            {
                rest = *b"00~";
            }
            if rest == *b"00~" {
                let mut paste = Vec::new();
                let mut window = Vec::new();
                while let Ok(Some(byte)) = read_stdin_byte_blocking() {
                    let byte = [byte];
                    window.push(byte[0]);
                    paste.push(byte[0]);
                    if window.ends_with(b"\x1b[201~") {
                        let len = paste.len().saturating_sub(6);
                        paste.truncate(len);
                        break;
                    }
                    if window.len() > 6 {
                        window.remove(0);
                    }
                }
                if let Ok(text) = String::from_utf8(paste) {
                    buffer.insert(&text);
                }
            }
        }
        _ => {}
    }
}

fn handle_completion_escape(reader: &mut dyn Read, state: &mut CompletionState) -> bool {
    let _ = reader;
    let Some(seq) = read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS) else {
        return false;
    };
    if seq[0] != b'[' {
        return false;
    }
    let Some(code) = read_stdin_byte_timeout(ESCAPE_SEQUENCE_TIMEOUT_MS) else {
        return false;
    };
    match code[0] {
        b'A' => state.move_up(),
        b'B' => state.move_down(),
        b'C' => state.move_right(),
        b'D' => state.move_left(),
        _ => return false,
    }
    true
}

fn history_prev(session: &Session, buffer: &mut LineBuffer, nav: &mut Option<HistoryNavigation>) {
    let (prefix, start) = match nav.as_ref() {
        Some(nav) => (nav.prefix.clone(), nav.index),
        None => (
            buffer.text[..buffer.cursor].to_string(),
            session.history.len(),
        ),
    };
    if let Some(index) = session.history.session_prefix_index_before(&prefix, start) {
        if nav.is_none() {
            *nav = Some(HistoryNavigation {
                saved_line: buffer.clone(),
                prefix,
                index,
            });
        } else if let Some(nav) = nav.as_mut() {
            nav.index = index;
        }
        buffer.text = session.history.get(index).to_string();
        buffer.cursor = buffer.text.len();
    }
}

fn history_next(session: &Session, buffer: &mut LineBuffer, nav: &mut Option<HistoryNavigation>) {
    let Some(current) = nav.as_ref() else {
        return;
    };
    if let Some(index) = session
        .history
        .session_prefix_index_after(&current.prefix, current.index)
    {
        if let Some(nav) = nav.as_mut() {
            nav.index = index;
        }
        buffer.text = session.history.get(index).to_string();
        buffer.cursor = buffer.text.len();
        return;
    }
    if let Some(current) = nav.take() {
        *buffer = current.saved_line;
    }
}

pub(super) fn fuzzy_history_match<'a>(session: &'a Session, needle: &str) -> Option<&'a str> {
    session.history.latest_subsequence_match(needle)
}

fn continuation_prompt(command: &str) -> Option<&'static str> {
    let trimmed = command.trim_end();
    if trimmed.ends_with('\\') {
        return Some("> ");
    }
    if trimmed.ends_with('|')
        || trimmed.ends_with("|&")
        || trimmed.ends_with("&&")
        || trimmed.ends_with("||")
    {
        return Some("> ");
    }
    match lex_shell(trimmed) {
        Err(message) if message.contains("unterminated") => Some("> "),
        _ => None,
    }
}

fn join_continuation(mut command: String, line: String) -> String {
    if command.trim_end().ends_with('\\') {
        while command.ends_with(char::is_whitespace) {
            command.pop();
        }
        command.pop();
        command.push(' ');
        command.push_str(line.trim_start());
    } else {
        command.push('\n');
        command.push_str(&line);
    }
    command
}

#[derive(Clone, Debug)]
pub(super) enum CompletionAction {
    None,
    Display(CompletionState),
}

pub(super) fn complete_buffer(
    session: &Session,
    buffer: &mut LineBuffer,
    cols: u16,
) -> CompletionAction {
    let state = complete::start_completion(
        session,
        CompletionRequest {
            text: &buffer.text,
            cursor: buffer.cursor,
            term_cols: cols,
        },
    );
    if state.comp.is_empty() {
        return CompletionAction::None;
    }
    let before_cursor = &buffer.text[..buffer.cursor];
    let (word_start, _) = complete::find_comp_word_start(before_cursor);
    let raw_word = &before_cursor[word_start..];
    if state.comp.len() == 1 {
        if let Some(replacement) = complete::completion_replacement(&state) {
            let new_text = format!(
                "{}{}{}",
                &buffer.text[..word_start],
                replacement,
                &buffer.text[buffer.cursor..]
            );
            buffer.set_with_cursor(&new_text, word_start + replacement.len());
        }
        return CompletionAction::None;
    }
    if let Some(common) = common_completion(&state) {
        let replacement = format!("{}{}", state.dir_prefix, common);
        if replacement.len() > raw_word.len() {
            let new_text = format!(
                "{}{}{}",
                &buffer.text[..word_start],
                replacement,
                &buffer.text[buffer.cursor..]
            );
            buffer.set_with_cursor(&new_text, word_start + replacement.len());
            return CompletionAction::None;
        }
    }
    let mut state = state;
    state.selected = usize::MAX;
    CompletionAction::Display(state)
}

fn common_completion(state: &CompletionState) -> Option<String> {
    let mut common = state.comp.name(0).to_string();
    for index in 1..state.comp.len() {
        let candidate = state.comp.name(index);
        while !candidate.starts_with(&common) {
            common.pop();
        }
    }
    Some(common)
}

fn preview_completion(line: &mut LineBuffer, state: &CompletionState, base: &LineBuffer) {
    let Some(replacement) = complete::completion_replacement(state) else {
        *line = base.clone();
        return;
    };
    let text = &base.text;
    let before_cursor = &text[..base.cursor];
    let (word_start, _) = complete::find_comp_word_start(before_cursor);
    let new_text = format!(
        "{}{}{}",
        &text[..word_start],
        replacement,
        &text[base.cursor..]
    );
    line.set_with_cursor(&new_text, word_start + replacement.len());
}

pub(super) fn autosuggestion<'a>(session: &'a Session, buffer: &LineBuffer) -> &'a str {
    if buffer.text.len() < 3 || buffer.text.contains('\n') || buffer.cursor != buffer.text.len() {
        return "";
    }
    history_prefix_match(session, &buffer.text)
        .and_then(|entry| entry.strip_prefix(&buffer.text))
        .unwrap_or("")
}

pub(super) fn history_prefix_match<'a>(session: &'a Session, prefix: &str) -> Option<&'a str> {
    session.history.prefix_search(prefix, 0)
}

#[cfg(test)]
mod tests {
    use super::{
        CompletionAction, LineBuffer, autosuggestion, complete_buffer, preview_completion,
        try_alias_expand,
    };
    use crate::xshi::interactive::complete::{CompletionState, Completions};
    use crate::xshi::interactive::history::History;
    use crate::xshi::interactive::session::Session;
    use std::collections::BTreeMap;

    #[test]
    fn line_buffer_edits_words_and_utf8_boundaries() {
        let mut line = LineBuffer::from_text("alpha beta");

        line.move_word_left();
        assert_eq!(line.cursor, "alpha ".len());
        line.kill_to_end();
        assert_eq!(line.text, "alpha ");
        line.yank();
        assert_eq!(line.text, "alpha beta");
        line.kill_word_backward();
        assert_eq!(line.text, "alpha ");
        line.insert("gamma");
        assert_eq!(line.text, "alpha gamma");

        line.set_with_cursor("aéz", 1);
        line.delete_forward();
        assert_eq!(line.text, "az");
        line.backspace();
        assert_eq!(line.text, "z");
    }

    #[test]
    fn alias_expansion_replaces_first_word_only_at_command_start() {
        let mut aliases = BTreeMap::new();
        aliases.insert("gp".to_string(), "git push -u".to_string());
        let mut line = LineBuffer::from_text("gp ");

        try_alias_expand(&mut line, &aliases);
        assert_eq!(line.text, "git push -u ");
        assert_eq!(line.cursor, line.text.len());

        let mut later = LineBuffer::from_text("echo gp ");
        try_alias_expand(&mut later, &aliases);
        assert_eq!(later.text, "echo gp ");
    }

    #[test]
    fn complete_buffer_inserts_single_path_and_extends_common_prefix() {
        let root = super::complete::completion_test_dir(&["alpha.txt", "alpine.log"]);

        let mut session = Session::new();
        session.cwd = root.path().to_path_buf();
        session.invalidate_cwd_snapshot();

        let mut single = LineBuffer::from_text("cat alpha");
        assert!(matches!(
            complete_buffer(&session, &mut single, 80),
            CompletionAction::None
        ));
        assert_eq!(single.text, "cat alpha.txt");
        assert_eq!(single.cursor, single.text.len());

        let mut common = LineBuffer::from_text("cat al");
        assert!(matches!(
            complete_buffer(&session, &mut common, 80),
            CompletionAction::None
        ));
        assert_eq!(common.text, "cat alp");
    }

    #[test]
    fn preview_completion_restores_base_when_no_selection() {
        let base = LineBuffer::from_text("cat file");
        let mut preview = base.clone();
        let mut comp = Completions::new();
        comp.push("file one", false, false, false);
        let state = CompletionState {
            comp,
            selected: usize::MAX,
            term_cols: 80,
            ..CompletionState::default()
        };

        preview_completion(&mut preview, &state, &base);
        assert_eq!(preview, base);
    }

    #[test]
    fn autosuggestion_uses_history_suffix_at_line_end_only() {
        let mut session = Session::new();
        session.history = History::from_entries(vec!["git status --short".to_string()]);
        let line = LineBuffer::from_text("git");
        assert_eq!(autosuggestion(&session, &line), " status --short");

        let mut middle = LineBuffer::from_text("git");
        middle.cursor = 1;
        assert_eq!(autosuggestion(&session, &middle), "");
    }
}
