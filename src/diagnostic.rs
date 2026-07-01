use crate::source::{SourceMap, Span};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Note,
}

impl Severity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
            Self::Note => "note",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelStyle {
    Primary,
    Secondary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Label {
    pub style: LabelStyle,
    pub span: Span,
    pub message: Option<String>,
}

impl Label {
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Self {
            style: LabelStyle::Primary,
            span,
            message: non_empty(message.into()),
        }
    }

    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Self {
            style: LabelStyle::Secondary,
            span,
            message: non_empty(message.into()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixHint {
    pub span: Option<Span>,
    pub message: String,
    pub replacement: Option<String>,
    pub dangerous: bool,
}

impl FixHint {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            span: None,
            message: message.into(),
            replacement: None,
            dangerous: false,
        }
    }

    pub fn replacement(
        span: Span,
        message: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            span: Some(span),
            message: message.into(),
            replacement: Some(replacement.into()),
            dangerous: false,
        }
    }

    pub fn deletion(span: Span, message: impl Into<String>) -> Self {
        Self {
            span: Some(span),
            message: message.into(),
            replacement: Some(String::new()),
            dangerous: false,
        }
    }

    pub fn dangerous(mut self) -> Self {
        self.dangerous = true;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<String>,
    pub message: String,
    pub span: Option<Span>,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub fix_hints: Vec<FixHint>,
}

impl Diagnostic {
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            code: None,
            message: message.into(),
            span: None,
            labels: Vec::new(),
            notes: Vec::new(),
            fix_hints: Vec::new(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_fix_hint(mut self, fix_hint: FixHint) -> Self {
        self.fix_hints.push(fix_hint);
        self
    }

    pub fn to_machine(&self, sources: &SourceMap) -> MachineDiagnostic {
        MachineDiagnostic {
            severity: self.severity.as_str().to_string(),
            code: self.code.clone(),
            message: self.message.clone(),
            span: self
                .span
                .and_then(|span| MachineSpan::from_span(sources, span)),
            labels: self
                .labels
                .iter()
                .filter_map(|label| MachineLabel::from_label(sources, label))
                .collect(),
            notes: self.notes.clone(),
            fix_hints: self
                .fix_hints
                .iter()
                .map(|hint| MachineFixHint {
                    message: hint.message.clone(),
                    replacement: hint.replacement.clone(),
                    span: hint
                        .span
                        .and_then(|span| MachineSpan::from_span(sources, span)),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineDiagnostic {
    pub severity: String,
    pub code: Option<String>,
    pub message: String,
    pub span: Option<MachineSpan>,
    pub labels: Vec<MachineLabel>,
    pub notes: Vec<String>,
    pub fix_hints: Vec<MachineFixHint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineLabel {
    pub style: String,
    pub span: MachineSpan,
    pub message: Option<String>,
}

impl MachineLabel {
    #[allow(clippy::single_call_fn)]
    fn from_label(sources: &SourceMap, label: &Label) -> Option<Self> {
        Some(Self {
            style: match label.style {
                LabelStyle::Primary => "primary",
                LabelStyle::Secondary => "secondary",
            }
            .to_string(),
            span: MachineSpan::from_span(sources, label.span)?,
            message: label.message.clone(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineFixHint {
    pub message: String,
    pub replacement: Option<String>,
    pub span: Option<MachineSpan>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineSpan {
    pub file: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl MachineSpan {
    fn from_span(sources: &SourceMap, span: Span) -> Option<Self> {
        let start = sources.location(span.source_id, span.start())?;
        let end = sources.location(span.source_id, span.end())?;
        Some(Self {
            file: start.file,
            start_offset: span.start(),
            end_offset: span.end(),
            start_line: start.line,
            start_column: start.column,
            end_line: end.line,
            end_column: end.column,
        })
    }
}

const SGR_RESET: &str = "\x1b[0m";
const SGR_BOLD_RED: &str = "\x1b[1;31m";
const SGR_BOLD_YELLOW: &str = "\x1b[1;33m";
const SGR_BOLD_BLUE: &str = "\x1b[1;34m";
const SGR_BOLD_CYAN: &str = "\x1b[1;36m";
const SGR_DIM: &str = "\x1b[2m";
const SGR_CYAN: &str = "\x1b[36m";
const SGR_GREEN: &str = "\x1b[32m";
const SGR_MAGENTA: &str = "\x1b[35m";

const XSH_KEYWORDS: &[&str] = &[
    "and", "as", "break", "catch", "const", "continue", "do", "elif", "else", "export", "false",
    "fn", "for", "if", "import", "in", "is", "let", "loop", "match", "mod", "mut", "not", "null",
    "or", "proc", "pub", "return", "throw", "true", "try", "use", "var", "while",
];

#[derive(Clone, Debug)]
pub struct DiagnosticRenderer {
    color: bool,
}

impl Default for DiagnosticRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosticRenderer {
    pub fn new() -> Self {
        Self {
            color: rustix::termios::isatty(rustix::stdio::stderr())
                && std::env::var_os("NO_COLOR").is_none(),
        }
    }

    pub fn with_color(mut self, color: bool) -> Self {
        self.color = color;
        self
    }

    pub fn render(&self, diagnostics: &[Diagnostic], sources: &SourceMap) -> String {
        let mut output = String::new();
        for (index, diagnostic) in diagnostics.iter().enumerate() {
            if index > 0 {
                output.push('\n');
            }
            self.render_one(diagnostic, sources, &mut output);
        }
        output
    }

    fn render_one(&self, diagnostic: &Diagnostic, sources: &SourceMap, output: &mut String) {
        let sev_str = match diagnostic.severity {
            Severity::Error => "err",
            Severity::Warning => "warn",
            Severity::Info => "info",
            Severity::Note => "note",
        };

        if self.color {
            output.push_str(severity_sgr(diagnostic.severity));
        }
        output.push_str(sev_str);
        if let Some(code) = &diagnostic.code {
            if self.color {
                output.push_str(SGR_DIM);
            }
            output.push('[');
            output.push_str(code);
            output.push(']');
            if self.color {
                output.push_str(SGR_RESET);
                output.push_str(severity_sgr(diagnostic.severity));
            }
        }
        output.push_str(": ");
        if self.color {
            output.push_str(SGR_RESET);
        }
        output.push_str(&diagnostic.message);
        output.push('\n');

        let primary_span = diagnostic
            .labels
            .iter()
            .find(|label| label.style == LabelStyle::Primary)
            .map(|label| label.span)
            .or_else(|| diagnostic.labels.first().map(|label| label.span))
            .or(diagnostic.span);

        if let Some(span) = primary_span
            && let Some(location) = sources.location(span.source_id, span.start())
        {
            output.push_str("  ");
            if self.color {
                output.push_str(SGR_CYAN);
            }
            output.push_str(&location.file);
            if self.color {
                output.push_str(SGR_DIM);
            }
            output.push(':');
            output.push_str(&location.line.to_string());
            output.push(':');
            output.push_str(&location.column.to_string());
            if self.color {
                output.push_str(SGR_RESET);
            }
            output.push('\n');
        }

        let mut labels = diagnostic.labels.clone();
        labels.sort_unstable_by_key(|label| {
            (
                label.span.source_id,
                label.span.start(),
                label.span.end(),
                match label.style {
                    LabelStyle::Primary => 0,
                    LabelStyle::Secondary => 1,
                },
            )
        });

        for label in &labels {
            self.render_label(label, diagnostic.severity, sources, output);
        }

        for note in &diagnostic.notes {
            if self.color {
                output.push_str(SGR_DIM);
            }
            output.push_str("note: ");
            if self.color {
                output.push_str(SGR_RESET);
            }
            output.push_str(note);
            output.push('\n');
        }

        for hint in &diagnostic.fix_hints {
            if self.color {
                output.push_str(SGR_DIM);
            }
            output.push_str("help: ");
            if self.color {
                output.push_str(SGR_RESET);
            }
            output.push_str(&hint.message);
            if let Some(replacement) = &hint.replacement
                && !replacement.is_empty()
            {
                output.push_str(" -> ");
                output.push_str(replacement);
            }
            output.push('\n');
        }
    }

    fn render_label(
        &self,
        label: &Label,
        severity: Severity,
        sources: &SourceMap,
        output: &mut String,
    ) {
        let Some(file) = sources.get(label.span.source_id) else {
            return;
        };
        let Some(start) = file.location(label.span.start()) else {
            return;
        };
        let Some(line_text) = file.line_text(start.line) else {
            return;
        };

        output.push_str("  ");
        if self.color {
            highlight_xsh_line(line_text, output);
        } else {
            output.push_str(line_text);
        }
        output.push('\n');

        output.push_str("  ");
        let prefix_width = start.column.saturating_sub(1);
        output.push_str(&" ".repeat(prefix_width));
        let underline_width = underline_width(file, label.span, start.line, start.column);
        let marker = match label.style {
            LabelStyle::Primary => '^',
            LabelStyle::Secondary => '-',
        };

        if self.color {
            output.push_str(severity_sgr(severity));
        }
        output.push_str(&marker.to_string().repeat(underline_width.max(1)));
        if let Some(message) = &label.message {
            output.push(' ');
            output.push_str(message);
        }
        if self.color {
            output.push_str(SGR_RESET);
        }
        output.push('\n');
    }
}

fn severity_sgr(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => SGR_BOLD_RED,
        Severity::Warning => SGR_BOLD_YELLOW,
        Severity::Info => SGR_BOLD_BLUE,
        Severity::Note => SGR_BOLD_CYAN,
    }
}

fn highlight_xsh_line(line: &str, output: &mut String) {
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if c == '#' {
            output.push_str(SGR_DIM);
            while i < chars.len() {
                output.push(chars[i]);
                i += 1;
            }
            output.push_str(SGR_RESET);
            break;
        }

        if c == '"' || c == '\'' {
            output.push_str(SGR_GREEN);
            let quote = c;
            output.push(c);
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                output.push(ch);
                if ch == '\\' && i + 1 < chars.len() {
                    i += 1;
                    output.push(chars[i]);
                } else if ch == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            output.push_str(SGR_RESET);
            continue;
        }

        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if XSH_KEYWORDS.contains(&word.as_str()) {
                output.push_str(SGR_MAGENTA);
                output.push_str(&word);
                output.push_str(SGR_RESET);
            } else {
                output.push_str(&word);
            }
            continue;
        }

        output.push(c);
        i += 1;
    }
}

#[allow(clippy::single_call_fn)]
fn underline_width(
    file: &crate::source::SourceFile,
    span: Span,
    line: usize,
    column: usize,
) -> usize {
    if span.is_empty() {
        return 1;
    }

    let Some(end) = file.location(span.end()) else {
        return 1;
    };
    if end.line == line {
        return end.column.saturating_sub(column).max(1);
    }

    let Some(line_text) = file.line_text(line) else {
        return 1;
    };
    line_text.chars().count().saturating_sub(column - 1).max(1)
}

fn non_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, DiagnosticRenderer, FixHint, Label, Severity};
    use crate::source::{SourceMap, Span};

    #[test]
    fn renders_source_context_without_terminal_color() {
        let mut sources = SourceMap::new();
        let id = sources.add_file("example.xsh", "let answer =\n");
        let span = Span::new(id, 11, 12);
        let diagnostic = Diagnostic::error("expected expression")
            .with_code("parse.expected-expression")
            .with_label(Label::primary(span, "expected expression after `=`"))
            .with_note("bindings require an initializer");

        let rendered = DiagnosticRenderer::new()
            .with_color(false)
            .render(&[diagnostic], &sources);

        assert_eq!(
            rendered,
            "err[parse.expected-expression]: expected expression\n  example.xsh:1:12\n  let answer =\n             ^ expected expression after `=`\nnote: bindings require an initializer\n"
        );
    }

    #[test]
    fn renders_multiple_diagnostics_deterministically() {
        let mut sources = SourceMap::new();
        let id = sources.add_file("example.xsh", "let x = 1\nvar y = 2\n");
        let first = Diagnostic::new(Severity::Warning, "unused binding")
            .with_label(Label::secondary(Span::new(id, 4, 5), "unused binding"));
        let second = Diagnostic::error("assignment to immutable binding")
            .with_label(Label::primary(Span::new(id, 10, 13), "`let` binding"));

        let rendered = DiagnosticRenderer::new()
            .with_color(false)
            .render(&[first, second], &sources);

        assert!(rendered.contains("warn: unused binding\n  example.xsh:1:5"));
        assert!(rendered.contains("\n\nerr: assignment to immutable binding\n  example.xsh:2:1"));
    }

    #[test]
    fn exposes_machine_readable_diagnostics() {
        let mut sources = SourceMap::new();
        let id = sources.add_file("example.xsh", "print x\n");
        let span = Span::new(id, 0, 7);
        let diagnostic = Diagnostic::error("invalid core")
            .with_span(span)
            .with_label(Label::primary(span, "core command starts here"))
            .with_fix_hint(FixHint::new("use one of the core commands"));

        let machine = diagnostic.to_machine(&sources);

        assert_eq!(machine.severity, "error");
        assert_eq!(machine.span.unwrap().start_column, 1);
        assert_eq!(machine.labels[0].span.end_column, 8);
        assert_eq!(machine.fix_hints[0].message, "use one of the core commands");
    }
}
