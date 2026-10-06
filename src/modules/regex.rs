use crate::runtime::value::RuntimeError;
use crate::source::Span;

pub(crate) mod posix;

pub(crate) fn find_bytes(pattern: &str, input: &[u8], extended: bool, ignore_case: bool, span: Span) -> Result<crate::runtime::value::Value, RuntimeError> {
    use crate::runtime::value::{RecordMap, Value};
    let fail = |message| RuntimeError::new("regex-match", message).with_span(span);
    let regex = posix::PosixRegex::compile(pattern, extended, ignore_case).map_err(fail)?;
    let mut matches = Vec::new();
    let mut offset = 0;
    while let Some(found) = regex.captures(input, offset).map_err(fail)? {
        let Some((start, end)) = found[0] else { break; };
        let mut record = RecordMap::new();
        record.insert("start".into(), Value::Int(start as i64));
        record.insert("end".into(), Value::Int(end as i64));
        matches.push(Value::Record(record));
        if end == input.len() { break; }
        offset = if end > start { end } else { end + 1 };
    }
    Ok(Value::List(matches))
}

pub(crate) fn captures_bytes(pattern: &str, input: &[u8], offset: i64, extended: bool, ignore_case: bool, span: Span) -> Result<crate::runtime::value::Value, RuntimeError> {
    use crate::runtime::value::{RecordMap, Value};
    let fail = |message| RuntimeError::new("regex-match", message).with_span(span);
    let regex = posix::PosixRegex::compile(pattern, extended, ignore_case).map_err(fail)?;
    let offset = usize::try_from(offset).map_err(|_| fail("match offset must be nonnegative".to_string()))?;
    let found = regex.captures(input, offset).map_err(fail)?;
    Ok(Value::List(found.unwrap_or_default().into_iter().map(|capture| match capture {
        Some((start, end)) => Value::Record(RecordMap::from([("start".into(), Value::Int(start as i64)), ("end".into(), Value::Int(end as i64))])),
        None => Value::Null,
    }).collect()))
}

#[allow(clippy::single_call_fn)]
pub(crate) fn compile(pattern: &str, span: Span) -> Result<regex_lite::Regex, RuntimeError> {
    regex_lite::Regex::new(pattern)
        .map_err(|error| RuntimeError::new("regex-compile", error.to_string()).with_span(span))
}

// Checked preparation owns compilation; lowering only borrows the completed cell.
pub(crate) fn prepare_literal(
    literal: &crate::syntax::arena::ArenaRegexLiteral,
) -> &Result<std::sync::Arc<regex_lite::Regex>, String> {
    literal.prepared.get_or_init(|| {
        compile(&literal.pattern, literal.span)
            .map(std::sync::Arc::new)
            .map_err(|error| error.message)
    })
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::DiagnosticCode;
    use crate::sema::check::Checker;
    use crate::source::SourceId;
    use crate::syntax::parser::Parser;
    use std::sync::Arc;

    #[test]
    fn checked_regex_literals_share_preparation_across_frontend_passes_and_clones() {
        let source = r#"pure prepared() -> Regex { rx"(?i)^\s*[a-z]+$" }"#;
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let cloned = parsed.arena.clone();
        let literal = &parsed.arena.arena.regex_literals[0];
        assert!(literal.prepared.get().is_none());
        assert_eq!(literal.pattern.as_ref(), r"(?i)^\s*[a-z]+$");
        assert_eq!(&source[literal.span.range()], literal.source_text.as_ref());
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let first = Arc::clone(literal.prepared.get().unwrap().as_ref().unwrap());
        let checked = Checker::check_arena(&cloned, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declarations = Checker::check_compact_declarations(&cloned);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        assert!(Arc::ptr_eq(
            &first,
            cloned.arena.regex_literals[0]
                .prepared
                .get()
                .unwrap()
                .as_ref()
                .unwrap()
        ));
    }

    #[test]
    fn invalid_regex_literals_are_diagnosed_even_in_unreachable_functions() {
        let source =
            "pure never_called() -> Regex { rx\"(\" }\npure another() -> Regex { rx\"[\" }\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        for checked in [
            Checker::check_arena(&parsed.arena, source),
            Checker::check_arena(&parsed.arena, source),
        ] {
            let errors = checked
                .diagnostics
                .iter()
                .filter(|error| error.code == Some(DiagnosticCode::CheckRegexLiteral))
                .collect::<Vec<_>>();
            assert_eq!(errors.len(), 2, "{:?}", checked.diagnostics);
            for (error, literal) in errors.iter().zip(&parsed.arena.arena.regex_literals) {
                assert_eq!(error.labels[0].span, literal.span);
            }
        }
        assert!(
            parsed
                .arena
                .arena
                .regex_literals
                .iter()
                .all(|literal| literal.prepared.get().unwrap().is_err())
        );
    }
}
