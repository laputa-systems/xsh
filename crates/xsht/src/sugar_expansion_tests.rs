//! Every sugar form is defined by its expansion. These tests hold each form's
//! expansion to the rules that make that safe, and to the hand-written
//! expansion the language specification states.

use super::expanded_walk;
use std::collections::BTreeMap;
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::syntax::arena::{
    ArenaProgram, ArenaRange, ArenaStmtKind, ArenaSugarOperand, BlockId, ExprId, StmtId, SugarForm,
};
use xsh::frontend::syntax::parser::Parser;

struct Case {
    sugar: &'static str,
    /// The same program written with the core forms the sugar stands for.
    core: &'static str,
}

/// The programs each form is tested on. The match is exhaustive, so a new
/// form cannot be added without cases. The first case of a form is the pair
/// of programs the specification shows for it.
fn cases(form: SugarForm) -> &'static [Case] {
    match form {
        SugarForm::Repeat => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/45-repeat.xsh"),
                core: include_str!("../../../docs/snippets/spec/46-repeat-expansion.xsh"),
            },
            Case {
                sugar: "var n = 0\nrepeat 2 times {\n  repeat (n + 1) * 2 times {\n    n += 1\n    continue when n == 3\n  }\n}\n",
                core: "var n = 0\nfor _ in range(2) {\n  for _ in range((n + 1) * 2) {\n    n += 1\n    if n == 3 {\n      continue\n    }\n  }\n}\n",
            },
            Case {
                sugar: "proc poll(times: Int) {\n  match times {\n    0 => repeat 1 times { print \"once\" }\n    _ => {\n      repeat times times {\n        print \"tick\"\n      }\n    }\n  }\n}\n",
                core: "proc poll(times: Int) {\n  match times {\n    0 => for _ in range(1) { print \"once\" }\n    _ => {\n      for _ in range(times) {\n        print \"tick\"\n      }\n    }\n  }\n}\n",
            },
        ],
        SugarForm::When => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/47-when.xsh"),
                core: include_str!("../../../docs/snippets/spec/47-when-expansion.xsh"),
            },
            Case {
                sugar: "stream picked(rows: List[Int]) -> Stream[Int] {\n  for row in rows {\n    break when row < 0\n    yield row when row > 0\n    yield @[row, row] when row == 0\n  }\n}\n",
                core: "stream picked(rows: List[Int]) -> Stream[Int] {\n  for row in rows {\n    if row < 0 {\n      break\n    }\n    if row > 0 {\n      yield row\n    }\n    if row == 0 {\n      yield @[row, row]\n    }\n  }\n}\n",
            },
            Case {
                sugar: "proc built(ready: Bool) [process] -> Status {\n  return (run.status make) when ready\n  return when (run.status true)\n  run.status false\n}\n",
                core: "proc built(ready: Bool) [process] -> Status {\n  if ready {\n    return (run.status make)\n  }\n  if (run.status true) {\n    return\n  }\n  run.status false\n}\n",
            },
        ],
        SugarForm::Unless => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/48-unless.xsh"),
                core: include_str!("../../../docs/snippets/spec/48-unless-expansion.xsh"),
            },
            Case {
                sugar: "pure read(raw: Str?) -> Str {\n  let present = raw != null\n  return \"missing\" unless present\n  raw.trim()\n}\n",
                core: "pure read(raw: Str?) -> Str {\n  let present = raw != null\n  if present {\n  } else {\n    return \"missing\"\n  }\n  raw.trim()\n}\n",
            },
        ],
        SugarForm::Guard => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/49-guard-else.xsh"),
                core: include_str!("../../../docs/snippets/spec/49-guard-else-expansion.xsh"),
            },
            Case {
                sugar: "for raw in [1, 2] {\n  guard raw > 0 else {\n    guard raw < 0 else { continue }\n    break unless raw == 0\n    continue\n  }\n  print $raw\n}\n",
                core: "for raw in [1, 2] {\n  if raw > 0 {\n  } else {\n    if raw < 0 {\n    } else {\n      continue\n    }\n    if raw == 0 {\n    } else {\n      break\n    }\n    continue\n  }\n  print $raw\n}\n",
            },
        ],
    }
}

fn parse(source: &str) -> ArenaProgram {
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(
        output.diagnostics.is_empty(),
        "{source}\n{:?}",
        output.diagnostics
    );
    output.arena
}

struct Sugar {
    form: SugarForm,
    operands: ArenaRange,
    expansion: StmtId,
    span: Span,
}

/// Every sugar statement of a program, in the order the parser built them,
/// which is the order of `AstArena::sugar_expansions`.
fn sugar_statements(program: &ArenaProgram) -> Vec<Sugar> {
    (0..program.arena.stmt_tags.len())
        .filter_map(|index| {
            let stmt = program.arena.stmt(StmtId::from_index(index));
            let ArenaStmtKind::Sugar {
                form,
                operands,
                expansion,
            } = stmt.kind
            else {
                return None;
            };
            Some(Sugar {
                form,
                operands,
                expansion,
                span: stmt.span,
            })
        })
        .collect()
}

fn rows(range: ArenaRange) -> std::ops::Range<usize> {
    range.start as usize..range.start as usize + range.len as usize
}

fn span_counts(spans: impl Iterator<Item = Span>) -> BTreeMap<(usize, usize), usize> {
    let mut counts = BTreeMap::new();
    for span in spans {
        *counts.entry((span.start(), span.end())).or_default() += 1;
    }
    counts
}

fn within(inner: Span, outer: Span) -> bool {
    outer.start() <= inner.start() && inner.end() <= outer.end()
}

#[test]
fn every_form_keeps_the_expansion_rules() {
    for form in SugarForm::ALL {
        let mut seen = false;
        for case in cases(form) {
            let program = parse(case.sugar);
            let arena = &program.arena;
            let source = case.sugar;
            let expr_spans = span_counts(
                (0..arena.expr_tags.len()).map(|index| arena.expr(ExprId::from_index(index)).span),
            );
            let stmt_spans = span_counts(
                (0..arena.stmt_tags.len()).map(|index| arena.stmt(StmtId::from_index(index)).span),
            );
            let block_spans = span_counts(arena.blocks.iter().map(|block| arena.span(block.span)));
            let count = |counts: &BTreeMap<(usize, usize), usize>, span: Span| {
                counts[&(span.start(), span.end())]
            };
            let sugars = sugar_statements(&program);
            assert_eq!(sugars.len(), arena.sugar_expansions.len(), "{source}");
            for (sugar, added) in sugars.iter().zip(arena.sugar_expansions.iter()) {
                seen |= sugar.form == form;
                let text = &source[sugar.span.range()];

                // Each operand is referenced once, and the expansion reaches
                // the operands in the order the user wrote them.
                let operands = arena
                    .sugar_operands(sugar.operands)
                    .iter()
                    .copied()
                    .filter(|operand| !matches!(operand, ArenaSugarOperand::Name(_)))
                    .collect::<Vec<_>>();
                let reached = expanded_walk(arena, source, &[sugar.expansion])
                    .visited
                    .into_iter()
                    .filter(|node| operands.contains(node))
                    .collect::<Vec<_>>();
                assert_eq!(reached, operands, "operands of `{text}`");

                // The root is the surface statement for every observer, and
                // it is not something a scan of a statement list collects.
                let root = arena.stmt(sugar.expansion);
                assert_eq!(root.span, sugar.span, "root span of `{text}`");
                assert!(
                    !matches!(
                        root.kind,
                        ArenaStmtKind::Use(_)
                            | ArenaStmtKind::Export(_)
                            | ArenaStmtKind::TypeDef(_)
                            | ArenaStmtKind::ErrorDef(_)
                            | ArenaStmtKind::Const { .. }
                            | ArenaStmtKind::Let { .. }
                            | ArenaStmtKind::Var { .. }
                            | ArenaStmtKind::ProcDef(_)
                            | ArenaStmtKind::CliMain(_)
                            | ArenaStmtKind::PureDef(_)
                            | ArenaStmtKind::StreamDef(_)
                            | ArenaStmtKind::SignalHook(_)
                    ),
                    "`{text}` expands to a declaration or binding: {:?}",
                    root.kind
                );
                assert!(
                    rows(added.stmts).contains(&sugar.expansion.index()),
                    "root of `{text}` was not built by its expansion"
                );

                // Every other added node has a span of its own inside the
                // surface statement. Checker facts are keyed by span.
                for index in rows(added.exprs) {
                    let id = ExprId::from_index(index);
                    assert!(arena.expr_is_synthetic(id));
                    let span = arena.expr(id).span;
                    assert!(within(span, sugar.span), "expression span in `{text}`");
                    assert_eq!(
                        count(&expr_spans, span),
                        1,
                        "expression span `{}` in `{text}` is shared",
                        &source[span.range()]
                    );
                }
                for index in rows(added.stmts) {
                    let id = StmtId::from_index(index);
                    assert!(arena.stmt_is_synthetic(id));
                    let span = arena.stmt(id).span;
                    assert!(within(span, sugar.span), "statement span in `{text}`");
                    // The root and the surface statement are the one pair
                    // that shares a span.
                    let expected = if id == sugar.expansion { 2 } else { 1 };
                    assert_eq!(
                        count(&stmt_spans, span),
                        expected,
                        "statement span `{}` in `{text}` is shared",
                        &source[span.range()]
                    );
                }
                for index in rows(added.blocks) {
                    let id = BlockId::from_index(index);
                    assert!(arena.block_is_synthetic(id));
                    let span = arena.span(arena.block(id).span);
                    assert!(within(span, sugar.span), "block span in `{text}`");
                    assert_eq!(
                        count(&block_spans, span),
                        1,
                        "block span `{}` in `{text}` is shared",
                        &source[span.range()]
                    );
                }
            }
            // What the user wrote is never synthetic.
            for operand in sugars
                .iter()
                .flat_map(|sugar| arena.sugar_operands(sugar.operands))
            {
                match *operand {
                    ArenaSugarOperand::Expr(id) => assert!(!arena.expr_is_synthetic(id)),
                    ArenaSugarOperand::Block(id) => assert!(!arena.block_is_synthetic(id)),
                    ArenaSugarOperand::Stmt(id) => assert!(!arena.stmt_is_synthetic(id)),
                    ArenaSugarOperand::BindingTarget(_)
                    | ArenaSugarOperand::TypeExpr(_)
                    | ArenaSugarOperand::Name(_) => {}
                }
            }
        }
        assert!(seen, "no case uses {form:?}");
    }
}

/// A `guard` adds one rule to its `if`: the failure block must leave. The
/// expansion records that on the block the user wrote and on no other.
#[test]
fn a_guard_requires_only_its_failure_block_to_exit() {
    let source = "for raw in [1, 2] {\n  continue unless raw > 0\n  guard raw > 1 else {\n    if raw == 0 { break } else { continue }\n  }\n  if raw > 2 {} else { continue }\n}\n";
    let program = parse(source);
    let arena = &program.arena;
    let required = (0..arena.blocks.len())
        .map(BlockId::from_index)
        .filter(|id| arena.block_must_exit(*id))
        .map(|id| &source[arena.span(arena.block(id).span).range()])
        .collect::<Vec<_>>();
    assert_eq!(
        required,
        ["{\n    if raw == 0 { break } else { continue }\n  }"]
    );
}

/// Reading each sugar statement as its expansion gives the same program as
/// the hand-written core source, node for node.
#[test]
fn every_form_expands_to_its_stated_core_program() {
    for form in SugarForm::ALL {
        for case in cases(form) {
            let sugar = parse(case.sugar);
            let core = parse(case.core);
            assert!(
                sugar_statements(&core).is_empty(),
                "the core program still uses sugar:\n{}",
                case.core
            );
            let sugar_roots = sugar.statement_ids().collect::<Vec<_>>();
            let core_roots = core.statement_ids().collect::<Vec<_>>();
            assert_eq!(
                expanded_walk(&sugar.arena, case.sugar, &sugar_roots).text,
                expanded_walk(&core.arena, case.core, &core_roots).text,
                "{form:?}:\n{}",
                case.sugar
            );
        }
    }
}
