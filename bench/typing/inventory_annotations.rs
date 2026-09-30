use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use xsh::frontend::load::{parse_load_check_file, resolve_user_module};
use xsh::frontend::check::CheckOptions;
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::syntax::arena::*;
use xsh::frontend::syntax::cst::{SyntaxGroupKind, SyntaxKind};
use xsh::frontend::syntax::parser::Parser;

fn quote(value: &str) -> String {
    let mut result = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => result.push_str("\\\""), '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"), '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"), c if c.is_control() => result.push_str(&format!("\\u{:04x}", c as u32)),
            c => result.push(c),
        }
    }
    result.push('"'); result
}
fn span_json(span: Span) -> String { format!("[{},{}]", span.start(), span.end()) }
fn text_at<'a>(source: &'a str, span: Span) -> &'a str { &source[span.range()] }
fn enclosing_owner(arena: &AstArena, span: Span) -> String {
    let mut found = None;
    for (i, def) in arena.function_defs.iter().enumerate() {
        let body = arena.span(arena.block(def.body).span);
        if body.source_id == span.source_id && body.start() <= span.start() && body.end() >= span.end()
            && found.as_ref().is_none_or(|(_, previous): &(usize, Span)| body.end()-body.start() < previous.end()-previous.start()) {
            found = Some((i, body));
        }
    }
    found.map_or_else(|| "top-level".to_owned(), |(i, _)| format!("function:{}:{}", i, arena.function_defs[i].name))
}
fn annotation(source: &str, category: &str, owner: &str, span: Span, exported: bool, extra: &str) -> String {
    format!("{{\"category\":{},\"owner\":{},\"span\":{},\"text\":{},\"exported\":{},{} }}", quote(category), quote(owner), span_json(span), quote(text_at(source, span)), exported, extra)
}
fn effect_span(parsed: &xsh::frontend::syntax::parser::ArenaParseOutput, header: Span, params_end: usize, return_start: usize) -> Span {
    let nodes: Vec<_> = parsed.cst.get().nodes().iter().filter(|node| node.kind == SyntaxKind::Group(SyntaxGroupKind::Bracket)
        && node.span.start() >= params_end && node.span.end() <= return_start && node.span.start() >= header.start()).collect();
    assert_eq!(nodes.len(), 1, "effect list must own exactly one CST bracket group at {:?}", header);
    nodes[0].span
}
fn parse_file(file: &Path) {
    let source = std::fs::read_to_string(file).unwrap();
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &source);
    let arena = &parsed.arena.arena;
    parsed.arena.symbol_owner().clone().with_current(|| {
        let exported: BTreeSet<usize> = (0..arena.stmt_tags.len()).filter_map(|i| match arena.stmt(StmtId::from_index(i)).kind {
            ArenaStmtKind::Export(inner) => Some(inner.index()), _ => None
        }).collect();
        let mut sites = Vec::new();
        let mut imports = Vec::new();
        let mut declarations = Vec::new();
        let mut used_params = BTreeSet::new();
        for i in 0..arena.stmt_tags.len() {
            let stmt = arena.stmt(StmtId::from_index(i));
            match stmt.kind {
                ArenaStmtKind::Use(id) => {
                    let use_stmt = arena.use_stmt(id);
                    let path: Vec<_> = arena.names(use_stmt.path).collect();
                    let target = resolve_user_module(file, &path, &[std::env::current_dir().unwrap(),std::env::current_dir().unwrap().join("dev")]).unwrap();
                    imports.push(format!("{{\"path\":{},\"resolved\":{},\"span\":{}}}", quote(&path.iter().map(ToString::to_string).collect::<Vec<_>>().join(".")), target.as_ref().map_or("null".to_owned(), |(p, _)| quote(&p.to_string_lossy())), span_json(stmt.span)));
                }
                ArenaStmtKind::Let {target,ty:Some(ty),initializer} | ArenaStmtKind::Const {target,ty:Some(ty),initializer} | ArenaStmtKind::Var {target,ty:Some(ty),initializer} | ArenaStmtKind::Guard {target,ty:Some(ty),initializer,..} => {
                    let name = match arena.binding_target(target).kind { ArenaBindingTargetKind::Name(name) => name.to_string(), _ => "destructure".to_owned() };
                    let initializer_span = match initializer { ArenaExprOrRun::Expr(e) => arena.expr(e).span, ArenaExprOrRun::Run(r) => arena.span(arena.run_form(r).span) };
                    let initializer_kind = match initializer {ArenaExprOrRun::Expr(e) => match arena.expr(e).kind {ArenaExprKind::Int(_)=>"integer-literal",ArenaExprKind::Record(_)=>"record-literal",_=>"expression"},ArenaExprOrRun::Run(_)=>"run"};
                    let owner = format!("{}/binding:{}:{}", enclosing_owner(arena,stmt.span),i,name);
                    sites.push(annotation(&source,"local",&owner,arena.type_expr_span(ty),exported.contains(&i), &format!("\"declaration_span\":{},\"initializer_span\":{},\"initializer_kind\":{}",span_json(stmt.span),span_json(initializer_span),quote(initializer_kind))));
                }
                ArenaStmtKind::ProcDef(id) | ArenaStmtKind::CliMain(id) | ArenaStmtKind::PureDef(id) | ArenaStmtKind::StreamDef(id) => {
                    let def = arena.function_def(id);
                    let kind = if def.test_declaration { "test" } else { match stmt.kind { ArenaStmtKind::PureDef(_) => "pure", ArenaStmtKind::StreamDef(_) => "stream", ArenaStmtKind::CliMain(_) => "cli-main", _ => "proc" }};
                    let owner = format!("function:{}:{}",id.index(),def.name);
                    let body = arena.span(arena.block(def.body).span);
                    let params = arena.params(def.params);
                    let p_end = if def.test_declaration {stmt.span.start()} else {params.last().map_or(stmt.span.start(), |p| arena.span(p.span).end())};
                    let r_span = arena.type_expr_span(def.return_ty);
                    for p in params {
                        used_params.insert(arena.span(p.span));
                        if !p.ty_defaulted && !def.test_declaration {
                            sites.push(annotation(&source,"parameter",&format!("{}/parameter:{}",owner,p.name),arena.type_expr_span(p.ty),exported.contains(&i),&format!("\"declaration_span\":{},\"callable_kind\":{},\"callable_name\":{},\"rest\":{}",span_json(stmt.span),quote(kind),quote(&def.name.to_string()),p.rest)));
                        }
                    }
                    if !def.return_ty_defaulted {
                        sites.push(annotation(&source,if kind=="stream" {"producer-item"} else {"return"},&owner,r_span,exported.contains(&i),&format!("\"declaration_span\":{},\"body_span\":{},\"callable_kind\":{},\"callable_name\":{}",span_json(stmt.span),span_json(body),quote(kind),quote(&def.name.to_string()))));
                    }
                    if let Some(effects) = def.effects {
                        let e_span = effect_span(&parsed,stmt.span,p_end,if def.return_ty_defaulted {body.start()} else {r_span.start()});
                        sites.push(annotation(&source,"effect",&owner,e_span,exported.contains(&i),&format!("\"declaration_span\":{},\"callable_kind\":{},\"callable_name\":{},\"empty\":{}",span_json(stmt.span),quote(kind),quote(&def.name.to_string()),effects.is_empty())));
                    }
                    declarations.push(format!("{{\"owner\":{},\"name\":{},\"kind\":{},\"declaration_span\":{},\"body_span\":{},\"omitted_return\":{},\"exported\":{}}}",quote(&owner),quote(&def.name.to_string()),quote(kind),span_json(stmt.span),span_json(body),def.return_ty_defaulted,exported.contains(&i)));
                }
                ArenaStmtKind::SignalHook(id) => {
                    let hook = arena.signal_hook(id);
                    let body = arena.span(arena.block(hook.body).span);
                    let e_span = effect_span(&parsed,stmt.span,stmt.span.start(),body.start());
                    sites.push(annotation(&source,"signal-hook-effect",&format!("signal-hook:{}:{}",id.index(),hook.signal),e_span,false,&format!("\"declaration_span\":{},\"empty\":{}",span_json(stmt.span),hook.effects.is_empty())));
                }
                ArenaStmtKind::TypeDef(id) => {
                    let def = arena.type_def(id);
                    if let ArenaTypeDefBody::Alias(ty) = def.body {
                        sites.push(annotation(&source,"type-alias",&format!("type:{}:{}",id.index(),def.name),arena.type_expr_span(ty),exported.contains(&i),&format!("\"declaration_span\":{}",span_json(stmt.span))));
                    }
                }
                _ => {}
            }
        }
        for i in 0..arena.expr_tags.len() {
            let expr = arena.expr(ExprId::from_index(i));
            if let ArenaExprKind::Require {schema:Some(ty),..} = expr.kind {
                sites.push(annotation(&source,"validation-target",&format!("{}/require:{}",enclosing_owner(arena,expr.span),i),arena.type_expr_span(ty),false,&format!("\"declaration_span\":{}",span_json(expr.span))));
            }
        }
        for (i, field) in arena.schema_fields.iter().enumerate() {
            sites.push(annotation(&source,"schema-field",&format!("schema-field:{}:{}",i,field.name),arena.type_expr_span(field.ty),false,&format!("\"declaration_span\":{}",span_json(arena.span(field.span)))));
        }
        for (i, field) in arena.error_fields.iter().enumerate() {
            sites.push(annotation(&source,"error-field",&format!("error-field:{}:{}",i,field.name),arena.type_expr_span(field.ty),false,&format!("\"declaration_span\":{}",span_json(arena.span(field.span)))));
        }
        for (i,param) in arena.params.iter().enumerate() {
            if !used_params.contains(&arena.span(param.span)) && !param.ty_defaulted {
                sites.push(annotation(&source,"module-contract-parameter",&format!("module-parameter:{}:{}",i,param.name),arena.type_expr_span(param.ty),true,&format!("\"declaration_span\":{}",span_json(arena.span(param.span)))));
            }
        }
        for (i,entry) in arena.module_contract_entries.iter().enumerate() {
            let owner = format!("module-entry:{}:{}",i,entry.name);
            let span = arena.span(entry.span);
            match entry.kind {
                ArenaModuleContractEntryKind::Value(ty) => sites.push(annotation(&source,"module-contract-value",&owner,arena.type_expr_span(ty),true,&format!("\"declaration_span\":{}",span_json(span)))),
                ArenaModuleContractEntryKind::Pure {return_ty,..} | ArenaModuleContractEntryKind::Proc {return_ty,..} => {
                    sites.push(annotation(&source,"module-contract-return",&owner,arena.type_expr_span(return_ty),true,&format!("\"declaration_span\":{}",span_json(span))));
                    if let ArenaModuleContractEntryKind::Proc {params,effects:Some(effects),return_ty} = entry.kind {
                        let p_end = arena.params(params).last().map_or(span.start(),|p| arena.span(p.span).end());
                        let e_span = effect_span(&parsed,span,p_end,arena.type_expr_span(return_ty).start());
                        sites.push(annotation(&source,"module-contract-effect",&owner,e_span,true,&format!("\"declaration_span\":{},\"empty\":{}",span_json(span),effects.is_empty())));
                    }
                }
            }
        }
        println!("{{\"file\":{},\"parser_diagnostics\":{},\"sites\":[{}],\"imports\":[{}],\"declarations\":[{}],\"ast_type_nodes\":{},\"cst_nodes\":{}}}",quote(&file.to_string_lossy()),parsed.diagnostics.len(),sites.join(","),imports.join(","),declarations.join(","),arena.type_expr_tags.len(),parsed.cst.get().node_count());
    });
}
fn facts_file(file:&Path, roots:Vec<PathBuf>) {
    let entry=parse_load_check_file(&file.to_string_lossy(),roots,CheckOptions::default()).unwrap();
    let arena=&entry.parsed.arena.arena;
    let mut facts=Vec::new();
    let mut bindings=Vec::new();
    entry.parsed.arena.symbol_owner().clone().with_current(|| {
        for (function_index,def) in arena.function_defs.iter().enumerate() {
            let body=arena.span(arena.block(def.body).span);
            let Some(source)=entry.sources.get(body.source_id) else {continue};
            let checked=entry.checked.as_ref();
            let ret=checked.and_then(|c|c.function_return_types.get(&body));
            let tail=arena.stmt_ids(arena.block(def.body).statements).last().map(|id|arena.stmt(id));
            let tail_span=tail.as_ref().map(|s|s.span);
            let tail_expr=tail.as_ref().and_then(|s|match s.kind {ArenaStmtKind::Expr(e)|ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(e)))=>Some(arena.expr(e).span),_=>None});
            let tail_ty=tail_expr.and_then(|s|checked.and_then(|c|c.expr_types.get(&s)));
            let assertion=tail_span.is_some_and(|s|checked.is_some_and(|c|c.assertion_spans.contains(&s))) || tail_expr.is_some_and(|s|checked.is_some_and(|c|c.assertion_spans.contains(&s)));
            let owner = format!("function:{}:{}",function_index,def.name);
            let mut completions=Vec::new();
            for index in 0..arena.stmt_tags.len() {
                let statement=arena.stmt(StmtId::from_index(index));
                if statement.span.source_id==body.source_id && statement.span.start()>=body.start() && statement.span.end()<=body.end()
                    && enclosing_owner(arena,statement.span)==owner {
                    if let ArenaStmtKind::Return(Some(ArenaExprOrRun::Expr(expr)))=statement.kind {
                        let span=arena.expr(expr).span;
                        let ty=checked.and_then(|c|c.expr_types.get(&span));
                        completions.push(format!("{{\"span\":{},\"type\":{}}}",span_json(span),ty.map_or("null".to_owned(),|t|quote(&t.to_string()))));
                    }
                }
            }
            let captures=(0..arena.expr_tags.len()).filter_map(|index| {
                let expr=arena.expr(ExprId::from_index(index));
                match expr.kind {ArenaExprKind::Capture(block)|ArenaExprKind::Retry {block,..}=>Some(arena.span(arena.block(block).span)),_=>None}
            }).filter(|span|span.source_id==body.source_id && span.start()>=body.start() && span.end()<=body.end()).collect::<Vec<_>>();
            let mut propagation=Vec::new();
            for index in 0..arena.expr_tags.len() {
                let expr=arena.expr(ExprId::from_index(index));
                if expr.span.source_id!=body.source_id || expr.span.start()<body.start() || expr.span.end()>body.end()
                    || captures.iter().any(|span|span.start()<=expr.span.start() && span.end()>=expr.span.end())
                    || enclosing_owner(arena,expr.span)!=owner {continue}
                let established=match expr.kind {ArenaExprKind::Try(inner)=>checked.and_then(|c|c.expr_types.get(&arena.expr(inner).span)).is_some_and(|ty|ty.is_result()),_=>checked.is_some_and(|c|c.statement_expression_spans.contains(&expr.span) && c.expr_types.get(&expr.span).is_some_and(|ty|ty.is_result_unit()))};
                if established {propagation.push(span_json(expr.span));}
            }
            let assertions=checked.map_or_else(Vec::new,|c|c.assertion_spans.iter().filter(|span|span.source_id==body.source_id && span.start()>=body.start() && span.end()<=body.end()).copied().map(span_json).collect::<Vec<_>>());
            let params=arena.params(def.params).iter().map(|p| {
                let span=arena.span(p.span);
                let ty=checked.and_then(|c|c.parameter_types.get(&span));
                format!("{{\"name\":{},\"span\":{},\"type\":{},\"omitted_type\":{}}}",quote(&p.name.to_string()),span_json(span),ty.map_or("null".to_owned(),|t|quote(&t.to_string())),p.ty_defaulted)
            }).collect::<Vec<_>>();
            facts.push(format!("{{\"file\":{},\"name\":{},\"body_span\":{},\"return_type\":{},\"omitted_return\":{},\"test_declaration\":{},\"tail_span\":{},\"tail_expression_type\":{},\"tail_assertion\":{},\"parameters\":[{}],\"explicit_completions\":[{}],\"assertion_spans\":[{}],\"outward_propagation_spans\":[{}]}}",quote(source.name()),quote(&def.name.to_string()),span_json(body),ret.map_or("null".to_owned(),|t|quote(&t.to_string())),def.return_ty_defaulted,def.test_declaration,tail_span.map_or("null".to_owned(),span_json),tail_ty.map_or("null".to_owned(),|t|quote(&t.to_string())),assertion,params.join(","),completions.join(","),assertions.join(","),propagation.join(",")));
        }
        for index in 0..arena.stmt_tags.len() {
            let statement=arena.stmt(StmtId::from_index(index));
            if let ArenaStmtKind::Let {ty:Some(ty),initializer:ArenaExprOrRun::Expr(expr),..}|ArenaStmtKind::Const {ty:Some(ty),initializer:ArenaExprOrRun::Expr(expr),..}|ArenaStmtKind::Var {ty:Some(ty),initializer:ArenaExprOrRun::Expr(expr),..}|ArenaStmtKind::Guard {ty:Some(ty),initializer:ArenaExprOrRun::Expr(expr),..}=statement.kind {
                let span=arena.type_expr_span(ty);
                let Some(source)=entry.sources.get(span.source_id) else {continue};
                let initializer_span=arena.expr(expr).span;
                let actual=entry.checked.as_ref().and_then(|c|c.expr_types.get(&initializer_span));
                bindings.push(format!("{{\"file\":{},\"span\":{},\"initializer_span\":{},\"initializer_type\":{}}}",quote(source.name()),span_json(span),span_json(initializer_span),actual.map_or("null".to_owned(),|t|quote(&t.to_string()))));
            }
        }
    });
    let diagnostics=entry.parsed.diagnostics.iter().chain(entry.checked.iter().flat_map(|c|c.diagnostics.iter())).map(|d|format!("{{\"code\":{},\"message\":{}}}",d.code.as_deref().map_or("null".to_owned(),quote),quote(&d.message))).collect::<Vec<_>>();
    let mut effects=entry.checked.as_ref().map_or_else(Vec::new,|c|c.callable_effects.iter().map(|(name,effects)|format!("{{\"name\":{},\"effects\":{}}}",quote(name),effects.as_ref().map_or("null".to_owned(),|values|format!("[{}]",values.iter().map(|e|quote(e.as_str())).collect::<Vec<_>>().join(","))))).collect::<Vec<_>>());
    effects.sort();
    println!("{{\"file\":{},\"diagnostics\":[{}],\"callables\":[{}],\"bindings\":[{}],\"callable_effects\":[{}]}}",quote(&file.to_string_lossy()),diagnostics.join(","),facts.join(","),bindings.join(","),effects.join(","));
}
fn main() {
    let args:Vec<String>=std::env::args().skip(1).collect();
    if args.first().is_some_and(|s|s=="--facts") {facts_file(Path::new(&args[1]),args[2..].iter().map(PathBuf::from).collect())}
    else {for file in args {parse_file(Path::new(&file));}}
}
