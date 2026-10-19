use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use xsh::frontend::check::{CheckOptions, EffectDeclarationId, FunctionEffectFact};
use xsh::frontend::load::parse_load_check_file;
use xsh::frontend::source::{SourceMap, Span};
use xsh::frontend::syntax::arena::{
    ArenaChild, ArenaExprKind, ArenaStmtKind, BlockId, FunctionDefId, SugarView,
};
use xsh::frontend::syntax::node::Effect;

struct Target {
    project: String,
    path: String,
    function: String,
}

fn escaped(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
        .replace('\n', "\\n")
        .replace('|', "\\|")
        .replace(';', "\\;")
}

fn line_at(sources: &SourceMap, span: Span) -> usize {
    let source = sources
        .get(span.source_id)
        .expect("source span belongs to loaded source map")
        .text();
    source[..span.start()].bytes().filter(|byte| *byte == b'\n').count() + 1
}

fn effect_list(effects: Option<&[Effect]>) -> String {
    match effects {
        None => "unknown".to_string(),
        Some(effects) if effects.is_empty() => "[]".to_string(),
        Some(effects) => effects
            .iter()
            .map(Effect::as_str)
            .collect::<Vec<_>>()
            .join(","),
    }
}

fn call_sites(sources: &SourceMap, arena: &xsh::frontend::syntax::arena::AstArena, body: BlockId) -> String {
    let mut pending = Vec::new();
    arena.for_each_block_child(body, |child| pending.push(child));
    let mut sites = BTreeSet::new();
    while let Some(child) = pending.pop() {
        match child {
            ArenaChild::Stmt(id) => {
                if let ArenaStmtKind::Command(command) = &arena.stmt(id).kind {
                    let span = arena.span(arena.command_stmt(*command).span);
                    let text = sources
                        .span_text(span)
                        .expect("command span belongs to loaded source map");
                    sites.insert((line_at(sources, span), format!("command:{}", escaped(text))));
                }
                arena.for_each_stmt_child(id, SugarView::Source, |child| pending.push(child));
            }
            ArenaChild::Block(id) => arena.for_each_block_child(id, |child| pending.push(child)),
            ArenaChild::Expr(id) => {
                let expression = arena.expr(id);
                if let ArenaExprKind::Call { callee, .. } = &expression.kind {
                    let span = arena.expr(*callee).span;
                    let text = sources
                        .span_text(span)
                        .expect("callee span belongs to loaded source map");
                    sites.insert((line_at(sources, span), escaped(text)));
                }
                if let ArenaExprKind::Run(run) = &expression.kind {
                    let span = arena.span(arena.run_form(*run).span);
                    let text = sources
                        .span_text(span)
                        .expect("run span belongs to loaded source map");
                    sites.insert((line_at(sources, span), format!("run:{}", escaped(text))));
                }
                arena.for_each_expr_child(id, |child| pending.push(child));
            }
            ArenaChild::Pattern(id) => arena.for_each_pattern_child(id, |child| pending.push(child)),
            ArenaChild::BindingTarget(id) => {
                arena.for_each_binding_target_child(id, |child| pending.push(child));
            }
            ArenaChild::AssignTarget(id) => {
                arena.for_each_assign_target_child(id, |child| pending.push(child));
            }
            ArenaChild::TypeExpr(id) => arena.for_each_type_expr_child(id, |child| pending.push(child)),
            ArenaChild::BuilderBlock(id) => arena.for_each_builder_child(id, |child| pending.push(child)),
        }
    }
    sites
        .into_iter()
        .map(|(line, callee)| format!("{line}:{callee}"))
        .collect::<Vec<_>>()
        .join(";")
}

fn function_definition(kind: &ArenaStmtKind) -> Option<FunctionDefId> {
    match kind {
        ArenaStmtKind::ProcDef(definition)
        | ArenaStmtKind::StreamDef(definition)
        | ArenaStmtKind::CliMain(definition) => Some(*definition),
        _ => None,
    }
}

fn facts_for_file(root: &Path, targets: &[Target]) -> Result<Vec<String>, String> {
    let relative = &targets[0].path;
    let file = root.join(relative);
    let entry = parse_load_check_file(
        file.to_str().ok_or_else(|| format!("non-UTF-8 path: {}", file.display()))?,
        vec![root.to_path_buf()],
        CheckOptions::for_check_cli(),
    )
    .map_err(|error| format!("{}: {error}", file.display()))?;
    if !entry.parsed.diagnostics.is_empty() {
        return Err(entry.render_parse_diagnostics());
    }
    let checked = entry
        .checked
        .as_ref()
        .ok_or_else(|| format!("{} did not produce checker output", file.display()))?;
    if !checked.diagnostics.is_empty() {
        return Err(entry.render_check_diagnostics());
    }
    let program = &entry.parsed.arena;
    let _owner = program.symbol_owner().enter();
    let rows = program.arena.source_rows();
    let definitions = rows
        .stmt_ids(entry.entry_source_id)
        .filter_map(|statement| {
            let statement = program.arena.stmt(statement);
            let definition = function_definition(&statement.kind)?;
            let function = program.arena.function_def(definition);
            Some((function.name.to_string(), definition))
        })
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for target in targets {
        let matches = definitions
            .iter()
            .filter(|(name, _)| name == &target.function)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!(
                "{}:{} matched {} top-level function declarations",
                target.path,
                target.function,
                matches.len()
            ));
        }
        let definition = program.arena.function_def(matches[0].1);
        let body = program.arena.span(program.arena.block(definition.body).span);
        let identity = EffectDeclarationId {
            namespace: None,
            body,
        };
        let fact = checked
            .function_effect_facts
            .get(&identity)
            .ok_or_else(|| format!("{}:{} has no effect fact", target.path, target.function))?;
        let declared = definition.effects.map(|range| {
            program
                .arena
                .effects(range)
                .map(|effect| effect.as_str())
                .collect::<Vec<_>>()
                .join(",")
        });
        result.push(format_row(
            target,
            body,
            declared.as_deref(),
            fact,
            &entry.sources,
            &program.arena,
            definition.body,
        ));
    }
    Ok(result)
}

fn format_row(
    target: &Target,
    body: Span,
    declared: Option<&str>,
    fact: &FunctionEffectFact,
    sources: &SourceMap,
    arena: &xsh::frontend::syntax::arena::AstArena,
    block: BlockId,
) -> String {
    let unknown_chain = if fact.unknown_chain.is_empty() {
        "-".to_string()
    } else {
        fact.unknown_chain
            .iter()
            .map(|name| escaped(name))
            .collect::<Vec<_>>()
            .join(" -> ")
    };
    let declared = declared.map_or_else(|| "none".to_string(), str::to_string);
    format!(
        "{}\t{}\troot(None)\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        target.path,
        target.function,
        body.source_id,
        body.start(),
        body.end(),
        line_at(sources, body),
        declared,
        effect_list(fact.effective.as_deref()),
        effect_list(fact.required.as_deref()),
        fact.inferred,
        fact.inference_allowed,
        fact.redundant_clause,
        unknown_chain,
    ) + &format!("\t{}", call_sites(sources, arena, block))
}

fn main() {
    let mut arguments = env::args().skip(1);
    let xsh_root = PathBuf::from(arguments.next().expect("XSH source root required"));
    let laputa_root = PathBuf::from(arguments.next().expect("Laputa source root required"));
    let targets_path = PathBuf::from(arguments.next().expect("target table required"));
    assert!(arguments.next().is_none(), "unexpected arguments");
    let target_text = fs::read_to_string(&targets_path).expect("read target table");
    let mut grouped = BTreeMap::<(String, String), Vec<Target>>::new();
    let mut target_keys = BTreeSet::new();
    for (line_number, line) in target_text.lines().enumerate() {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 3 {
            panic!("{}:{}: expected project, path, function", targets_path.display(), line_number + 1);
        }
        let target = Target {
            project: fields[0].to_string(),
            path: fields[1].to_string(),
            function: fields[2].to_string(),
        };
        assert!(matches!(target.project.as_str(), "xsh" | "laputa"));
        assert!(target_keys.insert((
            target.project.clone(),
            target.path.clone(),
            target.function.clone(),
        )), "{}:{}:{} appears more than once", target.project, target.path, target.function);
        grouped
            .entry((target.project.clone(), target.path.clone()))
            .or_default()
            .push(target);
    }
    let target_count = grouped.values().map(Vec::len).sum::<usize>();
    assert_eq!(target_count, 60, "target table must contain the frozen 60 declarations");
    assert_eq!(target_keys.len(), target_count, "every frozen declaration must be unique");
    let mut output = Vec::with_capacity(target_count);
    for ((project, _), targets) in grouped {
        let root = if project == "xsh" { &xsh_root } else { &laputa_root };
        match facts_for_file(root, &targets) {
            Ok(rows) => {
                for row in rows {
                    output.push(format!("{project}\t{row}"));
                }
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    assert_eq!(output.len(), target_count, "every frozen declaration must have one fact row");
    println!("project\tpath\tfunction\tnamespace\tbody_source\tbody_start\tbody_end\tbody_line\tdeclared\teffective\trequired\tinferred\tinference_allowed\tredundant_clause\tunknown_call_chain\tsource_call_sites");
    for row in output {
        println!("{row}");
    }
}
