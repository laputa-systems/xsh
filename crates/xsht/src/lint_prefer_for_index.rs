use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::check::Type;
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaExprKind, ArenaExprOrRun, ArenaStmtKind,
    ArenaSugar, AstArena, BlockId, ExprId, StmtId,
};
use xsh::frontend::syntax::node::{AssignOp, BinaryOp};
use xsh::frontend::syntax::parser::Parser;

/// A counter that only walks a list:
///
/// ```text
/// var I = 0
/// ...
/// while I < LIST.len() {
///   let ITEM = LIST[I]
///   ...
///   I += 1
/// }
/// ```
///
/// is `for I, ITEM in LIST { ... }`, or `for ITEM in LIST { ... }` when the
/// rest of the body never reads `I`, or `for ITEM in LIST[START..] { ... }`
/// when it also starts past zero. The loop is reported only when the two are
/// the same program:
///
/// - `LIST` is a `List` that nothing can assign while the loop runs: it is not
///   a `var`. A `for` iterates a snapshot, and the `while` reads the list
///   again on every pass.
/// - The body assigns `I` only in its last statement and has no `continue`,
///   which would skip that statement.
/// - Nothing between the binding of `I` and the loop, and nothing after the
///   loop, mentions `I`, whose binding the rewrite removes.
///
/// `block` is the block whose statements these are. The statements of a file
/// have none and are never reported: a top-level `var` is visible to every
/// function of the module.
pub(super) fn lint_counter_loops(
    linter: &mut super::Linter<'_>,
    stmts: &[StmtId],
    block: Option<BlockId>,
) {
    let Some(block) = block else {
        return;
    };
    for index in 1..stmts.len() {
        let Some(counter) = counter_loop(linter, stmts, index, block) else {
            continue;
        };
        let source = linter.source;
        let while_span = linter.arena.stmt(stmts[index]).span;
        let head = Span::new(
            while_span.source_id,
            while_span.start(),
            linter.arena.expr(counter.condition).span.end(),
        );
        let form = counter.head(source);
        let mut diagnostic = Diagnostic::new(
            Severity::Warning,
            "prefer a `for` loop over a counter that only walks a list",
        )
        .with_code(DiagnosticCode::LintPreferForIndex)
        .with_label(Label::secondary(
            head,
            format!("this loop walks the list by index; write `{form} ... }}`"),
        ));
        if let Some(fix) = counter.rewrite(linter, stmts[index], &form) {
            diagnostic = diagnostic.with_fix_hint(fix);
        }
        linter.diagnostics.push(diagnostic);
    }
}

struct CounterLoop {
    /// The `var I = START` statement.
    binding: StmtId,
    /// How many statements stand between the binding and the loop.
    between: usize,
    /// Whether the binding is the first statement of its block.
    binding_opens_block: bool,
    counter: Name,
    /// The literal the counter starts from, as written.
    start: Span,
    list: Name,
    item: Name,
    condition: ExprId,
    body: BlockId,
    /// The body's statements between the item binding and the increment.
    middle: Vec<StmtId>,
    /// Whether those statements mention the counter.
    reads_counter: bool,
}

impl CounterLoop {
    /// The head of the `for` loop, through its `{`.
    fn head(&self, source: &str) -> String {
        let start = &source[self.start.range()];
        if self.reads_counter {
            format!("for {}, {} in {} {{", self.counter, self.item, self.list)
        } else if start == "0" {
            format!("for {} in {} {{", self.item, self.list)
        } else {
            format!("for {} in {}[{start}..] {{", self.item, self.list)
        }
    }

    /// The replacement of everything from the counter binding through the
    /// `while`, when the text can be moved as it is: the binding has a line
    /// to itself, the loop and the body's statements each start their own
    /// line, no comment sits in the lines the rewrite drops, and the result
    /// parses to the statements in between and a `for` over the same body.
    fn rewrite(
        &self,
        linter: &super::Linter<'_>,
        while_stmt: StmtId,
        head: &str,
    ) -> Option<FixHint> {
        let arena = linter.arena;
        let source = linter.source;
        let binding_span = arena.stmt(self.binding).span;
        let while_span = arena.stmt(while_stmt).span;
        let line_start = |offset: usize| source[..offset].rfind('\n').map_or(0, |at| at + 1);
        let line_end =
            |offset: usize| source[offset..].find('\n').map_or(source.len(), |at| offset + at);
        let blank = |start: usize, end: usize| {
            source
                .get(start..end)
                .is_some_and(|text| text.chars().all(char::is_whitespace))
        };
        let own_line = |offset: usize| blank(line_start(offset), offset);

        let indent = source.get(line_start(binding_span.start())..binding_span.start())?;
        if !indent.chars().all(|ch| ch == ' ')
            || source.get(line_start(while_span.start())..while_span.start()) != Some(indent)
        {
            return None;
        }
        let first = *self.middle.first()?;
        let last = *self.middle.last()?;
        let kept_start = line_start(arena.stmt(first).span.start());
        let kept_end = line_end(text_end_of(source, arena.stmt(last).span)?);
        let while_end = text_end_of(source, while_span)?;
        let increment = arena
            .stmt_ids(arena.block(self.body).statements)
            .last()
            .map(|stmt| arena.stmt(stmt).span)?;
        let increment_end = text_end_of(source, increment)?;
        if !self.middle.iter().all(|stmt| own_line(arena.stmt(*stmt).span.start()))
            || !own_line(increment.start())
            || kept_end > increment.start()
            // Only the closing brace follows the increment.
            || source.get(increment_end..while_end)?.trim() != "}"
        {
            return None;
        }
        // The binding's line goes. A blank line after it goes too when it
        // would otherwise open the block or follow another blank line.
        let binding_line = line_start(binding_span.start());
        let binding_line_end = line_end(text_end_of(source, binding_span)?);
        if !blank(text_end_of(source, binding_span)?, binding_line_end)
            || binding_line_end >= while_span.start()
        {
            return None;
        }
        let mut between_start = binding_line_end + 1;
        let next_line_end = line_end(between_start);
        let after_blank_line = binding_line >= 2
            && blank(line_start(binding_line - 1), binding_line - 1);
        if next_line_end < while_span.start()
            && blank(between_start, next_line_end)
            && (self.binding_opens_block || after_blank_line)
        {
            between_start = next_line_end + 1;
        }
        let between = source.get(between_start..line_start(while_span.start()))?;
        let dropped_head = Span::new(binding_span.source_id, while_span.start(), kept_start);
        let dropped_tail = Span::new(binding_span.source_id, kept_end, while_end);
        if super::span_may_contain_comment(source, dropped_head)
            || super::span_may_contain_comment(source, dropped_tail)
        {
            return None;
        }
        let replacement = format!(
            "{between}{indent}{head}\n{}\n{indent}}}",
            source.get(kept_start..kept_end)?
        );

        // The rewrite is offered only when the replacement, parsed on its
        // own, is the statements in between and then one loop whose body is,
        // node for node, the statements between the item binding and the
        // increment.
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), &replacement);
        if !parsed.diagnostics.is_empty() {
            return None;
        }
        let rewritten = &parsed.arena.arena;
        let statements = parsed.arena.statement_ids().collect::<Vec<_>>();
        if statements.len() != self.between + 1 {
            return None;
        }
        let statement = *statements.last()?;
        let new_body = match rewritten.stmt(statement).kind {
            ArenaStmtKind::Sugar { form, operands, .. } if self.reads_counter => {
                match rewritten.sugar(form, operands) {
                    ArenaSugar::ForIndex { body, .. } => body,
                    _ => return None,
                }
            }
            ArenaStmtKind::For { block, .. } if !self.reads_counter => block,
            _ => return None,
        };
        if rewritten.stmt_ids(rewritten.block(new_body).statements).count() != self.middle.len() {
            return None;
        }
        let old_key = super::super::format::canonical_subtree(arena, source, Ok(self.body));
        let new_key = parsed.arena.symbol_owner().with_current(|| {
            super::super::format::canonical_subtree(rewritten, &replacement, Ok(new_body))
        });
        // A block's key is `B(|` and its statements' keys in order, so the new
        // body holds the middle statements exactly when its statements' keys
        // sit inside the old body's, after the item binding's and before the
        // increment's.
        let moved = new_key.strip_prefix("B(|")?.strip_suffix(')')?;
        let old = old_key.strip_prefix("B(|")?.strip_suffix(')')?;
        let at = old.find(moved)?;
        if moved.is_empty() || at == 0 || at + moved.len() == old.len() {
            return None;
        }
        Some(FixHint::replacement(
            Span::new(binding_span.source_id, binding_line, while_end),
            "rewrite as a `for` loop",
            replacement,
        ))
    }
}

fn text_end_of(source: &str, statement: Span) -> Option<usize> {
    let text = source.get(statement.range())?;
    Some(statement.start() + text.trim_end().len())
}

/// Whether `stmt` may bind `name`. Which names a destructuring binds is not
/// worth reading here, so it counts as binding every name.
fn may_bind(arena: &AstArena, stmt: StmtId, name: Name) -> bool {
    let (ArenaStmtKind::Let { target, .. }
    | ArenaStmtKind::Var { target, .. }
    | ArenaStmtKind::Const { target, .. }) = arena.stmt(stmt).kind
    else {
        return false;
    };
    match arena.binding_target(target).kind {
        ArenaBindingTargetKind::Name(bound) => bound == name,
        ArenaBindingTargetKind::Record { .. } => true,
    }
}

fn is_name(arena: &AstArena, expr: ExprId, name: Name) -> bool {
    matches!(arena.expr(expr).kind, ArenaExprKind::Ident(found) if found == name)
}

/// Whether `text` assigns the variable spelled `name`. An assignment is
/// written as the name and then its operator, so text without that spelling
/// has none; a match inside a string or comment only errs toward `true`.
fn may_assign(text: &str, name: &str) -> bool {
    let identifier_char = |ch: char| ch.is_alphanumeric() || ch == '_';
    text.match_indices(name).any(|(start, _)| {
        let after = &text[start + name.len()..];
        if text[..start].chars().next_back().is_some_and(identifier_char)
            || after.chars().next().is_some_and(identifier_char)
        {
            return false;
        }
        let rest = after.trim_start_matches([' ', '\t']);
        let operator = rest
            .strip_prefix(['+', '-', '*', '/', '%'])
            .unwrap_or(rest);
        rest.starts_with("++")
            || rest.starts_with("--")
            || (operator.starts_with('=') && !operator.starts_with("=="))
    })
}

/// The counter loop that the `while` at `stmts[index]` and an earlier
/// binding of its counter form, when it is one a `for` loop replaces exactly.
fn counter_loop(
    linter: &super::Linter<'_>,
    stmts: &[StmtId],
    index: usize,
    block: BlockId,
) -> Option<CounterLoop> {
    let arena = linter.arena;
    let source = linter.source;
    let ArenaStmtKind::While {
        condition,
        block: body,
    } = arena.stmt(stmts[index]).kind
    else {
        return None;
    };
    let ArenaExprKind::Binary {
        op: BinaryOp::Lt,
        left,
        right,
    } = arena.expr(condition).kind
    else {
        return None;
    };
    let ArenaExprKind::Ident(counter) = arena.expr(left).kind else {
        return None;
    };
    let counter_text = counter.as_str();
    let counter_text = counter_text.as_str();
    // The counter is the nearest earlier binding of its name in this block,
    // a `var` that starts from a literal, and nothing mentions it before the
    // loop.
    let bound_at = stmts[..index]
        .iter()
        .rposition(|stmt| may_bind(arena, *stmt, counter))?;
    let binding = stmts[bound_at];
    let ArenaStmtKind::Var {
        target,
        ty: None,
        initializer: ArenaExprOrRun::Expr(start),
    } = arena.stmt(binding).kind
    else {
        return None;
    };
    if !matches!(
        arena.binding_target(target).kind,
        ArenaBindingTargetKind::Name(_)
    ) {
        return None;
    }
    let start = arena.expr(start).span;
    let start_text = source.get(start.range())?;
    if start_text.is_empty()
        || !start_text.chars().all(|ch| ch.is_ascii_digit())
        || (start_text.len() > 1 && start_text.starts_with('0'))
    {
        return None;
    }
    let before = source
        .get(arena.stmt(binding).span.end()..arena.stmt(stmts[index]).span.start())?;
    if super::mentions_identifier(before, counter_text) {
        return None;
    }
    let ArenaExprKind::Call { callee, args } = arena.expr(right).kind else {
        return None;
    };
    let ArenaExprKind::Field { base, name } = arena.expr(callee).kind else {
        return None;
    };
    let ArenaExprKind::Ident(list) = arena.expr(base).kind else {
        return None;
    };
    if name != "len" || !arena.call_args(args).is_empty() || list == counter {
        return None;
    }
    if !matches!(
        linter.expr_types.get(&arena.expr(base).span),
        Some(Type::List(_))
    ) {
        return None;
    }
    // The list is a binding nothing assigns: one this block made with `let`
    // or `const`, or an immutable one of an enclosing scope.
    let immutable = match stmts[..index]
        .iter()
        .rev()
        .find(|stmt| may_bind(arena, **stmt, list))
    {
        Some(stmt) => !matches!(arena.stmt(*stmt).kind, ArenaStmtKind::Var { .. }),
        None => linter
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(list.as_str().as_str()))
            .is_some_and(|binding| !binding.mutable),
    };
    if !immutable {
        return None;
    }

    let body_stmts: Vec<StmtId> = arena.stmt_ids(arena.block(body).statements).collect();
    let [first, middle @ .., last] = &body_stmts[..] else {
        return None;
    };
    if middle.is_empty() {
        return None;
    }
    let ArenaStmtKind::Let {
        target,
        ty: None,
        initializer: ArenaExprOrRun::Expr(element),
    } = arena.stmt(*first).kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(item) = arena.binding_target(target).kind else {
        return None;
    };
    let ArenaExprKind::Index {
        base,
        index: position,
        guarded: false,
    } = arena.expr(element).kind
    else {
        return None;
    };
    if !is_name(arena, base, list)
        || !is_name(arena, position, counter)
        || item == counter
        || item == list
        || item == "_"
    {
        return None;
    }
    let ArenaStmtKind::Assign {
        target,
        op: AssignOp::Add,
        value: ArenaExprOrRun::Expr(step),
    } = arena.stmt(*last).kind
    else {
        return None;
    };
    if !matches!(arena.assign_target(target).kind, ArenaAssignTargetKind::Name(name) if name == counter)
        || source.get(arena.expr(step).span.range()) != Some("1")
    {
        return None;
    }

    let middle_text = source.get(
        arena.stmt(*middle.first()?).span.start()..arena.stmt(*middle.last()?).span.end(),
    )?;
    if may_assign(middle_text, counter_text)
        || super::mentions_identifier(middle_text, "continue")
    {
        return None;
    }
    let after = source.get(
        arena.stmt(stmts[index]).span.end()..arena.span(arena.block(block).span).end(),
    )?;
    if super::mentions_identifier(after, counter_text) {
        return None;
    }
    let reads_counter = super::mentions_identifier(middle_text, counter_text);
    if reads_counter && start_text != "0" {
        return None;
    }
    Some(CounterLoop {
        binding,
        between: index - bound_at - 1,
        binding_opens_block: bound_at == 0,
        counter,
        start,
        list,
        item,
        condition,
        body,
        middle: middle.to_vec(),
        reads_counter,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn counter_loops(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let checked = Checker::check_arena(&parsed.arena, source);
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        Linter::lint(
            &parsed.arena,
            source,
            LintOptions {
                expr_types: checked.expr_types,
                ..LintOptions::default()
            },
        )
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferForIndex))
        .collect()
    }

    fn apply(diagnostics: &[Diagnostic], source: &str) -> String {
        let mut fixes = diagnostics
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .collect::<Vec<_>>();
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
        let mut fixed = source.to_owned();
        for fix in fixes {
            fixed.replace_range(
                fix.span.unwrap().range(),
                fix.replacement.as_deref().unwrap(),
            );
        }
        fixed
    }

    #[test]
    fn a_counter_the_body_reads_becomes_an_indexed_for() {
        let source = "proc show(names: List[Str]) [io] {\n  print \"names\"\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n\n    print f\"{i}: {name}\" # numbered\n    if name == \"\" { break }\n\n    i += 1\n  }\n\n  print \"done\"\n}\n";
        let diagnostics = counter_loops(source);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc show(names: List[Str]) [io] {\n  print \"names\"\n  for i, name in names {\n    print f\"{i}: {name}\" # numbered\n    if name == \"\" { break }\n  }\n\n  print \"done\"\n}\n"
        );
        // The fixed program checks and is not reported again.
        assert!(counter_loops(&fixed).is_empty());
    }

    #[test]
    fn a_counter_the_body_ignores_becomes_a_plain_for_or_a_slice() {
        let source = "proc show(names: List[Str]) [io] {\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n    print $name\n    i += 1\n  }\n\n  var at = 2\n  while at < names.len() {\n    let name = names[at]\n    print $name\n    at += 1\n  }\n}\n";
        let diagnostics = counter_loops(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc show(names: List[Str]) [io] {\n  for name in names {\n    print $name\n  }\n\n  for name in names[2..] {\n    print $name\n  }\n}\n"
        );
        assert!(counter_loops(&fixed).is_empty());
    }

    /// The counter may be bound anywhere before the loop in its block. Its
    /// line goes, with the blank line after it when that line would open the
    /// block or double a blank line; everything in between stays as written.
    #[test]
    fn a_counter_bound_earlier_in_the_block_is_removed_where_it_stands() {
        let source = "proc show(names: List[Str]) [io] {\n  var i = 0\n\n  var shown = 0\n  # every name\n\n  while i < names.len() {\n    let name = names[i]\n    shown += i\n    print $name\n    i += 1\n  }\n\n  print $shown\n\n  var at = 0\n\n  while at < names.len() {\n    let name = names[at]\n    print $name\n    at += 1\n  }\n}\n";
        let diagnostics = counter_loops(source);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
        let fixed = apply(&diagnostics, source);
        assert_eq!(
            fixed,
            "proc show(names: List[Str]) [io] {\n  var shown = 0\n  # every name\n\n  for i, name in names {\n    shown += i\n    print $name\n  }\n\n  print $shown\n\n  for name in names {\n    print $name\n  }\n}\n"
        );
        assert!(counter_loops(&fixed).is_empty());
    }

    #[test]
    fn nested_counter_loops_are_each_reported() {
        let source = "proc show(rows: List[List[Int]]) [io] {\n  var r = 0\n  while r < rows.len() {\n    let row = rows[r]\n    var c = 0\n    while c < row.len() {\n      let cell = row[c]\n      print $r $c $cell\n      c += 1\n    }\n\n    r += 1\n  }\n}\n";
        assert_eq!(counter_loops(source).len(), 2);
    }

    /// Each of these loops differs from a `for` in something a program can
    /// observe, so none is reported.
    #[test]
    fn loops_a_for_would_change_are_left_alone() {
        for (why, source) in [
            (
                "the counter is read after the loop",
                "proc show(names: List[Str]) [io] {\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n    print $name\n    i += 1\n  }\n\n  print $i\n}\n",
            ),
            (
                "the counter is read before the loop",
                "proc show(names: List[Str]) [io] {\n  var i = 0\n  let first = i\n  while i < names.len() {\n    let name = names[i]\n    print $name\n    i += 1\n  }\n\n  print $first\n}\n",
            ),
            (
                "the body assigns the counter again",
                "proc show(names: List[Str]) [io] {\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n    if name == \"skip\" { i += 1 }\n    print $name\n    i += 1\n  }\n}\n",
            ),
            (
                "a `continue` skips the increment",
                "proc show(names: List[Str]) [io] {\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n    if name == \"\" { continue }\n    print $name\n    i += 1\n  }\n}\n",
            ),
            (
                "the list is a variable the body may assign",
                "proc show(given: List[Str]) [io] {\n  var names = given\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n    names += [name]\n    i += 1\n  }\n}\n",
            ),
            (
                "the counter starts past zero and the body reads it",
                "proc show(names: List[Str]) [io] {\n  var i = 1\n  while i < names.len() {\n    let name = names[i]\n    print $i $name\n    i += 1\n  }\n}\n",
            ),
            (
                "the subject is not a list",
                "proc show(names: Map[Int, Str]) [io] {\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n    print $name\n    i += 1\n  }\n}\n",
            ),
            (
                "the step is not one",
                "proc show(names: List[Str]) [io] {\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n    print $name\n    i += 2\n  }\n}\n",
            ),
        ] {
            assert!(counter_loops(source).is_empty(), "{why}");
        }
    }

    /// The loop is still reported when its text cannot be moved as it is, but
    /// it gets no fix that would drop a comment or join two statements.
    #[test]
    fn a_comment_in_a_dropped_line_leaves_the_rewrite_to_the_author() {
        for source in [
            "proc show(names: List[Str]) [io] {\n  var i = 0 # position\n  while i < names.len() {\n    let name = names[i]\n    print $name\n    i += 1\n  }\n}\n",
            "proc show(names: List[Str]) [io] {\n  var i = 0\n  while i < names.len() {\n    let name = names[i]\n    print $name\n    i += 1 # next\n  }\n}\n",
            "proc show(names: List[Str]) [io] {\n  var i = 0\n  while i < names.len() {\n    let name = names[i]; print $name\n    i += 1\n  }\n}\n",
        ] {
            let diagnostics = counter_loops(source);
            assert_eq!(diagnostics.len(), 1, "{source}");
            assert!(diagnostics[0].fix_hints.is_empty(), "{source}");
        }
    }
}
