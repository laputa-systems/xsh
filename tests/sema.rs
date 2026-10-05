#![allow(clippy::single_call_fn)]

use xsh::diagnostic::DiagnosticCode;
use xsh::frontend::check::{AnnotationFactKind, CheckOptions, Checker};
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::parser::Parser;

#[test]
fn context_scope_capture_tail_types_agree_in_full_and_compact_facts() {
    use xsh::frontend::check::{StatementPosition, Type};
    use xsh::frontend::syntax::arena::{ArenaExprKind, ArenaStmtKind};

    let source = r#"let text = cd (p".") { run.text sh -c "printf text" ? }?
let payload = env ({X: "value"}) { run.bytes sh -c "printf bytes" ? }?
let record = cd (p".") { run.capture --text sh -c "printf record" ? }?
let nested = cd (p".") { try run.text sh -c "printf nested" }?
let discarded: Unit = cd (p".") { run.text sh -c "printf discarded" ? }?
let predicate = cd (p".") { false }?
proc nested_tail() [env, process, error] -> Result[Result[Str, ProcessError]] {
  cd (p".") { try run.text sh -c "printf nested" }
}
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let mut scopes = 0;
    for (expr, ty) in &compact.expr_types {
        let ArenaExprKind::ContextScope { block, .. } = parsed.arena.arena.expr(*expr).kind else {
            continue;
        };
        scopes += 1;
        let span = parsed.arena.arena.expr(*expr).span;
        assert_eq!(
            checked.expr_types.get(&span),
            Some(ty),
            "{}",
            &source[span.range()]
        );
        let tail = parsed
            .arena
            .arena
            .stmt_ids(parsed.arena.arena.block(block).statements)
            .last()
            .unwrap();
        let statement = parsed.arena.arena.stmt(tail);
        assert_eq!(
            checked.statement_positions.get(&statement.span),
            compact.statement_positions.get(&tail)
        );
        if matches!(statement.kind, ArenaStmtKind::Command(_)) {
            let body_type = ty.result_ok().unwrap();
            let expected = if body_type == &Type::Unit {
                StatementPosition::Statement
            } else {
                StatementPosition::Value
            };
            assert_eq!(compact.statement_positions.get(&tail), Some(&expected));
        }
    }
    assert_eq!(scopes, 7);
}

#[test]
fn callable_alias_signatures_agree_in_full_and_compact_facts() {
    let source = "pure render(value: Str, prefix: Str = \"label:\") -> Str { prefix + value }\nlet format = render\nlet again = format\nlet result = again(prefix: \"item:\", value: \"one\")\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert_eq!(
        checked.static_callable_aliases,
        declarations.static_callable_aliases
    );
    let compact = declarations.bodies;
    for (expression, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(expression).span;
        if source[span.range()].starts_with("again(") {
            assert_eq!(ty, xsh::frontend::check::Type::Str);
            assert_eq!(checked.expr_types.get(&span), Some(&ty));
        }
    }
    assert!(!checked.annotation_facts.iter().any(|fact| matches!(fact.kind, AnnotationFactKind::Binding { initializer, .. } if source[initializer.range()] == *"render" || source[initializer.range()] == *"format")));
}

#[test]
fn removed_record_require_identity_fix_uses_plain_values_and_exact_named_schema() {
    for source in [
        "type Name = {name: Str}\nlet value = record.require({name: \"café\", extra: 7}, {name: \"Str\"})?\n",
        "type Name = {name: Str}\nconst required = {name: \"Str\"}\nconst raw = {name: \"demo\"}\nlet value = record.require(raw, required)?\n",
        "type Name = {name: Str}\nlet value = record.require(Name(name: \"demo\"), {name: \"Str\"})?\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let diagnostics = Checker::check_arena(&parsed.arena, source).diagnostics;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code.map(DiagnosticCode::name) == Some("check.removed-record-require")
            })
            .unwrap();
        let [hint] = diagnostic.fix_hints.as_slice() else {
            panic!("expected identity migration: {diagnostics:?}");
        };
        let mut fixed = source.to_string();
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_ref().unwrap(),
        );
        assert!(fixed.contains(".require(Name)"), "{fixed}");
        assert!(check(&fixed).is_empty(), "{fixed}: {:?}", check(&fixed));
    }
}

#[test]
fn removed_record_require_refuses_unproved_or_different_contracts() {
    for source in [
        "type Name = {name: Str}\nlet value = record.require(json.decode(\"{}\")?, {name: \"Str\"})?\n",
        "type Name = {name: Str}\nlet value = record.require({name: \"demo\"}, {name: \"Str\"}, optional: {version: \"Str\"})?\n",
        "type Name = {name: Str}\nlet value = record.require({name: \"demo\"}, {name: \"Str\"}, source: p\"manifest\")?\n",
        "type Name = {name: Str}\nlet value = record.require({name: 1}, {name: \"Str\"})?\n",
        "type Root = {root: Path}\nlet value = record.require({root: p\".\"}, {root: \"Path\"})?\n",
        "type Name = {name: UInt}\nlet value = record.require({name: 1}, {name: \"Int\"})?\n",
        "type Count = Int\ntype Name = {name: Count}\nlet value = record.require({name: 1}, {name: \"Int\"})?\n",
        "type Name = {name: Str}\nlet value = record.require({name: \"demo\"}, # retained comment\n {name: \"Str\"})?\n",
        "let value = record.require({}, {action: \"Proc(Path) -> Result[Unit]\"})?\n",
        "proc validate(raw: FsEntry) -> Result[Record] { record.require(raw, {name: \"Str\"}) }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let diagnostics = Checker::check_arena(&parsed.arena, source).diagnostics;
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code.map(DiagnosticCode::name) == Some("check.removed-record-require")
            })
            .unwrap();
        assert!(diagnostic.fix_hints.is_empty(), "{source}: {diagnostic:?}");
    }
}

#[test]
fn callable_alias_module_projection_contracts_preserve_full_compact_facts() {
    let source = "type Plugin = module { export pure render(value: Str, suffix: Str = \"!\") -> Str; export proc clock() [time] -> Int }\nproc invoke(plugin: Plugin) [time, error] -> Str { let format = plugin.get(\"render\")?; let clock = plugin[\"clock\"]; let _ = clock(); format(value: \"one\") }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert_eq!(
        checked.static_callable_aliases,
        declarations.static_callable_aliases
    );
    assert!(!checked.static_callable_aliases.is_empty());
    assert!(
        checked
            .static_callable_aliases
            .values()
            .all(|alias| alias.definition.is_none())
    );
    for invalid in [
        source.replace("value: \"one\"", "value: 1"),
        source.replace("[time, error]", "[error]"),
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &invalid);
        let checked = Checker::check_arena(&parsed.arena, &invalid);
        assert!(!checked.diagnostics.is_empty(), "{invalid}");
    }
}

#[test]
fn boolean_guard_checked_facts_preserve_refinement_and_statement_position() {
    let source = "pure choose(name: Str?) -> Str {\n  guard name != null else { return \"missing\" }\n  name.trim()\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.definitely_exiting_block_spans.len(), 1);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    for (id, position) in compact.statement_positions {
        assert_eq!(
            checked
                .statement_positions
                .get(&parsed.arena.arena.stmt(id).span),
            Some(&position)
        );
    }
    for (id, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(id).span;
        if &source[span.range()] == "name" && span.start() > source.find("return").unwrap() {
            assert_eq!(ty, xsh::frontend::check::Type::Str);
            assert_eq!(checked.expr_types.get(&span), Some(&ty));
        }
    }
}

#[test]
fn checker_rejects_signal_hooks_in_interactive_input() {
    let interactive_output = check_interactive("on SIGINT [] {\n}\n");
    assert!(
        has_code(&interactive_output, "check.signal-hook"),
        "expected interactive signal hook diagnostic: {interactive_output:?}"
    );
}

// The command-line tools load every `use` before checking, and report a module
// file they cannot read as `parse.module-read`; only a checker handed the
// program without its modules reports the unknown module itself.
#[test]
fn checker_rejects_use_of_an_unknown_module() {
    let output = check("use text\nlet ok = text.contains(\"abc\", \"a\")\n");
    assert!(
        has_code(&output, "check.unknown-module"),
        "expected check.unknown-module in diagnostics: {output:?}"
    );
}

#[test]
fn checker_rejects_undefined_utility_names_as_interactive_proc_commands() {
    let output = check_interactive(
        r#"
basename filetxt
cat x
clear
cp x y
cut -d , -f 1 x
df x
dirname filetxt
du x
echo hi
env FOO=bar true
fd pattern root
false
find root -name x
fold -w 2 x
fsync x
grep pattern x
head -n1 x
host localhost
hostname
ip addr
chmod 644 x
chown 0 x
chgrp 0 x
command echo
link x y
ln -s x y
ls x
mkdir -p x
mv x y
nproc
paste x y
printenv PATH
pstree -p
printf "%s\n" hi
pwd
readlink x
realpath x
rev x
rg pattern root
rm -f x
rmdir x
seq 1 1
shuf x
sleep 0
sort x
split -l 1 x y
stat x
strings x
sync
tail -n1 x
tar -tf x
tee x
test -f x
touch -c x
tr a b x
tree root
true
tty
uname
uniq x
unlink x
wc -lwc x
which sh
whoami
yes
"#,
    );

    assert!(has_code(&output, "check.unresolved-proc-command"));
}

#[test]
fn checker_cli_command_descriptors_match_inline_const_and_spread_shapes() {
    let source = r#"
const commands = {build: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}}
const invocation = {argv: ["build", "workspace"], commands: commands}
let prepared = cli.commands(["build", "workspace"], commands)?
let inline = cli.commands(["build", "workspace"], {build: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}})?
let spread = cli.commands(...invocation)?
let root: Path = prepared.root
proc dynamic(commands: Record) [error] -> Result[Record] { cli.commands([], commands) }
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let mut shapes = Vec::new();
    for (id, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(id).span;
        if source[span.range()].starts_with("cli.commands(") && !source[span.range()].ends_with('?')
        {
            assert_eq!(checked.expr_types.get(&span), Some(&ty));
            if source[span.range()].contains("cli.commands([],") {
                assert!(!shapes.contains(&ty));
            } else {
                shapes.push(ty);
            }
        }
    }
    assert_eq!(shapes.len(), 3);
    assert!(shapes.windows(2).all(|pair| pair[0] == pair[1]));
    parsed.arena.symbol_owner().with_current(|| {
        let xsh::frontend::check::Type::Result(value, _) = &shapes[0] else {
            panic!("command result")
        };
        let xsh::frontend::check::Type::Record(fields) = value.as_ref() else {
            panic!("command fields")
        };
        assert_eq!(
            fields.get(&xsh::frontend::symbols::Name::intern("root")),
            Some(&xsh::frontend::check::Type::Path)
        );
        assert_eq!(fields.len(), 4);
    });
}

#[test]
fn checker_cli_command_descriptors_keep_conditional_fields_and_dynamic_fallback_erased() {
    let source = r#"
const commands = {build: {positionals: ["root"], types: {root: "Path"}}, clean: {positionals: ["count"], types: {count: "Int"}}}
let common = cli.commands(["build", "workspace"], commands)?
proc fallback() [] -> Record { {rest: "raw"} }
let dynamic = cli.commands([], "build", commands, fallback())?
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let mut common = None;
    let mut dynamic = None;
    for (span, ty) in &checked.expr_types {
        let spelling = &source[span.range()];
        if spelling == "cli.commands([\"build\", \"workspace\"], commands)" {
            common = Some(ty.clone());
        }
        if spelling == "cli.commands([], \"build\", commands, fallback())" {
            dynamic = Some(ty.clone());
        }
    }
    parsed.arena.symbol_owner().with_current(|| {
        let Some(xsh::frontend::check::Type::Result(value, _)) = &common else {
            panic!("known common result")
        };
        let xsh::frontend::check::Type::Record(fields) = value.as_ref() else {
            panic!("common fields")
        };
        assert_eq!(fields.len(), 2);
        assert!(!fields.contains_key(&xsh::frontend::symbols::Name::intern("root")));
        assert!(!fields.contains_key(&xsh::frontend::symbols::Name::intern("count")));
    });
    assert!(dynamic.is_some());
    assert_ne!(common, dynamic);
}

#[test]
fn checker_cli_constant_descriptors_match_inline_and_compact_facts() {
    let source = r#"
const schema = {
  item: {kind: "Int", form: "ITEM", required: false},
  defaulted: {kind: "Int", form: "DEFAULT", default: 5},
  repeated: "List[UInt]",
  flag: {kind: "Bool", flag: false},
}

let constant = cli.parse([], schema)?
let inline = cli.parse([], {
  item: {kind: "Int", form: "ITEM", required: false},
  defaulted: {kind: "Int", form: "DEFAULT", default: 5},
  repeated: "List[UInt]",
  flag: {kind: "Bool", flag: false},
})?
let full = cli.parse_full([], schema)?
let item: Int? = constant.item
let defaulted: Int = constant.defaulted
let repeated: List[Int] = constant.repeated
let flag: Bool? = constant.flag
let full_item: Int? = full.values.item
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let mut shapes = Vec::new();
    for (id, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(id).span;
        if source[span.range()].starts_with("cli.parse(") && !source[span.range()].ends_with('?') {
            assert_eq!(checked.expr_types.get(&span), Some(&ty));
            shapes.push(ty);
        }
    }
    assert_eq!(shapes.len(), 2);
    assert_eq!(shapes[0], shapes[1]);
}

#[test]
fn checker_cli_dynamic_full_descriptors_keep_the_outcome_envelope() {
    let source = r#"
proc descriptor() [] -> Record { {count: {kind: "Int", default: 2}} }
let full = cli.parse_full([], descriptor())?
let warnings: List[Str] = full.warnings
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let (span, ty) = checked
        .expr_types
        .iter()
        .find(|(span, _)| &source[span.range()] == "cli.parse_full([], descriptor())")
        .unwrap();
    let compact_ty = compact
        .expr_types
        .iter()
        .find(|(id, _)| parsed.arena.arena.expr(**id).span == *span)
        .unwrap()
        .1;
    assert_eq!(ty, compact_ty);
    parsed.arena.symbol_owner().with_current(|| {
        let xsh::frontend::check::Type::Result(value, _) = ty else {
            panic!("full result")
        };
        let xsh::frontend::check::Type::Record(fields) = value.as_ref() else {
            panic!("full envelope")
        };
        assert_eq!(fields.len(), 3);
        assert_eq!(
            fields.get(&xsh::frontend::symbols::Name::intern("warnings")),
            Some(&xsh::frontend::check::Type::List(Box::new(
                xsh::frontend::check::Type::Str
            )))
        );
    });
}

#[test]
fn checker_retains_annotation_facts_and_reveal_notes() {
    let source = r#"
enum State { Ready, Stopped }
let count = 1
var names = ["a", "b"]
let state = Ready

proc local(input = Path(".")) {
}

export proc entry(flag = true) {
}

reveal_type(names)
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let output = Checker::check_arena_with_options(
        &parsed.arena,
        source,
        CheckOptions {
            interactive_commands: None,
            reveal_types: true,
            migration_diagnostics: false,
            embedded_bodies: false,
        },
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(output.annotation_facts.iter().any(|fact| matches!(
        fact.kind,
        AnnotationFactKind::Binding { .. }
    ) && fact.ty.annotation_source().as_deref()
        == Some("List[Str]")));
    assert!(!output.annotation_facts.iter().any(|fact| matches!(
        fact.kind,
        AnnotationFactKind::Binding { .. }
    ) && fact.ty.annotation_source().as_deref()
        == Some("Int")));
    assert!(output.annotation_facts.iter().any(|fact| matches!(
        fact.kind,
        AnnotationFactKind::DefaultedParam { .. }
    ) && fact.ty.annotation_source().as_deref()
        == Some("Bool")));
    assert!(output.annotation_facts.iter().any(|fact| matches!(
        fact.kind,
        AnnotationFactKind::DefaultedParam { .. }
    ) && fact.ty.annotation_source().as_deref()
        == Some("Path")));
    assert!(
        output
            .annotation_facts
            .iter()
            .any(|fact| matches!(fact.kind, AnnotationFactKind::ExportedProcReturn { .. }))
    );
    assert_eq!(output.reveal_types.len(), 1);
    assert_eq!(output.reveal_types[0].message, "revealed type: List[Str]");
}

fn check(source: &str) -> Vec<Option<String>> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Checker::check_arena(&parsed.arena, source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code.map(|code| code.name().to_owned()))
        .collect()
}

fn check_with_migration(source: &str) -> Vec<Option<String>> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Checker::check_arena_with_options(
        &parsed.arena,
        source,
        CheckOptions {
            interactive_commands: None,
            reveal_types: false,
            migration_diagnostics: true,
            embedded_bodies: false,
        },
    )
    .diagnostics
    .into_iter()
    .map(|diagnostic| diagnostic.code.map(|code| code.name().to_owned()))
    .collect()
}

fn check_interactive(source: &str) -> Vec<Option<String>> {
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    Checker::check_arena_interactive(&parsed.arena, source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.code.map(|code| code.name().to_owned()))
        .collect()
}

fn has_code(output: &[Option<String>], code: &str) -> bool {
    output.iter().any(|item| item.as_deref() == Some(code))
}

#[test]
fn doc_only_module_does_not_attribute_orphan_docs_to_another_source() {
    let main_source = "use helper\nlet value = helper.exported\n";
    let helper_source =
        "##! Helper module.\n\n## Exposes a documented value.\nexport let exported: Int = 1\n";
    let proof_source = "##! Presence-only fixture with no statements.\n";
    let main = Parser::parse_source_arena_only(SourceId::new(1), main_source);
    let helper = Parser::parse_source_arena_only(SourceId::new(0), helper_source);
    let proof = Parser::parse_source_arena_only(SourceId::new(2), proof_source);
    let output = Checker::check_arena_with_modules(
        (&main.arena, main_source),
        &[
            ("helper", "helper", &helper.arena, helper_source),
            ("proof", "proof", &proof.arena, proof_source),
        ],
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn checker_records_value_and_statement_bool_positions() {
    let source = "pure choose(flag: Bool) -> Bool {\n  let asserted: Result[Unit] = try { assert true }\n  let _ = asserted\n  if flag { false } else { true }\n}\nproc assertions() {\n  if true { assert true }\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let true_positions = checked
        .statement_positions
        .iter()
        .filter(|(span, _)| source[span.range()].trim().trim_start_matches("assert ") == "true")
        .map(|(_, position)| *position)
        .collect::<Vec<_>>();
    assert!(true_positions.contains(&xsh::frontend::check::StatementPosition::Value));
    assert!(true_positions.contains(&xsh::frontend::check::StatementPosition::Statement));
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    for (id, position) in compact.statement_positions {
        let span = parsed.arena.arena.stmt(id).span;
        assert_eq!(checked.statement_positions.get(&span), Some(&position));
    }
}

#[test]
fn checker_guarded_control_proofs_agree_on_full_and_compact_routes() {
    use xsh::frontend::check::Type;
    use xsh::frontend::syntax::arena::{ArenaExprKind, ExprId};
    for source in [
        "pure read(raw: Str?) -> Str { return \"missing\" when raw == null; raw.trim() }\n",
        "type Item = {raw: Str?}\npure read(item: Item) -> Str { let available = item.raw != null; let retained = available; return \"missing\" unless retained; item.raw.trim() }\n",
        "let values: List[Str?] = [null, \"ready\"]\nfor raw in values { continue when raw == null; print ${raw.trim()} }\n",
        "let values: List[Str?] = [null, \"ready\"]\nfor raw in values { break unless raw != null; print ${raw.trim()} }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let full = Checker::check_arena(&parsed.arena, source);
        assert!(full.diagnostics.is_empty(), "{:?}", full.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert!(
            declarations.diagnostics.is_empty(),
            "{:?}",
            declarations.diagnostics
        );
        let compact = declarations.bodies;
        let mut receivers = 0;
        for index in 0..parsed.arena.arena.expr_tags.len() {
            let id = ExprId::from_index(index);
            let expression = parsed.arena.arena.expr(id);
            let ArenaExprKind::Field { base, name } = expression.kind else {
                continue;
            };
            if name != "trim" {
                continue;
            }
            assert_eq!(
                full.expr_types[&parsed.arena.arena.expr(base).span],
                Type::Str
            );
            assert_eq!(compact.expr_types[&base], Type::Str, "{source}");
            receivers += 1;
        }
        assert_eq!(receivers, 1);
    }
    for source in [
        "stream values(raw: Str?) [] -> Stream[Str] { yield \"missing\" when raw == null; yield raw.trim() }\n",
        "let raw: Str? = null\nreturn \"missing\" when raw == null\nprint ${raw.trim()}\n",
        "let raw: Str? = null\nbreak when raw == null\nprint ${raw.trim()}\n",
        "let raw: Str? = null\ncontinue when raw == null\nprint ${raw.trim()}\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let full = Checker::check_arena(&parsed.arena, source);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let compact = declarations.bodies;
        let mut receivers = 0;
        for index in 0..parsed.arena.arena.expr_tags.len() {
            let id = ExprId::from_index(index);
            let ArenaExprKind::Field { base, name } = parsed.arena.arena.expr(id).kind else {
                continue;
            };
            if name != "trim" {
                continue;
            }
            let optional = Type::Optional(Box::new(Type::Str));
            assert_eq!(
                full.expr_types[&parsed.arena.arena.expr(base).span],
                optional
            );
            assert_eq!(compact.expr_types[&base], optional, "{source}");
            receivers += 1;
        }
        assert_eq!(receivers, 1);
    }
}

#[test]
fn error_fallback_checked_facts_keep_nominal_error_and_value_tail() {
    let source = "error Failure = invalid(message: Str)\nlet outcome: Result[Bool, Failure] = Err(Failure.invalid(message: \"bad\"))\nlet value = outcome ?? { |failure|\n  let message = failure.message\n  let _ = message\n  false\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    for (id, position) in &compact.statement_positions {
        assert_eq!(
            checked
                .statement_positions
                .get(&parsed.arena.arena.stmt(*id).span),
            Some(position)
        );
    }
    let mut exact_error = false;
    for (id, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(id).span;
        if &source[span.range()] == "failure" {
            assert!(matches!(ty, xsh::frontend::check::Type::ErrorFamily(_)));
            assert_eq!(checked.expr_types.get(&span), Some(&ty));
            exact_error = true;
        }
    }
    assert!(exact_error);
}

#[test]
fn private_pure_inference_publishes_exact_returns_and_annotations() {
    let source = "pure later(value: Int) { value + 1 }\npure earlier(value: Int) { later(value) }\npure predicate(value: Int) { value > 0 }\nlet number: Int = earlier(2)\nlet flag: Bool = predicate(-1)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.function_return_types.len(), 3);
    assert_eq!(
        checked
            .annotation_facts
            .iter()
            .filter(|fact| matches!(fact.kind, AnnotationFactKind::InferredPureReturn { .. }))
            .count(),
        3
    );
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    parsed.arena.symbol_owner().with_current(|| {
        assert_eq!(
            declarations.pures[&xsh::frontend::symbols::Name::intern("earlier")]
                .return_ty
                .to_string(),
            "Int"
        );
        assert_eq!(
            declarations.pures[&xsh::frontend::symbols::Name::intern("predicate")]
                .return_ty
                .to_string(),
            "Bool"
        );
    });
}

#[test]
fn value_pipeline_holes_publish_the_checked_input_and_ordinary_call_types() {
    let source = "pure wrapped(value: Int) { value |> increment(_) }\npure increment(value: Int) -> Int { value + 1 }\nlet selected: Int = wrapped(2)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let bodies = declarations.bodies;
    for raw in 0..parsed.arena.arena.expr_tags.len() {
        let id = xsh::frontend::syntax::arena::ExprId::from_index(raw);
        if let xsh::frontend::syntax::arena::ArenaExprKind::ValuePipelineCall {
            input,
            call,
            hole,
        } = parsed.arena.arena.expr(id).kind
        {
            for expr in [id, input, call, hole] {
                assert_eq!(
                    checked.expr_types[&parsed.arena.arena.expr(expr).span],
                    xsh::frontend::check::Type::Int
                );
                assert_eq!(bodies.expr_types[&expr], xsh::frontend::check::Type::Int);
            }
        }
    }
}

#[test]
fn duration_arithmetic_compact_and_full_checked_types_agree() {
    let source = "pure intervals(left: Duration, right: Duration) -> Int { let count = left / right; count }\npure scale(value: Duration, factor: Int) -> Duration { let pause = value * factor + 1s; pause }\nlet scaled = 2 * 1ms\nlet quantized = 5ms / 2\nlet compared = 1ms < 1s\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    for (id, ty) in compact.expr_types {
        if matches!(
            parsed.arena.arena.expr(id).kind,
            xsh::frontend::syntax::arena::ArenaExprKind::Binary { .. }
        ) {
            assert_eq!(
                checked.expr_types.get(&parsed.arena.arena.expr(id).span),
                Some(&ty),
                "{}",
                &source[parsed.arena.arena.expr(id).span.range()]
            );
        }
    }
}

#[test]
fn private_proc_effects_publish_matching_full_and_compact_facts() {
    let source = "proc caller() [time] -> Int { forwarding() }\nproc forwarding() -> Int { clock() }\nproc clock() -> Int { let _ = time.now(); 42 }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let compact = Checker::check_compact_declarations(&parsed.arena);
    parsed.arena.symbol_owner().with_current(|| {
        for name in ["forwarding", "clock"] {
            let effects = Some(vec![xsh::frontend::syntax::node::Effect::Time]);
            assert_eq!(checked.callable_effects[name], effects);
            let signature = &compact.procs[&xsh::frontend::symbols::Name::intern(name)];
            assert_eq!(signature.effects, effects);
            assert!(signature.inferred_effects);
        }
    });
    assert_eq!(checked.function_effect_facts.len(), 3);
    assert_eq!(
        checked
            .function_effect_facts
            .values()
            .filter(|fact| fact.inferred)
            .count(),
        2
    );
}

#[test]
fn effect_inference_keeps_unknown_dynamic_and_test_boundaries() {
    // Exports, entry points, and streams infer like private procs; only an
    // opaque dependency leaves the effects unknown.
    for (source, name, effects) in [
        (
            "proc opaque(callback: Proc) -> Int { let _ = callback.call(); 42 }\n",
            "opaque",
            None,
        ),
        ("proc main() -> Int { 42 }\n", "main", Some(Vec::new())),
        (
            "##! Boundary.\n## Public.\nexport proc published() -> Int { 42 }\n",
            "published",
            Some(Vec::new()),
        ),
        (
            "stream values() -> Stream[Int] { yield 42 }\n",
            "values",
            Some(Vec::new()),
        ),
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        assert_eq!(checked.callable_effects[name], effects, "{name}");
        let compact = Checker::check_compact_declarations(&parsed.arena);
        parsed.arena.symbol_owner().with_current(|| {
            let name = xsh::frontend::symbols::Name::intern(name);
            let signature = compact
                .procs
                .get(&name)
                .or_else(|| compact.streams.get(&name))
                .unwrap();
            assert_eq!(signature.effects, effects);
        });
    }
    let source = "test registered { assert true, \"checked\" }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert_eq!(checked.function_effect_facts.len(), 1);
    let fact = checked.function_effect_facts.values().next().unwrap();
    assert!(!fact.inference_allowed);
    assert!(!fact.inferred);
    assert_eq!(fact.effective, None);
    assert!(checked.callable_effects.is_empty());
}

#[test]
fn signature_cli_compact_metadata_keeps_typed_frames_without_callable_entries() {
    let source = "type Count = UInt\nconst DEFAULT_COUNT = 4\ncli main(count: Count = DEFAULT_COUNT) [] { let value: Int = count + 1; print $value }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    assert_eq!(declarations.function_defs, 1);
    assert!(declarations.procs.is_empty());
    assert!(declarations.pures.is_empty());
    assert!(declarations.streams.is_empty());
    let compact = declarations.bodies;
    for (expression, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(expression).span;
        assert_eq!(
            checked.expr_types.get(&span),
            Some(&ty),
            "{}",
            &source[span.range()]
        );
    }
}

#[test]
fn signature_cli_inferred_defaults_reuse_checked_parameter_facts() {
    let source = "const defaults = {jobs: 4, timeout: 20ms}\ncli main(jobs = defaults.jobs + 1, timeout = defaults.timeout) [] { let count: Int = jobs; let delay: Duration = timeout }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    for parameter in &parsed.arena.arena.params {
        let span = parsed.arena.arena.span(parameter.span);
        assert!(parameter.ty_defaulted);
        assert_eq!(
            checked.parameter_types.get(&span),
            declarations.parameter_types.get(&span)
        );
        assert!(checked.parameter_types.contains_key(&span));
    }
    let compact = declarations.bodies;
    for (expression, ty) in compact.expr_types {
        assert_eq!(
            checked
                .expr_types
                .get(&parsed.arena.arena.expr(expression).span),
            Some(&ty)
        );
    }
}

#[test]
fn typed_cause_full_and_compact_inference_retains_only_outer_error() {
    use xsh::frontend::check::Type;
    let source = "error Outer = Failed(message: Str)\nerror Inner = Failed(message: Str)\nlet value = Err(cause: Inner.Failed(message: \"inner\"), Outer.Failed(message: \"outer\"))\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let full = Checker::check_arena(&parsed.arena, source);
    assert!(full.diagnostics.is_empty(), "{:?}", full.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let (span, full_type) = full
        .expr_types
        .iter()
        .find(|(span, _)| source[span.range()].starts_with("Err("))
        .unwrap();
    let Type::Result(_, error) = full_type else {
        panic!("Err produces Result data")
    };
    parsed.arena.symbol_owner().with_current(|| {
        assert!(matches!(error.as_ref(), Type::ErrorVariant { family, .. } if family.as_str().as_str() == "Outer"));
    });
    let compact_type = compact
        .expr_types
        .iter()
        .find_map(|(id, ty)| (parsed.arena.arena.expr(*id).span == *span).then_some(ty))
        .unwrap();
    assert_eq!(compact_type, full_type);
}

#[test]
fn typed_cause_flattened_handler_guidance_has_no_automatic_fix() {
    let source = "error Input = Failed(message: Str)\nerror Outer = Failed(message: Str)\nproc translate(input: Result[Int, Input]) -> Result[Int, Outer] {\n match input { Ok(value) => return value; Err(failure) => return Err(Outer.Failed(message: failure.message)) }\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena_with_options(
        &parsed.arena,
        source,
        CheckOptions {
            migration_diagnostics: true,
            ..CheckOptions::default()
        },
    );
    let guidance = checked
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("check.error-cause"))
        .collect::<Vec<_>>();
    assert_eq!(guidance.len(), 1, "{:?}", checked.diagnostics);
    assert!(guidance[0].fix_hints.is_empty());
    for replacement in [
        "Err(Outer.Failed(message: failure.message), cause: failure)",
        "Err(Outer.Failed(message: \"custom\"))",
    ] {
        let improved = source.replace("Err(Outer.Failed(message: failure.message))", replacement);
        assert!(!check_with_migration(&improved).contains(&Some("check.error-cause".to_string())));
    }
    let richer = source
        .replace(
            "Outer = Failed(message: Str)",
            "Outer = Failed(message: Str, code: Int)",
        )
        .replace(
            "Outer.Failed(message: failure.message)",
            "Outer.Failed(message: failure.message, code: 7)",
        );
    assert!(!check_with_migration(&richer).contains(&Some("check.error-cause".to_string())));
}

#[test]
fn checker_record_proof_types_agree_on_full_and_compact_routes() {
    let source = "type Inner = {value: Str?}\ntype Outer = {inner: Inner}\npure select(report: Outer) -> Str {\n  let available = report.inner.value != null\n  let retained = available\n  if !retained { return \"missing\" }\n  report.inner.value\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let full = Checker::check_arena(&parsed.arena, source);
    assert!(full.diagnostics.is_empty(), "{:?}", full.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    for (id, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(id).span;
        if &source[span.range()] == "report.inner.value"
            && span.start() > source.find("return").unwrap()
        {
            assert_eq!(ty, xsh::frontend::check::Type::Str);
            assert_eq!(full.expr_types.get(&span), Some(&ty));
        }
    }
}

#[test]
fn constant_key_projection_full_and_compact_facts_preserve_field_types() {
    let source = r#"
type Config = {workers: Int, value: Str?, if: Bool}
const field = "workers"
proc read(config: Config) [error] {
  let workers = config.get(field)?
  let value = config.get("value")?
  let enabled = config["if"]
  let unknown = config.get("hidden")?
  let spread = config.get(...{field: "workers"})?
}
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    assert_eq!(checked.projections.len(), 4);
    assert_eq!(compact.projections.len(), 4);
    for (id, ty) in &compact.expr_types {
        let span = parsed.arena.arena.expr(*id).span;
        if &source[span.range()] == "config.get(\"hidden\")" {
            assert_eq!(Some(ty), checked.expr_types.get(&span));
        }
    }
    for id in compact.projections {
        let span = parsed.arena.arena.expr(id).span;
        assert!(checked.projections.contains_key(&span));
        assert_eq!(checked.expr_types.get(&span), compact.expr_types.get(&id));
    }
    let hidden = source.find("config.get(\"hidden\")").unwrap();
    let (_, ty) = checked
        .expr_types
        .iter()
        .find(|(span, _)| {
            span.start() == hidden && &source[span.range()] == "config.get(\"hidden\")"
        })
        .unwrap();
    assert_eq!(
        ty,
        &xsh::frontend::check::Type::Result(
            Box::new(xsh::frontend::check::Type::Any),
            Box::new(xsh::frontend::check::Type::Error)
        )
    );
}

#[test]
fn constant_key_projection_keeps_module_get_exports_and_callable_metadata() {
    let source = r#"
type Plugin = module { export pure get(value: Str) -> Str }
proc read(plugin: Plugin) {
  let ordinary = plugin.get("get")
  let selected = plugin["get"]
}
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    assert_eq!(checked.projections.len(), 1);
    assert_eq!(compact.projections.len(), 1);
    for id in compact.projections {
        let span = parsed.arena.arena.expr(id).span;
        assert!(matches!(
            checked.projections[&span].callable,
            Some(xsh::frontend::check::ModuleExportType::Pure { .. })
        ));
    }
    for (id, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(id).span;
        if &source[span.range()] == "plugin.get(\"get\")" {
            assert_eq!(ty, xsh::frontend::check::Type::Str);
            assert_eq!(checked.expr_types.get(&span), Some(&ty));
        }
    }
}

#[test]
fn constant_key_projection_guarded_access_keeps_optional_result_layers() {
    let source = "type Config = {workers: Int}\nproc read(config: Config?) [error] {\n  let getter = config?.get(\"workers\")\n  let index = config?[\"workers\"]\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    assert_eq!(checked.projections.len(), 2);
    assert_eq!(compact.projections.len(), 2);
    for id in compact.projections {
        let span = parsed.arena.arena.expr(id).span;
        assert!(checked.projections.contains_key(&span));
        assert_eq!(checked.expr_types.get(&span), compact.expr_types.get(&id));
    }
}

#[test]
fn default_parameter_types_are_checked_declaration_facts_shared_with_compact() {
    let source = "const config = {jobs: 4}\npure choose(jobs = config.jobs + 1) -> Int { jobs }\nlet a = choose()\nlet b = choose(jobs: 9)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    assert_eq!(checked.parameter_types, declarations.parameter_types);
    assert_eq!(
        checked.parameter_types.values().collect::<Vec<_>>(),
        vec![&xsh::frontend::check::Type::Int]
    );
    let compact = declarations.bodies;
    for (id, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(id).span;
        if &source[span.range()] == "jobs" {
            assert_eq!(checked.expr_types.get(&span), Some(&ty));
        }
    }
}

#[test]
fn default_parameter_contract_headers_keep_checked_omitted_literal_types() {
    let source = "type Runner = module { export pure choose(value = 4) -> Int }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    assert!(
        checked
            .parameter_types
            .values()
            .any(|ty| *ty == xsh::frontend::check::Type::Int)
    );
    let compact = Checker::check_compact_declarations(&parsed.arena);
    assert_eq!(compact.parameter_types, checked.parameter_types);
}

#[test]
fn inferred_require_full_and_compact_targets_preserve_schema_identity() {
    let source = "type Marker[T] = {name: Str}\npure validate(raw: Any) -> Result[Marker[Int]] { raw.require()? }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    assert_eq!(checked.requirement_targets.len(), 1);
    assert_eq!(compact.requirement_targets.len(), 1);
    for (expr, target) in compact.requirement_targets {
        assert_eq!(
            checked
                .requirement_targets
                .get(&parsed.arena.arena.expr(expr).span),
            Some(&target)
        );
        assert_eq!(
            target.context.instances[0].arguments,
            vec![xsh::frontend::check::Type::Int]
        );
    }
}

#[test]
fn builtin_template_checked_facts_agree_on_full_and_compact_routes() {
    let source = "error Nested = Missing(code: Int)\npure value(items: List[Result[List[Int], Nested]], table: Map[Int, List[Int]]) -> Int {\n  let one = items.get(index: 0)\n  let two = table.set(value: [2], key: 1)\n  let three = two.keys()\n  let four = two.values()\n  three.len() + four.len()\n}\npure dynamic(items: List[Any]) -> Unit { let value = items.get(0) }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let mut calls = 0;
    for (id, ty) in &compact.expr_types {
        let span = parsed.arena.arena.expr(*id).span;
        if [
            "items.get(index: 0)",
            "table.set(value: [2], key: 1)",
            "two.keys()",
            "two.values()",
            "items.get(0)",
        ]
        .contains(&&source[span.range()])
        {
            assert_eq!(
                checked.expr_types.get(&span),
                Some(ty),
                "{}",
                &source[span.range()]
            );
            assert!(!ty.contains_inference());
            if &source[span.range()] == "items.get(0)" {
                assert_eq!(ty.result_ok(), Some(&xsh::frontend::check::Type::Any));
            }
            calls += 1;
        }
    }
    assert_eq!(calls, 5);
}

#[test]
fn checker_cli_prepared_descriptor_context_uses_the_refined_result() {
    let source = r#"
type ParsedValues = {count: Int}
type CommandValues = {command: Str, action: Str, root: Path, raw: List[Str]}
const schema = {count: {kind: "Int", default: 2}}
const commands = {build: {positionals: ["root"], types: {root: "Path"}, rest: "raw"}}
let parsed: ParsedValues = cli.parse([], schema)?
let applet_values: ParsedValues = cli.applet([], schema)?
let command: CommandValues = cli.commands(["build", "workspace"], commands)?
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let mut calls = 0;
    for (id, ty) in &compact.expr_types {
        let span = parsed.arena.arena.expr(*id).span;
        if source[span.range()].starts_with("cli.")
            && source[span.range()].contains('(')
            && !source[span.range()].ends_with('?')
        {
            assert_eq!(checked.expr_types.get(&span), Some(ty));
            calls += 1;
        }
    }
    assert_eq!(calls, 3);
    let incorrect = source.replace("{count: Int}", "{count: Str}");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &incorrect);
    let checked = Checker::check_arena(&parsed.arena, &incorrect);
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                == Some("check.type-mismatch"))
    );
}

#[test]
fn dynamic_boundary_record_facts_agree_across_checked_representations() {
    let source = "let erased: Record = {name: \"demo\"}\nlet empty = {}\nlet erased_value = erased\nlet empty_value = empty\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let compact = declarations.bodies;
    let mut found = 0;
    for (expression, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(expression).span;
        let expected = match &source[span.range()] {
            "erased" => Some(xsh::frontend::check::Type::ErasedRecord),
            "empty" => Some(xsh::frontend::check::Type::Record(Default::default())),
            _ => None,
        };
        if let Some(expected) = expected {
            assert_eq!(ty, expected);
            assert_eq!(checked.expr_types.get(&span), Some(&expected));
            found += 1;
        }
    }
    assert_eq!(found, 2);
}

#[test]
fn yield_delegation_keeps_checked_stream_sources_in_full_and_compact_facts() {
    let source = "stream lines(file: Path) [fs, error] -> Stream[Str] { yield @file.lines()?; yield @[\"last\"] }\nstream batches() [] -> Stream[List[Int]] { yield @[[], [1]] }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    assert!(
        checked
            .expr_types
            .iter()
            .any(|(span, ty)| &source[span.range()] == "[[], [1]]"
                && *ty
                    == xsh::frontend::check::Type::List(Box::new(
                        xsh::frontend::check::Type::List(Box::new(xsh::frontend::check::Type::Int))
                    )))
    );
    let compact = declarations.bodies;
    let mut found = 0;
    for (expression, ty) in compact.expr_types {
        let span = parsed.arena.arena.expr(expression).span;
        if matches!(&source[span.range()], "file.lines()" | "file.lines()?") {
            assert_eq!(checked.expr_types.get(&span), Some(&ty));
            found += 1;
        }
    }
    assert_eq!(found, 2);
}

#[test]
fn compact_count_and_json_adapter_facts_match_checked_pipeline_outputs() {
    use xsh::frontend::check::Type;
    let source = r#"
let stats = ["rs", "md", "rs"] |> count { |ext| ext }
let total = ["rs", "md"] |> count
let rows = "{}\n{}\n" |> json.lines
let values = "{} {}" |> json.stream
let keys = stats.keys()
let lengths = {lines: rows.len(), values: values.len()}
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let mut found = 0;
    for (id, actual) in &compact.expr_types {
        let expression = parsed.arena.arena.expr(*id);
        if !matches!(
            expression.kind,
            xsh::frontend::syntax::arena::ArenaExprKind::StructuredPipeline { .. }
        ) {
            continue;
        }
        let expected = match &source[expression.span.range()] {
            text if text.ends_with("count { |ext| ext }") => {
                Type::Map(Box::new(Type::Str), Box::new(Type::Int))
            }
            text if text.ends_with("count") => Type::Int,
            text if text.ends_with("json.lines") || text.ends_with("json.stream") => {
                Type::List(Box::new(Type::Any))
            }
            text => panic!("unexpected pipeline: {text}"),
        };
        assert_eq!(checked.expr_types.get(&expression.span), Some(&expected));
        assert_eq!(actual, &expected);
        found += 1;
    }
    assert_eq!(found, 4);
}

#[test]
fn compact_stream_stage_result_matrix_matches_canonical_checked_facts() {
    let source = r#"
let parallel = ["a"] |> par-map { |value| value.byte_len() }
let batches = [1, 2] |> batch(count: 2)
let numbered = ["a"] |> enumerate
let pairs = ["a"] |> zip([1])
let generated = ["a"] |> range(start: 0, end: 2)
let groups = ["a"] |> group-by { |value| value }
let sum = [1, 2] |> sum
let first = [1, 2] |> first
let last = [1, 2] |> last
let minimum = [1, 2] |> min
let maximum = [1, 2] |> max
let accumulators = [1, 2] |> reduce-by(sum: true) { |value| {key: "n", value: value} }
let folded = [1, 2] |> fold(0) { |acc, value| Ok(acc + value)? }
let reduced = [1, 2] |> reduce(0) { |acc, value| Ok(acc + value)? }
let lines = "a\nb\n" |> text.lines
"#;
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let declarations = Checker::check_compact_declarations(&parsed.arena);
    let compact = declarations.bodies;
    let mut pipelines = 0;
    for (id, actual) in &compact.expr_types {
        let expression = parsed.arena.arena.expr(*id);
        if matches!(
            expression.kind,
            xsh::frontend::syntax::arena::ArenaExprKind::StructuredPipeline { .. }
        ) {
            assert_eq!(
                Some(actual),
                checked.expr_types.get(&expression.span),
                "{}",
                &source[expression.span.range()]
            );
            assert!(!actual.contains_inference());
            pipelines += 1;
        }
    }
    assert_eq!(pipelines, 15);
}

#[test]
fn canonical_stream_stage_facts_distinguish_equal_spans_in_modules() {
    use xsh::frontend::check::Type;
    use xsh::frontend::symbols::Name;
    use xsh::frontend::syntax::arena::{
        ArenaExprKind, ArenaExprOrRun, ArenaProgramBuilder, ArenaStmtKind,
    };
    let mut builder = ArenaProgramBuilder::with_token_capacity(64);
    let entry = Parser::parse_source_into_arena_builder(SourceId::new(0), "", &mut builder);
    for (name, source) in [
        ("left", "let value = [1] |> sum\n"),
        ("right", "let value = [1] |> min\n"),
    ] {
        let fragment =
            Parser::parse_source_into_arena_builder(SourceId::new(0), source, &mut builder);
        assert!(
            fragment.diagnostics.is_empty(),
            "{:?}",
            fragment.diagnostics
        );
        let name = builder.symbol_owner().with_current(|| Name::intern(name));
        builder.push_arena_module(name.to_string(), name, fragment.statements);
    }
    let program = builder.finish_with_statements(entry.statements);
    let declarations = Checker::check_compact_declarations(&program);
    assert!(
        declarations.diagnostics.is_empty(),
        "{:?}",
        declarations.diagnostics
    );
    let mut span = None;
    for (index, module) in program.modules.iter().enumerate() {
        let statement = program
            .module_statements(module)
            .next()
            .expect("module binding");
        let ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expression),
            ..
        } = program.arena.stmt(statement).kind
        else {
            panic!("binding")
        };
        let ArenaExprKind::StructuredPipeline { stages, .. } = program.arena.expr(expression).kind
        else {
            panic!("pipeline")
        };
        let stage_span = program
            .arena
            .span(program.arena.stream_stages(stages)[0].span);
        if let Some(previous) = span {
            assert_eq!(stage_span, previous);
        }
        span = Some(stage_span);
        let expected = if index == 0 {
            Type::Int
        } else {
            Type::Result(Box::new(Type::Int), Box::new(Type::Error))
        };
        assert_eq!(
            declarations
                .stream_stage_types
                .get(&(Some(module.name), stage_span))
                .map(|fact| &fact.output),
            Some(&expected)
        );
    }
}

#[test]
fn fold_complete_accumulator_contracts_match_full_and_compact_facts() {
    use xsh::frontend::check::Type;
    use xsh::frontend::syntax::arena::ArenaExprKind;
    for stage in ["fold", "reduce"] {
        let source = format!(
            "let initial: Result[Int] = Ok(0)\nlet total = [1, 2] |> {stage}(initial) {{ |acc, item| match acc {{ Ok(value) => Ok(value + item), Err(error) => Err(error) }} }}\n"
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        let compact = declarations.bodies;
        let expected = Type::Result(Box::new(Type::Int), Box::new(Type::Error));
        let mut pipelines = 0;
        for (id, actual) in &compact.expr_types {
            let expression = parsed.arena.arena.expr(*id);
            if matches!(expression.kind, ArenaExprKind::StructuredPipeline { .. }) {
                assert_eq!(actual, &expected);
                assert_eq!(checked.expr_types.get(&expression.span), Some(actual));
                pipelines += 1;
            }
        }
        assert_eq!(pipelines, 1);
    }
}

#[test]
fn stream_callback_result_contracts_publish_matching_full_and_compact_types() {
    use xsh::frontend::check::Type;
    use xsh::frontend::symbols::Name;
    use xsh::frontend::syntax::arena::ArenaExprKind;
    for stage in [
        "map { |item| produce(item) }",
        "par-map(jobs: 2) { |item| produce(item) }",
        "where { |item| Ok(item > 0)? }",
        "any { |item| Ok(item > 0)? }",
        "all { |item| Ok(item > 0)? }",
        "count { |item| Ok(item)? }",
        "sort-by { |item| Ok(item)? }",
        "group-by { |item| produce(item) }",
        "unique-by { |item| produce(item) }",
        "flat-map { |item| Ok([item]) }",
    ] {
        let source = format!(
            "error ItemError = Stop(item: Int)\npure produce(item: Int) -> Result[Int, ItemError] {{ if item == 2 {{ Err(ItemError.Stop(item)) }} else {{ item }} }}\nlet values = [1, 2, 3] |> {stage}\n"
        );
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, &source);
        assert!(
            checked.diagnostics.is_empty(),
            "{stage}: {:?}",
            checked.diagnostics
        );
        let declarations = Checker::check_compact_declarations(&parsed.arena);
        assert_eq!(
            checked.stream_stage_types.len(),
            declarations.stream_stage_types.len()
        );
        for (key, fact) in &checked.stream_stage_types {
            let compact_fact = declarations
                .stream_stage_types
                .get(key)
                .expect("checked stage retained");
            assert_eq!(fact.input, compact_fact.input);
            assert_eq!(fact.output, compact_fact.output);
        }
        let compact = declarations.bodies;
        parsed.arena.symbol_owner().with_current(|| {
            let payload = Type::Result(
                Box::new(Type::Int),
                Box::new(Type::ErrorFamily(Name::intern("ItemError"))),
            );
            let expected = if stage.starts_with("map ") || stage.starts_with("par-map") {
                Type::List(Box::new(payload))
            } else if stage.starts_with("any ") || stage.starts_with("all ") {
                Type::Bool
            } else if stage.starts_with("count ") {
                Type::Map(Box::new(Type::Str), Box::new(Type::Int))
            } else if stage.starts_with("group-by ") {
                Type::List(Box::new(Type::Record(std::collections::BTreeMap::from([
                    (Name::intern("key"), payload),
                    (Name::intern("items"), Type::List(Box::new(Type::Int))),
                ]))))
            } else {
                Type::List(Box::new(Type::Int))
            };
            let mut pipelines = 0;
            for (id, actual) in &compact.expr_types {
                let expression = parsed.arena.arena.expr(*id);
                if matches!(expression.kind, ArenaExprKind::StructuredPipeline { .. }) {
                    assert_eq!(actual, &expected, "{stage}");
                    assert_eq!(checked.expr_types.get(&expression.span), Some(actual));
                    assert!(!actual.contains_inference());
                    pipelines += 1;
                }
            }
            assert_eq!(pipelines, 1, "{stage}");
        });
    }
}
