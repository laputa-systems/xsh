//! `lint.prefer-set`: a `Map[K, Bool]` whose values are only ever `true` is a
//! `Set[K]`, and the `true` says nothing.
//!
//! The fix is offered for a local binding the lint can account for in full:
//! declared empty (or from `set.empty()` or `set.from(items)`), written only
//! by storing `true` under a key or by the `set.add`/`set.remove` helpers,
//! and read only through `in`, `not in`, `len()`, `is_empty()`, and `keys()`.
//! A `Set[K]` answers each of those the same way, so the rewrite keeps the
//! program's behavior. Any other use (an index read, `get`, iteration, which
//! yields `{key, value}` items, or handing the map to something else) could
//! tell a stored `false` from a missing key, and gets advice instead.
//!
//! The declaration and every use change together or not at all, so the fix
//! is one replacement of the text from the declaration to the last use.

use std::collections::BTreeMap;
use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::check::Type;
use xsh::frontend::source::Span;
use xsh::frontend::symbols::{Name, Symbol};
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind,
    ArenaExprOrRun, ArenaStmtKind, ArenaTypeExprTag, AstArena, ExprId, StmtId, TypeExprId,
};
use xsh::frontend::syntax::node::{AssignOp, BinaryOp};

/// The map-as-set bindings of the linted module.
///
/// The linter's traversal reports each statement, expression, and type
/// expression it visits, in source order, so only the linted module is read.
/// Bindings are told apart by name inside one outermost statement (a function
/// or a top-level statement): the fix is withheld unless the uses counted
/// here are every occurrence of the name in that statement's text, which
/// rules out a shadowing binding, a punned field, an interpolation, and any
/// use the traversal reports in a form this rule does not read.
#[derive(Default)]
pub(super) struct SetLikeBindings {
    /// The outermost statement being traversed.
    region: Option<Span>,
    candidates: Vec<Candidate>,
    /// `Map[K, Bool]` annotations seen anywhere, for the advice.
    annotations: Vec<Span>,
    /// Identifier expressions already accounted for as a use of a candidate.
    accepted: Vec<ExprId>,
}

struct Candidate {
    name: Name,
    region: Span,
    /// Where the report points: the annotation, or the declared name.
    label: Span,
    /// The element type as written: the map's key type.
    element: String,
    /// The counted occurrences of the name.
    uses: usize,
    edits: Vec<(Span, String)>,
    /// A use a set could not stand in for, or one this rule does not read.
    fixable: bool,
}

/// The identifier a name expression spells.
fn ident(arena: &AstArena, expr: ExprId) -> Option<Name> {
    match arena.expr(expr).kind {
        ArenaExprKind::Ident(name) => Some(name),
        _ => None,
    }
}

/// `receiver.method(arguments)`.
fn method_call(arena: &AstArena, expr: ExprId) -> Option<(ExprId, Name, Vec<ExprId>)> {
    let ArenaExprKind::Call { callee, args } = arena.expr(expr).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    let arguments = arena
        .call_args(args)
        .iter()
        .map(|argument| match argument.kind {
            ArenaCallArgKind::Positional(value) => Some(value),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some((base, name, arguments))
}

/// `Map[Bool]` or `Map[K, Bool]`, and the key type as written.
fn bool_map_annotation(arena: &AstArena, source: &str, ty: TypeExprId) -> Option<String> {
    if arena.type_expr_tags[ty.index()] != ArenaTypeExprTag::Map {
        return None;
    }
    let data = arena.type_expr_data[ty.index()];
    let value = TypeExprId::from_index(data.lhs as usize);
    if arena.type_expr_tags[value.index()] != ArenaTypeExprTag::Named
        || Name::from_symbol(Symbol::from_raw(arena.type_expr_data[value.index()].lhs)) != "Bool"
    {
        return None;
    }
    match TypeExprId::from_optional_raw(data.rhs) {
        Some(key) => Some(source.get(arena.type_expr_span(key).range())?.to_owned()),
        None => Some("Str".to_owned()),
    }
}

impl SetLikeBindings {
    fn candidate(&mut self, name: Name) -> Option<&mut Candidate> {
        let region = self.region?;
        self.candidates
            .iter_mut()
            .find(|candidate| candidate.name == name && candidate.region == region)
    }

    /// A type expression the traversal reached.
    pub(super) fn visit_type(&mut self, arena: &AstArena, source: &str, ty: TypeExprId) {
        let span = arena.type_expr_span(ty);
        if bool_map_annotation(arena, source, ty).is_some() && !self.annotations.contains(&span) {
            self.annotations.push(span);
        }
    }

    /// A statement the traversal is about to descend into.
    pub(super) fn visit_stmt(
        &mut self,
        arena: &AstArena,
        source: &str,
        stmt_id: StmtId,
        expr_types: &BTreeMap<Span, Type>,
    ) {
        let stmt = arena.stmt(stmt_id);
        if self
            .region
            .is_none_or(|region| stmt.span.start() >= region.end())
        {
            self.region = Some(stmt.span);
        }
        match stmt.kind {
            ArenaStmtKind::Let {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            }
            | ArenaStmtKind::Var {
                target,
                ty,
                initializer: ArenaExprOrRun::Expr(initializer),
            } => self.declare(arena, source, stmt.span, target, ty, initializer, expr_types),
            ArenaStmtKind::Assign {
                target,
                op,
                value: ArenaExprOrRun::Expr(value),
            } => self.assign(arena, source, stmt.span, target, op, value),
            ArenaStmtKind::Assign { target, .. } => {
                if let Some(root) = super::lint_list_any_union::assigned_root(arena, target)
                    && let Some(candidate) = self.candidate(root)
                {
                    candidate.fixable = false;
                }
            }
            _ => {}
        }
    }

    fn declare(
        &mut self,
        arena: &AstArena,
        source: &str,
        stmt: Span,
        target: xsh::frontend::syntax::arena::BindingTargetId,
        ty: Option<TypeExprId>,
        initializer: ExprId,
        expr_types: &BTreeMap<Span, Type>,
    ) {
        let Some(region) = self.region else {
            return;
        };
        let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
            return;
        };
        // The name follows the `let` or `var` that starts the statement.
        let spelled = name.to_string();
        let Some(name_span) = source.get(stmt.range()).and_then(|text| {
            let rest = text
                .strip_prefix("let")
                .or_else(|| text.strip_prefix("var"))?;
            let rest = rest.strip_prefix(char::is_whitespace)?.trim_start();
            let start = stmt.start() + (text.len() - rest.len());
            rest.starts_with(&spelled)
                .then(|| Span::new(stmt.source_id, start, start + spelled.len()))
        }) else {
            return;
        };
        let initializer_span = arena.expr(initializer).span;
        let annotation = ty.and_then(|ty| Some((ty, bool_map_annotation(arena, source, ty)?)));
        // The legacy helpers return a string set whatever the annotation.
        let legacy = matches!(
            method_call(arena, initializer),
            Some((module, function, _))
                if ident(arena, module).is_some_and(|module| module == "set")
                    && (function == "empty" || function == "from")
        ) && matches!(
            expr_types.get(&initializer_span),
            Some(Type::Map(key, value)) if **key == Type::Str && **value == Type::Bool
        );
        // `{}` and `map.empty()`, however they are spaced.
        let empty = match arena.expr(initializer).kind {
            ArenaExprKind::Record(fields) => arena.record_fields(fields).is_empty(),
            _ => matches!(
                method_call(arena, initializer),
                Some((module, function, arguments))
                    if ident(arena, module).is_some_and(|module| module == "map")
                        && function == "empty"
                        && arguments.is_empty()
            ),
        };
        let mut edits = Vec::new();
        let (label, element) = match (annotation, ty) {
            (Some((ty, element)), _) if empty || legacy => {
                let span = arena.type_expr_span(ty);
                edits.push((span, format!("Set[{element}]")));
                (span, element)
            }
            (None, None) if legacy => {
                let end = Span::new(name_span.source_id, name_span.end(), name_span.end());
                edits.push((end, ": Set[Str]".to_owned()));
                (name_span, "Str".to_owned())
            }
            _ => return,
        };
        if empty {
            edits.push((initializer_span, "set.empty()".to_owned()));
        }
        // A second binding of the name in the region cannot be told from
        // the first by its text.
        let duplicate = self.candidate(name).map(|earlier| earlier.fixable = false).is_some();
        self.candidates.push(Candidate {
            name,
            region,
            label,
            element,
            uses: 1,
            edits,
            fixable: !duplicate,
        });
    }

    fn assign(
        &mut self,
        arena: &AstArena,
        source: &str,
        stmt: Span,
        target: xsh::frontend::syntax::arena::AssignTargetId,
        op: AssignOp,
        value: ExprId,
    ) {
        let Some(root) = super::lint_list_any_union::assigned_root(arena, target) else {
            return;
        };
        if self.candidate(root).is_none() {
            return;
        }
        let name = root.to_string();
        let value_span = arena.expr(value).span;
        let text = |expr: ExprId| source.get(arena.expr(expr).span.range()).map(str::to_owned);
        // The edit, and the identifier inside the value that it accounts for.
        let mut accepted = None;
        let edit = match arena.assign_target(target).kind {
            // `seen[key] = true`
            ArenaAssignTargetKind::Index { base, index }
                if op == AssignOp::Set
                    && matches!(arena.assign_target(base).kind, ArenaAssignTargetKind::Name(_))
                    && matches!(arena.expr(value).kind, ArenaExprKind::Bool(true)) =>
            {
                text(index).map(|key| {
                    (
                        Span::new(stmt.source_id, stmt.start(), value_span.end()),
                        format!("{name} = {name}.add({key})"),
                    )
                })
            }
            ArenaAssignTargetKind::Name(_) if op == AssignOp::Set => {
                match method_call(arena, value) {
                    // `seen = set.add(seen, key)` and `seen = set.remove(seen, key)`
                    Some((module, function, arguments))
                        if ident(arena, module).is_some_and(|module| module == "set")
                            && (function == "add" || function == "remove")
                            && arguments.len() == 2
                            && ident(arena, arguments[0]) == Some(root) =>
                    {
                        accepted = Some(arguments[0]);
                        text(arguments[1])
                            .map(|key| (value_span, format!("{name}.{function}({key})")))
                    }
                    // `seen = seen.set(key, true)`
                    Some((receiver, function, arguments))
                        if ident(arena, receiver) == Some(root)
                            && function == "set"
                            && arguments.len() == 2
                            && matches!(
                                arena.expr(arguments[1]).kind,
                                ArenaExprKind::Bool(true)
                            ) =>
                    {
                        accepted = Some(receiver);
                        text(arguments[0]).map(|key| (value_span, format!("{name}.add({key})")))
                    }
                    // `seen = seen.remove(key)` reads the same on a set.
                    Some((receiver, function, arguments))
                        if ident(arena, receiver) == Some(root)
                            && function == "remove"
                            && arguments.len() == 1 =>
                    {
                        accepted = Some(receiver);
                        Some((Span::new(value_span.source_id, value_span.end(), value_span.end()), String::new()))
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(accepted) = accepted {
            self.accepted.push(accepted);
        }
        let candidate = self.candidate(root).expect("the candidate was found above");
        match edit {
            Some((span, replacement))
                if !super::span_may_contain_comment(source, span) =>
            {
                // The target, and the receiver or argument inside the value.
                candidate.uses += 1 + usize::from(accepted.is_some());
                if !span.is_empty() {
                    candidate.edits.push((span, replacement));
                }
            }
            _ => candidate.fixable = false,
        }
    }

    /// An expression the traversal is about to descend into.
    pub(super) fn visit_expr(&mut self, arena: &AstArena, source: &str, expr: ExprId) {
        match arena.expr(expr).kind {
            // `key in seen` and `key not in seen`
            ArenaExprKind::Binary {
                op: BinaryOp::In | BinaryOp::NotIn,
                right,
                ..
            } => {
                if let Some(name) = ident(arena, right)
                    && let Some(candidate) = self.candidate(name)
                {
                    candidate.uses += 1;
                    self.accepted.push(right);
                }
            }
            ArenaExprKind::Call { .. } => {
                let Some((receiver, method, arguments)) = method_call(arena, expr) else {
                    return;
                };
                let Some(name) = ident(arena, receiver) else {
                    return;
                };
                if !arguments.is_empty() || self.accepted.contains(&receiver) {
                    return;
                }
                let span = arena.expr(expr).span;
                let Some(candidate) = self.candidate(name) else {
                    return;
                };
                if method == "len" || method == "is_empty" {
                    candidate.uses += 1;
                } else if method == "keys"
                    && source
                        .get(span.range())
                        .is_some_and(|call| call.ends_with("keys()"))
                {
                    // The keys of the map are the elements of the set.
                    candidate.uses += 1;
                    candidate.edits.push((
                        Span::new(span.source_id, span.end() - "keys()".len(), span.end()),
                        "to_list()".to_owned(),
                    ));
                } else {
                    return;
                }
                self.accepted.push(receiver);
            }
            ArenaExprKind::Ident(name) => {
                if !self.accepted.contains(&expr)
                    && let Some(candidate) = self.candidate(name)
                {
                    candidate.fixable = false;
                }
            }
            _ => {}
        }
    }

    /// The reports, in source order: a fix for each binding that is provably
    /// a set, and with `advice` a note for every other `Map[K, Bool]`.
    pub(super) fn finish(self, source: &str, advice: bool) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut fixed = Vec::new();
        for candidate in self.candidates {
            let Some(fix) = candidate.fix(source) else {
                if advice && !self.annotations.contains(&candidate.label) {
                    diagnostics.push(advise(candidate.label, &candidate.element));
                }
                continue;
            };
            fixed.push(candidate.label);
            diagnostics.push(
                Diagnostic::warning(format!(
                    "`{}` is a map used as a set; its values are only ever `true`",
                    candidate.name
                ))
                .with_code(DiagnosticCode::LintPreferSet)
                .with_label(Label::secondary(
                    candidate.label,
                    format!("this is a `Set[{}]`", candidate.element),
                ))
                .with_fix_hint(fix),
            );
        }
        if advice {
            for annotation in self.annotations {
                if !fixed.contains(&annotation) {
                    let element = source
                        .get(annotation.range())
                        .and_then(|text| text.strip_prefix("Map["))
                        .and_then(|text| text.strip_suffix(']'))
                        .and_then(|text| text.rsplit_once(','))
                        .map_or("Str", |(key, _)| key.trim())
                        .to_owned();
                    diagnostics.push(advise(annotation, &element));
                }
            }
        }
        diagnostics.sort_by_key(|diagnostic| diagnostic.labels[0].span.start());
        diagnostics
    }
}

fn advise(span: Span, element: &str) -> Diagnostic {
    Diagnostic::warning("a map with `Bool` values may be a set")
        .with_code(DiagnosticCode::LintPreferSet)
        .with_label(Label::secondary(
            span,
            format!("if only `true` is ever stored, this is a `Set[{element}]`"),
        ))
        .with_note(
            "a set has `in`, `|`, `&`, `-`, `.add`, and `.remove`; keep the map when a stored `false` means something different from a missing key",
        )
}

impl Candidate {
    /// One replacement of the text from the first edit to the last, or
    /// `None` when the binding is not provably a set.
    fn fix(&self, source: &str) -> Option<FixHint> {
        if !self.fixable {
            return None;
        }
        // Every occurrence of the name in the region is a use counted above.
        let region = source.get(self.region.range())?;
        let name = self.name.to_string();
        let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-';
        let occurrences = region
            .match_indices(&name)
            .filter(|(start, _)| {
                let end = start + name.len();
                !region[..*start].bytes().next_back().is_some_and(word)
                    && !region[end..].bytes().next().is_some_and(word)
            })
            .count();
        if occurrences != self.uses {
            return None;
        }
        let mut edits = self.edits.clone();
        edits.sort_by_key(|(span, _)| (span.start(), span.end()));
        if edits
            .windows(2)
            .any(|pair| pair[0].0.end() > pair[1].0.start())
        {
            return None;
        }
        let (first, last) = (edits.first()?.0, edits.last()?.0);
        let span = Span::new(first.source_id, first.start(), last.end());
        let mut replacement = String::new();
        let mut cursor = span.start();
        for (edit, text) in &edits {
            replacement.push_str(source.get(cursor..edit.start())?);
            replacement.push_str(text);
            cursor = edit.end();
        }
        Some(FixHint::replacement(
            span,
            format!("make `{name}` a `Set[{}]`", self.element),
            replacement,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn lint(source: &str, advice: bool) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                prefer_set: advice,
                ..LintOptions::default()
            },
        )
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferSet))
        .collect()
    }

    /// Applies the fixes as the fixer does: one at a time, each to a file
    /// linted afresh, since two bindings used side by side have fixes that
    /// overlap and only one of those is applied per pass.
    fn fix_all(source: &str) -> String {
        let mut fixed = source.to_owned();
        while let Some(fix) = lint(&fixed, false)
            .first()
            .and_then(|diagnostic| diagnostic.fix_hints.first().cloned())
        {
            fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_deref().unwrap());
        }
        fixed
    }

    #[test]
    fn a_local_map_of_true_becomes_a_set_in_one_fix() {
        let source = "pure distinct(words: List[Str]) -> List[Str] {\n  var seen: Map[Bool] = {}\n  var ids: Map[Int, Bool] = map.empty()\n  for word in words {\n    if word not in seen {\n      seen[word] = true\n      ids[word.byte_len()] = true\n    }\n  }\n\n  if seen.is_empty() or ids.len() > 3 {\n    return []\n  }\n\n  seen.keys()\n}\n";
        let diagnostics = lint(source, false);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.fix_hints.len() == 1));
        let fixed = fix_all(source);
        assert_eq!(
            fixed,
            "pure distinct(words: List[Str]) -> List[Str] {\n  var seen: Set[Str] = set.empty()\n  var ids: Set[Int] = set.empty()\n  for word in words {\n    if word not in seen {\n      seen = seen.add(word)\n      ids = ids.add(word.byte_len())\n    }\n  }\n\n  if seen.is_empty() or ids.len() > 3 {\n    return []\n  }\n\n  seen.to_list()\n}\n"
        );
        // The fixed program checks and is not reported again.
        assert!(lint(&fixed, true).is_empty());
    }

    #[test]
    fn the_legacy_set_helpers_become_set_methods() {
        let source = "pure known(words: List[Str], word: Str) -> Bool {\n  var seen = set.from(words)\n  var gone = set.empty()\n  seen = set.add(seen, \"extra\")\n  seen = set.remove(seen, word)\n  gone = gone.set(word, true)\n  gone = gone.remove(\"extra\")\n  word in seen or word in gone\n}\n";
        let diagnostics = lint(source, false);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        let fixed = fix_all(source);
        assert_eq!(
            fixed,
            "pure known(words: List[Str], word: Str) -> Bool {\n  var seen: Set[Str] = set.from(words)\n  var gone: Set[Str] = set.empty()\n  seen = seen.add(\"extra\")\n  seen = seen.remove(word)\n  gone = gone.add(word)\n  gone = gone.remove(\"extra\")\n  word in seen or word in gone\n}\n"
        );
        assert!(lint(&fixed, true).is_empty());
    }

    /// A read that could tell `false` from a missing key, a stored `false`,
    /// a map that leaves the function, and a name the text uses in a way the
    /// rule does not count all keep the map: advice when asked, never a fix.
    #[test]
    fn a_map_that_is_not_provably_a_set_gets_advice_only() {
        let source = "pure flags(words: List[Str], known: Map[Bool]) -> Map[Bool] {\n  var read: Map[Bool] = {}\n  var stored: Map[Bool] = {}\n  var returned: Map[Bool] = {}\n  var punned: Map[Bool] = {}\n  for word in words {\n    read[word] = true\n    stored[word] = word in known\n    returned[word] = true\n    punned[word] = true\n  }\n\n  let first = read[\"a\"]\n  let record = {punned}\n  if first and record.punned.is_empty() and stored.is_empty() {\n    return {}\n  }\n\n  returned\n}\n";
        assert!(lint(source, false).is_empty());
        let advice = lint(source, true);
        // Two in the signature and one for each of the four bindings.
        assert_eq!(advice.len(), 6, "{advice:?}");
        assert!(advice.iter().all(|diagnostic| diagnostic.fix_hints.is_empty()));
    }

    #[test]
    fn two_bindings_of_one_name_are_left_alone() {
        let source = "pure both(words: List[Str]) -> Bool {\n  var seen: Map[Bool] = {}\n  for word in words {\n    seen[word] = true\n  }\n\n  let again = if words.is_empty() {\n    var seen: Map[Bool] = {}\n    seen[\"x\"] = true\n    \"x\" in seen\n  } else {\n    false\n  }\n  again and \"y\" in seen\n}\n";
        assert!(lint(source, false).is_empty());
    }
}
