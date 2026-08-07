#![allow(clippy::single_call_fn, dead_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableAlign {
    Left,
    Right,
}

#[derive(Clone, Debug)]
pub(crate) struct TextTableColumn {
    pub(crate) header: String,
    pub(crate) min_width: usize,
    pub(crate) max_width: usize,
    pub(crate) align: TableAlign,
}

impl TextTableColumn {
    pub(crate) fn new(
        header: impl Into<String>,
        min_width: usize,
        max_width: usize,
        align: TableAlign,
    ) -> Self {
        Self {
            header: header.into(),
            min_width,
            max_width: max_width.max(min_width),
            align,
        }
    }
}

pub(crate) fn render_text_table(
    columns: &[TextTableColumn],
    rows: &[Vec<String>],
    terminal_width: usize,
    output: &mut String,
) {
    let widths = table_column_widths(columns, rows, terminal_width);
    render_table_border('┌', '┬', '┐', &widths, output);
    render_table_row(
        &columns
            .iter()
            .map(|column| column.header.clone())
            .collect::<Vec<_>>(),
        columns,
        &widths,
        output,
    );
    render_table_border('├', '┼', '┤', &widths, output);
    for (index, row) in rows.iter().enumerate() {
        render_table_row(row, columns, &widths, output);
        if index + 1 < rows.len() {
            render_table_border('├', '┼', '┤', &widths, output);
        }
    }
    render_table_border('└', '┴', '┘', &widths, output);
}

fn table_column_widths(
    columns: &[TextTableColumn],
    rows: &[Vec<String>],
    terminal_width: usize,
) -> Vec<usize> {
    let mut widths: Vec<_> = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let content_width = rows
                .iter()
                .filter_map(|row| row.get(index))
                .map(|cell| table_text_width(&sanitize_table_text(cell)))
                .max()
                .unwrap_or(0);
            table_text_width(&sanitize_table_text(&column.header))
                .max(content_width)
                .max(column.min_width)
                .min(column.max_width)
        })
        .collect();
    let min_widths: Vec<_> = columns.iter().map(|column| column.min_width).collect();
    let target_width = terminal_width.max(table_width(&min_widths));
    while table_width(&widths) > target_width {
        let Some(index) = widths
            .iter()
            .enumerate()
            .filter(|(index, width)| **width > min_widths[*index])
            .max_by_key(|(_, width)| *width)
            .map(|(index, _)| index)
        else {
            break;
        };
        widths[index] -= 1;
    }
    widths
}

fn render_table_border(
    left: char,
    separator: char,
    right: char,
    widths: &[usize],
    output: &mut String,
) {
    output.push(left);
    for (index, width) in widths.iter().enumerate() {
        for _ in 0..(*width + 2) {
            output.push('─');
        }
        output.push(if index + 1 == widths.len() {
            right
        } else {
            separator
        });
    }
    output.push('\n');
}

fn render_table_row(
    cells: &[String],
    columns: &[TextTableColumn],
    widths: &[usize],
    output: &mut String,
) {
    let wrapped_cells: Vec<_> = widths
        .iter()
        .enumerate()
        .map(|(index, width)| {
            let cell = cells.get(index).map_or("", String::as_str);
            wrap_table_text(&sanitize_table_text(cell), *width)
        })
        .collect();
    let row_height = wrapped_cells.iter().map(Vec::len).max().unwrap_or(1);
    for line_index in 0..row_height {
        output.push('│');
        for (index, width) in widths.iter().enumerate() {
            let cell = wrapped_cells
                .get(index)
                .and_then(|lines| lines.get(line_index))
                .map_or("", String::as_str);
            let padding = width.saturating_sub(table_text_width(cell));
            output.push(' ');
            if columns[index].align == TableAlign::Right {
                for _ in 0..padding {
                    output.push(' ');
                }
            }
            output.push_str(cell);
            if columns[index].align == TableAlign::Left {
                for _ in 0..padding {
                    output.push(' ');
                }
            }
            output.push(' ');
            output.push('│');
        }
        output.push('\n');
    }
}

pub(crate) fn table_width(widths: &[usize]) -> usize {
    widths.iter().sum::<usize>() + (widths.len() * 3) + 1
}

pub(crate) fn table_text_width(text: &str) -> usize {
    text.chars().count()
}

fn wrap_table_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;
    for ch in text.chars() {
        if ch == '\n' {
            lines.push(current);
            current = String::new();
            current_width = 0;
            continue;
        }
        current.push(ch);
        current_width += 1;
        if current_width == width {
            lines.push(current);
            current = String::new();
            current_width = 0;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

pub(crate) fn sanitize_table_text(text: &str) -> String {
    let mut output = String::new();
    for ch in text.chars() {
        match ch {
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch.is_control() => output.push('?'),
            ch => output.push(ch),
        }
    }
    output
}

pub(crate) fn terminal_table_width_for_stderr(min_width: usize, default_width: usize) -> usize {
    std::env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|width| *width >= min_width)
        .unwrap_or(default_width)
        .max(min_width)
}
