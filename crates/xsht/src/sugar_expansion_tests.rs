//! Every sugar form is defined by its expansion. These tests hold each form's
//! expansion to the rules that make that safe, and to the hand-written
//! expansion the language specification states.

use super::super::Formatter;
use super::expanded_walk;
use std::collections::BTreeMap;
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::syntax::arena::{
    ArenaBindingTargetKind, ArenaProgram, ArenaRange, ArenaStmtKind, ArenaSugarOperand, BlockId,
    ExprId, StmtId, SugarForm,
};
use xsh::frontend::syntax::parser::Parser;

struct Case {
    sugar: &'static str,
    core: Core,
}

/// The program a sugar program must expand to.
enum Core {
    /// Hand-written with the core forms the sugar stands for. This is the
    /// independent statement of a form's meaning.
    Written(&'static str),
    /// What `xsht desugar` prints for the sugar program, which is the text
    /// the specification shows beside the snippet.
    Desugared,
}

/// The programs each form is tested on. The match is exhaustive, so a new
/// form cannot be added without cases. The first case of a form is the
/// snippet the specification shows for it, beside its desugared form; at
/// least one other case states the expansion by hand.
fn cases(form: SugarForm) -> &'static [Case] {
    match form {
        SugarForm::Repeat => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/45-repeat.xsh"),
                core: Core::Desugared,
            },
            Case {
                sugar: "var n = 0\nrepeat 2 times {\n  repeat (n + 1) * 2 times {\n    n += 1\n    continue when n == 3\n  }\n}\n",
                core: Core::Written("var n = 0\nfor _ in range(2) {\n  for _ in range((n + 1) * 2) {\n    n += 1\n    if n == 3 {\n      continue\n    }\n  }\n}\n"),
            },
            Case {
                sugar: "proc poll(times: Int) {\n  match times {\n    0 => repeat 1 times { print \"once\" }\n    _ => {\n      repeat times times {\n        print \"tick\"\n      }\n    }\n  }\n}\n",
                core: Core::Written("proc poll(times: Int) {\n  match times {\n    0 => for _ in range(1) { print \"once\" }\n    _ => {\n      for _ in range(times) {\n        print \"tick\"\n      }\n    }\n  }\n}\n"),
            },
        ],
        SugarForm::When => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/47-when.xsh"),
                core: Core::Desugared,
            },
            Case {
                sugar: "stream picked(rows: List[Int]) -> Stream[Int] {\n  for row in rows {\n    break when row < 0\n    yield row when row > 0\n    yield @[row, row] when row == 0\n  }\n}\n",
                core: Core::Written("stream picked(rows: List[Int]) -> Stream[Int] {\n  for row in rows {\n    if row < 0 {\n      break\n    }\n    if row > 0 {\n      yield row\n    }\n    if row == 0 {\n      yield @[row, row]\n    }\n  }\n}\n"),
            },
            Case {
                sugar: "proc built(ready: Bool) [process] -> Status {\n  return (run.status make) when ready\n  return when (run.status true)\n  run.status false\n}\n",
                core: Core::Written("proc built(ready: Bool) [process] -> Status {\n  if ready {\n    return (run.status make)\n  }\n  if (run.status true) {\n    return\n  }\n  run.status false\n}\n"),
            },
        ],
        SugarForm::Unless => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/48-unless.xsh"),
                core: Core::Desugared,
            },
            Case {
                sugar: "pure read(raw: Str?) -> Str {\n  let present = raw != null\n  return \"missing\" unless present\n  raw.trim()\n}\n",
                core: Core::Written("pure read(raw: Str?) -> Str {\n  let present = raw != null\n  if present {\n  } else {\n    return \"missing\"\n  }\n  raw.trim()\n}\n"),
            },
        ],
        SugarForm::Guard => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/49-guard-else.xsh"),
                core: Core::Desugared,
            },
            Case {
                sugar: "for raw in [1, 2] {\n  guard raw > 0 else {\n    guard raw < 0 else { continue }\n    break unless raw == 0\n    continue\n  }\n  print $raw\n}\n",
                core: Core::Written("for raw in [1, 2] {\n  if raw > 0 {\n  } else {\n    if raw < 0 {\n    } else {\n      continue\n    }\n    if raw == 0 {\n    } else {\n      break\n    }\n    continue\n  }\n  print $raw\n}\n"),
            },
        ],
        SugarForm::Tempdir => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/60-tempdir.xsh"),
                core: Core::Desugared,
            },
            // At the top level, nested, with a body that defers and leaves
            // early, and with a path that is a block of its own.
            Case {
                sugar: "let root = p\"/tmp\"\ntempdir outer at fp\"{root}/outer\" {\n  defer { print \"outer\" }\n  repeat 2 times {\n    tempdir inner at { fp\"{outer}/inner\" } {\n      break when inner.exists()?\n    }\n  }\n}\n",
                core: Core::Written("let root = p\"/tmp\"\n{\n  let outer: Path = fp\"{root}/outer\"\n  fs.remove(outer, missing_ok: true)\n  fs.mkdir(outer)\n  defer fs.remove(outer, missing_ok: true)\n  {\n    defer { print \"outer\" }\n    for _ in range(2) {\n      {\n        let inner: Path = { fp\"{outer}/inner\" }\n        fs.remove(inner, missing_ok: true)\n        fs.mkdir(inner)\n        defer fs.remove(inner, missing_ok: true)\n        {\n          if inner.exists()? {\n            break\n          }\n        }\n      }\n    }\n  }\n}\n"),
            },
            // The words stay names: `tempdir` and `at` as the bound name, the
            // path, and a match arm's statement.
            Case {
                sugar: "proc stage(at: Path, tempdir: Int) {\n  match tempdir {\n    0 => tempdir at at at { print $at }\n    _ => {\n      tempdir tempdir at at {\n        print $tempdir\n      }\n    }\n  }\n}\n",
                core: Core::Written("proc stage(at: Path, tempdir: Int) {\n  match tempdir {\n    0 => {\n      {\n        let at: Path = at\n        fs.remove(at, missing_ok: true)\n        fs.mkdir(at)\n        defer fs.remove(at, missing_ok: true)\n        { print $at }\n      }\n    }\n    _ => {\n      {\n        let tempdir: Path = at\n        fs.remove(tempdir, missing_ok: true)\n        fs.mkdir(tempdir)\n        defer fs.remove(tempdir, missing_ok: true)\n        {\n          print $tempdir\n        }\n      }\n    }\n  }\n}\n"),
            },
        ],
        SugarForm::Fail => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/61-fail.xsh"),
                core: Core::Desugared,
            },
            Case {
                sugar: include_str!("../../../docs/snippets/spec/61-fail-because.xsh"),
                core: Core::Desugared,
            },
            // A cause on each form, under a guard, where `because` is a name.
            Case {
                sugar: "error LoadError = Missing(path: Path) | Busy\n\nproc load(because: Error, target: Path) -> Result[Int, LoadError] {\n  fail .Busy() because because when target == p\"/\"\n  fail .Missing(path: target) because because\n}\n\nproc report(because: Error) -> Result[Int] {\n  fail because.message because because unless because.message == \"\"\n  Ok(1)\n}\n",
                core: Core::Written("error LoadError = Missing(path: Path) | Busy\n\nproc load(because: Error, target: Path) -> Result[Int, LoadError] {\n  if target == p\"/\" {\n    return Err(.Busy(), cause: because)\n  }\n  return Err(.Missing(path: target), cause: because)\n}\n\nproc report(because: Error) -> Result[Int] {\n  if because.message == \"\" {\n  } else {\n    return Err(error.failure(because.message), cause: because)\n  }\n  Ok(1)\n}\n"),
            },
            // Under each postfix guard, as a match arm's statement, as the
            // block of a `guard`, and where `error` and `fail` are locals.
            Case {
                sugar: "proc check(fail: Int, error: Str) -> Result[Int] {\n  fail \"negative\" when fail < 0\n  fail f\"odd: {error}\" unless fail % 2 == 0\n  guard fail < 100 else { fail error }\n  match fail {\n    0 => fail \"zero\"\n    _ => Ok(fail)\n  }\n}\n",
                core: Core::Written("proc check(fail: Int, error: Str) -> Result[Int] {\n  if fail < 0 {\n    return Err(error.failure(\"negative\"))\n  }\n  if fail % 2 == 0 {\n  } else {\n    return Err(error.failure(f\"odd: {error}\"))\n  }\n  if fail < 100 {\n  } else { return Err(error.failure(error)) }\n  match fail {\n    0 => return Err(error.failure(\"zero\"))\n    _ => Ok(fail)\n  }\n}\n"),
            },
        ],
        SugarForm::Atomically => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/61-atomically.xsh"),
                core: Core::Desugared,
            },
            // At the top level, nested, with a body that defers and leaves
            // early, and with a destination that is a block of its own. The
            // local that holds a destination is written under the name
            // `xsht desugar` gives it.
            Case {
                sugar: "let root = p\"/tmp\"\natomically replace fp\"{root}/out\" as partial {\n  defer { print \"outer\" }\n  repeat 2 times {\n    atomically replace { fp\"{partial}.d/inner\" } as inner {\n      break when inner.exists()?\n    }\n  }\n}\n",
                core: Core::Written("let root = p\"/tmp\"\n{\n  let dest_1: Path = fp\"{root}/out\"\n  let partial: Path = fp\"{dest_1.parent()}/.{dest_1.name()}.tmp\"\n  fs.remove(partial, missing_ok: true)\n  defer fs.remove(partial, missing_ok: true)\n  {\n    defer { print \"outer\" }\n    for _ in range(2) {\n      {\n        let dest_1: Path = { fp\"{partial}.d/inner\" }\n        let inner: Path = fp\"{dest_1.parent()}/.{dest_1.name()}.tmp\"\n        fs.remove(inner, missing_ok: true)\n        defer fs.remove(inner, missing_ok: true)\n        {\n          if inner.exists()? {\n            break\n          }\n        }\n        fs.rename(inner, dest_1, overwrite: true)?\n      }\n    }\n  }\n  fs.rename(partial, dest_1, overwrite: true)?\n}\n"),
            },
            // The words stay names: `atomically`, `replace`, and `as` as the
            // destination, the bound name, and a match arm's statement.
            Case {
                sugar: "proc publish(atomically: Path, replace: Int) {\n  match replace {\n    0 => atomically replace atomically as as { print $as }\n    _ => {\n      atomically replace atomically as replace {\n        print $replace\n      }\n    }\n  }\n}\n",
                core: Core::Written("proc publish(atomically: Path, replace: Int) {\n  match replace {\n    0 => {\n      {\n        let dest_1: Path = atomically\n        let as: Path = fp\"{dest_1.parent()}/.{dest_1.name()}.tmp\"\n        fs.remove(as, missing_ok: true)\n        defer fs.remove(as, missing_ok: true)\n        { print $as }\n        fs.rename(as, dest_1, overwrite: true)?\n      }\n    }\n    _ => {\n      {\n        let dest_1: Path = atomically\n        let replace: Path = fp\"{dest_1.parent()}/.{dest_1.name()}.tmp\"\n        fs.remove(replace, missing_ok: true)\n        defer fs.remove(replace, missing_ok: true)\n        {\n          print $replace\n        }\n        fs.rename(replace, dest_1, overwrite: true)?\n      }\n    }\n  }\n}\n"),
            },
        ],
        SugarForm::ForIndex => &[
            Case {
                sugar: include_str!("../../../docs/snippets/spec/50-for-index.xsh"),
                core: Core::Desugared,
            },
            // Nested, with a destructured item, a pipeline as the source, an
            // index named `index`, and a body that leaves early.
            Case {
                sugar: "type Entry = {name: Str, size: Int}\n\nstream sizes(entries: List[Entry]) -> Stream[Int] {\n  for row, {name, size} in entries {\n    continue when name == \"\"\n    for index, part in name.split(\"/\") |> where { . != \"\" } {\n      break when index > row\n      yield size + part.len()\n    }\n  }\n}\n",
                core: Core::Written("type Entry = {name: Str, size: Int}\n\nstream sizes(entries: List[Entry]) -> Stream[Int] {\n  for {index: row, value: {name, size}} in entries |> enumerate() {\n    if name == \"\" {\n      continue\n    }\n    for {index, value: part} in (name.split(\"/\") |> where { . != \"\" }) |> enumerate() {\n      if index > row {\n        break\n      }\n      yield size + part.len()\n    }\n  }\n}\n"),
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
                    // A binding the expansion adds either binds a name the
                    // user wrote as an operand or a name no identifier can
                    // spell, so it never captures a name the body uses.
                    if let ArenaStmtKind::Let { target, .. }
                    | ArenaStmtKind::Var { target, .. }
                    | ArenaStmtKind::Const { target, .. } = arena.stmt(id).kind
                    {
                        let written = arena
                            .sugar_operands(sugar.operands)
                            .contains(&ArenaSugarOperand::BindingTarget(target));
                        let spellable = match &arena.binding_target(target).kind {
                            ArenaBindingTargetKind::Name(name) => name
                                .as_str()
                                .chars()
                                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
                            ArenaBindingTargetKind::Record { .. } => true,
                        };
                        assert!(
                            written || !spellable,
                            "`{text}` binds a name of its own that the body could use"
                        );
                    }
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
        assert!(
            cases(form)
                .iter()
                .any(|case| matches!(case.core, Core::Written(_))),
            "no case states the expansion of {form:?} by hand"
        );
        for case in cases(form) {
            let sugar = parse(case.sugar);
            let desugared;
            let core_source = match case.core {
                Core::Written(core) => core,
                Core::Desugared => {
                    let output = Formatter::new().desugar_source(SourceId::new(0), case.sugar);
                    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
                    desugared = output.formatted;
                    &desugared
                }
            };
            let core = parse(core_source);
            assert!(
                sugar_statements(&core).is_empty(),
                "the core program still uses sugar:\n{core_source}"
            );
            let sugar_roots = sugar.statement_ids().collect::<Vec<_>>();
            let core_roots = core.statement_ids().collect::<Vec<_>>();
            // A local an expansion binds under a name no identifier can spell
            // reads as the name `xsht desugar` prints for it, which is the
            // one a core program can write.
            let mut expanded = expanded_walk(&sugar.arena, case.sugar, &sugar_roots).text;
            for (hidden, printed) in super::super::hidden_names(&sugar.arena, case.sugar) {
                expanded = expanded.replace(
                    &format!("{:?}", hidden.as_str().as_str()),
                    &format!("{printed:?}"),
                );
            }
            assert_eq!(
                expanded,
                expanded_walk(&core.arena, core_source, &core_roots).text,
                "{form:?}:\n{}",
                case.sugar
            );
        }
    }
}
