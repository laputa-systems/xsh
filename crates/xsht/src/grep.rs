#![allow(clippy::single_call_fn)]

use rustc_hash::FxHashMap;
use xsh::frontend::source::Span;
use xsh::frontend::syntax::arena::{
    ArenaCallArgKind, ArenaExprKind, ArenaPatternKind, ArenaProgram, AstArena, BlockId, ExprId, PatternId,
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
        (ArenaExprKind::Regex(a), ArenaExprKind::Regex(b)) => {
            p.regex_literal(*a).pattern == t.regex_literal(*b).pattern
        }
        (ArenaExprKind::Str(a), ArenaExprKind::Str(b)) => {
            p.string_literal(*a) == t.string_literal(*b)
        }
        (ArenaExprKind::Duration(a), ArenaExprKind::Duration(b)) => p.duration_literal(*a) == t.duration_literal(*b),
        (ArenaExprKind::Bytes(a), ArenaExprKind::Bytes(b)) => p.bytes_literal(*a) == t.bytes_literal(*b),
        (ArenaExprKind::Ident(a), ArenaExprKind::Ident(b)) => a == b,
        (
            ArenaExprKind::Field { base: pb, name: pn },
            ArenaExprKind::Field { base: tb, name: tn },
        ) => pn == tn && match_expr(p, *pb, t, *tb, source, bindings),
        (
            ArenaExprKind::NullSafeField { base: pb, name: pn },
            ArenaExprKind::NullSafeField { base: tb, name: tn },
        ) => pn == tn && match_expr(p, *pb, t, *tb, source, bindings),
        (
            ArenaExprKind::Index {
                base: pb,
                index: pi,
                guarded: pg,
            },
            ArenaExprKind::Index {
                base: tb,
                index: ti,
                guarded: tg,
            },
        ) => {
            pg == tg && match_expr(p, *pb, t, *tb, source, bindings)
                && match_expr(p, *pi, t, *ti, source, bindings)
        }
        (
            ArenaExprKind::Slice {
                base: pb,
                start: ps,
                end: pe,
                guarded: pg,
            },
            ArenaExprKind::Slice {
                base: tb,
                start: ts,
                end: te,
                guarded: tg,
            },
        ) => {
            pg == tg && match_expr(p, *pb, t, *tb, source, bindings)
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
        (ArenaExprKind::ValuePipelineCall { input: pi, call: pc, .. }, ArenaExprKind::ValuePipelineCall { input: ti, call: tc, .. }) => {
            let mut next = bindings.clone();
            if match_expr(p, *pi, t, *ti, source, &mut next) && match_expr(p, *pc, t, *tc, source, &mut next) {
                *bindings = next; true
            } else { false }
        }
        (ArenaExprKind::Unary { op: po, expr: pe }, ArenaExprKind::Unary { op: to, expr: te }) => {
            po == to && match_expr(p, *pe, t, *te, source, bindings)
        }
        (ArenaExprKind::ComparisonChain(pp), ArenaExprKind::ComparisonChain(tp)) => {
            let pp = p.expr_ids(*pp).collect::<Vec<_>>();
            let tp = t.expr_ids(*tp).collect::<Vec<_>>();
            pp.len() == tp.len() && pp.into_iter().zip(tp).all(|(pp, tp)| match_expr(p, pp, t, tp, source, bindings))
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
        (ArenaExprKind::PatternTest { value: pv, arms: pa }, ArenaExprKind::PatternTest { value: tv, arms: ta })
        | (ArenaExprKind::PatternCondition { value: pv, arms: pa }, ArenaExprKind::PatternCondition { value: tv, arms: ta })
        | (ArenaExprKind::Match { value: pv, arms: pa }, ArenaExprKind::Match { value: tv, arms: ta }) => {
            let pa = p.match_expr_arms(*pa);
            let ta = t.match_expr_arms(*ta);
            let mut candidate = bindings.clone();
            if pa.len() != ta.len() || !match_expr(p, *pv, t, *tv, source, &mut candidate) { return false; }
            if !pa.iter().zip(ta).all(|(pa, ta)| {
                match_pattern(p, pa.pattern, t, ta.pattern, source, &mut candidate)
                    && match (pa.guard, ta.guard) {
                        (Some(pg), Some(tg)) => match_expr(p, pg, t, tg, source, &mut candidate),
                        (None, None) => true,
                        _ => false,
                    }
                    && match_expr(p, pa.value, t, ta.value, source, &mut candidate)
            }) { return false; }
            *bindings = candidate;
            true
        }
        (ArenaExprKind::Capture(pb), ArenaExprKind::Capture(tb))
        | (ArenaExprKind::ValueBlock(pb), ArenaExprKind::ValueBlock(tb)) => match_value_block(p, *pb, t, *tb, source, bindings),
        (ArenaExprKind::Retry { delays: pd, pattern: pp, block: pb }, ArenaExprKind::Retry { delays: td, pattern: tp, block: tb }) => {
            let pd = p.expr_ids(*pd).collect::<Vec<_>>();
            let td = t.expr_ids(*td).collect::<Vec<_>>();
            let mut candidate = bindings.clone();
            if pd.len() != td.len() || !pd.into_iter().zip(td).all(|(pd, td)| match_expr(p, pd, t, td, source, &mut candidate)) { return false; }
            if !match (pp, tp) {
                (Some(pp), Some(tp)) => match_pattern(p, *pp, t, *tp, source, &mut candidate),
                (None, None) => true,
                _ => false,
            } || !match_value_block(p, *pb, t, *tb, source, &mut candidate) { return false; }
            *bindings = candidate;
            true
        }
        (ArenaExprKind::Try(pe), ArenaExprKind::Try(te)) => {
            match_expr(p, *pe, t, *te, source, bindings)
        }
        (ArenaExprKind::Record(pfields), ArenaExprKind::Record(tfields)) => {
            use xsh::frontend::syntax::arena::ArenaRecordFieldKind as Field;
            let pfields = p.record_fields(*pfields);
            let tfields = t.record_fields(*tfields);
            if pfields.len() != tfields.len() { return false; }
            let mut local = bindings.clone();
            for (pf, tf) in pfields.iter().zip(tfields) {
                let matched = match (&pf.kind, &tf.kind) {
                    (Field::Computed { key: pk, value: pv, .. }, Field::Computed { key: tk, value: tv, .. }) => match_expr(p, *pk, t, *tk, source, &mut local) && match_expr(p, *pv, t, *tv, source, &mut local),
                    (Field::Path { path: pp, value: pv, .. }, Field::Path { path: tp, value: tv, .. }) => p.names(*pp).eq(t.names(*tp)) && match_expr(p, *pv, t, *tv, source, &mut local),
                    (Field::Named { name: pn, value: pv, .. }, Field::Named { name: tn, value: tv, .. }) => pn == tn && match_expr(p, *pv, t, *tv, source, &mut local),
                    (Field::Shorthand { name: pn, .. }, Field::Shorthand { name: tn, .. }) => pn == tn,
                    (Field::Spread { expr: pe, .. }, Field::Spread { expr: te, .. }) => match_expr(p, *pe, t, *te, source, &mut local),
                    _ => false,
                };
                if !matched { return false; }
            }
            *bindings = local; true
        }
        (ArenaExprKind::List(pi), ArenaExprKind::List(ti)) => {
            let pitems: Vec<_> = p.list_elements(*pi).collect();
            let titems: Vec<_> = t.list_elements(*ti).collect();
            if pitems.len() != titems.len() {
                return false;
            }
            let mut b2 = bindings.clone();
            for (pe, te) in pitems.into_iter().zip(titems) {
                if pe.splice_span.is_some() != te.splice_span.is_some()
                    || !match_expr(p, pe.value, t, te.value, source, &mut b2) {
                    return false;
                }
            }
            *bindings = b2;
            true
        }
        _ => false,
    }
}

fn match_pattern(p: &AstArena, pi: PatternId, t: &AstArena, ti: PatternId, source: &str, bindings: &mut FxHashMap<String, Span>) -> bool {
    use xsh::frontend::check::Type;
    match (&p.pattern(pi).kind, &t.pattern(ti).kind) {
        (ArenaPatternKind::Group(a), _) => match_pattern(p, *a, t, ti, source, bindings),
        (_, ArenaPatternKind::Group(b)) => match_pattern(p, pi, t, *b, source, bindings),
        (ArenaPatternKind::Alias { pattern: a, name: an, .. }, ArenaPatternKind::Alias { pattern: b, name: bn, .. }) => an == bn && match_pattern(p, *a, t, *b, source, bindings),
        (ArenaPatternKind::Wildcard, ArenaPatternKind::Wildcard) => true,
        (ArenaPatternKind::Binding(a), ArenaPatternKind::Binding(b)) | (ArenaPatternKind::Facet(a), ArenaPatternKind::Facet(b)) => a == b,
        (ArenaPatternKind::TestName { name: a, ty: at }, ArenaPatternKind::TestName { name: b, ty: bt }) => a == b && Type::from_arena(p, *at) == Type::from_arena(t, *bt),
        (ArenaPatternKind::Type { binding: a, ty: at }, ArenaPatternKind::Type { binding: b, ty: bt }) => a == b && Type::from_arena(p, *at) == Type::from_arena(t, *bt),
        (ArenaPatternKind::Literal(a), ArenaPatternKind::Literal(b)) => match_expr(p, *a, t, *b, source, bindings),
        (ArenaPatternKind::List { elements: a, rest: ar }, ArenaPatternKind::List { elements: b, rest: br }) => {
            let a: Vec<_> = p.pattern_ids(*a).collect();
            let b: Vec<_> = t.pattern_ids(*b).collect();
            a.len() == b.len() && a.into_iter().zip(b).all(|(a, b)| match_pattern(p, a, t, b, source, bindings))
                && match ar.zip(*br) { Some((a,b)) => match_pattern(p,a,t,b,source,bindings), None => ar.is_none() && br.is_none() }
        }
        (ArenaPatternKind::Record { fields: a, rest: ar }, ArenaPatternKind::Record { fields: b, rest: br }) => {
            let a = p.pattern_fields(*a);
            let b = t.pattern_fields(*b);
            ar == br && a.len() == b.len() && a.iter().zip(b).all(|(a,b)| a.name == b.name && match_pattern(p,a.pattern,t,b.pattern,source,bindings))
        }
        (ArenaPatternKind::Constructor { name: a, arg: aa }, ArenaPatternKind::Constructor { name: b, arg: ba }) => a == b && match aa.zip(*ba) { Some((a,b)) => match_pattern(p,a,t,b,source,bindings), None => aa.is_none() && ba.is_none() },
        (ArenaPatternKind::Tuple(a), ArenaPatternKind::Tuple(b)) | (ArenaPatternKind::Alternation(a), ArenaPatternKind::Alternation(b)) => {
            let a: Vec<_> = p.pattern_ids(*a).collect();
            let b: Vec<_> = t.pattern_ids(*b).collect();
            a.len() == b.len() && a.into_iter().zip(b).all(|(a,b)| match_pattern(p,a,t,b,source,bindings))
        }
        (ArenaPatternKind::ErrorVariant { family: af, variant: av, fields: a }, ArenaPatternKind::ErrorVariant { family: bf, variant: bv, fields: b }) => {
            let a = p.pattern_fields(*a); let b = t.pattern_fields(*b);
            af == bf && av == bv && a.len() == b.len() && a.iter().zip(b).all(|(a,b)| a.name == b.name && match_pattern(p,a.pattern,t,b.pattern,source,bindings))
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
    let pargs = p.call_args(pattern_args);
    let targs = t.call_args(target_args);
    if pargs.len() != targs.len() {
        return false;
    }
    let mut b2 = bindings.clone();
    for (pa, ta) in pargs.iter().zip(targs) {
        let same_position = match (&pa.kind, &ta.kind) {
            (ArenaCallArgKind::Positional(_), ArenaCallArgKind::Positional(_))
            | (ArenaCallArgKind::Splice { .. }, ArenaCallArgKind::Splice { .. })
            | (ArenaCallArgKind::NamedSpread { .. }, ArenaCallArgKind::NamedSpread { .. }) => true,
            (ArenaCallArgKind::Named { name: pn, .. }, ArenaCallArgKind::Named { name: tn, .. }) => pn == tn,
            _ => false,
        };
        if !same_position || !match_expr(p, call_arg_expr(pa), t, call_arg_expr(ta), source, &mut b2) {
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
        ArenaCallArgKind::Splice { value, .. } | ArenaCallArgKind::NamedSpread { value, .. } => *value,
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
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            let input = build_replacement_text(arena, *input, m, target_source, pattern_source)?;
            let call = build_replacement_text(arena, *call, m, target_source, pattern_source)?;
            Some(format!("{input} |> {call}"))
        }
        ArenaExprKind::Field { base, name } => {
            let base_text = build_replacement_text(arena, *base, m, target_source, pattern_source)?;
            Some(format!("{base_text}.{name}"))
        }
        ArenaExprKind::NullSafeField { base, name } => {
            let base = build_replacement_text(arena, *base, m, target_source, pattern_source)?;
            Some(format!("({base})?.{name}"))
        }
        ArenaExprKind::Try(inner) => {
            let inner = build_replacement_text(arena, *inner, m, target_source, pattern_source)?;
            Some(format!("({inner})?"))
        }
        ArenaExprKind::Call { callee, args } => {
            let callee_text =
                build_replacement_text(arena, *callee, m, target_source, pattern_source)?;
            let mut arg_parts = Vec::new();
            for arg in arena.call_args(*args) {
                let e = call_arg_expr(arg);
                let value = build_replacement_text(arena, e, m, target_source, pattern_source)?;
                arg_parts.push(match arg.kind {
                    ArenaCallArgKind::Positional(_) => value,
                    ArenaCallArgKind::Named { name, .. } => format!("{name}: {value}"),
                    ArenaCallArgKind::Splice { .. } => format!("@({value})"),
                    ArenaCallArgKind::NamedSpread { .. } => format!("...({value})"),
                });
            }
            Some(format!("{callee_text}({})", arg_parts.join(", ")))
        }
        ArenaExprKind::Record(fields) => {
            use xsh::frontend::syntax::arena::ArenaRecordFieldKind;
            let mut text = pattern_source.get(expr.span.start()..expr.span.end())?.to_string();
            let mut edits = Vec::new();
            for field in arena.record_fields(*fields) {
                let children = match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => vec![key, value],
                    ArenaRecordFieldKind::Named { value, .. } | ArenaRecordFieldKind::Path { value, .. } => vec![value],
                    ArenaRecordFieldKind::Spread { expr, .. } => vec![expr],
                    ArenaRecordFieldKind::Shorthand { .. } => Vec::new(),
                };
                for child in children {
                    let span = arena.expr(child).span;
                    let replacement = build_replacement_text(arena, child, m, target_source, pattern_source)?;
                    edits.push((span.start() - expr.span.start(), span.end() - expr.span.start(), replacement));
                }
            }
            edits.sort_unstable_by_key(|(start, _, _)| std::cmp::Reverse(*start));
            for (start, end, replacement) in edits { text.replace_range(start..end, &replacement); }
            Some(text)
        }
        ArenaExprKind::PatternTest { value, arms } => {
            let subject = build_replacement_text(arena, *value, m, target_source, pattern_source)?;
            let selected = arena.match_expr_arms(*arms).first()?;
            let pattern_span = arena.span(arena.pattern(selected.pattern).span);
            let pattern = pattern_source.get(pattern_span.range())?;
            Some(format!("({subject}) is {pattern}"))
        }
        ArenaExprKind::Retry { delays, block, .. } => {
            let mut text = pattern_source.get(expr.span.range())?.to_string();
            let mut children = arena.expr_ids(*delays).collect::<Vec<_>>();
            for statement in arena.stmt_ids(arena.block(*block).statements) {
                let xsh::frontend::syntax::arena::ArenaStmtKind::Expr(value) = arena.stmt(statement).kind else { return None };
                children.push(value);
            }
            children.sort_by_key(|child| std::cmp::Reverse(arena.expr(*child).span.start()));
            for child in children {
                let replacement = build_replacement_text(arena, child, m, target_source, pattern_source)?;
                let span = arena.expr(child).span;
                text.replace_range(span.start() - expr.span.start()..span.end() - expr.span.start(), &replacement);
            }
            Some(text)
        }
        ArenaExprKind::Match { value, arms } => {
            let mut text = pattern_source.get(expr.span.range())?.to_string();
            let mut edits = vec![(*value, build_replacement_text(arena, *value, m, target_source, pattern_source)?)];
            for arm in arena.match_expr_arms(*arms) {
                edits.push((arm.value, build_replacement_text(arena, arm.value, m, target_source, pattern_source)?));
                if let Some(guard) = arm.guard { edits.push((guard, build_replacement_text(arena, guard, m, target_source, pattern_source)?)); }
            }
            edits.sort_by_key(|(id, _)| std::cmp::Reverse(arena.expr(*id).span.start()));
            for (id, replacement) in edits {
                let span = arena.expr(id).span;
                text.replace_range(span.start() - expr.span.start()..span.end() - expr.span.start(), &replacement);
            }
            Some(text)
        }
        ArenaExprKind::ValueBlock(block) => {
            let mut replacements = Vec::new();
            for statement in arena.stmt_ids(arena.block(*block).statements) {
                let xsh::frontend::syntax::arena::ArenaStmtKind::Expr(value) = arena.stmt(statement).kind else { return None; };
                let value_span = arena.expr(value).span;
                let replacement = build_replacement_text(arena, value, m, target_source, pattern_source)?;
                replacements.push((value_span, replacement));
            }
            let mut text = pattern_source.get(expr.span.range())?.to_owned();
            for (span, replacement) in replacements.into_iter().rev() {
                text.replace_range(span.start() - expr.span.start()..span.end() - expr.span.start(), &replacement);
            }
            Some(text)
        }
        ArenaExprKind::Ident(name) => Some(name.to_string()),
        // For non-metavar, non-structural nodes: fall back to pattern source text.
        _ => {
            let text = pattern_source.get(expr.span.start()..expr.span.end())?;
            Some(text.to_string())
        }
    }
}

fn match_value_block(
    p: &AstArena, pb: BlockId, t: &AstArena, tb: BlockId,
    source: &str, bindings: &mut FxHashMap<String, Span>,
) -> bool {
    let ps = p.stmt_ids(p.block(pb).statements).collect::<Vec<_>>();
    let ts = t.stmt_ids(t.block(tb).statements).collect::<Vec<_>>();
    if ps.len() != ts.len() || !p.block(pb).params.is_empty() || !t.block(tb).params.is_empty() { return false; }
    let mut candidate = bindings.clone();
            for (ps, ts) in ps.into_iter().zip(ts) {
                match (p.stmt(ps).kind, t.stmt(ts).kind) {
                    (xsh::frontend::syntax::arena::ArenaStmtKind::Expr(pe), xsh::frontend::syntax::arena::ArenaStmtKind::Expr(te)) => {
                        if !match_expr(p, pe, t, te, source, &mut candidate) { return false; }
                    }
                    (xsh::frontend::syntax::arena::ArenaStmtKind::TailBareIdent(name), xsh::frontend::syntax::arena::ArenaStmtKind::Expr(te)) if is_metavar(name.as_str().as_str()) => {
                        let target = t.expr(te).span;
                        if let Some(previous) = candidate.get(name.as_str().as_str()) {
                            if source.get(previous.start()..previous.end()) != source.get(target.start()..target.end()) { return false; }
                        } else { candidate.insert(name.to_string(), target); }
                    }
                    (xsh::frontend::syntax::arena::ArenaStmtKind::TailBareIdent(a), xsh::frontend::syntax::arena::ArenaStmtKind::TailBareIdent(b)) if a == b => {}
                    _ => return false,
                }
            }
    *bindings = candidate;
    true
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
        } | ArenaStmtKind::Const {
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
