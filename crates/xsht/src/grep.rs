#![allow(clippy::single_call_fn)]

use rustc_hash::FxHashMap;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaProgram, AstArena, ExprId,
};

/// A structural grep match from `xsht::grep::find_matches_in_program`: the
/// source span and bindings for each metavariable.
#[derive(Clone, Debug)]
pub struct Match {
    pub span: Span,
    pub bindings: FxHashMap<String, Span>,
}

/// A parsed pattern/replacement expression. The expression lives in its own
/// arena; `root` is the top-level expression id and `source` is the wrapped
/// pattern text (`let _x = <pattern>`) that the spans index into.
#[derive(Clone, Debug)]
pub struct PatternExpr {
    pub program: ArenaProgram,
    pub root: ExprId,
    pub source: String,
}

impl PatternExpr {
    fn arena(&self) -> &AstArena {
        &self.program.arena
    }
}

/// Returns true if the identifier is a metavariable (all uppercase + underscores, non-empty).
pub fn is_metavar(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
}

/// Try to match the pattern expression `p_id` (in `p`) against the target
/// expression `t_id` (in `t`). On success, fill `bindings` and return true.
/// `source` is the *target* file's text (used for consistency checks on
/// repeated metavars).
fn match_expr(
    p: &AstArena,
    p_id: ExprId,
    t: &AstArena,
    t_id: ExprId,
    source: &str,
    bindings: &mut FxHashMap<String, Span>,
) -> bool {
    let pattern = p.expr(p_id);
    let target = t.expr(t_id);
    if let ArenaExprKind::Ident(name) = &pattern.kind
        && is_metavar(name.as_str().as_str())
    {
        return if let Some(&prev) = bindings.get(name.as_str().as_str()) {
            // Consistency check: same source text.
            source.get(prev.start()..prev.end())
                == source.get(target.span.start()..target.span.end())
        } else {
            bindings.insert(name.to_string(), target.span);
            true
        };
    }
    match_expr_structural(p, &pattern.kind, t, &target.kind, source, bindings)
}

fn match_expr_structural(
    p: &AstArena,
    pattern: &ArenaExprKind,
    t: &AstArena,
    target: &ArenaExprKind,
    source: &str,
    bindings: &mut FxHashMap<String, Span>,
) -> bool {
    match (pattern, target) {
        (ArenaExprKind::Null, ArenaExprKind::Null) => true,
        (ArenaExprKind::Bool(a), ArenaExprKind::Bool(b)) => a == b,
        (ArenaExprKind::Int(a), ArenaExprKind::Int(b)) => p.int_literal(*a) == t.int_literal(*b),
        (ArenaExprKind::Float(a), ArenaExprKind::Float(b)) => {
            p.float_literal(*a) == t.float_literal(*b)
        }
        (ArenaExprKind::Str(a), ArenaExprKind::Str(b)) => {
            p.string_literal(*a) == t.string_literal(*b)
        }
        (ArenaExprKind::Ident(a), ArenaExprKind::Ident(b)) => a == b,
        (
            ArenaExprKind::Field { base: pb, name: pn },
            ArenaExprKind::Field { base: tb, name: tn },
        ) => pn == tn && match_expr(p, *pb, t, *tb, source, bindings),
        (
            ArenaExprKind::Index {
                base: pb,
                index: pi,
            },
            ArenaExprKind::Index {
                base: tb,
                index: ti,
            },
        ) => {
            match_expr(p, *pb, t, *tb, source, bindings)
                && match_expr(p, *pi, t, *ti, source, bindings)
        }
        (
            ArenaExprKind::Slice {
                base: pb,
                start: ps,
                end: pe,
            },
            ArenaExprKind::Slice {
                base: tb,
                start: ts,
                end: te,
            },
        ) => {
            match_expr(p, *pb, t, *tb, source, bindings)
                && match ps.zip(*ts) {
                    Some((ps, ts)) => match_expr(p, ps, t, ts, source, bindings),
                    None => ps.is_none() && ts.is_none(),
                }
                && match pe.zip(*te) {
                    Some((pe, te)) => match_expr(p, pe, t, te, source, bindings),
                    None => pe.is_none() && te.is_none(),
                }
        }
        (
            ArenaExprKind::Call {
                callee: pc,
                args: pa,
            },
            ArenaExprKind::Call {
                callee: tc,
                args: ta,
            },
        ) => {
            let mut b2 = bindings.clone();
            if !match_expr(p, *pc, t, *tc, source, &mut b2) {
                return false;
            }
            if !match_args(p, *pa, t, *ta, source, &mut b2) {
                return false;
            }
            *bindings = b2;
            true
        }
        (ArenaExprKind::Unary { op: po, expr: pe }, ArenaExprKind::Unary { op: to, expr: te }) => {
            po == to && match_expr(p, *pe, t, *te, source, bindings)
        }
        (
            ArenaExprKind::Binary {
                op: po,
                left: pl,
                right: pr,
            },
            ArenaExprKind::Binary {
                op: to,
                left: tl,
                right: tr,
            },
        ) => {
            po == to
                && match_expr(p, *pl, t, *tl, source, bindings)
                && match_expr(p, *pr, t, *tr, source, bindings)
        }
        (ArenaExprKind::Try(pe), ArenaExprKind::Try(te)) => {
            match_expr(p, *pe, t, *te, source, bindings)
        }
        (ArenaExprKind::List(pi), ArenaExprKind::List(ti)) => {
            let pitems: Vec<ExprId> = p.expr_ids(*pi).collect();
            let titems: Vec<ExprId> = t.expr_ids(*ti).collect();
            if pitems.len() != titems.len() {
                return false;
            }
            let mut b2 = bindings.clone();
            for (pe, te) in pitems.into_iter().zip(titems) {
                if !match_expr(p, pe, t, te, source, &mut b2) {
                    return false;
                }
            }
            *bindings = b2;
            true
        }
        _ => false,
    }
}

fn match_args(
    p: &AstArena,
    pattern_args: xsh::frontend::syntax::arena::ArenaRange,
    t: &AstArena,
    target_args: xsh::frontend::syntax::arena::ArenaRange,
    source: &str,
    bindings: &mut FxHashMap<String, Span>,
) -> bool {
    let pargs: Vec<ExprId> = p
        .call_args(pattern_args)
        .iter()
        .map(call_arg_expr)
        .collect();
    let targs: Vec<ExprId> = t.call_args(target_args).iter().map(call_arg_expr).collect();
    if pargs.len() != targs.len() {
        return false;
    }
    let mut b2 = bindings.clone();
    for (pe, te) in pargs.into_iter().zip(targs) {
        if !match_expr(p, pe, t, te, source, &mut b2) {
            return false;
        }
    }
    *bindings = b2;
    true
}

fn call_arg_expr(arg: &xsh::frontend::syntax::arena::ArenaCallArg) -> ExprId {
    match &arg.kind {
        ArenaCallArgKind::Positional(expr) => *expr,
        ArenaCallArgKind::Named { value, .. } => *value,
        ArenaCallArgKind::Splice { value, .. } => *value,
    }
}

/// Find all occurrences of `pattern` in `program`, collecting each match into
/// `matches`. Every expression the parser produced lives in the arena's flat
/// expression pool, so iterating that pool visits every expression — including
/// those nested in blocks, pipelines, and `try` forms — without a recursive
/// walk.
pub fn find_matches_in_program(
    pattern: &PatternExpr,
    program: &ArenaProgram,
    source: &str,
    matches: &mut Vec<Match>,
) {
    let p = pattern.arena();
    let t = &program.arena;
    for index in 0..t.expr_tags.len() {
        let id = ExprId::from_index(index);
        let mut bindings = FxHashMap::default();
        if match_expr(p, pattern.root, t, id, source, &mut bindings) {
            matches.push(Match {
                span: t.expr(id).span,
                bindings,
            });
        }
    }
}

/// Given a match and a replacement pattern expression, produce the replacement
/// source text. Metavariables in `replacement` are substituted with their bound
/// source spans from `m`.
pub fn apply_replacement(
    replacement: &PatternExpr,
    m: &Match,
    target_source: &str,
) -> Option<String> {
    build_replacement_text(
        replacement.arena(),
        replacement.root,
        m,
        target_source,
        &replacement.source,
    )
}

fn build_replacement_text(
    arena: &AstArena,
    id: ExprId,
    m: &Match,
    target_source: &str,
    pattern_source: &str,
) -> Option<String> {
    let expr = arena.expr(id);
    match &expr.kind {
        ArenaExprKind::Ident(name) if is_metavar(name.as_str().as_str()) => {
            let span = m.bindings.get(name.as_str().as_str())?;
            Some(target_source.get(span.start()..span.end())?.to_string())
        }
        ArenaExprKind::Field { base, name } => {
            let base_text = build_replacement_text(arena, *base, m, target_source, pattern_source)?;
            Some(format!("{base_text}.{name}"))
        }
        ArenaExprKind::Call { callee, args } => {
            let callee_text =
                build_replacement_text(arena, *callee, m, target_source, pattern_source)?;
            let mut arg_parts = Vec::new();
            for arg in arena.call_args(*args) {
                let e = call_arg_expr(arg);
                arg_parts.push(build_replacement_text(
                    arena,
                    e,
                    m,
                    target_source,
                    pattern_source,
                )?);
            }
            Some(format!("{callee_text}({})", arg_parts.join(", ")))
        }
        ArenaExprKind::Ident(name) => Some(name.to_string()),
        // For non-metavar, non-structural nodes: fall back to pattern source text.
        _ => {
            let text = pattern_source.get(expr.span.start()..expr.span.end())?;
            Some(text.to_string())
        }
    }
}

/// Extract the line number (1-based) containing byte offset `offset` from `source`.
pub fn offset_to_line(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .bytes()
        .filter(|&b| b == b'\n')
        .count()
        + 1
}

/// Extract the full source line at `offset` (trimmed of trailing newline).
pub fn line_at_offset(source: &str, offset: usize) -> &str {
    let start = source[..offset.min(source.len())]
        .rfind('\n')
        .map_or(0, |p| p + 1);
    let end = source[start..]
        .find('\n')
        .map_or(source.len(), |p| start + p);
    &source[start..end]
}

/// Parse a bare expression from a string by wrapping it as `let _x = <expr>`.
/// Returns the extracted expression on success.
pub fn parse_pattern_expr(pattern: &str) -> Result<PatternExpr, String> {
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::arena::{ArenaExprOrRun, ArenaStmtKind};
    use xsh::frontend::syntax::parser::Parser;

    let wrapped = format!("let _x = {pattern}");
    let parsed = Parser::parse_source_arena_only(SourceId::new(0), &wrapped);
    if !parsed.diagnostics.is_empty() {
        return Err(format!(
            "failed to parse pattern '{}': {}",
            pattern, parsed.diagnostics[0].message
        ));
    }
    let program = parsed.arena;
    let stmt_id = program
        .arena
        .stmt_ids(program.statements)
        .next()
        .ok_or_else(|| format!("failed to parse pattern '{pattern}': no statements"))?;
    let root = match program.arena.stmt(stmt_id).kind {
        ArenaStmtKind::Let {
            initializer: ArenaExprOrRun::Expr(expr),
            ..
        } => expr,
        _ => {
            return Err(format!(
                "failed to parse pattern '{pattern}': unexpected form"
            ));
        }
    };
    Ok(PatternExpr {
        program,
        root,
        source: wrapped,
    })
}
