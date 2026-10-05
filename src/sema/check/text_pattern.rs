//! Text patterns: `f"{key}={value}"` in pattern position.
//!
//! The checker compiles each one here, once, into the literal segments and
//! hole kinds that matching needs. Lowering copies that into the program and
//! the executor matches with it; neither reads the pattern's source again.

use super::Checker;
use crate::diagnostic::{Diagnostic, DiagnosticCode, Label};
use crate::sema::types::Type;
use crate::source::Span;
use crate::syntax::arena::{ArenaExprKind, ArenaPatternKind, ArenaProgram, ArenaRange};
use std::sync::Arc;

/// What a hole makes of the text it matched. A spec letter selects one, with
/// Python's meaning for the letter read in reverse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum TextHoleKind {
    /// No spec, or `s`: the text itself.
    Text,
    /// `d`: a decimal integer with an optional sign.
    Decimal,
    /// `x`: a hexadecimal integer with an optional sign and `0x`.
    Hex,
    /// `o`: an octal integer with an optional sign and `0o`.
    Octal,
    /// `b`: a binary integer with an optional sign and `0b`.
    Binary,
    /// `f`, `e`, or `g`: a finite decimal float.
    Float,
}

/// What a hole produced from the text it matched.
#[derive(Clone, Debug, PartialEq)]
pub enum TextHoleValue<'a> {
    Text(&'a str),
    Int(i64),
    Float(f64),
}

impl TextHoleKind {
    pub const ALL: [Self; 6] = [
        Self::Text,
        Self::Decimal,
        Self::Hex,
        Self::Octal,
        Self::Binary,
        Self::Float,
    ];

    /// The kind a spec selects. A spec is one letter: width, fill,
    /// alignment, sign, and precision are not part of matching.
    pub fn from_spec(spec: &str) -> Option<Self> {
        Some(match spec {
            "s" => Self::Text,
            "d" => Self::Decimal,
            "x" => Self::Hex,
            "o" => Self::Octal,
            "b" => Self::Binary,
            "f" | "e" | "g" => Self::Float,
            _ => return None,
        })
    }

    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    /// The type of the name a hole of this kind binds.
    pub fn ty(self) -> Type {
        match self {
            Self::Text => Type::Str,
            Self::Decimal | Self::Hex | Self::Octal | Self::Binary => Type::Int,
            Self::Float => Type::Float,
        }
    }

    /// The value of `text` for a hole of this kind, or `None` when the text
    /// is not one: the pattern then does not match.
    pub fn convert(self, text: &str) -> Option<TextHoleValue<'_>> {
        let integer = |radix: u32, prefix: &str| {
            let (negative, digits) = match text.as_bytes().first() {
                Some(b'-') => (true, &text[1..]),
                Some(b'+') => (false, &text[1..]),
                _ => (false, text),
            };
            let digits = if prefix.is_empty() {
                digits
            } else {
                digits
                    .strip_prefix(prefix)
                    .or_else(|| digits.strip_prefix(&prefix.to_ascii_uppercase()))
                    .unwrap_or(digits)
            };
            // The sign was read above; the digits carry none of their own.
            if digits.is_empty() || !digits.bytes().all(|digit| digit.is_ascii_alphanumeric()) {
                return None;
            }
            let signed = if negative {
                format!("-{digits}")
            } else {
                digits.to_owned()
            };
            i64::from_str_radix(&signed, radix)
                .ok()
                .map(TextHoleValue::Int)
        };
        match self {
            Self::Text => Some(TextHoleValue::Text(text)),
            Self::Decimal => integer(10, ""),
            Self::Hex => integer(16, "0x"),
            Self::Octal => integer(8, "0o"),
            Self::Binary => integer(2, "0b"),
            Self::Float => {
                let spelled = text.bytes().all(|byte| {
                    byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.' | b'e' | b'E')
                }) && text.bytes().any(|byte| byte.is_ascii_digit());
                if !spelled {
                    return None;
                }
                text.parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite())
                    .map(TextHoleValue::Float)
            }
        }
    }
}

/// A text pattern as compiled: `holes.len() + 1` literal segments with one
/// hole between each two. The first and last segments may be empty; the ones
/// between two holes never are, so where each hole ends is decided by the
/// text alone.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextPattern {
    pub segments: Vec<Arc<str>>,
    pub holes: Vec<TextHoleKind>,
}

impl TextPattern {
    /// The text each hole takes when `subject` matches, in order.
    pub fn split<'a>(&self, subject: &'a str) -> Option<Vec<&'a str>> {
        split_text(&self.segments, subject)
    }
}

/// The text each hole takes when `subject` matches the literal `segments`
/// with one hole between each two, in order.
///
/// The whole subject is matched. Each hole takes the shortest text that lets
/// the rest match: up to the first place its following literal occurs, with
/// the last literal anchored at the end of the subject, so the last hole
/// takes whatever remains before it. A hole may take the empty text.
pub fn split_text<'a, S: AsRef<str>>(segments: &[S], subject: &'a str) -> Option<Vec<&'a str>> {
    let (first, rest) = segments.split_first()?;
    let Some((last, middle)) = rest.split_last() else {
        return (subject == first.as_ref()).then(Vec::new);
    };
    let body = subject.strip_prefix(first.as_ref())?;
    let mut body = body.strip_suffix(last.as_ref())?;
    let mut texts = Vec::with_capacity(rest.len());
    for literal in middle {
        let at = body.find(literal.as_ref())?;
        texts.push(&body[..at]);
        body = &body[at + literal.as_ref().len()..];
    }
    texts.push(body);
    Some(texts)
}

impl Checker {
    /// Checks the text pattern `pattern` against a value of type `value_ty`,
    /// binds its holes, and publishes the compiled pattern.
    pub(super) fn check_text_pattern_arena(
        &mut self,
        arena: &ArenaProgram,
        parts: ArenaRange,
        value_ty: &Type,
        span: Span,
    ) {
        self.text_patterns.remove(&span);
        let reported = self.diagnostics.len();
        let text_subject = match value_ty {
            Type::Str | Type::Any | Type::Unknown | Type::Invalid => true,
            Type::Optional(inner) => **inner == Type::Str,
            _ => false,
        };
        if !text_subject {
            self.text_pattern_error(
                span,
                format!("a text pattern matches a Str, but the value is {value_ty}"),
                "this pattern matches text",
            );
        }
        let mut segments: Vec<Arc<str>> = vec![Arc::from("")];
        let mut holes = Vec::new();
        let mut after_hole = false;
        for part_id in arena.arena.pattern_ids(parts) {
            let part = arena.arena.pattern(part_id);
            let part_span = arena.arena.span(part.span);
            match &part.kind {
                ArenaPatternKind::Literal(expr) => {
                    let ArenaExprKind::Str(text) = arena.arena.expr(*expr).kind else {
                        continue;
                    };
                    *segments.last_mut().expect("a segment is open") =
                        arena.arena.string_literal(text).clone();
                    after_hole = false;
                }
                ArenaPatternKind::TextHole { binding, spec } => {
                    if after_hole {
                        self.diagnostics.push(
                            Diagnostic::error("two holes of a text pattern have no text between them")
                                .with_code(DiagnosticCode::CheckTextPattern)
                                .with_label(Label::primary(
                                    part_span,
                                    "where the hole before this one ends is ambiguous",
                                ))
                                .with_note("write the text that separates them, or match one hole and split it afterwards"),
                        );
                    }
                    let kind = match spec {
                        None => Some(TextHoleKind::Text),
                        Some(spec) => TextHoleKind::from_spec(&spec.as_str()),
                    };
                    let Some(kind) = kind else {
                        let spec = spec.map(|spec| spec.to_string()).unwrap_or_default();
                        self.diagnostics.push(
                            Diagnostic::error(format!("unsupported spec `{spec}` in a text pattern hole"))
                                .with_code(DiagnosticCode::CheckTextPattern)
                                .with_label(Label::primary(
                                    part_span,
                                    "a spec is one of `s`, `d`, `x`, `o`, `b`, `f`, `e`, `g`",
                                ))
                                .with_note("a hole takes no width, fill, alignment, sign, or precision: it matches the shortest text and then converts it"),
                        );
                        // The name is still a name of the arm, so the one
                        // error is the spec.
                        if let Some(name) = binding {
                            self.define_pattern_binding(*name, Type::Unknown, part_span);
                        }
                        after_hole = true;
                        continue;
                    };
                    if let Some(name) = binding {
                        self.define_pattern_binding(*name, kind.ty(), part_span);
                    }
                    holes.push(kind);
                    segments.push(Arc::from(""));
                    after_hole = true;
                }
                _ => {}
            }
        }
        if self.diagnostics.len() == reported {
            self.text_patterns
                .insert(span, TextPattern { segments, holes });
        }
    }

    fn text_pattern_error(&mut self, span: Span, message: String, label: &str) {
        self.diagnostics.push(
            Diagnostic::error(message)
                .with_code(DiagnosticCode::CheckTextPattern)
                .with_label(Label::primary(span, label)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{TextHoleKind, TextHoleValue, TextPattern};
    use std::sync::Arc;

    fn pattern(segments: &[&str]) -> TextPattern {
        TextPattern {
            segments: segments.iter().map(|segment| Arc::from(*segment)).collect(),
            holes: vec![TextHoleKind::Text; segments.len() - 1],
        }
    }

    #[test]
    fn each_hole_takes_the_shortest_text_and_the_last_takes_the_rest() {
        assert_eq!(
            pattern(&["", "=", ""]).split("a=b=c"),
            Some(vec!["a", "b=c"])
        );
        assert_eq!(pattern(&["", "=", ""]).split("a="), Some(vec!["a", ""]));
        assert_eq!(pattern(&["", "=", ""]).split("abc"), None);
        assert_eq!(
            pattern(&["#define ", " ", ""]).split("#define MAX 4 + 4"),
            Some(vec!["MAX", "4 + 4"])
        );
        // The last literal is the end of the subject, not its first occurrence.
        assert_eq!(
            pattern(&["", ".txt"]).split("a.txt.txt"),
            Some(vec!["a.txt"])
        );
        assert_eq!(
            pattern(&["", ".", ".txt"]).split("a.b.txt"),
            Some(vec!["a", "b"])
        );
        assert_eq!(pattern(&["<", ">"]).split("<>"), Some(vec![""]));
        assert_eq!(pattern(&["<", ">"]).split("<"), None);
        assert_eq!(pattern(&["ab", "ba"]).split("aba"), None);
        assert_eq!(pattern(&["exact"]).split("exact"), Some(vec![]));
        assert_eq!(pattern(&["exact"]).split("exactly"), None);
        assert_eq!(
            pattern(&["", "é", ""]).split("caféine"),
            Some(vec!["caf", "ine"])
        );
    }

    #[test]
    fn a_typed_hole_converts_exactly_the_text_its_letter_names() {
        let int = |kind: TextHoleKind, text: &str| match kind.convert(text) {
            Some(TextHoleValue::Int(value)) => Some(value),
            _ => None,
        };
        assert_eq!(int(TextHoleKind::Decimal, "-007"), Some(-7));
        assert_eq!(int(TextHoleKind::Decimal, "+12"), Some(12));
        assert_eq!(
            int(TextHoleKind::Decimal, "9223372036854775807"),
            Some(i64::MAX)
        );
        assert_eq!(
            int(TextHoleKind::Decimal, "-9223372036854775808"),
            Some(i64::MIN)
        );
        for refused in [
            "",
            "-",
            "1_000",
            " 1",
            "0x10",
            "9223372036854775808",
            "--1",
            "+-1",
            "1.0",
        ] {
            assert_eq!(int(TextHoleKind::Decimal, refused), None, "{refused:?}");
        }
        assert_eq!(int(TextHoleKind::Hex, "ff"), Some(255));
        assert_eq!(int(TextHoleKind::Hex, "-0xFF"), Some(-255));
        assert_eq!(int(TextHoleKind::Hex, "0X1f"), Some(31));
        assert_eq!(int(TextHoleKind::Hex, "0x"), None);
        assert_eq!(int(TextHoleKind::Hex, "g"), None);
        assert_eq!(int(TextHoleKind::Octal, "0o17"), Some(15));
        assert_eq!(int(TextHoleKind::Octal, "8"), None);
        assert_eq!(int(TextHoleKind::Binary, "0b101"), Some(5));
        assert_eq!(int(TextHoleKind::Binary, "2"), None);
        assert_eq!(
            TextHoleKind::Float.convert("-1.5e3"),
            Some(TextHoleValue::Float(-1500.0))
        );
        assert_eq!(
            TextHoleKind::Float.convert("7"),
            Some(TextHoleValue::Float(7.0))
        );
        for refused in ["", "nan", "inf", "-inf", "1e999", ".", "e", "1.5 "] {
            assert_eq!(TextHoleKind::Float.convert(refused), None, "{refused:?}");
        }
        assert_eq!(
            TextHoleKind::Text.convert(""),
            Some(TextHoleValue::Text(""))
        );
        for (index, kind) in TextHoleKind::ALL.iter().enumerate() {
            assert_eq!(TextHoleKind::from_index(index), Some(*kind));
            assert_eq!(*kind as usize, index);
        }
        assert_eq!(TextHoleKind::from_index(TextHoleKind::ALL.len()), None);
    }
}
