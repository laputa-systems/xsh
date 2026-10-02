use super::*;
use crate::sema::check::{Checker, PatternCaptureIdentity};
use crate::syntax::parser::Parser;

#[test]
fn original_alternative_pattern_captures_keep_slots_and_uses_after_arena_disposal() {
    crate::runtime::eval::run_eval(|| {
        let source = "let selected = match [1, 2] {\n ([left, right] | [left, right]) as pair => left * 10 + right\n _ => 0\n}\n";
        let mut sources = SourceMap::new();
        let source_id = sources.add_file("pattern-transport.xsh", source);
        let parsed = Parser::parse_source_arena_only(source_id, source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
        let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
        assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
        assert!(Arc::ptr_eq(&checked.solved, &bodies.solved));
        let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
        let names = FxHashSet::default(); let qualified = FxHashSet::default();
        let functions = LowerableFunctions::all(&names, &names, &qualified, &qualified);
        let (lowered, stats) = lower_compact_top_level_program_with_probe(&parsed.arena, &declarations, &bodies, source, &sources, &functions);
        assert!(lowered.statements.iter().all(Option::is_some), "{:?}", stats);
        let expected = checked.solved.patterns.values().flat_map(|pattern| pattern.captures.iter().map(|capture| capture.identity)).collect::<Vec<_>>();
        assert!(expected.len() >= 7);
        drop(parsed);
        let scratch = lowered.scratch.borrow();
        assert!(!scratch.pattern_origins.is_empty(), "actual pattern rows retain their original checked identities");
        assert!(scratch.pattern_origins.values().all(|identity| checked.solved.patterns.contains_key(identity)));
        assert!(scratch.pattern_origins.iter().any(|(row, identity)| {
            matches!(checked.solved.patterns[identity].shape, crate::sema::check::SolvedPatternShape::Group)
                && matches!(scratch.patterns[row.index()], BuildPatternRow::Alternation { .. })
        }), "a represented group row keeps its original outer identity");
        for capture in expected {
            assert!(scratch.pattern_capture_slots.contains_key(&capture), "every original branch, join, and alias capture retains its allocated slot: {capture:?}");
        }
        let joined = checked.solved.patterns.iter().find(|(_, pattern)| matches!(pattern.decision, crate::sema::check::SolvedPatternDecision::Alternation)).unwrap();
        for capture in &joined.1.captures {
            let slot = scratch.pattern_capture_slots[&capture.identity];
            assert!(capture.branches.iter().all(|branch| scratch.pattern_capture_slots[branch] == slot));
            assert!(scratch.pattern_use_origins.values().any(|origin| *origin == capture.identity), "body reads retain the joined capture, not an arbitrary same-name branch");
        }
        assert!(scratch.pattern_use_origins.values().all(|capture: &PatternCaptureIdentity| checked.solved.patterns[&capture.pattern].captures.iter().any(|original| original.identity == *capture)));
    });
}

#[test]
fn original_slot_only_payload_patterns_keep_each_checked_capture_identity() {
    crate::runtime::eval::run_eval(|| {
        let source = r#"enum TransportTag { TransportEmpty, TransportPayload(Int, Str) }
error TransportFailure = Missing(message: Str) : NotFound
pure tag(value: TransportTag) -> Int {
 match value {
  TransportPayload(number, text) => number
  TransportEmpty => 0
 }
}
pure result(value: Result[Int]) -> Int {
 match value {
  Ok(number) => number
  Err(_) => 0
 }
}
pure error(value: TransportFailure) -> Str {
 match value {
  TransportFailure.Missing {message} => message
  _ => ""
 }
}
pure narrow(value: Any) -> Int {
 match value {
  number is Int => number
  _ => 0
 }
}
"#;
        let (checked, functions, _) = lower_checked_functions(source);
        let _symbols = checked.solved.symbol_owner().enter();
        let expected = checked.solved.patterns.values().flat_map(|pattern| pattern.captures.iter().map(|capture| capture.identity)).collect::<Vec<_>>();
        assert_eq!(expected.len(), 5);
        for capture in expected {
            let owner = checked.solved.patterns[&capture.pattern].caller.unwrap();
            let function = functions.iter().find(|function| function.solved_declaration == Some(owner)).unwrap();
            assert!(function.scratch.borrow().pattern_capture_slots.contains_key(&capture), "slot-only payloads retain leaf source identities: {capture:?}");
        }
        assert_eq!(functions.iter().map(|function| function.scratch.borrow().pattern_statement_use_origins.len()).sum::<usize>(), 4);
        for function in &functions {
            let scratch = function.scratch.borrow();
            for (row, capture) in scratch.pattern_statement_use_origins.values() {
                let super::super::BuildPatternUseRow::Expression(row) = row else { panic!("bare payload uses retain their actual expression row"); };
                assert!(matches!(scratch.expressions[row.index()], BuildExprRow::Param(slot) if slot == scratch.pattern_capture_slots[capture]));
            }
        }
    });
}

#[test]
fn original_pattern_capture_reads_survive_local_shadowing_and_scalar_specialization() {
    crate::runtime::eval::run_eval(|| {
        let source = r#"pure selected(values: List[Int]) -> Int {
 match values {
  [number] => {
   let before = number + 1
   {
    let number = 99
    let hidden = number + 1
   }
   if number > 0 { before + number } else { number }
  }
  _ => 0
 }
}
pure selected_flag(values: List[Bool]) -> Int {
 match values {
  [flag] => {
   var selected = 0
   if flag { selected = 1 }
   selected
  }
  _ => 0
 }
}
"#;
        let (checked, functions, original_uses) = lower_checked_functions(source);
        let _symbols = checked.solved.symbol_owner().enter();
        let scratch = functions[0].scratch.borrow();
        let expected = checked.solved.patterns.values().flat_map(|pattern| &pattern.captures).next().unwrap().identity;
        assert_eq!(scratch.pattern_capture_slots.len(), 1);
        let visible = source.match_indices("number").map(|(offset, _)| offset).collect::<Vec<_>>();
        assert_eq!(visible.len(), 7);
        for index in [1, 4, 5] {
            let identity = original_uses.identifiers[&visible[index]];
            assert_eq!(scratch.pattern_use_origins.get(&identity), Some(&expected), "original capture reads retain their exact lexical authority");
        }
        let shadowed_read = original_uses.identifiers[&visible[3]];
        assert!(!scratch.pattern_use_origins.contains_key(&shadowed_read), "an ordinary local shadows the pattern capture without inheriting its authority");
        assert!(scratch.pattern_use_origins.values().all(|capture| *capture == expected));
        assert_eq!(scratch.pattern_use_origins.len(), 3, "ordinary shadowed local reads do not acquire pattern authority");
        let terminal = original_uses.statements[&visible[6]];
        let (row, capture) = scratch.pattern_statement_use_origins[&terminal];
        assert_eq!(capture, expected);
        let super::super::BuildPatternUseRow::Expression(row) = row else { panic!("original bare tail retains the actual physical row"); };
        assert!(matches!(scratch.expressions[row.index()], BuildExprRow::Param(slot) if slot == scratch.pattern_capture_slots[&expected]));
        assert!(scratch.int_expression_origins.values().any(|origin| *origin == original_uses.identifiers[&visible[1]]), "specialized Int slot reads keep the original capture use identity");
        let boolean_scratch = functions[1].scratch.borrow();
        let flag_offset = source.find("if flag").unwrap() + 3;
        let flag_use = original_uses.identifiers[&flag_offset];
        let flag_capture = checked.solved.patterns.values().filter(|pattern| pattern.caller == functions[1].solved_declaration).flat_map(|pattern| &pattern.captures).next().unwrap().identity;
        assert_eq!(boolean_scratch.pattern_use_origins.get(&flag_use), Some(&flag_capture));
        assert!(boolean_scratch.bool_expression_origins.values().any(|origin| *origin == flag_use), "specialized Bool slot reads keep the actual original capture use identity");
    });
}

struct OriginalCaptureUseIds {
    identifiers: BTreeMap<usize, crate::sema::check::ExpressionIdentity>,
    statements: BTreeMap<usize, crate::sema::check::StatementIdentity>,
}

fn lower_checked_functions(source: &'static str) -> (crate::sema::check::CheckOutput, Vec<FunctionBuild>, OriginalCaptureUseIds) {
    let mut sources = SourceMap::new();
    let source_id = sources.add_file("pattern-function-transport.xsh", source);
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut original_uses = OriginalCaptureUseIds { identifiers: BTreeMap::new(), statements: BTreeMap::new() };
    for index in 0..parsed.arena.stats().expressions {
        let id = ExprId::from_index(index);
        let expression = parsed.arena.arena.expr(id);
        let identity = crate::sema::check::ExpressionIdentity { source: expression.span.source_id, namespace: None, expression: id };
        if matches!(expression.kind, ArenaExprKind::Ident(_)) { original_uses.identifiers.insert(expression.span.start(), identity); }
    }
    for index in 0..parsed.arena.stats().statements {
        let id = StmtId::from_index(index);
        let statement = parsed.arena.arena.stmt(id);
        if matches!(statement.kind, ArenaStmtKind::TailBareIdent(_)) {
            original_uses.statements.insert(statement.span.start(), crate::sema::check::StatementIdentity { source: statement.span.source_id, namespace: None, statement: id });
        }
    }
    let declarations = Checker::compact_declarations_from_checked(&parsed.arena, &checked);
    let bodies = Checker::probe_compact_bodies(&parsed.arena, &declarations);
    assert!(bodies.diagnostics.is_empty(), "{:?}", bodies.diagnostics);
    assert!(Arc::ptr_eq(&checked.solved, &bodies.solved));
    let symbols = parsed.arena.symbol_owner().clone(); let _symbols = symbols.enter();
    let mut functions = Vec::new();
    lower_compact_function_units_into(&parsed.arena, &declarations, &bodies, source, &sources, StdlibLowerLinkage::Local, |unit| {
        assert!(unit.is_lowered(), "{:?}", unit.blocker_detail);
        functions.push(unit.body.unwrap());
        Ok(())
    }).unwrap();
    drop(parsed);
    (checked, functions, original_uses)
}
