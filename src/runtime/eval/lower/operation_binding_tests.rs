use super::*;
use crate::sema::check::Checker;
use crate::source::SourceId;
use crate::syntax::parser::Parser;

#[test]
fn selected_method_binding_cannot_be_reconstructed_when_source_proof_is_missing() {
    crate::runtime::eval::run_eval(|| {
        let source = "let parts = \"a,b\".split(separator: \",\")\n";
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let mut declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(declarations.diagnostics.is_empty(), "{:?}", declarations.diagnostics);
        let mut bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        parsed.arena.symbol_owner().with_current(|| {
            let valid = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
            assert_eq!(valid.blocker_events, 0, "a checked method uses its selected source binding");
            assert!(!bodies.solved.operations.is_empty());
            declarations.solved = Default::default();
            Arc::get_mut(&mut bodies.solved).expect("test owns the last solved snapshot").operations.clear();
            let absent = probe_compact_lower_constructed_bodies(&parsed.arena, &declarations, &bodies, source);
            assert!(absent.blocker_events > 0, "missing source evidence must refuse lowering instead of searching the registry again");
        });
    });
}
