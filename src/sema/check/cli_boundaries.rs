#[cfg(test)]
mod tests {
    use crate::sema::check::Checker;

    #[test]
    fn cli_static_descriptor_postfix_preserves_original_refined_boundary() {
        let source = "let operands = cli.parse([\"--\", \"--name\"], {name: {positional: true}}, \"demo\")?.get(\"name\") ?? \"\"\n";
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(crate::source::SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        checked.solved.validate().unwrap();
    }
}
