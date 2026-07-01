#![allow(clippy::single_call_fn)]

use super::complete::{CompletionState, str_width};
use super::edit::LineBuffer;
use super::history::{FuzzyMatch, History};
use rustix::{stdio as rstdio, termios};
use std::io::{self, Write};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct RenderedRegion {
    pub(super) anchored: bool,
    pub(super) painted_rows: u16,
    pub(super) cursor_row: u16,
    pub(super) cursor_col: u16,
}

#[derive(Clone, Debug, Default)]
pub(super) struct RenderOpts<'a> {
    pub(super) suggestion: &'a str,
}

struct PromptLayout {
    region: RenderedRegion,
    rows_up_from_end: u16,
    needs_forced_wrap: bool,
}

struct CompletionGridLayout {
    visible_rows: usize,
    scroll_start: usize,
    col_widths: [usize; 6],
}

struct PagerLayout {
    region: RenderedRegion,
    rows_up_from_end: u16,
    needs_forced_wrap: bool,
    max_results: usize,
    max_width: usize,
    scroll: usize,
}

struct PagerInput {
    prefix_width: usize,
    query_width: usize,
    query_cursor: usize,
    total_entries: usize,
    selected: usize,
    term_rows: u16,
    term_cols: u16,
}

pub(super) struct TermWriter {
    buf: Vec<u8>,
}

impl TermWriter {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(2048),
        }
    }

    fn write_str(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }

    fn clear_to_end_of_line(&mut self) {
        self.write_str("\x1b[K");
    }

    fn clear_to_end_of_screen(&mut self) {
        self.write_str("\x1b[J");
    }

    fn hide_cursor(&mut self) {
        self.write_str("\x1b[?25l");
    }

    fn show_cursor(&mut self) {
        self.write_str("\x1b[?25h");
    }

    fn carriage_return(&mut self) {
        self.write_str("\r");
    }

    fn save_cursor(&mut self) {
        self.write_str("\x1b[s");
    }

    fn move_cursor_right(&mut self, n: u16) {
        if n > 0 {
            self.push_csi(n, b'C');
        }
    }

    fn move_cursor_up(&mut self, n: u16) {
        if n > 0 {
            self.push_csi(n, b'A');
        }
    }

    fn move_cursor_down(&mut self, n: u16) {
        if n > 0 {
            self.push_csi(n, b'B');
        }
    }

    fn flush_to(&mut self, out: &mut dyn Write) -> io::Result<()> {
        out.write_all(&self.buf)?;
        out.flush()?;
        self.buf.clear();
        Ok(())
    }

    fn push_csi(&mut self, n: u16, suffix: u8) {
        self.buf.extend_from_slice(b"\x1b[");
        let mut tmp = [0_u8; 5];
        let mut i = tmp.len();
        let mut val = n;
        loop {
            i -= 1;
            tmp[i] = b'0' + (val % 10) as u8;
            val /= 10;
            if val == 0 {
                break;
            }
        }
        self.buf.extend_from_slice(&tmp[i..]);
        self.buf.push(suffix);
    }
}

pub(super) fn term_size() -> (u16, u16) {
    match termios::tcgetwinsize(rstdio::stdout()) {
        Ok(size) if size.ws_col > 0 => (size.ws_row, size.ws_col),
        _ => (24, 80),
    }
}

fn layout_single_line_prompt(
    prompt_display_len: usize,
    line: &LineBuffer,
    suggestion_display_len: usize,
    cols: usize,
) -> PromptLayout {
    let total_before_cursor = prompt_display_len + line.display_cursor_pos();
    let total_full = prompt_display_len + line.display_len() + suggestion_display_len;
    let cursor_row = total_before_cursor / cols;
    let cursor_col = total_before_cursor % cols;
    let total_rows = total_full / cols;

    PromptLayout {
        region: RenderedRegion {
            anchored: false,
            painted_rows: (total_rows + 1) as u16,
            cursor_row: cursor_row as u16,
            cursor_col: cursor_col as u16,
        },
        rows_up_from_end: total_rows.saturating_sub(cursor_row) as u16,
        needs_forced_wrap: total_full > 0 && total_full.is_multiple_of(cols),
    }
}

fn layout_multiline_prompt(
    prompt_display_len: usize,
    line: &LineBuffer,
    cols: usize,
) -> PromptLayout {
    let text = &line.text;
    let cursor_byte = line.cursor;
    let cont_prompt_len = 2;
    let segment_count = text.split('\n').count();
    let mut row: usize = 0;
    let mut cursor_row: usize = 0;
    let mut cursor_col: usize = 0;
    let mut line_idx = 0;
    let mut last_seg_width: usize = 0;

    for (i, segment) in text.split('\n').enumerate() {
        let seg_start = line_idx;
        let seg_end = seg_start + segment.len();
        if cursor_byte >= seg_start && cursor_byte <= seg_end {
            let prefix = if i == 0 {
                prompt_display_len
            } else {
                cont_prompt_len
            };
            let cursor_in_seg = cursor_byte - seg_start;
            let display_before = str_width(&segment[..cursor_in_seg]);
            let total = prefix + display_before;
            cursor_row = row + total / cols;
            cursor_col = total % cols;
        }
        let prefix = if i == 0 {
            prompt_display_len
        } else {
            cont_prompt_len
        };
        let seg_width = prefix + str_width(segment);
        if seg_width > 0 {
            row += (seg_width - 1) / cols;
        }
        last_seg_width = seg_width;
        line_idx = seg_end + 1;
        if i + 1 < segment_count {
            row += 1;
        }
    }

    let needs_forced_wrap = last_seg_width > 0 && last_seg_width.is_multiple_of(cols);
    if needs_forced_wrap {
        row += 1;
    }

    PromptLayout {
        region: RenderedRegion {
            anchored: false,
            painted_rows: (row + 1) as u16,
            cursor_row: cursor_row as u16,
            cursor_col: cursor_col as u16,
        },
        rows_up_from_end: row.saturating_sub(cursor_row) as u16,
        needs_forced_wrap,
    }
}

fn layout_pager(input: PagerInput) -> PagerLayout {
    let cols = input.term_cols.max(1) as usize;
    let header_before_cursor = input.prefix_width + input.query_cursor;
    let header_full = input.prefix_width + input.query_width;
    let header_rows = header_full.saturating_sub(1) / cols + 1;
    let cursor_row = header_before_cursor / cols;
    let cursor_col = header_before_cursor % cols;
    let max_results = (input.term_rows as usize).saturating_sub(2).min(20);
    let displayed = input.total_entries.min(max_results);
    let last_row = header_rows + displayed.saturating_sub(1);
    let scroll = if input.total_entries <= max_results || input.selected < max_results / 2 {
        0
    } else if input.selected + max_results / 2 >= input.total_entries {
        input.total_entries.saturating_sub(max_results)
    } else {
        input.selected - max_results / 2
    };

    PagerLayout {
        region: RenderedRegion {
            anchored: false,
            painted_rows: (header_rows + displayed) as u16,
            cursor_row: cursor_row as u16,
            cursor_col: cursor_col as u16,
        },
        rows_up_from_end: last_row.saturating_sub(cursor_row) as u16,
        needs_forced_wrap: header_full > 0 && header_full.is_multiple_of(cols),
        max_results,
        max_width: input.term_cols.saturating_sub(2).max(1) as usize,
        scroll,
    }
}

fn begin_region_render(tw: &mut TermWriter, prev: RenderedRegion, next: RenderedRegion) {
    tw.hide_cursor();
    if prev.anchored {
        let prev_rows_below = prev.painted_rows.saturating_sub(1);
        let next_rows_below = next.painted_rows.saturating_sub(1);
        if next_rows_below <= prev_rows_below {
            clear_from_cursor(tw, prev);
        } else {
            clear_from_cursor(tw, prev);
            if prev_rows_below > 0 {
                tw.move_cursor_down(prev_rows_below);
            }
            reserve_rows_below(tw, next_rows_below - prev_rows_below);
            if prev_rows_below > 0 {
                tw.move_cursor_up(prev_rows_below);
                tw.carriage_return();
            }
        }
    } else {
        reserve_rows_below(tw, next.painted_rows.saturating_sub(1));
    }
    tw.save_cursor();
}

fn reserve_rows_below(tw: &mut TermWriter, rows_below: u16) {
    if rows_below == 0 {
        return;
    }
    for _ in 0..rows_below {
        tw.write_str("\n");
    }
    tw.move_cursor_up(rows_below);
    tw.carriage_return();
}

fn clear_from_cursor(tw: &mut TermWriter, region: RenderedRegion) {
    if region.cursor_row > 0 {
        tw.move_cursor_up(region.cursor_row);
    }
    tw.carriage_return();
    tw.clear_to_end_of_screen();
}

fn restore_cursor_from_end(tw: &mut TermWriter, rows_up: u16, cursor_col: u16) {
    if rows_up > 0 {
        tw.move_cursor_up(rows_up);
    }
    tw.carriage_return();
    tw.move_cursor_right(cursor_col);
}

pub(super) fn render_line(
    stdout: &mut dyn Write,
    prompt: &str,
    line: &LineBuffer,
    term_cols: u16,
    prev: RenderedRegion,
    opts: &RenderOpts<'_>,
) -> io::Result<RenderedRegion> {
    let mut tw = TermWriter::new();
    let prompt_display_len = str_width_without_ansi(prompt);
    let region = if line.text.contains('\n') {
        render_line_multiline(
            &mut tw,
            prompt,
            prompt_display_len,
            line,
            term_cols as usize,
            prev,
        )
    } else {
        render_line_single(
            &mut tw,
            prompt,
            prompt_display_len,
            line,
            term_cols as usize,
            prev,
            opts,
        )
    };
    tw.flush_to(stdout)?;
    Ok(region)
}

fn render_line_single(
    tw: &mut TermWriter,
    prompt: &str,
    prompt_display_len: usize,
    line: &LineBuffer,
    cols: usize,
    prev: RenderedRegion,
    opts: &RenderOpts<'_>,
) -> RenderedRegion {
    let layout =
        layout_single_line_prompt(prompt_display_len, line, str_width(opts.suggestion), cols);
    begin_region_render(tw, prev, layout.region);
    tw.write_str(prompt);
    tw.write_str(&line.text);
    if !opts.suggestion.is_empty() {
        tw.write_str("\x1b[38;5;8m");
        tw.write_str(opts.suggestion);
        tw.write_str("\x1b[0m");
    }
    if layout.needs_forced_wrap {
        tw.write_str(" \r");
    }
    restore_cursor_from_end(tw, layout.rows_up_from_end, layout.region.cursor_col);
    tw.show_cursor();
    RenderedRegion {
        anchored: true,
        ..layout.region
    }
}

fn render_line_multiline(
    tw: &mut TermWriter,
    prompt: &str,
    prompt_display_len: usize,
    line: &LineBuffer,
    cols: usize,
    prev: RenderedRegion,
) -> RenderedRegion {
    let layout = layout_multiline_prompt(prompt_display_len, line, cols);
    begin_region_render(tw, prev, layout.region);
    for (index, segment) in line.text.split('\n').enumerate() {
        if index == 0 {
            tw.write_str(prompt);
        } else {
            tw.write_str("\r\n  ");
        }
        tw.write_str(segment);
    }
    if layout.needs_forced_wrap {
        tw.write_str(" \r");
    }
    restore_cursor_from_end(tw, layout.rows_up_from_end, layout.region.cursor_col);
    tw.show_cursor();
    RenderedRegion {
        anchored: true,
        ..layout.region
    }
}

pub(super) fn render_completions(
    stdout: &mut dyn Write,
    state: &CompletionState,
    info: RenderedRegion,
    initial: bool,
    previous_visible_rows: usize,
) -> io::Result<usize> {
    let mut tw = TermWriter::new();
    let visible_rows = render_completion_grid(&mut tw, state, info, initial, previous_visible_rows);
    tw.flush_to(stdout)?;
    Ok(visible_rows)
}

pub(super) fn render_history_search(
    stdout: &mut dyn Write,
    query: &LineBuffer,
    matches: &[FuzzyMatch],
    history: &History,
    selected: usize,
    term_size: (u16, u16),
    prev: RenderedRegion,
) -> io::Result<RenderedRegion> {
    let mut tw = TermWriter::new();
    let prefix = "search: ";
    let (term_rows, term_cols) = term_size;
    let layout = layout_pager(PagerInput {
        prefix_width: str_width(prefix),
        query_width: query.display_len(),
        query_cursor: query.display_cursor_pos(),
        total_entries: matches.len(),
        selected,
        term_rows,
        term_cols,
    });
    begin_region_render(&mut tw, prev, layout.region);

    tw.write_str("\x1b[1m");
    tw.write_str(prefix);
    tw.write_str("\x1b[0m");
    tw.write_str(&query.text);
    if layout.needs_forced_wrap {
        tw.write_str(" \r");
        tw.clear_to_end_of_line();
    } else {
        tw.write_str("\n");
    }

    let displayed = matches.len().min(layout.max_results);
    for (visible_idx, m) in matches
        .iter()
        .skip(layout.scroll)
        .take(layout.max_results)
        .enumerate()
    {
        tw.carriage_return();
        tw.clear_to_end_of_line();
        let row_idx = layout.scroll + visible_idx;
        let is_selected = row_idx == selected;
        if is_selected {
            tw.write_str("\x1b[7m");
        }
        write_history_row(
            &mut tw,
            history.get(m.entry_idx),
            m,
            layout.max_width,
            is_selected,
        );
        if is_selected {
            tw.write_str("\x1b[0m");
        }
        tw.clear_to_end_of_line();
        if visible_idx + 1 < displayed {
            tw.write_str("\n");
        }
    }

    tw.clear_to_end_of_screen();
    restore_cursor_from_end(&mut tw, layout.rows_up_from_end, layout.region.cursor_col);
    tw.show_cursor();
    tw.flush_to(stdout)?;
    Ok(RenderedRegion {
        anchored: true,
        ..layout.region
    })
}

fn write_history_row(
    tw: &mut TermWriter,
    text: &str,
    m: &FuzzyMatch,
    max_width: usize,
    is_selected: bool,
) {
    let mut col = 0;
    let mut pos_idx = 0;
    let mut in_match = false;
    for (char_idx, ch) in text.chars().enumerate() {
        let width = super::complete::char_width(ch);
        if col + width > max_width {
            break;
        }
        col += width;
        let is_match =
            pos_idx < m.match_count as usize && m.match_positions[pos_idx] == char_idx as u16;
        if is_match {
            pos_idx += 1;
        }
        if !is_selected {
            if is_match && !in_match {
                tw.write_str("\x1b[1;33m");
                in_match = true;
            } else if !is_match && in_match {
                tw.write_str("\x1b[0m");
                in_match = false;
            }
        }
        let mut buf = [0_u8; 4];
        tw.write_str(ch.encode_utf8(&mut buf));
    }
    if in_match {
        tw.write_str("\x1b[0m");
    }
}

pub(super) fn clear_completions(
    stdout: &mut dyn Write,
    info: RenderedRegion,
    visible_rows: usize,
) -> io::Result<()> {
    if visible_rows == 0 {
        return Ok(());
    }
    let mut tw = TermWriter::new();
    tw.hide_cursor();
    let down_to_grid = info.painted_rows.saturating_sub(info.cursor_row);
    if down_to_grid > 0 {
        tw.move_cursor_down(down_to_grid);
    }
    for row in 0..visible_rows {
        if row > 0 {
            tw.write_str("\n");
        }
        tw.carriage_return();
        tw.clear_to_end_of_line();
    }
    restore_cursor_from_end(
        &mut tw,
        down_to_grid + visible_rows.saturating_sub(1) as u16,
        info.cursor_col,
    );
    tw.show_cursor();
    tw.flush_to(stdout)
}

fn render_completion_grid(
    tw: &mut TermWriter,
    state: &CompletionState,
    info: RenderedRegion,
    initial: bool,
    previous_visible_rows: usize,
) -> usize {
    let layout = layout_completion_grid(state);
    if layout.visible_rows == 0 {
        return 0;
    }
    tw.hide_cursor();

    if initial {
        let rows_below = info.painted_rows.saturating_sub(1 + info.cursor_row);
        if rows_below > 0 {
            tw.move_cursor_down(rows_below);
        }
        for _ in 0..layout.visible_rows {
            tw.write_str("\n");
        }
        restore_cursor_from_end(tw, rows_below + layout.visible_rows as u16, info.cursor_col);
    }

    let down_to_grid = info.painted_rows.saturating_sub(info.cursor_row);
    if down_to_grid > 0 {
        tw.move_cursor_down(down_to_grid);
    }
    tw.carriage_return();
    draw_grid(tw, state, &layout);
    let painted_rows = layout.visible_rows.max(previous_visible_rows);
    if previous_visible_rows > layout.visible_rows {
        for _ in layout.visible_rows..previous_visible_rows {
            tw.write_str("\n");
            tw.carriage_return();
            tw.clear_to_end_of_line();
        }
    }
    restore_cursor_from_end(
        tw,
        down_to_grid + painted_rows.saturating_sub(1) as u16,
        info.cursor_col,
    );
    tw.show_cursor();
    layout.visible_rows
}

fn layout_completion_grid(state: &CompletionState) -> CompletionGridLayout {
    let visible_rows = grid_visible_rows(state);
    if visible_rows == 0 {
        return CompletionGridLayout {
            visible_rows: 0,
            scroll_start: 0,
            col_widths: [0; 6],
        };
    }
    let mut col_widths = [0_usize; 6];
    for (i, entry) in state.comp.entries.iter().enumerate() {
        let col = i / state.rows;
        if col < state.cols {
            col_widths[col] = col_widths[col].max(entry.display_width());
        }
    }

    let scroll_start = if state.selected >= state.comp.entries.len() {
        state.scroll.min(state.rows.saturating_sub(visible_rows))
    } else {
        let selected_row = state.selected % state.rows;
        if selected_row < state.scroll {
            selected_row
        } else if selected_row >= state.scroll + visible_rows {
            selected_row + 1 - visible_rows
        } else {
            state.scroll
        }
    };

    CompletionGridLayout {
        visible_rows,
        scroll_start,
        col_widths,
    }
}

fn grid_visible_rows(state: &CompletionState) -> usize {
    if state.comp.is_empty() || state.rows == 0 {
        return 0;
    }
    state.rows.min(10)
}

fn draw_grid(tw: &mut TermWriter, state: &CompletionState, layout: &CompletionGridLayout) {
    for vr in 0..layout.visible_rows {
        let row = layout.scroll_start + vr;
        tw.carriage_return();
        tw.clear_to_end_of_line();
        let mut remaining_width = state.term_cols.saturating_sub(1) as usize;

        for (col, &col_w) in layout.col_widths[..state.cols].iter().enumerate() {
            let idx = col * state.rows + row;
            if idx >= state.comp.entries.len() || remaining_width == 0 {
                break;
            }
            let entry = &state.comp.entries[idx];
            let is_selected = idx == state.selected;
            if is_selected {
                tw.write_str("\x1b[7m");
            }
            if entry.is_host() {
                tw.write_str("\x1b[35m");
            } else if entry.is_link() {
                tw.write_str("\x1b[36m");
            } else if entry.is_dir() {
                tw.write_str("\x1b[34m");
            } else if entry.is_exec() {
                tw.write_str("\x1b[32m");
            }

            let has_next = (col + 1..state.cols)
                .any(|next_col| next_col * state.rows + row < state.comp.entries.len());
            let reserved_gap = if has_next { 2 } else { 0 };
            let content_width = col_w.min(remaining_width.saturating_sub(reserved_gap));
            let written_width = write_completion_name(tw, state, idx, content_width);

            if entry.is_host() || entry.is_link() || entry.is_dir() || entry.is_exec() {
                tw.write_str("\x1b[0m");
            }
            if is_selected {
                tw.write_str("\x1b[0m");
            }

            let desired_pad = if has_next {
                col_w.saturating_sub(written_width) + reserved_gap
            } else {
                0
            };
            let pad = desired_pad.min(remaining_width.saturating_sub(written_width));
            for _ in 0..pad {
                tw.write_str(" ");
            }
            remaining_width = remaining_width.saturating_sub(written_width + pad);
        }
        if vr + 1 < layout.visible_rows {
            tw.write_str("\n");
        }
    }
}

fn write_completion_name(
    tw: &mut TermWriter,
    state: &CompletionState,
    idx: usize,
    max_width: usize,
) -> usize {
    if max_width == 0 {
        return 0;
    }
    let entry = &state.comp.entries[idx];
    let name = state.comp.entry_name(entry);
    let mut written = 0;
    for ch in name.chars() {
        let w = super::complete::char_width(ch);
        if written + w > max_width {
            break;
        }
        let mut buf = [0_u8; 4];
        tw.write_str(ch.encode_utf8(&mut buf));
        written += w;
    }
    let suffix = if entry.is_dir() {
        Some('/')
    } else if entry.is_host() {
        Some(':')
    } else {
        None
    };
    if let Some(ch) = suffix
        && written < max_width
    {
        let mut buf = [0_u8; 4];
        tw.write_str(ch.encode_utf8(&mut buf));
        written += 1;
    }
    written
}

pub(super) fn str_width_without_ansi(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for ch in chars.by_ref() {
                if ('@'..='~').contains(&ch) {
                    break;
                }
            }
        } else {
            width += super::complete::char_width(ch);
        }
    }
    width
}

#[cfg(test)]
mod tests {
    use super::{
        History, LineBuffer, RenderOpts, RenderedRegion, clear_completions, render_completions,
        render_history_search, render_line,
    };
    use crate::xshi::interactive::complete::{CompletionState, Completions};

    #[test]
    fn render_line_single_forced_wrap_keeps_cursor_before_suggestion() {
        let line = LineBuffer::from_text("abcdef");
        let mut out = Vec::new();

        let region = render_line(
            &mut out,
            "$ ",
            &line,
            10,
            RenderedRegion::default(),
            &RenderOpts { suggestion: "gh" },
        )
        .expect("render line");
        let rendered = String::from_utf8(out).expect("utf8 render");

        assert_eq!(region.painted_rows, 2);
        assert_eq!(region.cursor_row, 0);
        assert_eq!(region.cursor_col, 8);
        assert!(rendered.contains("\x1b[38;5;8mgh\x1b[0m"), "{rendered:?}");
        assert!(rendered.contains(" \r"), "{rendered:?}");
    }

    #[test]
    fn render_line_multiline_uses_continuation_prompt_and_clears_previous() {
        let mut line = LineBuffer::from_text("one\ntwo");
        line.cursor = "one\n".len();
        let mut out = Vec::new();

        let region = render_line(
            &mut out,
            "$ ",
            &line,
            8,
            RenderedRegion {
                anchored: true,
                painted_rows: 3,
                cursor_row: 2,
                cursor_col: 1,
            },
            &RenderOpts::default(),
        )
        .expect("render multiline");
        let rendered = String::from_utf8(out).expect("utf8 render");

        assert_eq!(region.painted_rows, 2);
        assert_eq!(region.cursor_row, 1);
        assert_eq!(region.cursor_col, 2);
        assert!(
            rendered.starts_with("\x1b[?25l\x1b[2A\r\x1b[J"),
            "{rendered:?}"
        );
        assert!(rendered.contains("\r\n  two"), "{rendered:?}");
    }

    #[test]
    fn render_and_clear_completion_grid_restores_editor_cursor() {
        let mut comp = Completions::new();
        for name in ["alpha", "beta", "gamma"] {
            comp.push(name, false, false, false);
        }
        let state = CompletionState {
            comp,
            selected: 1,
            cols: 1,
            rows: 3,
            term_cols: 40,
            ..CompletionState::default()
        };
        let info = RenderedRegion {
            anchored: true,
            painted_rows: 1,
            cursor_row: 0,
            cursor_col: 4,
        };
        let mut rendered_grid = Vec::new();

        let rows = render_completions(&mut rendered_grid, &state, info, true, 0)
            .expect("render completions");
        assert_eq!(rows, 3);
        let rendered = String::from_utf8(rendered_grid).expect("utf8 render");
        assert!(rendered.contains("alpha"), "{rendered:?}");
        assert!(rendered.contains("\x1b[7mbeta\x1b[0m"), "{rendered:?}");

        let mut cleared = Vec::new();
        clear_completions(&mut cleared, info, rows).expect("clear completions");
        let clear = String::from_utf8(cleared).expect("utf8 clear");
        assert!(clear.contains("\x1b[K"), "{clear:?}");
        assert!(clear.ends_with("\x1b[?25h"), "{clear:?}");
    }

    #[test]
    fn history_search_wrapped_query_places_cursor_in_header() {
        let history = History::from_entries(Vec::new());
        let query = LineBuffer::from_text("abcd");
        let mut out = Vec::new();

        let region = render_history_search(
            &mut out,
            &query,
            &[],
            &history,
            0,
            (24, 10),
            RenderedRegion::default(),
        )
        .expect("render history search");

        assert_eq!(region.cursor_row, 1);
        assert_eq!(region.cursor_col, 2);
        assert_eq!(region.painted_rows, 2);
    }

    #[test]
    fn history_search_rerender_after_wrapped_query_clears_from_top() {
        let history = History::from_entries(Vec::new());
        let wrapped = LineBuffer::from_text("abcd");
        let short = LineBuffer::from_text("a");
        let mut out = Vec::new();

        let prev = render_history_search(
            &mut out,
            &wrapped,
            &[],
            &history,
            0,
            (24, 10),
            RenderedRegion::default(),
        )
        .expect("render wrapped history search");
        out.clear();

        render_history_search(&mut out, &short, &[], &history, 0, (24, 10), prev)
            .expect("rerender history search");
        let rendered = String::from_utf8(out).expect("utf8 render");

        assert!(rendered.starts_with("\x1b[?25l\x1b[1A\r"), "{rendered:?}");
        assert!(
            rendered
                .as_bytes()
                .windows(8)
                .any(|w| w == b"\x1b[1A\r\x1b[J"),
            "{rendered:?}"
        );
    }
}
