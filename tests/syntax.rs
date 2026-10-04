use xsh::diagnostic::DiagnosticCode;
use xsh::frontend::check::Checker;
use xsh::frontend::source::SourceId;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaBuilderEntryKind, ArenaCommand,
    ArenaCommandArgKind, ArenaExprKind, ArenaExprOrRun, ArenaFmtPart, ArenaPatternKind,
    ArenaPipeStageKind, ArenaRecordFieldKind, ArenaSpawnTarget, ArenaStmtKind, ArenaTypeDefBody,
    ArenaWordPart, ExprId, StmtId,
};
use xsh::frontend::syntax::cst::{SyntaxElement, SyntaxGroupKind, SyntaxKind, TriviaKind};
use xsh::frontend::syntax::lexer::Lexer;
use xsh::frontend::syntax::node::{
    AssignOp, BinaryOp, Effect, RedirectionKind, RunKind, StreamStageKind,
};
use xsh::frontend::syntax::parser::{ArenaParseOutput, Parser};
use xsh::frontend::syntax::token::TokenTag;
use xsht::format::Formatter;

#[test]
fn boolean_guards_keep_separate_arena_kind_and_cst_round_trip() {
    let source = "proc checked(name: Str?) [] -> Str {\n  guard name != null else {\n    # Preserve the authored failure.\n    return \"missing\"\n  }\n\n  name.trim()\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.cst.get().exact_text(), source);
    let arena = &parsed.arena.arena;
    let function = arena.stmt_ids(parsed.arena.statements).next().unwrap();
    let ArenaStmtKind::ProcDef(def) = arena.stmt(function).kind else {
        panic!("expected proc")
    };
    let guard = arena
        .stmt_ids(arena.block(arena.function_def(def).body).statements)
        .next()
        .unwrap();
    assert!(matches!(
        arena.stmt(guard).kind,
        ArenaStmtKind::BooleanGuard { .. }
    ));
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
    assert_parse_and_check(SourceId::new(0), &formatted.formatted);
}

fn assert_parse_and_check(source_id: SourceId, source: &str) {
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "formatted output has parse errors:\n---\n{source}\n---\n{:?}",
        parsed.diagnostics
    );
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(
        checked.diagnostics.is_empty(),
        "formatted output has check errors:\n---\n{source}\n---\n{:?}",
        checked.diagnostics
    );
}

#[test]
fn identifier_subtraction_preserves_negative_command_arguments() {
    let source = "value - 1\nemit -1\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let statements = parsed.arena.statement_ids().collect::<Vec<_>>();
    let arena = &parsed.arena.arena;
    for statement in &statements[..1] {
        let ArenaStmtKind::Expr(expr) = arena.stmt(*statement).kind else {
            panic!("expected subtraction expression");
        };
        assert!(matches!(
            arena.expr(expr).kind,
            ArenaExprKind::Binary {
                op: BinaryOp::Sub,
                ..
            }
        ));
    }
    assert!(matches!(
        arena.stmt(statements[1]).kind,
        ArenaStmtKind::Command(_)
    ));
}

/// The expression initializer of the `index`-th root `let` statement, via the
/// arena. Panics if that statement is not a `let`-with-expression.
fn root_let_init_expr(output: &ArenaParseOutput, index: usize) -> ExprId {
    let arena = &output.arena.arena;
    let id = output
        .arena
        .statement_ids()
        .nth(index)
        .expect("root statement at index");
    match arena.stmt(id).kind {
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        } => expr,
        ref kind => panic!("expected let-with-expression, got {kind:?}"),
    }
}

#[test]
fn lexer_fixture_covers_valid_and_invalid_inputs() {
    let valid = include_str!("fixtures/syntax/valid/language.xsh");
    let invalid = "let data = b\"\\u{41}\"\n";

    let valid_output = Lexer::new(SourceId::new(0), valid).lex_compact();
    let invalid_output = Lexer::new(SourceId::new(0), invalid).lex_compact();

    assert!(valid_output.diagnostics.is_empty());
    assert!(
        (0..valid_output.token_table.len())
            .any(|index| valid_output.token_table.tag_at(index) == Some(TokenTag::Comment))
    );
    assert!(
        invalid_output
            .diagnostics
            .iter()
            .any(|diag| diag.code.map(DiagnosticCode::name) == Some("lex.invalid-bytes-escape"))
    );
}

#[test]
fn parser_fixture_covers_baseline_shapes() {
    let source = include_str!("fixtures/syntax/valid/language.xsh");
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(
        output
            .arena
            .statement_ids()
            .any(|id| matches!(arena.stmt(id).kind, ArenaStmtKind::ProcDef(_)))
    );
    assert!(
        output
            .arena
            .statement_ids()
            .any(|id| matches!(arena.stmt(id).kind, ArenaStmtKind::PureDef(_)))
    );
}

#[test]
fn parser_retains_module_and_export_doc_comment_spans() {
    let source = r#"
##! Test module documentation.

## Exposes a documented value.
export let value: Int = 1
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let module_doc = output.arena.module_doc().expect("module doc span");
    assert_eq!(
        &source[module_doc.range()],
        "##! Test module documentation.\n"
    );
    let export = output
        .arena
        .statement_ids()
        .next()
        .expect("export statement");
    let export_doc = output.arena.export_doc(export).expect("export doc span");
    assert_eq!(
        &source[export_doc.range()],
        "## Exposes a documented value."
    );
}

#[test]
fn parser_does_not_treat_multiline_string_headings_as_doc_comments() {
    let source = r##"
let report = "# Manager\n\n## North-star impact\n\nfixture\n\n## task-tags\n"
"##;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert!(output.arena.docs.module.is_none());
    assert!(output.arena.docs.orphaned.is_empty());
    assert!(output.arena.docs.duplicate_modules.is_empty());
}

#[test]
fn arena_accessors_decode_compact_frontend_shapes() {
    let source = r#"
proc main(name: Str) [fs, error] -> Result[Unit] {
  let greeting = f"hi {name}"
  let nums = [1, 2, 3]
  print "hi ${greeting}" tail
}
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena_program = &output.arena;
    let arena = &arena_program.arena;
    let root_stmt_ids: Vec<_> = arena_program.statement_ids().collect();
    assert_eq!(root_stmt_ids.len(), 1);

    let function_id = match arena.stmt(root_stmt_ids[0]).kind {
        ArenaStmtKind::ProcDef(id) => id,
        ref kind => panic!("expected proc definition, got {kind:?}"),
    };
    let function = arena.function_def(function_id);
    let params = arena.params(function.params);
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].name, "name");
    let effects: Vec<_> = arena
        .effects(function.effects.expect("function effects"))
        .collect();
    assert_eq!(effects, vec![Effect::Fs, Effect::Error]);

    let body = arena.block(function.body);
    let body_stmt_ids: Vec<_> = arena.stmt_ids(body.statements).collect();
    assert_eq!(body_stmt_ids.len(), 3);

    let greeting_expr = match &arena.stmt(body_stmt_ids[0]).kind {
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(id),
            ..
        } => *id,
        kind => panic!("expected greeting let, got {kind:?}"),
    };
    let fmt_parts = match &arena.expr(greeting_expr).kind {
        ArenaExprKind::FmtString(range) => arena.fmt_parts(*range).collect::<Vec<_>>(),
        kind => panic!("expected formatted string, got {kind:?}"),
    };
    assert!(
        matches!(&fmt_parts[0], ArenaFmtPart::Text(text) if arena.text_value(text, source) == Some("hi "))
    );
    assert!(fmt_parts.iter().any(|part| match part {
        ArenaFmtPart::Expr(id, None) => {
            matches!(arena.expr(*id).kind, ArenaExprKind::Ident(name) if name == "name")
        }
        _ => false,
    }));

    let nums_expr = match &arena.stmt(body_stmt_ids[1]).kind {
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(id),
            ..
        } => *id,
        kind => panic!("expected nums let, got {kind:?}"),
    };
    let item_ids: Vec<_> = match &arena.expr(nums_expr).kind {
        ArenaExprKind::List(range) => arena.list_element_exprs(*range).collect(),
        kind => panic!("expected list expression, got {kind:?}"),
    };
    let values: Vec<_> = item_ids
        .iter()
        .map(|id| match &arena.expr(*id).kind {
            ArenaExprKind::Int(literal_id) => arena.int_literal(*literal_id).value(),
            kind => panic!("expected integer literal, got {kind:?}"),
        })
        .collect();
    assert_eq!(values, vec![Some(1), Some(2), Some(3)]);

    let command_id = match arena.stmt(body_stmt_ids[2]).kind {
        ArenaStmtKind::Command(id) => id,
        ref kind => panic!("expected command statement, got {kind:?}"),
    };
    let command = arena.command_stmt(command_id);
    let args = match &command.command {
        ArenaCommand::Core {
            args, env, block, ..
        } => {
            assert!(arena.env_assignments(*env).is_empty());
            assert!(block.is_none());
            arena.command_args(*args)
        }
        command => panic!("expected core command, got {command:?}"),
    };
    assert_eq!(args.len(), 2);
    let first_parts = match args[0].kind {
        ArenaCommandArgKind::Word(range) => arena.word_parts(range).collect::<Vec<_>>(),
        ref kind => panic!("expected word command arg, got {kind:?}"),
    };
    assert!(first_parts
        .iter()
        .any(|part| matches!(part, ArenaWordPart::Quoted(text) if arena.text_value(text, source) == Some("hi "))));
    assert!(first_parts.iter().any(|part| match part {
        ArenaWordPart::Interpolation(id) => {
            matches!(arena.expr(*id).kind, ArenaExprKind::Ident(name) if name == "greeting")
        }
        _ => false,
    }));
    let second_parts = match args[1].kind {
        ArenaCommandArgKind::Word(range) => arena.word_parts(range).collect::<Vec<_>>(),
        ref kind => panic!("expected word command arg, got {kind:?}"),
    };
    assert!(
        matches!(&second_parts[0], ArenaWordPart::Bare(text) if arena.text_value(text, source) == Some("tail"))
    );
}

#[test]
fn cst_preserves_source_text_tokens_and_trivia() {
    let source = "# leading\r\nlet value = [1, 2]\n\nproc main() {\n\tprint ${value}\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let cst = output.cst.get();
    assert_eq!(cst.exact_text(), source);
    assert!(
        cst.trivia_items()
            .iter()
            .any(|trivia| trivia.kind == TriviaKind::Whitespace)
    );
    assert!(
        cst.trivia_items()
            .iter()
            .any(|trivia| trivia.kind == TriviaKind::Comment)
    );
    assert!(
        cst.trivia_items()
            .iter()
            .any(|trivia| trivia.kind == TriviaKind::Newline)
    );
}

#[test]
fn cst_groups_delimiters_and_maps_ast_spans() {
    let source = "proc main() {\n  # kept\n  let row = {name: \"demo\", values: [1, 2]}\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let stmt_id = output.arena.statement_ids().next().expect("statement");
    let stmt_span = output.arena.arena.stmt(stmt_id).span;
    let cst = output.cst.get();
    assert!(cst.contains_comment(stmt_span));
    assert!(
        cst.tokens_in_span(stmt_span)
            .iter()
            .any(|id| cst.token_text(*id) == "proc")
    );

    let root = cst.node(cst.root());
    assert!(root.children.iter().any(|child| match child {
        SyntaxElement::Node(id) => {
            matches!(
                cst.node(*id).kind,
                SyntaxKind::Group(SyntaxGroupKind::Brace)
            )
        }
        SyntaxElement::Token(_) | SyntaxElement::Trivia(_) => false,
    }));

    let covering = cst.covering_node(stmt_span).expect("covering node");
    assert_eq!(cst.node(covering).kind, SyntaxKind::Root);
}

#[test]
fn parser_and_formatter_accept_float_literals() {
    let source = "let a = 1.0\nlet b = 0.25\nlet c = 10e-3\nlet d = 1.5e6\nlet m = 1.float()\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        matches!(arena.stmt(id).kind, ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(e), .. }
            if matches!(arena.expr(e).kind, ArenaExprKind::Float(_)))
    }));

    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn formatter_reuses_parsed_program_without_changing_output() {
    let source = "let value =   1\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let direct = Formatter::new().format_source(SourceId::new(0), source);
    let reused = Formatter::new().format_parsed_source(source, &parsed);

    assert_eq!(reused.formatted, direct.formatted);
    assert!(reused.diagnostics.is_empty(), "{:?}", reused.diagnostics);
}

#[test]
fn parser_and_formatter_accept_map_comprehensions() {
    let source =
        "let by_name = {item.name: item.version for item in items if item.version != \"\"}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        matches!(arena.stmt(id).kind, ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(e), .. }
            if matches!(arena.expr(e).kind, ArenaExprKind::MapComp { .. }))
    }));

    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn parser_accepts_bracketed_computed_map_literal_keys() {
    use xsh::frontend::syntax::arena::ArenaRecordFieldKind;
    let source = "let by_name = {[item.name]: item.version}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let ArenaExprKind::Record(fields) = arena.expr(root_let_init_expr(&output, 0)).kind else {
        panic!("map literal");
    };
    assert!(matches!(
        arena.record_fields(fields)[0].kind,
        ArenaRecordFieldKind::Computed { .. }
    ));
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn parser_accepts_keyword_schema_and_record_field_labels() {
    let source =
        "type Accum = {run: Int, lines: List[Str]}\nlet rec: Accum = {run: 0, lines: []}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_parse_and_check(SourceId::new(0), source);
}

#[test]
fn parser_accepts_quoted_reserved_record_fields() {
    let source = "let rec = {\"run\": 0, \"lines\": []}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn parser_reports_unsupported_c_style_boolean_operators_constructively() {
    // Unsupported C-style boolean operators and the `then` keyword must be
    // named by a constructive diagnostic that points at the offending token,
    // not at the block brace that follows the condition.
    let cases = [
        (
            "proc main() { if a || b { } }\n",
            "parse.unsupported-boolean-operator",
        ),
        (
            "proc main() { if a && b { } }\n",
            "parse.unsupported-boolean-operator",
        ),
        (
            "proc main() { if a | b { } }\n",
            "parse.unsupported-boolean-operator",
        ),
        (
            "proc main() { if a & b { } }\n",
            "parse.unsupported-boolean-operator",
        ),
        ("proc main() { if a then { } }\n", "parse.unsupported-then"),
    ];
    for (source, code) in cases {
        let output = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            output
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some(code)),
            "expected {code} but got for source:\n{source}\n{:?}",
            output.diagnostics
        );
    }
}

#[test]
fn parser_reports_integer_division_spellings_with_int_division_guidance() {
    for source in ["let quotient = 7 // 2\n", "let quotient = 7 div 2\n"] {
        let output = Parser::parse_source_arena_only(SourceId::new(0), source);
        let diagnostic = output
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code.map(DiagnosticCode::name)
                    == Some("parse.unsupported-integer-division")
            })
            .unwrap_or_else(|| panic!("expected integer-division diagnostic: {output:?}"));
        assert!(diagnostic.message.contains("use `/` on Int operands"));
        assert_eq!(diagnostic.fix_hints[0].replacement.as_deref(), Some("/"));
    }

    assert_parse_and_check(SourceId::new(0), "let quotient = 7 / 2\n");
}

#[test]
fn parser_accepts_word_form_boolean_operators() {
    // The valid `or`/`and` word forms must parse without diagnostics so the
    // new constructive error does not change valid-program behavior.
    for source in [
        "proc main() { if a or b { } }\n",
        "proc main() { if a or b and c { } }\n",
    ] {
        let output = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            output.diagnostics.is_empty(),
            "source:\n{source}\n{:?}",
            output.diagnostics
        );
    }
}

#[test]
fn parser_accepts_nominal_error_declarations_and_patterns() {
    let source = r#"
error FsError = NotFound(file: Path) : NotFound | PermissionDenied(file: Path, op: Str) : PermissionDenied

let result = Err(FsError.NotFound(file: Path("missing")))
match result {
  Err(FsError.NotFound { file }) => { print ${file.display()} }
  Err(is PermissionDenied) => { print "permission denied" }
  Err(error) => { print ${error.message} }
}
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let error_def_id = output
        .arena
        .statement_ids()
        .find_map(|id| match arena.stmt(id).kind {
            ArenaStmtKind::ErrorDef(id) => Some(id),
            _ => None,
        })
        .expect("expected an error definition statement");
    let error_def = arena.error_def(error_def_id);
    assert_eq!(error_def.name, "FsError");
    let variants = arena.error_variants(error_def.variants);
    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0].name, "NotFound");
    let not_found_fields = arena.error_fields(variants[0].fields);
    assert_eq!(not_found_fields.len(), 1);
    assert_eq!(not_found_fields[0].name, "file");
    let not_found_facets: Vec<_> = arena.names(variants[0].facets).collect();
    assert_eq!(not_found_facets, ["NotFound"]);
    assert_eq!(variants[1].name, "PermissionDenied");
    let denied_fields = arena.error_fields(variants[1].fields);
    assert_eq!(denied_fields.len(), 2);
    assert_eq!(denied_fields[0].name, "file");
    assert_eq!(denied_fields[1].name, "op");
    let denied_facets: Vec<_> = arena.names(variants[1].facets).collect();
    assert_eq!(denied_facets, ["PermissionDenied"]);
}

#[test]
fn parser_and_formatter_accept_type_patterns() {
    let source = r#"match json.decode("1.25")? {
  i is Int => print ${i.float()}
  f is Float => print ${f}
  _ is Null => print "null"
  _ => print "other"
}
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        let ArenaStmtKind::Match { arms, .. } = arena.stmt(id).kind else {
            return false;
        };
        let arms = arena.match_arms(arms);
        !arms.is_empty()
            && matches!(
                arena.pattern(arms[0].pattern).kind,
                ArenaPatternKind::Type { binding: Some(name), ty }
                    if name.as_str() == "i" && arena.type_expr_named(ty, "Int")
            )
    }));

    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn parser_and_formatter_accept_module_contract_types() {
    let source = r#"type Plugin = module {
  export let name: Str
  export optional let description: Str
  export proc execute(root: Path) [fs, error] -> Result[Unit]
  export pure label(name: Str) -> Str
}

let plugin: Module[Plugin] = module.load(p"plugin.xsh")?
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        matches!(arena.stmt(id).kind, ArenaStmtKind::TypeDef(def)
            if matches!(arena.type_def(def).body, ArenaTypeDefBody::ModuleContract(_)))
    }));

    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn run_command_fixture_preserves_argv_boundary() {
    let output = Parser::parse_source_arena_only(SourceId::new(0), "run make -j${cpu.count()} ?\n");

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
        panic!("expected run command");
    };
    let form = arena.run_form(*run_id);
    let segments = arena.run_segments(form.segments);
    let args = arena.command_args(segments[0].args);
    assert_eq!(args.len(), 1);
    assert!(matches!(args[0].kind, ArenaCommandArgKind::Word(_)));
}

#[test]
fn parser_accepts_grouped_multiline_run_invocation() {
    let source = "run (\n  $make\n  \"ARCH=arm64\"\n  f\"CC={cc}\"\n  \"Image\"\n) ?\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
        panic!("expected run command");
    };
    let form = arena.run_form(*run_id);
    let segments = arena.run_segments(form.segments);
    assert!(segments[0].grouped);
    assert_eq!(arena.command_args(segments[0].args).len(), 3);
    assert!(form.propagate);
}

#[test]
fn formatter_preserves_grouped_run_invocation_shape() {
    let source = "run (\n$make\n\"ARCH=arm64\"\nf\"CC={cc}\"\n\"Image\"\n)?\n";
    let output = xsht::format::Formatter::new().format_source(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(
        output.formatted,
        "run (\n  $make\n  \"ARCH=arm64\"\n  f\"CC={cc}\"\n  \"Image\"\n) ?\n"
    );
}

#[test]
fn parser_accepts_byte_pipeline_and_redirections() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "run sort < (input) | run uniq > ${out} 2>> (errlog) ?\n",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
        panic!("expected run command");
    };
    let form = arena.run_form(*run_id);
    assert!(form.propagate);
    let segments = arena.run_segments(form.segments);
    assert_eq!(segments.len(), 2);
    assert_eq!(
        arena.redirections(segments[0].redirections)[0].kind,
        RedirectionKind::StdinRead
    );
    assert_eq!(
        arena
            .redirections(segments[1].redirections)
            .iter()
            .map(|redirection| redirection.kind)
            .collect::<Vec<_>>(),
        vec![RedirectionKind::StdoutWrite, RedirectionKind::StderrAppend]
    );
}

#[test]
fn parser_accepts_env_assignments_blocks_and_membership() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "\
run CC=cc CFLAGS=\"-O2 -pipe\" make
env DESTDIR=/tmp/stage {
  if \"/tmp/stage/bin\" not in env.PATH {
    print missing
  }
}
",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
        panic!("expected run command");
    };
    let form = arena.run_form(*run_id);
    let segments = arena.run_segments(form.segments);
    assert_eq!(arena.env_assignments(segments[0].env).len(), 2);

    let ArenaStmtKind::Command(cmd2) = arena.stmt(root[1]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Core { env, block, .. } = &arena.command_stmt(cmd2).command else {
        panic!("expected core command");
    };
    assert_eq!(arena.env_assignments(*env).len(), 1);
    assert!(block.is_some());
}

#[test]
fn parser_and_formatter_accept_signal_hooks() {
    let source =
        "on TERM --pre-cancel=50ms [error, process, fs] {\nprint \"stop\"\n}\nlet on = 1\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let root: Vec<_> = parsed.arena.statement_ids().collect();
    assert!(matches!(
        arena.stmt(root[0]).kind,
        ArenaStmtKind::SignalHook(_)
    ));
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(
        formatted.formatted,
        "on TERM --pre-cancel=50ms [fs, process, error] {\n  print \"stop\"\n}\n\nlet on = 1\n"
    );
}

#[test]
fn formatter_preserves_signal_hook_comments() {
    let source = "# before\non TERM [error, fs] {\n# inside\nprint \"stop\"\n}\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(
        formatted.formatted,
        "# before\non TERM [fs, error] {\n  # inside\n  print \"stop\"\n}\n"
    );
}

#[test]
fn parser_reports_malformed_signal_hook_syntax() {
    let missing_effects = Parser::parse_source_arena_only(SourceId::new(0), "on SIGINT {\n}\n");
    assert!(
        missing_effects
            .diagnostics
            .iter()
            .any(|diag| diag.code.map(DiagnosticCode::name) == Some("parse.signal-hook"))
    );

    let bad_option =
        Parser::parse_source_arena_only(SourceId::new(0), "on TERM --pre-cancel=soon [] {\n}\n");
    assert!(
        bad_option
            .diagnostics
            .iter()
            .any(|diag| diag.code.map(DiagnosticCode::name) == Some("parse.signal-hook"))
    );
}

#[test]
fn parser_accepts_path_literals_and_expr_env_blocks() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "\
let root = ./src
let cc = /usr/bin/cc
let parent = ../src/main.c
env ({
  HOME: root,
  JOBS: cpu.count(),
}) {
  run make -C p\"src\" ?
} ?
",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    for index in 0..3 {
        assert!(matches!(
            arena.expr(root_let_init_expr(&output, index)).kind,
            ArenaExprKind::PathStr(_)
        ));
    }
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Expr(expr) = arena.stmt(root[3]).kind else {
        panic!("expected environment scope");
    };
    let ArenaExprKind::Try(scope) = arena.expr(expr).kind else {
        panic!("expected propagation");
    };
    assert!(matches!(
        arena.expr(scope).kind,
        ArenaExprKind::ContextScope {
            kind: xsh::frontend::syntax::arena::ContextScopeKind::Env,
            value_body: false,
            ..
        }
    ));
}

#[test]
fn parser_keeps_bare_paths_contextual_with_division() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "\
let rel = ./x
let parent = ../x
let root=/tmp
let paths = [/tmp/a, /tmp/b]
let ratio = 1/2
let spaced = 1 / 2
",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    for index in 0..4 {
        let expr = root_let_init_expr(&output, index);
        if index == 3 {
            let ArenaExprKind::List(items) = arena.expr(expr).kind else {
                panic!("expected path list");
            };
            assert!(
                arena
                    .list_element_exprs(items)
                    .all(|id| matches!(arena.expr(id).kind, ArenaExprKind::PathStr(_)))
            );
        } else {
            assert!(matches!(arena.expr(expr).kind, ArenaExprKind::PathStr(_)));
        }
    }
    for index in 4..6 {
        let expr = root_let_init_expr(&output, index);
        assert!(matches!(
            arena.expr(expr).kind,
            ArenaExprKind::Binary {
                op: BinaryOp::Div,
                ..
            }
        ));
    }
}

#[test]
fn parser_accepts_raw_triple_and_nested_fmt_strings() {
    let source = r#"
let raw = r"\n ${literal}"
let multi = """alpha
beta"""
let label = f"""{ {name: "demo"}.name }:{if true { "x}" } else { "y" }}:{f"{1}"}"""
let literal = f"${{name}}:{name}"
let path = fp"{Path("root")}/bin/tool"
run echo "${{name: "demo"}.name}"
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(matches!(arena.expr(root_let_init_expr(&output, 0)).kind,
        ArenaExprKind::Str(v) if arena.string_literal(v).as_ref() == r"\n ${literal}"));
    assert!(matches!(arena.expr(root_let_init_expr(&output, 1)).kind,
        ArenaExprKind::Str(v) if arena.string_literal(v).as_ref() == "alpha\nbeta"));
    assert!(matches!(arena.expr(root_let_init_expr(&output, 2)).kind,
        ArenaExprKind::FmtString(parts) if arena.fmt_parts(parts).count() == 5));
    assert!(matches!(arena.expr(root_let_init_expr(&output, 3)).kind,
        ArenaExprKind::FmtString(parts) if arena.fmt_parts(parts).count() == 2));
    assert!(matches!(arena.expr(root_let_init_expr(&output, 4)).kind,
        ArenaExprKind::PathFmtString(parts) if arena.fmt_parts(parts).count() == 2));
}

#[test]
fn parser_accepts_display_string_shorthand_interpolation() {
    let source = r#"
let name = "world"
let label = f"hello {name}"
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(matches!(
        arena.expr(root_let_init_expr(&output, 1)).kind,
        ArenaExprKind::FmtString(parts) if arena.fmt_parts(parts).count() == 2
    ));
}

#[test]
fn parser_accepts_nested_interpolation_boundaries_from_shared_scanner() {
    let source = r#"
let label = f"{ {raw: r"}", triple: """}""", nested: f"{ {brace: "}"} .brace }"}.nested }"
run echo "${{name: f"{1}", text: "}"} .name}"
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn parser_marks_only_block_tail_plain_identifiers_as_tail_candidates() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        r#"
proc tail(value: Str) -> Result[Str] {
  value
}
proc non_tail(value: Str) -> Result[Str] {
  value
  print done
}
proc hyphen() -> Result[Unit] {
  build-all
}
proc dotted() -> Result[Unit] {
  h.greet("done")?
}
"#,
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let body_ids = |idx: usize| -> Vec<StmtId> {
        let ArenaStmtKind::ProcDef(def) = arena.stmt(root[idx]).kind else {
            panic!("expected proc");
        };
        arena
            .stmt_ids(arena.block(arena.function_def(def).body).statements)
            .collect()
    };
    let b0 = body_ids(0);
    assert!(matches!(
        arena.stmt(b0[0]).kind,
        ArenaStmtKind::TailBareIdent(name) if name.as_str() == "value"
    ));
    let b1 = body_ids(1);
    assert!(matches!(arena.stmt(b1[0]).kind, ArenaStmtKind::Command(_)));
    assert!(matches!(arena.stmt(b1[1]).kind, ArenaStmtKind::Command(_)));
    let b2 = body_ids(2);
    assert!(matches!(arena.stmt(b2[0]).kind, ArenaStmtKind::Command(_)));
    let b3 = body_ids(3);
    assert!(matches!(arena.stmt(b3[0]).kind, ArenaStmtKind::Expr(_)));
}

#[test]
fn parser_accepts_structured_pipeline_stages() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "fs.walk(\"src\") |> where { .kind == \"file\" } |> map { |file| file.path } |> collect()\n",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Expr(expr) = arena.stmt(root[0]).kind else {
        panic!("expected expression statement");
    };
    let ArenaExprKind::StructuredPipeline { stages, .. } = arena.expr(expr).kind else {
        panic!("expected structured pipeline");
    };
    let stages = arena.stream_stages(stages);
    assert_eq!(
        stages.iter().map(|stage| &stage.kind).collect::<Vec<_>>(),
        vec![
            &StreamStageKind::Where,
            &StreamStageKind::Map,
            &StreamStageKind::Collect
        ]
    );
    let block = arena.block(stages[1].block.unwrap());
    assert_eq!(arena.block_params(block.params)[0].name, "file");

    let nested = Parser::parse_source_arena_only(
        SourceId::new(0),
        "[1, 2, 3] |> batch(count: ([4, 5] |> batch(count: 1)).len(), max_argv: true)\n",
    );
    assert!(nested.diagnostics.is_empty(), "{:?}", nested.diagnostics);
    let narena = &nested.arena.arena;
    let nroot: Vec<_> = nested.arena.statement_ids().collect();
    let ArenaStmtKind::Expr(nexpr) = narena.stmt(nroot[0]).kind else {
        panic!("expected expression");
    };
    let ArenaExprKind::StructuredPipeline { stages, .. } = narena.expr(nexpr).kind else {
        panic!("expected pipeline");
    };
    let stage = &narena.stream_stages(stages)[0];
    assert_eq!(stage.kind, StreamStageKind::Batch);
    let args = narena.call_args(stage.args);
    assert_eq!(args.len(), 2);
    assert!(
        matches!(args[0].kind, xsh::frontend::syntax::arena::ArenaCallArgKind::Named { name, .. } if name == "count")
    );
    assert!(
        matches!(args[1].kind, xsh::frontend::syntax::arena::ArenaCallArgKind::Named { name, .. } if name == "max_argv")
    );

    let table = Parser::parse_source_arena_only(
        SourceId::new(0),
        "fs.children(\".\") |> sort-by { .size } |> table.print(columns: [\"name\", \"size\"])\n",
    );
    assert!(table.diagnostics.is_empty(), "{:?}", table.diagnostics);
    let tarena = &table.arena.arena;
    let troot: Vec<_> = table.arena.statement_ids().collect();
    let ArenaStmtKind::Expr(texpr) = tarena.stmt(troot[0]).kind else {
        panic!("expected expression statement");
    };
    let ArenaExprKind::StructuredPipeline { stages, .. } = tarena.expr(texpr).kind else {
        panic!("expected structured pipeline");
    };
    let stages = tarena.stream_stages(stages);
    assert_eq!(stages[0].kind, StreamStageKind::SortBy);
    assert_eq!(stages[1].kind, StreamStageKind::TablePrint);

    let adapters = Parser::parse_source_arena_only(
        SourceId::new(0),
        "\"a\\n\" |> text.lines() |> map { |line| line }\n\"a\\n\" |> text.lines() |> first()\nb\"abcd\" |> bytes.chunks(2)\n\"{\\\"name\\\":\\\"a\\\"}\\n\" |> json.lines()\n\"{\\\"name\\\":\\\"b\\\"}\\n\" |> json.stream()\n",
    );
    assert!(
        adapters.diagnostics.is_empty(),
        "{:?}",
        adapters.diagnostics
    );
}

#[test]
fn pipeline_value_calls_accept_plain_receivers_result_tails_and_named_blocks() {
    let source = r#"
let parts = "a,b" |> split(",")
let selected = [{value: "b"}] |> where { |entry| entry.value == "b" } |> first()?
let first = ["a", "b"] |> get(0)?
"#;
    assert_parse_and_check(SourceId::new(0), source);
}

#[test]
fn parser_and_desugar_accept_pipeline_call_shorthand() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "\
let warnings = file.read_bytes()?
|> bytes.utf8()?
|> text.lines()
|> where { \"warn\" in . }
let names = items |> map .path |> sort
",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let ArenaExprKind::StructuredPipeline { stages, .. } =
        arena.expr(root_let_init_expr(&output, 0)).kind
    else {
        panic!("expected structured pipeline after value-stage lowering");
    };
    let stages = arena.stream_stages(stages);
    assert_eq!(stages[0].kind, StreamStageKind::TextStreamLines);
    assert_eq!(stages[1].kind, StreamStageKind::Where);

    let ArenaExprKind::StructuredPipeline { stages, .. } =
        arena.expr(root_let_init_expr(&output, 1)).kind
    else {
        panic!("expected structured pipeline");
    };
    let stages = arena.stream_stages(stages);
    assert_eq!(stages[0].kind, StreamStageKind::Map);
    assert_eq!(stages[1].kind, StreamStageKind::Sort);
}

#[test]
fn parser_seals_structured_pipeline_before_wrapping_in_value_expr_stage() {
    // A structured (stream-only) pipeline that's then followed by a value-expr
    // stage must be sealed into its own node and used as the `input` of a new
    // mixed `Pipeline`, rather than the value-expr stage joining the
    // structured pipeline's own stage list directly.
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "let mixed = [1, 2, 3] |> map { |x| x } |> sort |> (1 + 1)\n",
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let ArenaExprKind::Pipeline { input, stages } = arena.expr(root_let_init_expr(&output, 0)).kind
    else {
        panic!("expected outer mixed pipeline");
    };
    let ArenaExprKind::StructuredPipeline {
        stages: inner_stages,
        ..
    } = arena.expr(input).kind
    else {
        panic!("expected structured pipeline sealed as the pipeline's input");
    };
    let inner_stages = arena.stream_stages(inner_stages);
    assert_eq!(inner_stages.len(), 2);
    assert_eq!(inner_stages[0].kind, StreamStageKind::Map);
    assert_eq!(inner_stages[1].kind, StreamStageKind::Sort);
    let stages = arena.pipe_stages(stages);
    assert_eq!(stages.len(), 1);
    assert!(matches!(stages[0].kind, ArenaPipeStageKind::Expr(_)));
}

#[test]
fn parser_accepts_stage_11_and_12_shapes() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        r#"
type Package = { name: Str, root: Path }
type PackageList = List[Package]

proc log(level: Str = "info", ...parts: List[Str]) -> Result[Unit] {
  var tries = 0
  while tries < 3 {
    tries = tries + 1
    if tries == 2 {
      continue
    }
    break
  }

  match Ok(level) {
    Ok(value) if value == "info" => print ${value},
    Err(_) => return Err(Error(kind: "log")),
    _ => print "other"
  }
}
"#,
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::TypeDef(tdef) = arena.stmt(root[0]).kind else {
        panic!("expected type definition");
    };
    assert!(matches!(
        arena.type_def(tdef).body,
        ArenaTypeDefBody::RecordSchema(_)
    ));
    let ArenaStmtKind::ProcDef(pdef) = arena.stmt(root[2]).kind else {
        panic!("expected proc definition");
    };
    let func = arena.function_def(pdef);
    let params = arena.params(func.params);
    assert!(params[0].default.is_some());
    assert!(params[1].rest);
    let body_ids: Vec<_> = arena.stmt_ids(arena.block(func.body).statements).collect();
    assert!(
        body_ids
            .iter()
            .any(|id| matches!(arena.stmt(*id).kind, ArenaStmtKind::While { .. }))
    );
    let match_id = body_ids
        .iter()
        .find(|id| matches!(arena.stmt(**id).kind, ArenaStmtKind::Match { .. }))
        .expect("expected match");
    let ArenaStmtKind::Match { arms, .. } = arena.stmt(*match_id).kind else {
        panic!("expected match");
    };
    let arms = arena.match_arms(arms);
    assert!(matches!(
        arena.pattern(arms[0].pattern).kind,
        ArenaPatternKind::Constructor { .. }
    ));
}

#[test]
fn parser_accepts_foundation_shapes() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        r###"
let mode = 0o755
let timeout = 30s
let message = f"mode {mode}"
defer run true ?
let command = process.command {
  cwd = Path("src")
  timeout = 2s
  cpu_max = 80
  run --timeout=1s --cpumax=80 make check
}
run.status --cpumax=80 echo ok
run.capture --text echo ok
let raw_lines = run.stream --text printf "%s\n" a b
let lines = raw_lines |> take(1)
match Err(Error(kind: "not-found")) {
  Err({ kind: "not-found" | "missing", .. }) => print "expected"
}
"###,
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    assert!(
        root.iter()
            .any(|id| matches!(arena.stmt(*id).kind, ArenaStmtKind::Defer(_)))
    );
    let block_id = root
        .iter()
        .find_map(|id| {
            let ArenaStmtKind::Let {
                initializer: ArenaExprOrRun::Expr(expr),
                ..
            } = arena.stmt(*id).kind
            else {
                return None;
            };
            let ArenaExprKind::BuilderCall { block, .. } = arena.expr(expr).kind else {
                return None;
            };
            Some(block)
        })
        .expect("expected builder call");
    let entries = arena.builder_entries(arena.builder_block(block_id).entries);
    assert!(matches!(
        entries[0].kind,
        ArenaBuilderEntryKind::Field { .. }
    ));
}

#[test]
fn parser_accepts_builder_task_and_generic_entry_shapes() {
    // `Task` and generic command-style `Entry` builder entries are parser-only
    // grammar (no accepting API's check currently exercises them), so this
    // checks parsing alone via Parser::parse_source_arena_only, not the full
    // assert_parse_and_check helper.
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "\
let exec = process.command {
  task build {
    run echo hi
  }
  some_entry arg1 arg2 {
    nested = 1
  }
}
",
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let ArenaExprKind::BuilderCall { block, .. } = arena.expr(root_let_init_expr(&output, 0)).kind
    else {
        panic!("expected builder call");
    };
    let entries = arena.builder_entries(arena.builder_block(block).entries);
    assert_eq!(entries.len(), 2);
    let ArenaBuilderEntryKind::Task {
        name,
        block: task_block,
    } = entries[0].kind
    else {
        panic!("expected task entry");
    };
    assert_eq!(name, "build");
    let task_stmts: Vec<_> = arena.stmt_ids(arena.block(task_block).statements).collect();
    assert_eq!(task_stmts.len(), 1);

    let ArenaBuilderEntryKind::Entry {
        name,
        args,
        block: nested_block,
    } = &entries[1].kind
    else {
        panic!("expected generic entry");
    };
    assert_eq!(*name, "some_entry");
    assert_eq!(arena.command_args(*args).len(), 2);
    let nested_block = nested_block.expect("entry has a nested builder block");
    let nested_entries = arena.builder_entries(arena.builder_block(nested_block).entries);
    assert_eq!(nested_entries.len(), 1);
    assert!(matches!(
        nested_entries[0].kind,
        ArenaBuilderEntryKind::Field { .. }
    ));
}

#[test]
fn parser_accepts_keyword_expressions_mid_expression() {
    // if/match/loop/retry/run/spawn/wait are full expression forms regardless
    // of nesting depth (parse_primary_arena_only handles them directly), so
    // they must parse correctly even when they're not the first token of the
    // overall expression being scanned for arena-only candidacy — a list
    // literal's later elements are the simplest way to put a keyword
    // construct at a non-zero offset within one candidate scan.
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "\
let h = spawn run true ?
let values = [1, if true { 2 } else { 3 }, match 4 { _ => 5 }, loop { break 6 }, retry [] { 7 }]
let commands = [1, run true ?, wait h?]
",
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;

    let ArenaExprKind::List(values) = arena.expr(root_let_init_expr(&output, 1)).kind else {
        panic!("expected list literal");
    };
    let values: Vec<_> = arena.list_element_exprs(values).collect();
    assert_eq!(values.len(), 5);
    assert!(matches!(
        arena.expr(values[1]).kind,
        ArenaExprKind::If { .. }
    ));
    assert!(matches!(
        arena.expr(values[2]).kind,
        ArenaExprKind::Match { .. }
    ));
    assert!(matches!(
        arena.expr(values[3]).kind,
        ArenaExprKind::Loop { .. }
    ));
    assert!(matches!(
        arena.expr(values[4]).kind,
        ArenaExprKind::Retry { .. }
    ));

    let ArenaExprKind::List(commands) = arena.expr(root_let_init_expr(&output, 2)).kind else {
        panic!("expected list literal");
    };
    let commands: Vec<_> = arena.list_element_exprs(commands).collect();
    assert_eq!(commands.len(), 3);
    assert!(matches!(
        arena.expr(commands[1]).kind,
        ArenaExprKind::Try(_)
    ));
    assert!(matches!(
        arena.expr(commands[2]).kind,
        ArenaExprKind::Try(_)
    ));
}

#[test]
fn parser_preserves_cpumax_and_timeout_run_segments() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "run.stream --bytes --cpumax=80 --timeout=1s echo ok\n",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
        panic!("expected run");
    };
    let segments = arena.run_segments(arena.run_form(*run_id).segments);
    let segment = &segments[0];
    assert_eq!(segment.kind, RunKind::StreamBytes);
    assert!(segment.cpu_max.is_some());
    assert!(segment.timeout.is_some());
}

#[test]
fn parser_accepts_stage_13_module_exports_and_aliases() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        r#"
use helper as h

export type Package = {name: Str, root: Path}

export let pkg = {name: "demo"}

export pure label(pkg: h.Package) -> Str {
  return pkg.name
}

export proc build(name: Str) -> Result[Unit] {
  h.greet(name)?
  return Ok()
}
"#,
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Use(use_id) = arena.stmt(root[0]).kind else {
        panic!("expected aliased use");
    };
    let use_stmt = arena.use_stmt(use_id);
    assert_eq!(
        arena
            .names(use_stmt.path)
            .map(|n| n.to_string())
            .collect::<Vec<_>>(),
        vec!["helper".to_string()]
    );
    assert_eq!(use_stmt.alias.map(|n| n.to_string()).as_deref(), Some("h"));
    assert!(matches!(arena.stmt(root[1]).kind, ArenaStmtKind::Export(_)));
    assert!(matches!(arena.stmt(root[2]).kind, ArenaStmtKind::Export(_)));
    let ArenaStmtKind::Export(inner) = arena.stmt(root[4]).kind else {
        panic!("expected exported proc");
    };
    let ArenaStmtKind::ProcDef(pdef) = arena.stmt(inner).kind else {
        panic!("expected proc definition");
    };
    let body_ids: Vec<_> = arena
        .stmt_ids(arena.block(arena.function_def(pdef).body).statements)
        .collect();
    let ArenaStmtKind::Expr(expr) = arena.stmt(body_ids[0]).kind else {
        panic!("expected dotted proc expression call");
    };
    assert!(matches!(arena.expr(expr).kind, ArenaExprKind::Try(_)));
}

#[test]
fn parser_accepts_hyphenated_module_names_in_use_declarations() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "use PKGBUILD-x86_64 as PKGBUILD_x86_64\nuse build-essential-native.proof as build_proof\n",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    assert_eq!(root.len(), 2);

    let ArenaStmtKind::Use(use0) = arena.stmt(root[0]).kind else {
        panic!("expected use");
    };
    let use0 = arena.use_stmt(use0);
    assert_eq!(
        arena
            .names(use0.path)
            .map(|n| n.to_string())
            .collect::<Vec<_>>(),
        vec!["PKGBUILD-x86_64".to_string()]
    );
    assert_eq!(
        use0.alias.map(|n| n.to_string()).as_deref(),
        Some("PKGBUILD_x86_64")
    );

    let ArenaStmtKind::Use(use1) = arena.stmt(root[1]).kind else {
        panic!("expected use");
    };
    let use1 = arena.use_stmt(use1);
    assert_eq!(
        arena
            .names(use1.path)
            .map(|n| n.to_string())
            .collect::<Vec<_>>(),
        vec!["build-essential-native".to_string(), "proof".to_string()]
    );
    assert_eq!(
        use1.alias.map(|n| n.to_string()).as_deref(),
        Some("build_proof")
    );
}

#[test]
fn parser_keeps_byte_pipeline_and_structured_pipeline_distinct() {
    let structured = Parser::parse_source_arena_only(SourceId::new(0), "[1] |> count()\n");
    let byte = Parser::parse_source_arena_only(SourceId::new(0), "run printf x | run cat\n");

    assert!(
        structured.diagnostics.is_empty(),
        "{:?}",
        structured.diagnostics
    );
    assert!(byte.diagnostics.is_empty(), "{:?}", byte.diagnostics);
    let sarena = &structured.arena.arena;
    let sroot: Vec<_> = structured.arena.statement_ids().collect();
    let ArenaStmtKind::Expr(sexpr) = sarena.stmt(sroot[0]).kind else {
        panic!("expected structured expression");
    };
    assert!(matches!(
        sarena.expr(sexpr).kind,
        ArenaExprKind::StructuredPipeline { .. }
    ));
    let barena = &byte.arena.arena;
    let broot: Vec<_> = byte.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = barena.stmt(broot[0]).kind else {
        panic!("expected byte pipeline command");
    };
    let ArenaCommand::Run(run_id) = &barena.command_stmt(cmd_id).command else {
        panic!("expected run");
    };
    assert_eq!(
        barena.run_segments(barena.run_form(*run_id).segments).len(),
        2
    );
}

#[test]
fn quoted_command_interpolation_spans_use_source_offsets() {
    let source = "run echo \"a${one}b${two}\"\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
        panic!("expected run command");
    };
    let segments = arena.run_segments(arena.run_form(*run_id).segments);
    let args = arena.command_args(segments[0].args);
    let ArenaCommandArgKind::Word(parts) = args[0].kind else {
        panic!("expected word");
    };
    let spans: Vec<_> = arena
        .word_parts(parts)
        .filter_map(|part| match part {
            ArenaWordPart::Interpolation(expr) => Some(arena.expr(expr).span),
            _ => None,
        })
        .collect();

    assert_eq!(spans[0].start(), source.find("one").unwrap());
    assert_eq!(spans[1].start(), source.find("two").unwrap());
}

#[test]
fn braced_command_interpolation_is_accepted() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "let pkg = {name: \"demo\"}\nprint ${pkg.name} ${pkg.name}-src\n",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[1]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Core { args, .. } = &arena.command_stmt(cmd_id).command else {
        panic!("expected print command");
    };
    let cargs = arena.command_args(*args);
    let ArenaCommandArgKind::Word(parts) = &cargs[0].kind else {
        panic!("expected word");
    };
    assert!(matches!(
        arena.word_parts(*parts).collect::<Vec<_>>().as_slice(),
        [ArenaWordPart::Interpolation(_)]
    ));
}

#[test]
fn nested_command_word_interpolation_is_accepted() {
    // The outer command word's `${...}` chunk is parsed via a throwaway
    // sub-lexer/parser writing into the same arena; that sub-parse reaches a
    // run-form whose own command arg is ANOTHER quoted string with its own
    // `${...}` interpolation, recursing into command_string_parts_arena_only
    // (and begin_word_parts/begin_command_args) again before the outer ones
    // finish. Exercises the word_part_inputs/command_arg_inputs staging fix.
    let source = "let name = \"world\"\nprint \"outer ${run echo \"inner ${name}\" ?}\"\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[1]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Core { args, .. } = &arena.command_stmt(cmd_id).command else {
        panic!("expected print command");
    };
    let cargs = arena.command_args(*args);
    assert_eq!(cargs.len(), 1);
    let ArenaCommandArgKind::Word(parts) = &cargs[0].kind else {
        panic!("expected word");
    };
    let parts: Vec<_> = arena.word_parts(*parts).collect();
    assert_eq!(parts.len(), 2);
    let ArenaWordPart::Quoted(prefix) = &parts[0] else {
        panic!("expected quoted prefix");
    };
    assert_eq!(arena.text_value(prefix, source), Some("outer "));
    assert!(matches!(parts[1], ArenaWordPart::Interpolation(_)));
}

#[test]
fn parser_accepts_ergonomic_sugar_pass_forms() {
    let source = r#"
fs.mkdir build ?
fs.remove dist --missing-ok ?
json.write out (metadata) ?
let {name, version, ..} = pkg
var {path, kind, ..} = entry
for {path, kind, ..} in entries {
  print $path "$kind"
}
let jobs = env.Str.JOBS ?? "1"
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd0) = arena.stmt(root[0]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Proc { name, .. } = &arena.command_stmt(cmd0).command else {
        panic!("expected module command surface");
    };
    assert_eq!(name.as_str(), "fs.mkdir");

    let ArenaStmtKind::Let { target, .. } = arena.stmt(root[3]).kind else {
        panic!("expected let destructuring");
    };
    assert!(matches!(
        arena.binding_target(target).kind,
        ArenaBindingTargetKind::Record { fields, rest }
            if arena.destructure_fields(fields).len() == 2 && rest
    ));

    let ArenaStmtKind::For { target, block, .. } = arena.stmt(root[5]).kind else {
        panic!("expected for destructuring");
    };
    assert!(matches!(
        arena.binding_target(target).kind,
        ArenaBindingTargetKind::Record { rest: true, .. }
    ));
    let block_ids: Vec<_> = arena.stmt_ids(arena.block(block).statements).collect();
    let ArenaStmtKind::Command(pcmd) = arena.stmt(block_ids[0]).kind else {
        panic!("expected print command");
    };
    let ArenaCommand::Core { args, .. } = &arena.command_stmt(pcmd).command else {
        panic!("expected print command");
    };
    for arg in arena.command_args(*args) {
        let ArenaCommandArgKind::Word(parts) = &arg.kind else {
            panic!("expected word");
        };
        assert!(
            arena
                .word_parts(*parts)
                .any(|part| matches!(part, ArenaWordPart::Shorthand(_)))
        );
    }

    assert!(matches!(
        arena.expr(root_let_init_expr(&output, 6)).kind,
        ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            ..
        }
    ));
}

#[test]
fn standalone_dollar_command_interpolation_is_accepted() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        "let pkg = {name: \"demo\"}\nprint $pkg.name\n",
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[1]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Core { args, .. } = &arena.command_stmt(cmd_id).command else {
        panic!("expected print command");
    };
    let cargs = arena.command_args(*args);
    let ArenaCommandArgKind::Word(parts) = &cargs[0].kind else {
        panic!("expected word");
    };
    assert!(matches!(
        arena.word_parts(*parts).collect::<Vec<_>>().as_slice(),
        [ArenaWordPart::Shorthand(_)]
    ));
}

#[test]
fn embedded_and_quoted_dollar_command_interpolation_is_accepted() {
    let source = "let pkg = {name: \"demo\"}\nrun echo prefix$pkg.name \"$pkg.name\" \"\\$pkg.name\" \"\\\\$pkg.name\"\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[1]).kind else {
        panic!("expected command");
    };
    let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
        panic!("expected run command");
    };
    let segments = arena.run_segments(arena.run_form(*run_id).segments);
    let args = arena.command_args(segments[0].args);
    for arg in &args[0..2] {
        let ArenaCommandArgKind::Word(parts) = &arg.kind else {
            panic!("expected word");
        };
        assert!(
            arena
                .word_parts(*parts)
                .any(|part| matches!(part, ArenaWordPart::Shorthand(_)))
        );
    }
    let ArenaCommandArgKind::Word(parts) = &args[2].kind else {
        panic!("expected word");
    };
    let parts2 = arena.word_parts(*parts).collect::<Vec<_>>();
    assert!(matches!(
        parts2.as_slice(),
        [ArenaWordPart::Quoted(text)] if arena.text_value(text, source) == Some("$pkg.name")
    ));

    let ArenaCommandArgKind::Word(parts) = &args[3].kind else {
        panic!("expected word");
    };
    let parts3 = arena.word_parts(*parts).collect::<Vec<_>>();
    assert!(matches!(
        parts3.as_slice(),
        [ArenaWordPart::Quoted(prefix), ArenaWordPart::Shorthand(_)] if arena.text_value(prefix, source) == Some("\\")
    ));
}

#[test]
fn parser_accepts_compact_sugar_forms() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        r#"
proc touch(path: Path) {
  path.write("ok")?
}
proc serve(port = 8080, debug = false, root = Path("www")) {
  print ${port} ${debug} ${root}
}
var count = 1
count += 2
let files = g"src/*.rs"
let label = if count > 1 { "many" } else { "one" }
let code = match Ok(2) { Ok(value) => value, Err(_) => 0 }
run.text printf "%s" ${label} ?
"#,
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::ProcDef(p0) = arena.stmt(root[0]).kind else {
        panic!("expected proc definition");
    };
    assert!(arena.function_def(p0).return_ty_defaulted);
    let ArenaStmtKind::ProcDef(p1) = arena.stmt(root[1]).kind else {
        panic!("expected proc definition");
    };
    let params = arena.params(arena.function_def(p1).params);
    assert!(params.iter().all(|param| param.ty_defaulted));
    assert!(
        params
            .iter()
            .all(|param| arena.type_expr_data[param.ty.index()].lhs
                == xsh::frontend::symbols::Name::UNKNOWN.symbol().raw())
    );
    let checked = Checker::check_arena(&output.arena, "");
    let types = params
        .iter()
        .map(|param| {
            checked
                .parameter_types
                .get(&arena.span(param.span))
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        types,
        vec![
            &xsh::frontend::check::Type::Int,
            &xsh::frontend::check::Type::Bool,
            &xsh::frontend::check::Type::Path
        ]
    );
    let ArenaStmtKind::Assign { op, .. } = arena.stmt(root[3]).kind else {
        panic!("expected assignment");
    };
    assert_eq!(op, AssignOp::Add);
    assert!(matches!(
        arena.expr(root_let_init_expr(&output, 4)).kind,
        ArenaExprKind::GlobStr(_)
    ));
    assert!(matches!(
        arena.expr(root_let_init_expr(&output, 5)).kind,
        ArenaExprKind::If { .. }
    ));
    assert!(matches!(
        arena.expr(root_let_init_expr(&output, 6)).kind,
        ArenaExprKind::Match { .. }
    ));
}

#[test]
fn parser_accepts_assignment_targets() {
    let output = Parser::parse_source_arena_only(
        SourceId::new(0),
        r#"
var stats = {code: 0, comments: 0}
stats.code += 1
stats["comments"] = 2
"#,
    );

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Assign { target, .. } = arena.stmt(root[1]).kind else {
        panic!("expected field assignment");
    };
    assert!(matches!(
        arena.assign_target(target).kind,
        ArenaAssignTargetKind::Field { .. }
    ));
    let ArenaStmtKind::Assign { target, .. } = arena.stmt(root[2]).kind else {
        panic!("expected index assignment");
    };
    assert!(matches!(
        arena.assign_target(target).kind,
        ArenaAssignTargetKind::Index { .. }
    ));
}

#[test]
fn parser_formatter_golden_covers_current_surface_syntax() {
    let source = r#"
proc write_note(path: Path) {
path.write("ok")?
}
var count=1
count+=2
var stats={code:0,comments:0}
stats.code+=1
stats["comments"]=2
let label=f"count {count}"
let tool=fp"{Path("root")}/bin/tool"
let text=run.text printf "%s" ${label} ?
let bytes=run.bytes printf "%s" raw ?
let files=g"src/*.rs"
run printf "%s\n" @g"src/*.rs" ?
let choice=if count>1{"many"}else{"one"}
let value=match Ok(count){Ok(n)=>n,Err(_)=>0}
p"tmp".remove(missing_ok:true)?
let slash=p"/tmp/xsh"
let multiline="""alpha
beta"""
let quoted_multiline="alpha\n\"\"\"\nbeta"
"#;
    let expected = r#"proc write_note(path: Path) {
  path.write("ok")?
}

var count = 1
count += 2
var stats = {code: 0, comments: 0}
stats.code += 1
stats["comments"] = 2
let label = f"count {count}"
let tool = fp"{Path("root")}/bin/tool"
let text = run.text printf "%s" ${label} ?
let bytes = run.bytes printf "%s" raw ?
let files = g"src/*.rs"
run printf "%s\n" @g"src/*.rs" ?
let choice = if count > 1 { "many" } else { "one" }
let value = match Ok(count) { Ok(n) => n, Err(_) => 0 }
p"tmp".remove(missing_ok: true)?
let slash = /tmp/xsh
let multiline = """alpha
beta"""
let quoted_multiline = "alpha\n\"\"\"\nbeta"
"#;

    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);

    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted.formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
}

#[test]
fn parser_and_formatter_preserve_spawn_wait_forms() {
    let source = r#"
let h=spawn run --cpumax=80 true ?
let s=wait h?
let hs=[spawn run true ?,spawn run false ?]
let statuses=wait hs?
let cmd=process.command {
cpu_max=80
run true
}
let h2=spawn (cmd)?
h2.cancel(signal:"TERM",kill_after:0ms)?
"#;
    let expected = r#"let h = spawn run --cpumax=80 true ?
let s = wait h?
let hs = [spawn run true?, spawn run false?]
let statuses = wait hs?
let cmd = process.command {
  cpu_max = 80
  run true
}
let h2 = spawn cmd?
h2.cancel(signal: "TERM", kill_after: 0ms)?
"#;

    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let ArenaExprKind::Try(inner) = arena.expr(root_let_init_expr(&parsed, 0)).kind else {
        panic!("expected spawn result propagation");
    };
    let ArenaExprKind::Spawn(form) = arena.expr(inner).kind else {
        panic!("expected spawn form");
    };
    assert!(matches!(form.target, ArenaSpawnTarget::Run(_)));

    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted.formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
}

#[test]
fn parser_and_formatter_preserve_retry_blocks() {
    let source = r#"
let value=retry [1s,2s,0ms] {
  fetch()?
}?
let once=retry [] {
  Ok("done")
}
"#;
    let expected = r#"let value = retry [1s, 2s, 0ms] {
  fetch()?
}?
let once = retry [] {
  Ok("done")
}
"#;

    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let ArenaExprKind::Try(inner) = arena.expr(root_let_init_expr(&parsed, 0)).kind else {
        panic!("expected retry result propagation");
    };
    let ArenaExprKind::Retry { delays, block, .. } = arena.expr(inner).kind else {
        panic!("expected retry expression");
    };
    assert_eq!(arena.expr_ids(delays).count(), 3);
    assert_eq!(arena.stmt_ids(arena.block(block).statements).count(), 1);

    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn parser_and_formatter_preserve_require_type_syntax() {
    let source = r#"
type Config = {name: Str, ports: List[Int], note: Str?}
let cfg=json.read(path)?.require(Config)?
let names=json.decode("[]")?.require(List[Str])?
"#;
    let expected = r#"type Config = {name: Str, ports: List[Int], note: Str?}

let cfg = json.read(path)?.require(Config)?
let names = json.decode("[]")?.require(List[Str])?
"#;

    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);

    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted.formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
}

#[test]
fn parser_treats_old_schema_helper_name_as_plain_call() {
    let old_name = ["vali", "date"].concat();
    let source = format!(
        "type Row = {{name: Str}}\nlet raw = {{name: \"demo\"}}\nlet row = {old_name}(raw, Row)?\n"
    );
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let ArenaExprKind::Try(inner) = arena.expr(root_let_init_expr(&parsed, 2)).kind else {
        panic!("expected try expression");
    };
    let ArenaExprKind::Call { callee, .. } = arena.expr(inner).kind else {
        panic!("expected ordinary call");
    };
    assert!(
        matches!(arena.expr(callee).kind, ArenaExprKind::Ident(name) if name.as_str() == old_name.as_str())
    );
}

#[test]
fn formatter_escapes_literal_dollar_interpolation_markers() {
    let source = r#"
let plain = r"${name}"
let label = f"${{name}}:{name}"
run echo "\$name" "\${name}"
"#;
    let expected = r#"let plain = r"${name}"
let label = f"${{name}}:{name}"
run echo "\$name" "\${name}"
"#;

    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);

    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted.formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
}

#[test]
fn formatter_preserves_ergonomic_sugar_pass_forms() {
    let source = "fs.mkdir build?\nfs.remove dist --missing-ok?\nlet {name,version,..}=pkg\nlet jobs=env.Str.JOBS??\"1\"\nprint $pkg.name \"$pkg.name\"\n";
    let expected = "fs.mkdir build ?\nfs.remove dist --missing-ok ?\nlet {name, version, ..} = pkg\nlet jobs = env.Str.JOBS ?? \"1\"\nprint $pkg.name $pkg.name\n";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_uses_two_space_pipeline_continuation_indent() {
    let source = "\
let names = fs.walk(root)
|> where .kind == \"file\"
|> map .name
";
    let expected = "\
let names = fs.walk(root)
  |> where .kind == \"file\"
  |> map .name
";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_separates_multiline_pipeline_statements() {
    let source = "\
let names = fs.walk(root)
|> where .kind == \"file\"
let sizes = fs.walk(root)
|> where .kind == \"file\"
|> map .size
print ${names[0]} ${sizes[0]}
";
    let expected = "\
let names = fs.walk(root) |> where .kind == \"file\"
let sizes = fs.walk(root)
  |> where .kind == \"file\"
  |> map .size
print ${names[0]} ${sizes[0]}
";

    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);

    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_preserves_intentional_top_level_blank_lines() {
    let source = "\
let label = \"one\"

let lines = label.lines()

print ${lines |> count()}
";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn formatter_indents_nested_blocks_under_pipeline_stages() {
    let source = "\
let doubled = [1, 2, 3]
|> par-map { |value|
value*2
}
";
    let expected = "\
let doubled = [1, 2, 3]
  |> par-map { |value|
    value * 2
  }
";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_wraps_long_constructs_at_default_width() {
    let source = r#"
type Package = {dir: Path, exports: Record, name: Str, ver: Str, rel: Str, deps: List[Str], mkdeps: List[Str], sources: List[Path], checksums: List[Str], nostrip: Bool}
let paths = [fp"{root}/bin", fp"{root}/dev", fp"{root}/etc/rc.d", fp"{root}/proc", fp"{root}/root", fp"{root}/run", fp"{root}/sys", fp"{root}/tmp", fp"{root}/usr/lib/services"]
let metadata = {name: pkg.name, version: pkg.ver, release: pkg.rel, tarball: tarball.display(), manifest_count: manifest.len(), checksum: checksums[source_index], installed_root: root.display(), work_dir: work.display()}
let command = process.command_argv(service_target, argv_prefix.extend([proof_log.display(), "heartbeat"]), cwd: root, timeout: 5s, detach: true, new_session: true, ignore_hup: true)
proc main(root: Path = Path("target/xsh-rootfs"), xsh_bin: Path = Path("target/debug/xsh"), auth_bin_dir: Path = Path("target/debug")) -> Result[Unit] {
return Ok()
}
"#;
    let expected = "\
type Package = {
  dir: Path,
  exports: Record,
  name: Str,
  ver: Str,
  rel: Str,
  deps: List[Str],
  mkdeps: List[Str],
  sources: List[Path],
  checksums: List[Str],
  nostrip: Bool,
}

let paths = [
  fp\"{root}/bin\",
  fp\"{root}/dev\",
  fp\"{root}/etc/rc.d\",
  fp\"{root}/proc\",
  fp\"{root}/root\",
  fp\"{root}/run\",
  fp\"{root}/sys\",
  fp\"{root}/tmp\",
  fp\"{root}/usr/lib/services\",
]
let metadata = {
  name: pkg.name,
  version: pkg.ver,
  release: pkg.rel,
  tarball: tarball.display(),
  manifest_count: manifest.len(),
  checksum: checksums[source_index],
  installed_root: root.display(),
  work_dir: work.display(),
}
let command = process.command_argv(
  service_target,
  argv_prefix.extend([proof_log.display(), \"heartbeat\"]),
  cwd: root,
  timeout: 5s,
  detach: true,
  new_session: true,
  ignore_hup: true,
)

proc main(
  root: Path = Path(\"target/xsh-rootfs\"),
  xsh_bin: Path = Path(\"target/debug/xsh\"),
  auth_bin_dir: Path = Path(\"target/debug\"),
) -> Result[Unit] {
  return Ok()
}
";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
    assert!(
        formatted
            .formatted
            .lines()
            .filter(|line| !line.starts_with("  auth_bin_dir:"))
            .all(|line| line.chars().count() <= 120)
    );
}

#[test]
fn formatter_preserves_readable_multiline_package_shapes() {
    let source = "export type CMultiTarget = {
  tasks: List[MakeTask],
  groups: Map[CompileTasks],
  outputs: Map[Path],
  deps: List[Str],
}
let target = make.c_program({
  cc,
  triple,
  cflags,
  defs,
  includes,
  root: p\".\",
  sources,
  out_dir: p\"obj\",
  out: p\"obj/tool\",
  libs: [],
  ldflags: [],
  deps: [],
})
let value = path_value.display().replace(\"/\", \"_\").replace(\".cxx\", ext).replace(\".cpp\", ext).replace(\".cc\", ext).replace(\".c\", ext).replace(\".S\", ext).replace(\".s\", ext)
let script = r\"\"\"print f\"{value}\"
\"\"\"
";
    let expected = "export type CMultiTarget = {
  tasks: List[MakeTask],
  groups: Map[CompileTasks],
  outputs: Map[Path],
  deps: List[Str],
}

let target = make.c_program({
  cc,
  triple,
  cflags,
  defs,
  includes,
  root: p\".\",
  sources,
  out_dir: p\"obj\",
  out: p\"obj/tool\",
  libs: [],
  ldflags: [],
  deps: [],
})
let value = path_value.display()
  .replace(\"/\", \"_\")
  .replace(\".cxx\", ext)
  .replace(\".cpp\", ext)
  .replace(\".cc\", ext)
  .replace(\".c\", ext)
  .replace(\".S\", ext)
  .replace(\".s\", ext)
let script = r\"\"\"print f\"{value}\"
\"\"\"
";

    let first = Formatter::new().format_source(SourceId::new(0), source);

    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);

    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_indents_single_multiline_record_call_args_in_nested_contexts() {
    let source = "pure make_row() -> Result[Record] {
  return Ok({
    name: \"demo\",
    enabled: true,
  })
}

pure push_row(rows: List[Record]) -> List[Record] {
  return rows.push({
    name: \"demo\",
    enabled: true,
  })
}
";
    let expected = source;

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_keeps_expanded_single_record_call_stable() {
    let source = "proc example() {\n  return RepoPlan({repo: resolve_repo_root(parsed.repo)?, all: parsed.all, roots: parsed.roots, target: parsed.target, output: parsed.output})\n}\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(first.formatted.contains("return RepoPlan({\n"));
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_preserves_indented_multiline_format_strings() {
    let source = "proc write_line(text: Str, name: Str) [fs, error] {
  text.write_atomic(f\"\"\"hello {name}
\"\"\")?
}

proc write_path(text: Str, name: Str) [fs, error] {
  text.write_atomic(fp\"\"\"hello {name}
\"\"\")?
}
";
    let expected = source;

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_keeps_single_multiline_literal_call_compact() {
    let source = "tool.write(\"\"\"demo\n\"\"\")?\n";
    let expected = "tool.write(\"\"\"demo\n\"\"\")?\n";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_canonicalizes_proc_effect_order() {
    let source = "proc main() [io, error, fs, env, process, net, time] {\n  return Ok()\n}\n";
    let expected = "proc main() [fs, net, process, env, time, error, io] {\n  return Ok()\n}\n";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_skips_next_statement_after_fmt_skip_comment() {
    let source = "\
let before = 1
# fmt: skip
let value=1+2
let after = 3
";
    let expected = "\
let before = 1
# fmt: skip
let value=1+2
let after = 3
";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_keeps_trailing_comment_on_skipped_statement() {
    let source = "# fmt: skip\nlet value=1+2 # keep with the skipped statement\n\nlet after=3\n";
    let expected =
        "# fmt: skip\nlet value=1+2 # keep with the skipped statement\n\nlet after = 3\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_keeps_authored_block_gap_after_skipped_trailing_comment() {
    let source = "proc main() {\n  # fmt: skip\n  let value=1+2 # keep with the skipped statement\n\n  let after=3\n}\n";
    let expected = "proc main() {\n  # fmt: skip\n  let value=1+2 # keep with the skipped statement\n\n  let after = 3\n}\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_wraps_long_if_and_match_expressions_in_safe_contexts() {
    let source = "\
let choice = if user_name == \"administrator\" and mode == \"production\" { \"allow\" } else { \"deny\" }
let label = render(match result { Ok(value) => value, Err(_) => \"fallback\" })
";
    let expected = "\
let choice = if user_name == \"administrator\" and mode == \"production\" {
  \"allow\"
} else {
  \"deny\"
}
let label = render(
  match result {
    Ok(value) => value,
    Err(_) => \"fallback\",
  },
)
";

    let first = Formatter::new()
        .with_line_width(60)
        .format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);

    let second = Formatter::new()
        .with_line_width(60)
        .format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_preserves_multiline_match_expressions() {
    let source = "let shebang = match fs.read_text(script) {\n  Ok(text_value) => (text_value.split(\"\\n\").get(0) ?? \"\")\n  Err(_) => \"\"\n}\n";
    let expected = "let shebang = match fs.read_text(script) {\n  Ok(text_value) => text_value.split(\"\\n\").get(0) ?? \"\",\n  Err(_) => \"\",\n}\n";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_preserves_multiline_call_argument_lists() {
    let source = "let command = make_command(\n  target,\n  args,\n  cwd: root,\n)\n";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn formatter_indents_broken_call_arguments_in_nested_blocks() {
    let source = "proc main() {\nlet value=make(\ntarget,\n{\nalpha:1,beta:2,gamma:3,delta:4,epsilon:5,zeta:6},\n)\n}\n";
    let expected = "proc main() {\n  let value = make(\n    target,\n    {\n      alpha: 1,\n      beta: 2,\n      gamma: 3,\n      delta: 4,\n      epsilon: 5,\n      zeta: 6,\n    },\n  )\n}\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_preserves_multiline_comprehensions() {
    let source = "let upstream_sources = [{\n  source: source.source.display(),\n  kind: source.kind,\n  architectures: source.architectures,\n  checksums: source.checksums,\n} for source in pkg.upstream_sources]\nlet by_name = {\n  item.name: item.version\n  for item in items\n}\n";
    let expected = "let upstream_sources = [\n  {\n    source: source.source.display(),\n    kind: source.kind,\n    architectures: source.architectures,\n    checksums: source.checksums,\n  }\n  for source in pkg.upstream_sources\n]\nlet by_name = {\n  item.name: item.version\n  for item in items\n}\n";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
}

#[test]
fn formatter_breaks_long_call_chains_between_calls() {
    let source = "let files = common.push({path: p\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\", kind: \"binary\"}).push({path: p\"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\", kind: \"binary\"}).push({path: p\"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc\", kind: \"binary\"})\n";

    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert!(
        formatted.formatted.contains("\n  .push"),
        "{}",
        formatted.formatted
    );

    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted.formatted);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.arena.statement_ids().count(), 1);
    assert!(
        formatted.formatted.contains("common.push("),
        "{}",
        formatted.formatted
    );

    let second = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(second.formatted, formatted.formatted);
}

#[test]
fn parser_rejects_stale_surface_syntax() {
    let cases = [
        ("let label = fmt\"hello\"\n", None),
        ("let files = glob\"*.rs\"\n", None),
        ("let ok = not ready\n", None),
    ];

    for (source, code) in cases {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(!parsed.diagnostics.is_empty(), "{source}");
        if let Some(code) = code {
            assert!(
                parsed
                    .diagnostics
                    .iter()
                    .any(|diag| diag.code.map(DiagnosticCode::name) == Some(code)),
                "{source}: {:?}",
                parsed.diagnostics
            );
        }
    }
}

#[test]
fn parser_accepts_run_capture_records() {
    let source = "let text = run.capture --text printf \"%s\" hi\nlet bytes = run.capture --bytes printf \"%s\" hi\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);

    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn bare_command_fixture_is_proc_command() {
    let output = Parser::parse_source_arena_only(SourceId::new(0), "make -j4\n");

    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let root: Vec<_> = output.arena.statement_ids().collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(root[0]).kind else {
        panic!("expected command");
    };
    assert!(matches!(
        arena.command_stmt(cmd_id).command,
        ArenaCommand::Proc { .. }
    ));
}

#[test]
fn proc_without_signature_fixture_is_rejected() {
    let source = "proc build {\n  print \"bad\"\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);

    assert!(
        output
            .diagnostics
            .iter()
            .any(|diag| diag.code.map(DiagnosticCode::name) == Some("parse.required-signature"))
    );
}

#[test]
fn reserved_keywords_and_proc_identifiers_are_not_expression_names() {
    let keyword = Parser::parse_source_arena_only(SourceId::new(0), "let if = 1\n");
    let proc_ident = Parser::parse_source_arena_only(SourceId::new(0), "let build-all = 1\n");

    assert!(
        keyword
            .diagnostics
            .iter()
            .any(|diag| diag.code.map(DiagnosticCode::name) == Some("parse.expected-ident"))
    );
    assert!(
        proc_ident
            .diagnostics
            .iter()
            .any(|diag| diag.code.map(DiagnosticCode::name) == Some("parse.expected-ident"))
    );
}

#[test]
fn formatter_fixture_covers_comments_commands_blocks_and_records() {
    let source = include_str!("fixtures/syntax/valid/formatting.xsh");
    let expected = "\
# formatter fixture
use fs

proc main(args: List[Str]) {
  # nested comment
  let config = {name: \"demo\", enabled: true}
  let values = [\"one\", \"two\"]

  if true {
    run echo \"hello \"${args[0]} ?
  } else {
    eprint \"no\"
  }
}

main(args)?
";

    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);

    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_preserves_trailing_statement_comments() {
    let source = "\
let value=1 # keep this with the binding
let after=3
";
    let expected = "\
let value = 1 # keep this with the binding
let after = 3
";

    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);

    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_preserves_nested_comment_blocks_without_duplicate_comments() {
    let source = "\
proc main() {
  let before=1
  if true {
    # explain command shape
    run echo \"ok\" ?
  }
  let after=2
}
print \"done\"
";
    let expected = "\
proc main() {
  let before = 1
  if true {
    # explain command shape
    run echo \"ok\" ?
  }

  let after = 2
}

print \"done\"
";

    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);
    assert_eq!(
        first.formatted.matches("# explain command shape").count(),
        1
    );

    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn parser_formatter_roundtrip_property_over_baseline_snippets() {
    let snippets = [
        "let value = 1 + 2 * 3\n",
        "let tmp_path = fp\"{Path(\"tmp\")}/space name\"\n",
        "let raw_bytes = b\"a\\xff\\n\"\n",
        "run printf \"%s\\n\" \"hello world\" ?\n",
        "run make -j${cpu.count()} ?\n",
        "proc main(args: List[Str]) -> Result[Unit] {\n  return Ok()\n}\n\nmain(args)?\n",
        "pure trim_one(value: Str) -> Str {\n  return value.trim()\n}\n",
        "if true {\n  print \"yes\"\n} else {\n  eprint \"no\"\n}\n",
        "while false {\n  break\n}\n",
        "for value in [\"a\", \"b\"] {\n  print ${value}\n}\n",
        "match Ok(\"x\") {\n  Ok(value) => print ${value}\n  _ => print \"other\"\n}\n",
        "type Package = {name: Str, root: Path}\nlet pkg: Package = {name: \"demo\", root: Path(\"src\")}\n",
        "use types as t\nlet pkg: t.Package = {name: \"demo\", root: Path(\"src\")}\n",
        "let record = {name: \"demo\", enabled: true}\n",
        "let files = fs.walk(\"src\")\n|> where {\n  .kind == \"file\"\n}\n\n",
        "let out = [1, 2] |> par-map { |x|\n  x * 2\n}\n",
    ];

    for (index, snippet) in snippets.iter().enumerate() {
        let source_id = SourceId::new(index);
        let parsed = Parser::parse_source_arena_only(source_id, snippet);
        assert!(
            parsed.diagnostics.is_empty(),
            "{snippet}: {:?}",
            parsed.diagnostics
        );

        let first = Formatter::new().format_source(source_id, snippet);
        assert!(
            first.diagnostics.is_empty(),
            "{snippet}: {:?}",
            first.diagnostics
        );

        let reparsed = Parser::parse_source_arena_only(source_id, &first.formatted);
        assert!(
            reparsed.diagnostics.is_empty(),
            "{}: {:?}",
            first.formatted,
            reparsed.diagnostics
        );

        let second = Formatter::new().format_source(source_id, &first.formatted);
        assert_eq!(second.formatted, first.formatted);
    }
}

#[test]
fn formatter_is_idempotent_on_example_catalog() {
    use xsh::frontend::source::SourceId;
    use xsht::examples::load_catalog;

    let catalog = load_catalog(".").expect("load examples/catalog.json");
    for case in catalog.examples {
        let source =
            std::fs::read_to_string(&case.path).unwrap_or_else(|_| panic!("read {}", case.path));

        let source_id = SourceId::new(0);
        let first = Formatter::new().format_source(source_id, &source);
        assert!(
            first.diagnostics.is_empty(),
            "{}: formatter produced diagnostics on original source: {:?}",
            case.path,
            first.diagnostics
        );

        let reparsed = Parser::parse_source_arena_only(source_id, &first.formatted);
        assert!(
            reparsed.diagnostics.is_empty(),
            "{}: formatted output has parse errors:\n---\n{}\n---\n{:?}",
            case.path,
            first.formatted,
            reparsed.diagnostics
        );
        assert_parse_and_check(source_id, &first.formatted);

        let second = Formatter::new().format_source(source_id, &first.formatted);
        assert_eq!(
            second.formatted, first.formatted,
            "{}: formatter is not idempotent (running fmt twice gives different output)",
            case.path,
        );
    }
}

#[test]
fn formatter_pretty_corpus_has_stable_golden_shape() {
    let source = include_str!("fixtures/syntax/valid/pretty.xsh");
    let formatted = Formatter::new()
        .with_line_width(60)
        .format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    let expected = "# curated formatter corpus
let source = p\".\"
let items = [
  {name: \"one\", enabled: true},
  {name: \"two\", enabled: false},
]
let source_shaped = [
  1,
  2,
]
let rows = [
  {
    name: \"short\",
  },
  {
    name: \"a deliberately long record value that forces its sibling to break too\",
  },
]
let nested = [
  {
    meta: {
      name: \"short\",
    },
  },
  {
    meta: {
      name: \"another deliberately long nested record value\",
    },
  },
]
let filtered = [item.name for item in items if item.enabled]
let by_name = {
  item.name: f\"{item.name}\"
  for item in items
  if item.enabled
}
let chain = source.display()
  .replace(\"/\", \"_\")
  .replace(\"-\", \"_\")
# fmt: skip
let skipped=1+2
";
    assert_eq!(formatted.formatted, expected);
    assert_parse_and_check(SourceId::new(0), &formatted.formatted);
    let second = Formatter::new()
        .with_line_width(60)
        .format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(second.formatted, formatted.formatted);
}

#[test]
fn formatter_beauty_corpus_matches_golden_and_remains_idempotent() {
    let source = include_str!("fixtures/fmt/beauty.xsh");
    let expected = include_str!("fixtures/fmt/beauty.expected.xsh");
    let source_id = SourceId::new(0);
    let first = Formatter::new().format_source(source_id, source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, expected);
    assert_parse_and_check(source_id, &first.formatted);
    let second = Formatter::new().format_source(source_id, &first.formatted);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn formatter_is_idempotent_on_laputa_corpus() {
    fn files(root: &std::path::Path, output: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            let derived = path.file_name().is_some_and(|name| {
                matches!(
                    name.to_str(),
                    Some(".git" | ".out" | ".cache" | "target" | "node_modules")
                )
            });
            if path.is_dir() && !derived {
                files(&path, output);
            } else if path.extension().is_some_and(|extension| extension == "xsh") {
                output.push(path);
            }
        }
    }

    // The Laputa monorepo is the largest XSH corpus outside this repository.
    let root = std::env::var_os("XSH_LAPUTA_CORPUS").map_or_else(
        || std::path::PathBuf::from("../laputa"),
        std::path::PathBuf::from,
    );
    let root = root.as_path();
    if !root.is_dir() {
        return;
    }
    let mut paths = Vec::new();
    files(root, &mut paths);
    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        let first = Formatter::new().format_source(SourceId::new(0), &source);
        assert!(
            first.diagnostics.is_empty(),
            "{}: {:?}",
            path.display(),
            first.diagnostics
        );
        let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
        assert_eq!(second.formatted, first.formatted, "{}", path.display());
    }
}

// ── expression continuation across newlines ──

#[test]
fn parser_continues_binary_op_with_leading_operator_on_next_line() {
    let source = "let x = 1\n+ 2\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        matches!(arena.stmt(id).kind, ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(e), .. }
            if matches!(arena.expr(e).kind, ArenaExprKind::Binary { op: BinaryOp::Add, .. }))
    }));
}

/// A line break never silently joins two statements: no spelling that
/// continues an expression onto the next line can also begin a statement.
/// Both sets come from running the parser over a spelling of every token
/// kind and must equal the grammar's continuation set.
#[test]
fn line_continuation_tokens_cannot_begin_a_statement() {
    use xsh::frontend::syntax::lexer::representative_token_texts;
    let mut candidates: Vec<String> = representative_token_texts()
        .into_iter()
        .filter(|(tag, _)| !matches!(tag, TokenTag::Newline | TokenTag::Comment))
        .map(|(_, text)| text)
        .collect();
    // Two-token operator and the contextual pattern-test word.
    candidates.extend(["not in".to_string(), "is".to_string()]);
    let starts_at_zero = |diagnostic: &xsh::diagnostic::Diagnostic| {
        diagnostic
            .span
            .or(diagnostic.labels.first().map(|label| label.span))
            .is_some_and(|span| span.start() == 0)
    };
    let mut continuing = Vec::new();
    let mut overlap = Vec::new();
    for text in &candidates {
        let source = format!("f()\n{text} x\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        let first_end = parsed
            .arena
            .statement_ids()
            .next()
            .map(|id| parsed.arena.arena.stmt(id).span.end());
        if !first_end.is_some_and(|end| end > "f()\n".len()) {
            continue;
        }
        continuing.push(text.clone());
        let source = format!("{text} x\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        if !parsed.diagnostics.iter().any(starts_at_zero) {
            overlap.push(text.clone());
        }
    }
    continuing.sort();
    let mut grammar: Vec<String> = xsh::frontend::syntax::grammar::line_continuation_spellings()
        .into_iter()
        .map(str::to_string)
        .collect();
    grammar.sort();
    assert_eq!(
        continuing, grammar,
        "the parser's continuation set differs from the grammar's operator table"
    );
    // An item expression `.name` cannot begin a line after an expression; the
    // line is always postfix.
    assert_eq!(
        overlap,
        ["."],
        "continuation tokens that also begin a statement"
    );
}

/// The printer joins adjacent tokens with `lexer::join_tokens`, which adds a
/// space only where the lexer would merge them. Every ordered pair of token
/// spellings, covering every token kind, lexes back to the same tokens.
#[test]
fn joined_token_pairs_lex_back_to_the_same_tokens() {
    use xsh::frontend::syntax::lexer::{join_tokens, lex_spellings, representative_token_texts};
    let representatives = representative_token_texts();
    for tag in TokenTag::ALL
        .into_iter()
        .filter(|tag| *tag != TokenTag::Eof)
    {
        assert!(
            representatives.iter().any(|(kind, _)| *kind == tag),
            "no spelling for {tag:?}"
        );
    }
    for (tag, text) in &representatives {
        assert_eq!(lex_spellings(text), [(*tag, text.as_str())], "{text:?}");
    }
    // A comment runs to the end of the line, so nothing is joined after one.
    let texts: Vec<&str> = representatives
        .iter()
        .filter(|(tag, _)| !matches!(tag, TokenTag::Comment | TokenTag::Newline))
        .map(|(_, text)| text.as_str())
        .collect();
    let mut spaced = 0;
    for left in &texts {
        for right in &texts {
            let joined = join_tokens(left, right);
            let mut expected = lex_spellings(left);
            expected.extend(lex_spellings(right));
            assert_eq!(
                lex_spellings(&joined),
                expected,
                "{left:?} then {right:?} joined as {joined:?}"
            );
            spaced += usize::from(joined.len() > left.len() + right.len());
        }
    }
    assert_eq!(texts.len(), 148);
    assert_eq!(spaced, 11078, "pairs that need a space");
}

#[test]
fn line_starting_with_a_statement_token_starts_a_new_statement() {
    for line in ["-1", "/tmp/x", "./x", "is_ok(1)"] {
        let source = format!("let value = 1\n{line}\n");
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{line}: {:?}",
            parsed.diagnostics
        );
        assert_eq!(parsed.arena.statement_ids().count(), 2, "{line}");
    }
}

#[test]
fn parser_continues_binary_op_with_trailing_operator_on_previous_line() {
    let source = "let x = 1 +\n2\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        matches!(arena.stmt(id).kind, ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(e), .. }
            if matches!(arena.expr(e).kind, ArenaExprKind::Binary { op: BinaryOp::Add, .. }))
    }));
}

#[test]
fn parser_continues_chained_comparisons_across_newlines() {
    let source = "let ok = x > 0\nand x < 10\nand y != 0\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn parser_breaks_expression_when_newline_not_followed_by_operator() {
    let source = "let x = 1\nlet y = 2\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    assert_eq!(output.arena.statement_ids().count(), 2);
}

#[test]
fn parser_allows_multiline_parenthesized_expression() {
    let source = "let x = (1 +\n2)\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn parser_allows_multiline_list() {
    let source = "let xs = [\n1,\n2,\n3\n]\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn parser_allows_multiline_record() {
    let source = "let r = {\na: 1,\nb: 2,\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn parser_allows_multiline_tag_union() {
    let source = "enum T {\n  A,\n B,\n C(Int),\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        matches!(arena.stmt(id).kind, ArenaStmtKind::TypeDef(def)
            if matches!(arena.type_def(def).body, ArenaTypeDefBody::TagUnion(ref variants) if arena.tag_variants(*variants).len() == 3))
    }));
}

#[test]
fn parser_allows_multiline_tag_union_with_paren_variants() {
    let source = "enum Tok {\n  TNum(Float),\n TStr(Str),\n TOp(Str),\n TEOF,\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        matches!(arena.stmt(id).kind, ArenaStmtKind::TypeDef(def)
            if matches!(arena.type_def(def).body, ArenaTypeDefBody::TagUnion(ref variants) if arena.tag_variants(*variants).len() == 4))
    }));
}

// ── string concatenation operator ──

#[test]
fn parser_accepts_string_concatenation_operator() {
    let source = r#"let x = "a" + "b"
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    assert!(output.arena.statement_ids().any(|id| {
        matches!(arena.stmt(id).kind, ArenaStmtKind::Let { initializer: ArenaExprOrRun::Expr(e), .. }
            if matches!(arena.expr(e).kind, ArenaExprKind::Binary { op: BinaryOp::Add, .. }))
    }));
}

#[test]
fn parser_accepts_chained_string_concatenation() {
    let source = r#"let x = "a" + "b" + "c"
"#;
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
}

#[test]
fn formatter_preserves_string_concatenation() {
    let source = r#"let x = "a" + b + "c"
"#;
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}

#[test]
fn formatter_keeps_path_method_receivers_quoted() {
    let source = "\
proc patch_path() [fs, error] -> Result[Path] {
  if p\"../pkg/files/x86-jump-label-patch.c\".exists()? {
    return p\"../pkg/files/x86-jump-label-patch.c\"
  }
  return p\"../pkg/files/default.c\"
}
";
    let expected = "\
proc patch_path() [fs, error] -> Result[Path] {
  if p\"../pkg/files/x86-jump-label-patch.c\".exists()? {
    return ../pkg/files/x86-jump-label-patch.c
  }

  return ../pkg/files/default.c
}
";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
    assert_parse_and_check(SourceId::new(0), &formatted.formatted);
}

#[test]
fn parser_accepts_call_and_index_chains_in_command_args() {
    let print_chain = "proc main() {\n  let c = {stderr: \"err\\n\"}\n  print c.stderr.trim()\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), print_chain);
    assert!(
        output.diagnostics.is_empty(),
        "print with method chain should parse cleanly: {:?}",
        output.diagnostics
    );

    let interp = "proc main() {\n  let c = {stderr: \"err\\n\"}\n  print ${c.stderr.trim()}\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), interp);
    assert!(
        output.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        output.diagnostics
    );

    let other_cmd = "proc main() {\n  let x = \"hi\"\n  run.status x.trim()\n}\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), other_cmd);
    assert!(
        output.diagnostics.is_empty(),
        "run command with method chain should parse cleanly: {:?}",
        output.diagnostics
    );

    let bare = "print (\"x\")\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), bare);
    assert!(
        !output
            .diagnostics
            .iter()
            .any(|d| d.code.map(DiagnosticCode::name) == Some("parse.command-call-expr")),
        "bare name() should not trigger parse.command-call-expr: {:?}",
        output.diagnostics
    );
}

#[test]
fn formatter_keeps_command_call_args_bare_without_stealing_propagate_flag() {
    let source = "\
pure basename_value(name: Str, suffix: Str) -> Str {
  name + suffix
}

pure xsh_bin() -> Path {
  Path(\"target/debug/xsh\")
}

proc main(input: Path, candidate: Path, tarball: Path, name: Str, suffix: Str) {
  let maybe_name: Result[Str] = Ok(name)
  print (basename_value(name, suffix))
  print (maybe_name?)
  run.text (xsh_bin()) date.xsh -- (input.display()) ?
  run.text (xsh_bin()) tar.xsh -- -cf (tarball.display()) -C (input.display()) . ?
  let output = run.text (xsh_bin()) backup.xsh -- --dir (input.display()) --keep 2 --dry-run=false ?
  let _ = run.capture --text \"readelf\" -d (candidate.display())?
}
";
    let expected = "\
pure basename_value(name: Str, suffix: Str) -> Str {
  name + suffix
}

pure xsh_bin() -> Path {
  Path(\"target/debug/xsh\")
}

proc main(input: Path, candidate: Path, tarball: Path, name: Str, suffix: Str) {
  let maybe_name: Result[Str] = Ok(name)
  print basename_value(name, suffix)
  print (maybe_name?)
  run.text xsh_bin() date.xsh -- input.display() ?
  run.text xsh_bin() tar.xsh -- -cf tarball.display() -C input.display() . ?
  let output = run.text xsh_bin() backup.xsh -- --dir input.display() --keep 2 --dry-run=false ?
  let _ = run.capture --text \"readelf\" -d candidate.display() ?
}
";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);

    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted.formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
    assert_parse_and_check(SourceId::new(0), &formatted.formatted);
    let arena = &reparsed.arena.arena;
    let root: Vec<_> = reparsed.arena.statement_ids().collect();
    let ArenaStmtKind::ProcDef(pdef) = arena.stmt(root[2]).kind else {
        panic!("expected proc");
    };
    let body_ids: Vec<_> = arena
        .stmt_ids(arena.block(arena.function_def(pdef).body).statements)
        .collect();
    let ArenaStmtKind::Command(cmd_id) = arena.stmt(body_ids[3]).kind else {
        panic!("expected run.text command");
    };
    let ArenaCommand::Run(run_id) = &arena.command_stmt(cmd_id).command else {
        panic!("expected run form");
    };
    assert!(arena.run_form(*run_id).propagate);
}

#[test]
fn formatter_round_trips_value_branch_blocks_and_record_arms() {
    let source = "let choice = if true { let detail = 2; detail } else { 3 }\nlet record = match choice { 2 => {}, _ => {\"run\": 4} }\nlet text = match choice { 2 => { let detail = \"é\"; detail }, _ => \"other\" }\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(!first.formatted.contains("{ {"));
    assert!(first.formatted.contains("let detail = 2"));
    assert!(first.formatted.contains("\"run\": 4"));
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(first.formatted, second.formatted);
}

#[test]
fn parser_and_formatter_preserve_bare_block_literal_distinctions() {
    let source = "let value = 7\nlet shorthand = {value}\nlet empty = {}\nlet named = {if: 1}\nlet computed = {[\"key\"]: 2}\nlet grouped = { (value) }\nlet result = { let next = 8; next }\nlet negative = { false }\nlet accessed = { shorthand.value }\n";
    assert_parse_and_check(SourceId::new(0), source);
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(first.formatted.contains("(value)"));
    assert_parse_and_check(SourceId::new(0), &first.formatted);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(first.formatted, second.formatted);
    for malformed in ["let bad = {field:}\n", "let bad = {[\"key\"]:}\n"] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), malformed);
        assert!(!parsed.diagnostics.is_empty());
    }
}

#[test]
fn parser_named_argument_puns_keep_identifier_spans_and_formatting() {
    use xsh::frontend::syntax::arena::ArenaCallArgKind;

    let source = include_str!("fixtures/syntax/valid/named-argument-puns.xsh");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_parse_and_check(SourceId::new(0), source);
    let arena = &parsed.arena.arena;
    let mut puns = 0;
    for index in 0..arena.expr_tags.len() {
        let expression = arena.expr(ExprId::from_index(index));
        let ArenaExprKind::Call { args, .. } = expression.kind else {
            continue;
        };
        for argument in arena.call_args(args) {
            let ArenaCallArgKind::Named { name, value, span } = argument.kind else {
                continue;
            };
            let value = arena.expr(value);
            let span = arena.span(span);
            if value.span.start() == span.start() {
                puns += 1;
                assert!(
                    matches!(value.kind, ArenaExprKind::Ident(identifier) if identifier == name)
                );
                assert_eq!(&source[value.span.range()], name.as_str().as_str());
                assert_eq!(&source[span.range()], format!("{name}:"));
            }
        }
    }
    assert_eq!(puns, 10);
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, source);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn checker_named_argument_pun_missing_name_labels_original_identifier() {
    let source = include_str!("fixtures/sema/named-argument-pun-missing.xsh");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    let diagnostic = checked
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("check.unresolved-name")
        })
        .expect("the pun must resolve an ordinary lexical name");
    assert_eq!(diagnostic.labels.len(), 1);
    assert_eq!(&source[diagnostic.labels[0].span.range()], "value");
    assert_eq!(
        diagnostic.labels[0].span.start(),
        source.rfind("value:").unwrap(),
    );
}

#[test]
fn formatter_preserves_half_open_slicing_bounds_and_unicode() {
    let source = "\
let data = b\"abcd\"
let all = data[..]
let prefix = data[..2]
let suffix = data[2..]
let middle = data[-3..-1]
let unicode = \"é🦀\"[..1][..]
print ${all.base64()} ${prefix.base64()} ${suffix.base64()} ${middle.base64()} $unicode
";
    let source_id = SourceId::new(0);
    assert_parse_and_check(source_id, source);
    let formatted = Formatter::new().format_source(source_id, source);
    assert!(formatted.diagnostics.is_empty());
    assert_eq!(formatted.formatted, source);
    assert_parse_and_check(source_id, &formatted.formatted);
    assert_eq!(
        Formatter::new()
            .format_source(source_id, &formatted.formatted)
            .formatted,
        source
    );
}

#[test]
fn parser_rejects_colon_inclusive_and_stride_slices() {
    for source in [
        "let part = b\"abcd\"[0:2]\n",
        "let part = b\"abcd\"[..=2]\n",
        "let part = b\"abcd\"[0..2..1]\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            !parsed.diagnostics.is_empty(),
            "unexpectedly accepted {source}"
        );
    }
}

#[test]
fn parser_retains_nested_renamed_record_binding_targets_and_spans() {
    let source = "let {root, build: {jobs, target: target_name, ..}, ..} = config\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let ArenaStmtKind::Let { target, .. } = arena
        .stmt(parsed.arena.statement_ids().next().unwrap())
        .kind
    else {
        panic!("expected binding");
    };
    let ArenaBindingTargetKind::Record { fields, rest: true } = arena.binding_target(target).kind
    else {
        panic!("expected record");
    };
    let outer = arena.destructure_fields(fields);
    assert_eq!(outer.len(), 2);
    let ArenaBindingTargetKind::Record { fields, rest: true } =
        arena.binding_target(outer[1].target).kind
    else {
        panic!("expected nested record");
    };
    let renamed = &arena.destructure_fields(fields)[1];
    assert_eq!(
        &source[arena.span(renamed.span).range()],
        "target: target_name"
    );
    assert!(
        matches!(arena.binding_target(renamed.target).kind, ArenaBindingTargetKind::Name(name) if name.as_str() == "target_name")
    );
}

#[test]
fn guarded_value_controls_round_trip_without_absorbing_guard_into_run_argv() {
    let source = r#"proc cached(value: Str?) [] -> Str {
  return value when value != null
  return "missing"
}
proc command(selected: Bool) [process] -> Status {
  return (run.status /usr/bin/true) unless !selected
  return run.status /usr/bin/true when unless
}
stream items() [] -> Stream[Int] {
  yield 1 when true
  yield 2 unless false
}
let value = loop {
  break 4 when false
  break 5 unless false
}
"#;
    let source_id = SourceId::new(0);
    assert_parse_and_check(source_id, source);
    let formatted = Formatter::new().format_source(source_id, source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert!(
        formatted
            .formatted
            .contains("return (run.status /usr/bin/true) unless ! selected"),
        "{}",
        formatted.formatted
    );
    assert!(
        formatted
            .formatted
            .contains("return run.status /usr/bin/true when unless")
    );
    assert_parse_and_check(source_id, &formatted.formatted);
    let second = Formatter::new().format_source(source_id, &formatted.formatted);
    assert_eq!(formatted.formatted, second.formatted);
}

#[test]
fn guarded_value_control_keeps_payload_and_condition_source_spans() {
    let source = "return \"é\" when false\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let root = parsed.arena.statement_ids().next().unwrap();
    let arena = &parsed.arena.arena;
    let ArenaStmtKind::GuardedStmt {
        stmt,
        condition,
        negate,
    } = arena.stmt(root).kind
    else {
        panic!("expected guarded return");
    };
    assert!(!negate);
    assert_eq!(&source[arena.stmt(stmt).span.range()], "return \"é\"");
    assert_eq!(&source[arena.expr(condition).span.range()], "false");
}

#[test]
fn grouped_run_payload_preserves_adjacent_propagation_before_guard() {
    let source = r#"proc capture(selected: Bool) [process, error] -> Str {
  return (run.text /usr/bin/printf "selected")? when selected
  return "fallback"
}
"#;
    let source_id = SourceId::new(0);
    assert_parse_and_check(source_id, source);
    let formatted = Formatter::new().format_source(source_id, source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_parse_and_check(source_id, &formatted.formatted);
    let second = Formatter::new().format_source(source_id, &formatted.formatted);
    assert_eq!(formatted.formatted, second.formatted);
}

#[test]
fn parser_multi_clause_comprehensions_share_qualifiers_and_source_spans() {
    use xsh::frontend::syntax::arena::ArenaCompQualifier;
    let source = "let list = [inner for outer in [1] if outer > 0 for inner in [outer] if inner < 2]\nlet map = {entry.key: inner for entry in entries if entry.ok for inner in entry.values}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let qualifiers = parsed
        .arena
        .statement_ids()
        .map(|stmt| {
            let ArenaStmtKind::Let {
                initializer: ArenaExprOrRun::Expr(expr),
                ..
            } = arena.stmt(stmt).kind
            else {
                panic!("expected let")
            };
            let range = match arena.expr(expr).kind {
                ArenaExprKind::ListComp { qualifiers, .. }
                | ArenaExprKind::MapComp { qualifiers, .. } => qualifiers,
                _ => panic!("expected comprehension"),
            };
            arena.comp_qualifiers(range)
        })
        .collect::<Vec<_>>();
    assert_eq!(qualifiers[0].len(), 4);
    assert_eq!(qualifiers[1].len(), 3);
    assert!(matches!(qualifiers[0][0], ArenaCompQualifier::For { .. }));
    assert!(matches!(qualifiers[0][1], ArenaCompQualifier::If { .. }));
    assert!(matches!(qualifiers[0][2], ArenaCompQualifier::For { .. }));
    for sequence in qualifiers {
        for qualifier in sequence {
            assert!(
                source[qualifier.span().range()].starts_with(match qualifier {
                    ArenaCompQualifier::For { .. } => "for ",
                    ArenaCompQualifier::If { .. } => "if ",
                })
            );
        }
    }
}

#[test]
fn formatter_multi_clause_comprehensions_are_readable_and_idempotent() {
    let source = "let values = [inner for outer in [1] if outer > 0 for inner in [outer] if inner < 2]\nlet by_key = {entry.key: inner for entry in entries if entry.ok for inner in entry.values}\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(
        first.formatted.contains(
            "\n  for outer in [1]\n  if outer > 0\n  for inner in [outer]\n  if inner < 2\n"
        ),
        "{}",
        first.formatted
    );
    assert!(
        first
            .formatted
            .contains("\n  for entry in entries\n  if entry.ok\n  for inner in entry.values\n"),
        "{}",
        first.formatted
    );
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(first.formatted, second.formatted);
}

#[test]
fn formatter_multi_clause_comprehensions_retain_unicode_comments() {
    let source = "let values = [\n  # sélection\n  inner\n  for outer in [1]\n  # répétition\n  for inner in [outer]\n  # filtre\n  if inner > 0\n]\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_eq!(first.formatted, source);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(second.formatted, first.formatted);
}

#[test]
fn guarded_postfix_records_index_and_slice_flags_and_byte_spans() {
    let source =
        "let text: Str? = \"αβ\"\nlet a = text?[0]\nlet b = text?[..2]\nlet c = text[1..]\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let statements: Vec<_> = parsed
        .arena
        .arena
        .stmt_ids(parsed.arena.statements)
        .collect();
    for (stmt, guarded, spelling) in [
        (statements[1], true, "text?[0]"),
        (statements[2], true, "text?[..2]"),
        (statements[3], false, "text[1..]"),
    ] {
        let xsh::frontend::syntax::arena::ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        } = parsed.arena.arena.stmt(stmt).kind
        else {
            panic!("binding");
        };
        let expression = parsed.arena.arena.expr(expr);
        assert_eq!(
            &source[expression.span.start()..expression.span.end()],
            spelling
        );
        match expression.kind {
            ArenaExprKind::Index {
                guarded: actual, ..
            }
            | ArenaExprKind::Slice {
                guarded: actual, ..
            } => assert_eq!(actual, guarded),
            _ => panic!("guarded postfix"),
        }
    }
}

#[test]
fn parser_requires_grouping_between_ordering_and_pattern_tests() {
    for source in [
        "let result = 0 < 1 < 2 is Bool\n",
        "let result = value is Str < true\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("parse.mixed-comparison")),
            "{source}: {:?}",
            parsed.diagnostics
        );
    }
    for source in [
        "let result = (0 < 1 < 2) is Bool\n",
        "let result = (value is Str) < true\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{source}: {:?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn parser_keeps_type_pattern_match_arms_after_unbraced_values() {
    let source = "pure describe(failure: Error) -> Str {\n  match failure {\n    is PermissionDenied => return \"permission_denied\"\n    is NotFound => return \"not_found\"\n    _ => return \"other\"\n  }\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}

#[test]
fn list_literal_splices_retain_element_and_splice_spans() {
    let source = "let values = [1, @more, @[2],]\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let ArenaExprKind::List(items) = arena.expr(root_let_init_expr(&parsed, 0)).kind else {
        panic!("list");
    };
    let items: Vec<_> = arena.list_elements(items).collect();
    assert_eq!(items.len(), 3);
    assert!(items[0].splice_span.is_none());
    assert_eq!(
        &source[arena.span(items[1].splice_span.unwrap()).range()],
        "@more"
    );
    assert_eq!(
        &source[arena.span(items[2].splice_span.unwrap()).range()],
        "@[2]"
    );
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty());
    assert!(formatted.formatted.contains("@more"));
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        formatted.formatted
    );
}

#[test]
fn deferred_block_parses_and_formats_as_statement_body() {
    let source_id = SourceId::new(0);
    let source = "proc cleanup() [error] {\n  defer {\n    # café remains inside cleanup\n    let message = \"done\"\n    print $message\n    assert true\n  }\n}\n\ncleanup()\n";
    let parsed = Parser::parse_source_arena_only(source_id, source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_parse_and_check(source_id, source);
    let formatted = Formatter::new().format_source(source_id, source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
    let again = Formatter::new().format_source(source_id, &formatted.formatted);
    assert_eq!(again.formatted, source);
}

#[test]
fn regex_literals_parse_as_prepared_regex_atoms_with_raw_source_spans() {
    let source = "let rx = \"ordinary identifier\"\nlet single = rx\"^\\d+\\$\\{literal\\}$\"\nlet multiline = rx\"\"\"(?x)\n  ^ [a-z]+ # raw pattern comment\n  $\n\"\"\"\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(7), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    for index in [1, 2] {
        let expression = parsed.arena.arena.expr(root_let_init_expr(&parsed, index));
        let ArenaExprKind::Regex(id) = expression.kind else {
            panic!("expected regex literal");
        };
        let literal = parsed.arena.arena.regex_literal(id);
        assert_eq!(expression.span, literal.span);
        assert_eq!(literal.span.source_id, SourceId::new(7));
        assert_eq!(&source[literal.span.range()], literal.source_text.as_ref());
    }
    assert_eq!(
        parsed.arena.arena.regex_literals[0].pattern.as_ref(),
        r"^\d+\$\{literal\}$"
    );
    let formatted = Formatter::new().format_source(SourceId::new(7), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_parse_and_check(SourceId::new(7), &formatted.formatted);
    assert_eq!(
        formatted.formatted,
        Formatter::new()
            .format_source(SourceId::new(7), &formatted.formatted)
            .formatted
    );
}

#[test]
fn regex_literal_unterminated_delimiters_are_lexical_errors() {
    for source in ["let pattern = rx\"abc", "let pattern = rx\"\"\"abc\n"] {
        let lexed = Lexer::new(SourceId::new(0), source).lex_compact();
        assert!(
            lexed
                .diagnostics
                .iter()
                .any(|d| d.code.map(DiagnosticCode::name) == Some("lex.unterminated-string")),
            "{:?}",
            lexed.diagnostics
        );
    }
}

#[test]
fn parser_yield_delegation_retains_source_expression_and_unicode_byte_span() {
    let source = "stream rows() [] -> Stream[Str] {\n  yield \"α\"\n  yield @[\"β\", \"γ\"]\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let stmt = parsed
        .arena
        .arena
        .stmt_ids(parsed.arena.statements)
        .next()
        .unwrap();
    let ArenaStmtKind::StreamDef(function) = parsed.arena.arena.stmt(stmt).kind else {
        panic!("stream definition");
    };
    let block = parsed.arena.arena.function_def(function).body;
    let statements: Vec<_> = parsed
        .arena
        .arena
        .stmt_ids(parsed.arena.arena.block(block).statements)
        .collect();
    assert!(matches!(
        parsed.arena.arena.stmt(statements[0]).kind,
        ArenaStmtKind::Yield(_)
    ));
    let ArenaStmtKind::YieldDelegate(expr) = parsed.arena.arena.stmt(statements[1]).kind else {
        panic!("delegation");
    };
    assert_eq!(
        &source[parsed.arena.arena.expr(expr).span.range()],
        "[\"β\", \"γ\"]"
    );
    assert!(matches!(
        parsed.arena.arena.expr(expr).kind,
        ArenaExprKind::List(_)
    ));
}

#[test]
fn error_fallback_blocks_round_trip_without_record_ambiguity() {
    let source = "let recovered = Ok(false) ?? { |failure|\n  let _ = failure\n  false\n}\nlet record = Ok({name: \"original\"}) ?? {name: \"record\"}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let ArenaExprKind::Binary {
        op: BinaryOp::ResultFallback,
        right,
        ..
    } = parsed.arena.arena.expr(root_let_init_expr(&parsed, 0)).kind
    else {
        panic!("fallback");
    };
    let ArenaExprKind::ValueBlock(block) = parsed.arena.arena.expr(right).kind else {
        panic!("error block");
    };
    assert_eq!(
        parsed
            .arena
            .arena
            .block_params(parsed.arena.arena.block(block).params)
            .len(),
        1
    );
    let ArenaExprKind::Binary { right, .. } =
        parsed.arena.arena.expr(root_let_init_expr(&parsed, 1)).kind
    else {
        panic!("fallback");
    };
    assert!(matches!(
        parsed.arena.arena.expr(right).kind,
        ArenaExprKind::Record(_)
    ));
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        source
    );
}

#[test]
fn parser_record_defaults_preserve_spans_and_stable_formatting() {
    let source = "let names = [\"é\"]\ntype Config = {name: Str = \"é\", names: List[Str] = names, nested: Map[Int] = {}}\nlet config = Config()\nprint $config.name\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let ArenaStmtKind::TypeDef(definition) = parsed
        .arena
        .arena
        .stmt(parsed.arena.statement_ids().nth(1).unwrap())
        .kind
    else {
        panic!("schema");
    };
    let ArenaTypeDefBody::RecordSchema(fields) = parsed.arena.arena.type_def(definition).body
    else {
        panic!("record schema");
    };
    let defaults: Vec<_> = parsed
        .arena
        .arena
        .schema_fields(fields)
        .iter()
        .map(|field| {
            let span = parsed.arena.arena.expr(field.default.unwrap()).span;
            &source[span.range()]
        })
        .collect();
    assert_eq!(defaults, ["\"é\"", "names", "{}"]);
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(first.formatted, second.formatted);
    assert_parse_and_check(SourceId::new(0), &first.formatted);
}

#[test]
fn field_label_braces_spans_and_formatter_preserve_keyword_and_dotted_keys() {
    let source = "type Entry = {type: Str, in: Int}\nlet value = Entry(type: \"file\", in: 2)\nlet selected = match true { _ => {type: value.type, in: value.in, r\"wire.type\": 3} }\nlet {type: kind, in: ordinal, ..} = value\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(9), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.cst.get().exact_text(), source);
    let formatted = Formatter::new().format_source(SourceId::new(9), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert!(
        formatted
            .formatted
            .contains("{type: value.type, in: value.in, \"wire.type\": 3}"),
        "{}",
        formatted.formatted
    );
    assert_parse_and_check(SourceId::new(9), &formatted.formatted);
    assert_eq!(
        formatted.formatted,
        Formatter::new()
            .format_source(SourceId::new(9), &formatted.formatted)
            .formatted
    );
}

#[test]
fn field_label_keywords_cannot_be_shorthand_puns_or_lexical_bindings() {
    for source in [
        "let row = {type}\n",
        "let {type} = {type: 1}\n",
        "type Entry = {type: Int}\nlet entry = Entry(type:)\n",
        "let row = {type: 1}\nlet selected = match row { {type} => 1, _ => 2 }\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.code.map(DiagnosticCode::name) == Some("parse.keyword-label-binding")),
            "{source}: {:?}",
            parsed.diagnostics
        );
    }
    for source in [
        "let type = 1\n",
        "pure value(match: Int) -> Int { 1 }\n",
        "use fs as type\n",
    ] {
        assert!(
            !Parser::parse_source_arena_only(SourceId::new(0), source)
                .diagnostics
                .is_empty(),
            "{source}"
        );
    }
}

#[test]
fn computed_map_keys_retain_expression_nodes_and_original_spans() {
    use xsh::frontend::syntax::arena::ArenaRecordFieldKind;
    let source = "let values = {[key.trim()]: amount, \"literal.dot\": 2, ...more,}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let ArenaExprKind::Record(fields) = arena.expr(root_let_init_expr(&parsed, 0)).kind else {
        panic!("brace literal");
    };
    let ArenaRecordFieldKind::Computed { key, value, span } = arena.record_fields(fields)[0].kind
    else {
        panic!("computed entry");
    };
    assert_eq!(&source[arena.expr(key).span.range()], "key.trim()");
    assert_eq!(&source[arena.expr(value).span.range()], "amount");
    assert_eq!(&source[arena.span(span).range()], "[key.trim()]: amount");
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(formatted.diagnostics.is_empty());
    assert!(formatted.formatted.contains("[key.trim()]: amount"));
    assert!(formatted.formatted.contains("\"literal.dot\""));
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        formatted.formatted
    );
}

#[test]
fn parser_nested_record_update_paths_keep_labels_spans_and_comments() {
    let source = "let base = {build: {jobs: 1, if: false}}\nlet after = {\n  ...base,\n  build.jobs: 2, # é worker count\n  build.if: true,\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let statement = parsed.arena.statement_ids().nth(1).unwrap();
    let ArenaStmtKind::Let {
        initializer: ArenaExprOrRun::Expr(value),
        ..
    } = arena.stmt(statement).kind
    else {
        panic!("binding");
    };
    let ArenaExprKind::Record(fields) = arena.expr(value).kind else {
        panic!("update");
    };
    let ArenaRecordFieldKind::Path { path, value, span } = arena.record_fields(fields)[1].kind
    else {
        panic!("path");
    };
    assert_eq!(
        arena
            .names(path)
            .map(|name| name.to_string())
            .collect::<Vec<_>>(),
        ["build", "jobs"]
    );
    assert_eq!(&source[arena.span(span).range()], "build.jobs: 2");
    assert_eq!(&source[arena.expr(value).span.range()], "2");
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(first.formatted.contains("# é worker count"));
    assert_parse_and_check(SourceId::new(0), &first.formatted);
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(first.formatted, second.formatted);
}

#[test]
fn error_handler_headers_have_one_authoritative_parameter_range() {
    let source = include_str!("fixtures/frontend-indexed/block-parameters.xsh");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let mut handlers = 0;
    for raw in 0..parsed.arena.arena.stmt_tags.len() {
        let statement = parsed.arena.arena.stmt(StmtId::from_index(raw));
        let block = match statement.kind {
            ArenaStmtKind::With { else_block, .. } | ArenaStmtKind::Guard { else_block, .. } => {
                else_block
            }
            _ => continue,
        };
        let params = parsed
            .arena
            .arena
            .block_params(parsed.arena.arena.block(block).params);
        assert_eq!(params.len(), 1);
        let parameter = parsed.arena.arena.span(params[0].span);
        assert_eq!(
            &source[parameter.start()..parameter.end()],
            if handlers == 0 { "_" } else { "failure" }
        );
        handlers += 1;
    }
    assert_eq!(handlers, 2);
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_parse_and_check(SourceId::new(0), &formatted.formatted);
    assert!(!formatted.formatted.contains("else |"));
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        formatted.formatted
    );
}

#[test]
fn try_capture_parser_formatter_preserves_value_body_and_result_tail() {
    let source = "let value = try {\n  let nested = Ok(7)\n  nested\n}\nlet empty = try {}\nlet fields = {try: 7}\nrun printf try\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert!(
        parsed
            .arena
            .arena
            .expr_tags
            .iter()
            .any(|tag| matches!(tag, xsh::frontend::syntax::arena::ArenaExprTag::Capture))
    );
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    let second = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(second.formatted, formatted.formatted);
    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted.formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
}

#[test]
fn parser_value_pipeline_holes_retain_immediate_call_shape_and_formatting() {
    let source = "pure render(prefix: Str, value: Str) -> Str { prefix + value }\nlet rendered = \"é\" |> render(\"[\", value: _)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let value = root_let_init_expr(&parsed, 1);
    let ArenaExprKind::ValuePipelineCall { input, call, hole } =
        parsed.arena.arena.expr(value).kind
    else {
        panic!("explicit pipeline call");
    };
    assert_eq!(
        &source[parsed.arena.arena.expr(input).span.range()],
        "\"é\""
    );
    assert_eq!(&source[parsed.arena.arena.expr(hole).span.range()], "_");
    assert!(matches!(
        parsed.arena.arena.expr(call).kind,
        ArenaExprKind::Call { .. }
    ));
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert!(
        first.formatted.contains("|> render(\"[\", value: _)"),
        "{}",
        first.formatted
    );
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(first.formatted, second.formatted);
    assert_parse_and_check(SourceId::new(0), &first.formatted);
}

#[test]
fn parser_value_pipeline_holes_reject_nested_multiple_and_spread_arguments() {
    for source in [
        "1 |> render(_, _)\n",
        "1 |> render(_ + 1)\n",
        "1 |> render(nested(_))\n",
        "1 |> render(@_)\n",
        "1 |> render(if true { _ } else { 0 })\n",
    ] {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.map(DiagnosticCode::name)
                    == Some("parse.pipeline-hole")),
            "{source}: {:?}",
            parsed.diagnostics
        );
    }
}

#[test]
fn stream_stage_flags_are_fatal_migration_diagnostics_with_exact_fixes() {
    let source = "# café\nlet values = [1] |> par-map --jobs=workers { |item| item } # retain\nlet groups = values |> reduce-by --sum --jobs=2 { |item| {key: \"all\", value: item} }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert_eq!(parsed.diagnostics.len(), 2, "{:?}", parsed.diagnostics);
    let mut fixed = source.to_string();
    for diagnostic in parsed.diagnostics.iter().rev() {
        assert_eq!(
            diagnostic.code.map(DiagnosticCode::name),
            Some("parse.stream-option-migration")
        );
        let hint = diagnostic
            .fix_hints
            .first()
            .expect("unambiguous stage flag fix");
        fixed.replace_range(
            hint.span.unwrap().range(),
            hint.replacement.as_deref().unwrap(),
        );
    }
    assert!(fixed.contains("par-map (jobs: workers) { |item| item } # retain"));
    assert!(fixed.contains("reduce-by (sum: true, jobs: 2)"));
    assert!(fixed.starts_with("# café\n"));
    let second = Parser::parse_source_arena_only(SourceId::new(0), &fixed);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
}

#[test]
fn stream_stage_flag_migration_refuses_ambiguous_argument_lists() {
    let source = "let values = [1] |> sort-by --desc (.size)\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    let migration = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("parse.stream-option-migration")
        })
        .expect("stage migration diagnostic");
    assert!(migration.fix_hints.is_empty());
    let command =
        Parser::parse_source_arena_only(SourceId::new(0), "run printf --jobs --desc --max-bytes\n");
    assert!(command.diagnostics.is_empty(), "{:?}", command.diagnostics);
}

#[test]
fn formatter_keeps_named_stream_configuration_and_spreads_idempotent() {
    let source = "let jobs = 2\nlet values = [1, 2] |> par-map(jobs:) { |item| item + 1 } |> sort(...{desc: true})\nlet batches = values |> batch(count: 2, max_argv: false)\nprint batches.len()\n";
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty(), "{:?}", first.diagnostics);
    assert_parse_and_check(SourceId::new(0), &first.formatted);
    assert!(first.formatted.contains("par-map(jobs:)"));
    assert!(first.formatted.contains("sort(...{desc: true})"));
    let second = Formatter::new().format_source(SourceId::new(0), &first.formatted);
    assert_eq!(first.formatted, second.formatted);
}

#[test]
fn parser_enum_migration_preserves_comments_exports_and_aliases() {
    let source = "## Nominal café.\nexport type Choice =\n  Selected(Int) # first variant\n  | Empty # second variant\ntype Alias = Choice\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert_eq!(parsed.diagnostics.len(), 1, "{:?}", parsed.diagnostics);
    let migration = &parsed.diagnostics[0];
    assert_eq!(
        migration.code.map(DiagnosticCode::name),
        Some("parse.enum-migration")
    );
    let mut edits: Vec<_> = migration
        .fix_hints
        .iter()
        .map(|hint| {
            (
                hint.span.expect("migration edit span"),
                hint.replacement.as_ref().expect("migration replacement"),
            )
        })
        .collect();
    edits.sort_by_key(|(span, _)| span.start());
    let mut fixed = source.to_string();
    for (span, replacement) in edits.into_iter().rev() {
        assert!(!parsed.cst.get().contains_comment(span));
        fixed.replace_range(span.range(), replacement);
    }
    assert!(fixed.contains("export enum Choice {"), "{fixed}");
    assert!(fixed.contains("# first variant"));
    assert!(fixed.contains("# second variant"));
    assert!(fixed.contains("type Alias = Choice"));
    assert!(fixed.contains("café"));
    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &fixed);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
}

#[test]
fn parser_enum_singleton_is_nominal_and_identifier_rhs_stays_alias() {
    let source = "enum Token { Present(Str), }\ntype Alias = Token\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let kinds: Vec<_> = parsed
        .arena
        .statement_ids()
        .map(|id| parsed.arena.arena.stmt(id).kind)
        .collect();
    let ArenaStmtKind::TypeDef(token) = kinds[0] else {
        panic!("enum declaration");
    };
    let ArenaTypeDefBody::TagUnion(variants) = parsed.arena.arena.type_def(token).body else {
        panic!("nominal enum");
    };
    assert_eq!(parsed.arena.arena.tag_variants(variants).len(), 1);
    let ArenaStmtKind::TypeDef(alias) = kinds[1] else {
        panic!("alias declaration");
    };
    assert!(matches!(
        parsed.arena.arena.type_def(alias).body,
        ArenaTypeDefBody::Alias(_)
    ));
}

#[test]
fn parser_and_formatter_preserve_selective_retry() {
    let source =
        "let result=retry [0ms] on (FetchError.Busy | FetchError.Timeout) {\n  fetch()?\n}\n";
    let expected =
        "let result = retry [0ms] on (FetchError.Busy | FetchError.Timeout) {\n  fetch()?\n}\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, expected);
    let second = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert!(second.diagnostics.is_empty(), "{:?}", second.diagnostics);
    assert_eq!(second.formatted, expected);
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), expected);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let ArenaExprKind::Retry { pattern, .. } =
        parsed.arena.arena.expr(root_let_init_expr(&parsed, 0)).kind
    else {
        panic!("expected retry")
    };
    assert!(pattern.is_some());
}

#[test]
fn selective_retry_requires_parenthesized_clause_and_retains_on_names() {
    let invalid = Parser::parse_source_arena_only(
        SourceId::new(0),
        "let result = retry [] on FetchError.Busy { fetch()? }",
    );
    assert!(invalid.diagnostics.iter().any(
        |diagnostic| diagnostic.code.map(DiagnosticCode::name) == Some("parse.expected-token")
    ));
    let ordinary = Parser::parse_source_arena_only(
        SourceId::new(0),
        "let on = 1\nlet result = retry [] { on }\nrun echo on\n",
    );
    assert!(
        ordinary.diagnostics.is_empty(),
        "{:?}",
        ordinary.diagnostics
    );
}

// These assertions own CST and original byte spans, including offsets into
// interpolation source that a native value assertion cannot inspect.
#[test]
fn block_string_parser_preserves_original_expression_and_diagnostic_spans() {
    let source = "let name = \"café\"\nlet value = f\"\"\"\n  {if true {\nlet _ = \"}\"\n  r\"\"\"nested\n exact\"\"\"\n} else { \"\" }}\n  {name}\n  \"\"\"\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(19), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.cst.get().exact_text(), source);
    let arena = &parsed.arena.arena;
    let ArenaExprKind::FmtString(parts) = arena.expr(root_let_init_expr(&parsed, 1)).kind else {
        panic!("formatted block");
    };
    let expressions = arena
        .fmt_parts(parts)
        .filter_map(|part| match part {
            ArenaFmtPart::Expr(expr, _) => Some(arena.expr(expr).span),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(expressions.len(), 2);
    assert!(source[expressions[0].range()].starts_with("if true {"));
    assert_eq!(&source[expressions[1].range()], "name");
    assert!(
        expressions
            .iter()
            .all(|span| span.source_id == SourceId::new(19))
    );
    let formatted = Formatter::new().format_source(SourceId::new(19), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_parse_and_check(SourceId::new(19), &formatted.formatted);
    assert_eq!(
        formatted.formatted,
        Formatter::new()
            .format_source(SourceId::new(19), &formatted.formatted)
            .formatted
    );

    let invalid = "let name = \"café\"\nlet value = f\"\"\"\n  {@}\n  \"\"\"\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(19), invalid);
    assert!(
        parsed
            .diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.labels)
            .any(|label| &invalid[label.span.range()] == "@"),
        "{:?}",
        parsed.diagnostics
    );
    let invalid = "let name = \"café\"\nlet value = \"\"\"\n  good\n wrong\n  \"\"\"\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(19), invalid);
    let issue = parsed
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.map(DiagnosticCode::name) == Some("parse.block-string-margin")
        })
        .unwrap();
    assert_eq!(&invalid[issue.labels[0].span.range()], " w");
}

#[test]
fn parser_and_formatter_retain_bare_bytes_stdin_as_one_typed_operand() {
    let source = "let copied = run.bytes cat < b\"a\\0\\xff\" ?\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted.formatted)
            .formatted,
        source
    );
}

#[test]
fn named_argument_spread_preserves_source_spans_and_formatter_round_trips() {
    use xsh::frontend::syntax::arena::{ArenaCallArgKind, ExprId};
    let source = include_str!("fixtures/syntax/valid/named-argument-spreads.xsh");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_parse_and_check(SourceId::new(0), source);
    let arena = &parsed.arena.arena;
    let mut spreads = 0;
    for index in 0..arena.expr_tags.len() {
        let ArenaExprKind::Call { args, .. } = arena.expr(ExprId::from_index(index)).kind else {
            continue;
        };
        for argument in arena.call_args(args) {
            if let ArenaCallArgKind::NamedSpread { value, span } = argument.kind {
                spreads += 1;
                let span = arena.span(span);
                assert!(source[span.range()].starts_with("..."));
                assert!(span.start() < arena.expr(value).span.start());
                assert_eq!(span.end(), arena.expr(value).span.end());
            }
        }
    }
    assert_eq!(spreads, 3);
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert!(formatted.formatted.contains("...options"));
    assert!(
        formatted
            .formatted
            .contains("# Keep the spread entry comment.")
    );
    assert_parse_and_check(SourceId::new(0), &formatted.formatted);
    let second = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(second.formatted, formatted.formatted);
}

#[test]
fn parser_and_formatter_preserve_wire_enum_constant_expressions() {
    let source = "const prefix = \"rea\"\nenum State: Str {\n  # External spelling stays stable.\n  Ready = prefix + \"dy\",\n  Empty = \"\",\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.cst.get().exact_text(), source);
    let ArenaStmtKind::TypeDef(id) = parsed
        .arena
        .arena
        .stmt(parsed.arena.statement_ids().nth(1).unwrap())
        .kind
    else {
        panic!("enum declaration")
    };
    let ArenaTypeDefBody::TagUnion(variants) = parsed.arena.arena.type_def(id).body else {
        panic!("enum variants")
    };
    assert!(
        parsed
            .arena
            .arena
            .tag_variants(variants)
            .iter()
            .all(|variant| variant.wire_value.is_some())
    );
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    for fragment in [
        "enum State: Str",
        "# External spelling stays stable.",
        "prefix + \"dy\"",
        "Empty = \"\"",
    ] {
        assert!(
            formatted.formatted.contains(fragment),
            "{}",
            formatted.formatted
        );
    }
    let second = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(second.formatted, formatted.formatted);
    let checked = Checker::check_arena(&parsed.arena, source);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
}

#[test]
fn signature_cli_retains_a_distinct_entry_declaration_and_contextual_cli_calls() {
    let source = "cli main(root: Path, jobs: Int = 4) [fs, error] { print $root $jobs }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let root = parsed.arena.statement_ids().collect::<Vec<_>>();
    assert!(matches!(
        parsed.arena.arena.stmt(root[0]).kind,
        xsh::frontend::syntax::arena::ArenaStmtKind::CliMain(_)
    ));
    let first = Formatter::new().format_source(SourceId::new(0), source);
    assert!(first.diagnostics.is_empty());
    assert!(
        first.formatted.starts_with("cli main("),
        "{}",
        first.formatted
    );
    assert_eq!(
        first.formatted,
        Formatter::new()
            .format_source(SourceId::new(0), &first.formatted)
            .formatted
    );
    let ordinary =
        Parser::parse_source_arena_only(SourceId::new(0), "let parsed = cli.parse(args, {})?\n");
    assert!(
        ordinary.diagnostics.is_empty(),
        "{:?}",
        ordinary.diagnostics
    );
}

#[test]
fn parser_and_formatter_preserve_accept_policy_expressions() {
    let source = "pure policy() -> List[Int] { [0,1] }\nlet codes = [0,1]\nrun.status --accept=(codes) sh --accept=[9]\nrun.status --accept=policy() --timeout=1s sh\nlet child = spawn run --timeout=1s --accept=[0,1] sh ?\nlet command = process.command {\naccept=[0,1]\nrun sh\n}\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.cst.get().exact_text(), source);
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert!(
        formatted
            .formatted
            .contains("--accept=codes sh --accept=[9]")
    );
    assert_parse_and_check(SourceId::new(0), &formatted.formatted);
    let again = Formatter::new().format_source(SourceId::new(0), &formatted.formatted);
    assert_eq!(again.formatted, formatted.formatted);
}

#[test]
fn parser_context_scopes_keep_nested_expression_spans_and_body_modes() {
    let source = "let selected = env ({X: 7}) { false }?\nenv ({X: 8}) { print ok }?\n";
    let output = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let arena = &output.arena.arena;
    let init = root_let_init_expr(&output, 0);
    let ArenaExprKind::Try(scope) = arena.expr(init).kind else {
        panic!("scope propagation");
    };
    let ArenaExprKind::ContextScope {
        input,
        value_body: true,
        ..
    } = arena.expr(scope).kind
    else {
        panic!("value body");
    };
    assert_eq!(&source[arena.expr(input).span.range()], "{X: 7}");
    assert_eq!(
        &source[arena.expr(scope).span.range()],
        "env ({X: 7}) { false }"
    );
}

#[test]
fn default_parameter_parser_retains_omission_without_synthesizing_source_types() {
    let source = "const config = {jobs: 4}\npure choose(jobs = config.jobs + 1, label = \"café\") -> Int { jobs }\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.cst.get().exact_text(), source);
    assert!(
        parsed
            .arena
            .arena
            .function_defs
            .iter()
            .flat_map(|def| parsed.arena.arena.params(def.params))
            .all(|param| param.ty_defaulted)
    );
    let output = Formatter::new().format_source(SourceId::new(0), source);
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let formatted = output.formatted;
    assert!(formatted.contains("jobs = config.jobs + 1"));
    assert!(!formatted.contains("Unknown"));
    let reparsed = Parser::parse_source_arena_only(SourceId::new(0), &formatted);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
    assert!(
        Checker::check_arena(&reparsed.arena, &formatted)
            .diagnostics
            .is_empty()
    );
    assert_eq!(
        Formatter::new()
            .format_source(SourceId::new(0), &formatted)
            .formatted,
        formatted
    );
}

// Interpolation expressions are parsed from slices of the literal, so these
// assertions own the byte offsets that map them back across `{{` escapes,
// multibyte text, and width specs.
#[test]
fn fmt_interpolation_spans_index_the_original_source() {
    let source = "let x = 1\nlet v = f\"é {{ {x}}} {{{ {a: x}.a :>3}!\"\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(4), source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let arena = &parsed.arena.arena;
    let ArenaExprKind::FmtString(parts) = arena.expr(root_let_init_expr(&parsed, 1)).kind else {
        panic!("f-string")
    };
    let parts = arena.fmt_parts(parts).collect::<Vec<_>>();
    let texts = parts
        .iter()
        .filter_map(|part| match part {
            ArenaFmtPart::Text(text) => arena.text_value(text, source).map(str::to_owned),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(texts, ["é { ", "} {", "!"]);
    let expressions = parts
        .iter()
        .filter_map(|part| match part {
            ArenaFmtPart::Expr(expr, spec) => Some((
                &source[arena.expr(*expr).span.range()],
                spec.as_ref().map(|spec| spec.width),
            )),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(expressions, [("x", None), ("{a: x}.a", Some(3))]);

    let invalid = "let v = f\"é {{ {1 +}\"\n";
    let parsed = Parser::parse_source_arena_only(SourceId::new(4), invalid);
    let label = parsed
        .diagnostics
        .iter()
        .flat_map(|diagnostic| &diagnostic.labels)
        .next()
        .expect("diagnostic label");
    assert_eq!(
        label.span.start(),
        invalid.find("}\"").unwrap(),
        "{:?}",
        parsed.diagnostics
    );
}

#[path = "grammar.rs"]
mod grammar;

#[test]
fn formatter_keeps_a_literal_dollar_before_an_interpolation() {
    let source = "let i = 1\nlet ph = f\"\\${i}\"\nlet both = f\"\\\\\\${i}\"\n";
    let formatted = Formatter::new().format_source(SourceId::new(0), source);
    assert!(
        formatted.diagnostics.is_empty(),
        "{:?}",
        formatted.diagnostics
    );
    assert_eq!(formatted.formatted, source);
}
