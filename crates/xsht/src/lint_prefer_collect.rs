use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label, Severity};
use xsh::frontend::source::{SourceId, Span};
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaAssignTargetKind, ArenaBindingTargetKind, ArenaCallArgKind, ArenaExprKind,
    ArenaExprOrRun, ArenaStmtKind, ArenaSugar, AstArena, BlockId, StmtId,
};
use xsh::frontend::syntax::node::AssignOp;
use xsh::frontend::syntax::parser::Parser;

/// A local list that is declared empty and then only appended to, up to its
/// first read, is the list a `collect` block builds:
///
/// ```text
/// var names = []
/// for unit in units {
///   continue unless unit.enabled
///   names += [unit.name]
/// }
/// ```
///
/// becomes
///
/// ```text
/// let names = collect {
///   for unit in units {
///     continue unless unit.enabled
///     yield unit.name
///   }
/// }
/// ```
///
/// The block is no boundary, so every `?`, `return`, `break`, and `continue`
/// in it goes where it went. The rewrite is offered only where it cannot
/// change anything else, and the lint reports nothing where it is not:
///
/// - The list is named only by appends (`xs = xs.push(v)`, `xs += [v]`,
///   `xs += more`) in the statements that become the block, and no appended
///   value names it. Text is compared, so a comment that names it counts.
/// - Each append is a statement of a loop, a branch, a `match` arm, or a
///   bare block. One inside a `try`, a `retry`, a scope, or a callable is
///   not reached, and the list is then left alone.
/// - The statements that become the block declare nothing and defer nothing
///   themselves, since a block would end those early, and hold no `yield`,
///   which the block would take.
/// - No other list is appended to in those statements: one list per block.
/// - No text that spans lines keeps its indentation by being copied: a
///   triple-quoted string in those statements leaves the list alone, and
///   so does a comment, which no fix is applied across.
///
/// A declaration directly followed by one loop that only appends is a
/// comprehension and is left to `lint.prefer-list-comp`.
pub(super) fn lint_built_lists(linter: &mut super::Linter<'_>, stmts: &[StmtId]) {
    let arena = linter.arena;
    let source = linter.source;
    for index in 0..stmts.len() {
        let Some(found) = built_list(arena, source, stmts, index) else {
            continue;
        };
        // The comprehension lint rewrites the same statements.
        if linter.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code,
                Some(DiagnosticCode::LintPreferListComp | DiagnosticCode::LintPreferMapComp)
            ) && diagnostic.fix_hints.iter().any(|hint| {
                hint.span
                    .is_some_and(|span| span.start() < found.span.end() && found.span.start() < span.end())
            })
        }) {
            continue;
        }
        linter.diagnostics.push(
            Diagnostic::new(
                Severity::Warning,
                format!(
                    "`{}` is declared empty and then only appended to; build it with `collect {{ ... }}`",
                    found.name
                ),
            )
            .with_code(DiagnosticCode::LintPreferCollect)
            .with_label(Label::secondary(
                arena.stmt(stmts[index]).span,
                "each append to this list becomes a `yield`",
            ))
            .with_fix_hint(FixHint::replacement(
                found.span,
                "build the list with `collect`",
                found.text,
            )),
        );
    }
}

struct BuiltList {
    name: Name,
    span: Span,
    text: String,
}

/// One append to the list, as the `yield` that replaces it.
struct Append {
    /// The statement's text, without the line break that may end it.
    start: usize,
    end: usize,
    replacement: String,
    /// The append is a statement of the block itself.
    top: bool,
}

struct Scan<'a> {
    arena: &'a AstArena,
    source: &'a str,
    name: &'a str,
    appends: Vec<Append>,
}

fn built_list(arena: &AstArena, source: &str, stmts: &[StmtId], index: usize) -> Option<BuiltList> {
    let declaration = arena.stmt(stmts[index]);
    let ArenaStmtKind::Var {
        target,
        ty,
        initializer: ArenaExprOrRun::Expr(initializer),
    } = declaration.kind
    else {
        return None;
    };
    let ArenaBindingTargetKind::Name(name) = arena.binding_target(target).kind else {
        return None;
    };
    if !matches!(arena.expr(initializer).kind, ArenaExprKind::List(items) if items.is_empty()) {
        return None;
    }
    let text = |start: usize, end: usize| source.get(start..end);
    let stmt_text = |stmt: StmtId| {
        let span = arena.stmt(stmt).span;
        text(span.start(), span.end())
    };
    let spelled = name.as_str();
    let spelled: &str = &spelled;
    // The statements before the first that names the list stay where they
    // are; the declaration moves down to that statement.
    let mut first = index + 1;
    while !mentions(stmt_text(*stmts.get(first)?)?, spelled) {
        first += 1;
    }
    let mut scan = Scan {
        arena,
        source,
        name: spelled,
        appends: Vec::new(),
    };
    let mut last = None;
    for at in first..stmts.len() {
        let before = scan.appends.len();
        if !scan.statement(stmts[at], true) {
            scan.appends.truncate(before);
            break;
        }
        if scan.appends.len() > before {
            last = Some((at, scan.appends.len()));
        }
    }
    let (last, appended) = last?;
    scan.appends.truncate(appended);

    let start = declaration.span.start();
    let body_start = arena.stmt(stmts[first]).span.start();
    let end = {
        let end = arena.stmt(stmts[last]).span.end();
        body_start + text(body_start, end)?.trim_end().len()
    };
    let body = text(body_start, end)?;
    // Both the declaration and the first statement of the block begin their
    // lines, at one indentation.
    let indent = line_indent(source, start)?;
    if line_indent(source, body_start)? != indent
        || body.contains("\"\"\"")
        || mentions(body, "yield")
    {
        return None;
    }
    // The list is built once: nothing assigns it afterwards, so it is a
    // `let`. At least one append is in a loop or a branch; a list whose
    // appends are all statements of the block is a list literal.
    let rest_end = arena.stmt(*stmts.last()?).span.end();
    if assigns(text(end, rest_end)?, spelled) || scan.appends.iter().all(|append| append.top) {
        return None;
    }
    let annotation = match ty {
        Some(ty) => format!(": {}", source.get(arena.type_expr_span(ty).range())?),
        None => String::new(),
    };
    let mut block = String::new();
    let mut cursor = body_start;
    for append in &scan.appends {
        block.push_str(text(cursor, append.start)?);
        block.push_str(&append.replacement);
        cursor = append.end;
    }
    block.push_str(text(cursor, end)?);
    let mut replacement = String::new();
    if first > index + 1 {
        replacement.push_str(text(arena.stmt(stmts[index + 1]).span.start(), body_start)?);
    }
    replacement.push_str(&format!("let {spelled}{annotation} = collect {{\n"));
    for (number, line) in block.lines().enumerate() {
        if number == 0 {
            replacement.push_str(indent);
        }
        if !line.is_empty() {
            replacement.push_str("  ");
            replacement.push_str(line);
        }
        replacement.push('\n');
    }
    replacement.push_str(indent);
    replacement.push('}');

    // The block's statements move two columns right. A line that no longer
    // fits would be broken by the formatter, and this rewrite does not
    // reproduce how.
    if replacement
        .lines()
        .any(|line| line.chars().count() > super::super::format::DEFAULT_LINE_WIDTH)
    {
        return None;
    }
    let span = Span::new(declaration.span.source_id, start, end);
    // A fix is never applied across a comment, so a list with one among
    // these statements would be reported and never rewritten.
    if text(start, end)?.contains('#') {
        return None;
    }
    // The new statements must parse: an appended value is moved under
    // `yield`, which not every expression may follow. They are whole
    // statements, so they parse alone as they do in their place.
    if !Parser::parse_source_arena_only(SourceId::new(0), &replacement)
        .diagnostics
        .is_empty()
    {
        return None;
    }
    Some(BuiltList {
        name,
        span,
        text: replacement,
    })
}

impl Scan<'_> {
    fn text(&self, span: Span) -> &str {
        self.source.get(span.range()).unwrap_or("")
    }

    /// Whether the statement may stand in the block: it names the list only
    /// in appends that a `yield` can replace. `top` says the statement is
    /// one of the block's own, where a declaration or a `defer` would get a
    /// shorter life.
    fn statement(&mut self, stmt: StmtId, top: bool) -> bool {
        let arena = self.arena;
        let statement = arena.stmt(stmt);
        let span = statement.span;
        match statement.kind {
            ArenaStmtKind::Assign { .. } => match self.append(stmt, top) {
                Some(append) => {
                    self.appends.push(append);
                    true
                }
                // One list per block: statements that also build another
                // list would leave that one built by appends inside a
                // `collect`.
                None => !mentions(self.text(span), self.name) && !self.appends_elsewhere(stmt),
            },
            ArenaStmtKind::For { block, .. }
            | ArenaStmtKind::While { block, .. }
            | ArenaStmtKind::Loop { block } => self.compound(span, &[block]),
            ArenaStmtKind::If {
                branches,
                else_block,
            } => {
                let blocks: Vec<BlockId> = arena
                    .if_branches(branches)
                    .iter()
                    .map(|branch| branch.block)
                    .chain(else_block)
                    .collect();
                self.compound(span, &blocks)
            }
            ArenaStmtKind::Match { arms, .. } => {
                let blocks: Vec<BlockId> =
                    arena.match_arms(arms).iter().map(|arm| arm.block).collect();
                self.compound(span, &blocks)
            }
            ArenaStmtKind::Sugar { form, operands, .. } => match arena.sugar(form, operands) {
                ArenaSugar::Guarded {
                    stmt: guarded,
                    condition,
                    ..
                } => {
                    !mentions(self.text(arena.expr(condition).span), self.name)
                        && self.statement(guarded, top)
                }
                ArenaSugar::Repeat { body, .. } | ArenaSugar::ForIndex { body, .. } => {
                    self.compound(span, &[body])
                }
                _ => !mentions(self.text(span), self.name),
            },
            ArenaStmtKind::Expr(expr) => match arena.expr(expr).kind {
                ArenaExprKind::ValueBlock(block) => self.compound(span, &[block]),
                _ => !mentions(self.text(span), self.name),
            },
            ArenaStmtKind::Break { .. }
            | ArenaStmtKind::Continue
            | ArenaStmtKind::Return(_)
            | ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::Command(_) => !mentions(self.text(span), self.name),
            // A declaration, a `defer`, or anything else: only inside a
            // nested block, and only if it does not name the list.
            _ => !top && !mentions(self.text(span), self.name),
        }
    }

    /// A statement with blocks: its own text names the list nowhere, and
    /// every statement of its blocks may stand.
    fn compound(&mut self, span: Span, blocks: &[BlockId]) -> bool {
        let arena = self.arena;
        let mut cursor = span.start();
        for block in blocks {
            let block_span = arena.span(arena.block(*block).span);
            if block_span.start() < cursor
                || mentions(
                    self.source.get(cursor..block_span.start()).unwrap_or(""),
                    self.name,
                )
            {
                return false;
            }
            cursor = block_span.end();
        }
        if cursor > span.end()
            || mentions(self.source.get(cursor..span.end()).unwrap_or(""), self.name)
        {
            return false;
        }
        blocks.iter().all(|block| {
            let block = arena.block(*block);
            block.params.is_empty()
                && arena
                    .stmt_ids(block.statements)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .all(|stmt| self.statement(stmt, false))
        })
    }

    /// Whether the assignment appends to some list: `X = X.push(V)` or
    /// `X += [...]`.
    fn appends_elsewhere(&self, stmt: StmtId) -> bool {
        let arena = self.arena;
        let ArenaStmtKind::Assign {
            op,
            value: ArenaExprOrRun::Expr(value),
            ..
        } = arena.stmt(stmt).kind
        else {
            return false;
        };
        match (op, arena.expr(value).kind) {
            (AssignOp::Add, ArenaExprKind::List(_)) => true,
            (AssignOp::Set, ArenaExprKind::Call { callee, .. }) => matches!(
                arena.expr(callee).kind,
                ArenaExprKind::Field { name, .. } if name == "push"
            ),
            _ => false,
        }
    }

    /// The `yield` that replaces `NAME = NAME.push(V)`, `NAME += [V]`, or
    /// `NAME += MORE`, when the appended value does not name the list.
    fn append(&self, stmt: StmtId, top: bool) -> Option<Append> {
        let arena = self.arena;
        let statement = arena.stmt(stmt);
        let ArenaStmtKind::Assign {
            target,
            op,
            value: ArenaExprOrRun::Expr(value),
        } = statement.kind
        else {
            return None;
        };
        if !matches!(
            arena.assign_target(target).kind,
            ArenaAssignTargetKind::Name(found) if found.as_str() == *self.name
        ) {
            return None;
        }
        let is_list = |expr| matches!(arena.expr(expr).kind, ArenaExprKind::Ident(found) if found.as_str() == *self.name);
        let (prefix, appended) = match (op, arena.expr(value).kind) {
            (AssignOp::Set, ArenaExprKind::Call { callee, args }) => {
                let ArenaExprKind::Field { base, name: method } = arena.expr(callee).kind else {
                    return None;
                };
                if method != "push" || !is_list(base) {
                    return None;
                }
                let [argument] = arena.call_args(args) else {
                    return None;
                };
                let ArenaCallArgKind::Positional(element) = argument.kind else {
                    return None;
                };
                ("yield ", element)
            }
            (AssignOp::Add, ArenaExprKind::List(items)) => {
                let mut elements = arena.list_elements(items);
                match (elements.next(), elements.next()) {
                    (Some(element), None) if element.splice_span.is_none() => {
                        ("yield ", element.value)
                    }
                    _ => ("yield @", value),
                }
            }
            (AssignOp::Add, _) => ("yield @", value),
            _ => return None,
        };
        let appended_span = arena.expr(appended).span;
        let appended = self.text(appended_span);
        if appended.is_empty() || mentions(appended, self.name) {
            return None;
        }
        let span = statement.span;
        // A value that begins a line of its own inside the brackets is
        // indented under them. Under `yield` it begins on the statement's
        // line, so its other lines move left by that much.
        let appended = match line_indent(self.source, appended_span.start()) {
            Some(indent) if appended.contains('\n') => {
                let outdent = indent
                    .len()
                    .checked_sub(line_indent(self.source, span.start())?.len())?;
                let mut moved = String::new();
                for (number, line) in appended.split('\n').enumerate() {
                    if number > 0 {
                        moved.push('\n');
                    }
                    if number == 0 || line.is_empty() {
                        moved.push_str(line);
                    } else {
                        let spaces = line.len() - line.trim_start_matches(' ').len();
                        moved.push_str(line.get(outdent.min(spaces)..)?);
                        if spaces < outdent {
                            return None;
                        }
                    }
                }
                moved
            }
            _ => appended.to_string(),
        };
        Some(Append {
            start: span.start(),
            end: span.start() + self.text(span).trim_end().len(),
            replacement: format!("{prefix}{appended}"),
            top,
        })
    }
}

/// The whitespace that begins the line of `offset`, when nothing else
/// stands before `offset` on that line.
fn line_indent(source: &str, offset: usize) -> Option<&str> {
    let line = source[..offset].rfind('\n').map_or(0, |newline| newline + 1);
    let text = &source[line..offset];
    text.trim().is_empty().then_some(text)
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `text` holds `name` as a whole word.
fn mentions(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let before = at.checked_sub(1).map(|index| text.as_bytes()[index]);
        let after = text.as_bytes().get(at + name.len()).copied();
        !before.is_some_and(is_word) && !after.is_some_and(is_word)
    })
}

/// Whether `text` may assign `name`: the word directly before an assignment
/// operator, or before an index or a field on a line that holds one.
fn assigns(text: &str, name: &str) -> bool {
    let assignment = |text: &str| {
        text.starts_with("+=")
            || text.starts_with("-=")
            || text.starts_with("*=")
            || (text.starts_with('=') && !text.starts_with("==") && !text.starts_with("=>"))
    };
    text.match_indices(name).any(|(at, _)| {
        let before = at.checked_sub(1).map(|index| text.as_bytes()[index]);
        let rest = &text[at + name.len()..];
        if before.is_some_and(is_word) || rest.bytes().next().is_some_and(is_word) {
            return false;
        }
        let line = rest.lines().next().unwrap_or("").trim_start_matches(' ');
        assignment(line)
            || ((line.starts_with('[') || line.starts_with('.'))
                && line
                    .char_indices()
                    .any(|(index, _)| line[..index].ends_with(' ') && assignment(&line[index..])))
    })
}

#[cfg(test)]
mod tests {
    use super::super::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn built_lists(source: &str) -> Vec<Diagnostic> {
        let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Linter::lint(&parsed.arena, source, LintOptions::default())
            .diagnostics
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferCollect))
            .collect()
    }

    /// The source after its one finding's fix.
    fn rewritten(source: &str) -> String {
        let diagnostics = built_lists(source);
        assert_eq!(diagnostics.len(), 1, "{source}\n{diagnostics:?}");
        let [hint] = &diagnostics[0].fix_hints[..] else {
            panic!("no rewrite: {diagnostics:?}");
        };
        assert!(!hint.dangerous);
        let mut text = source.to_string();
        text.replace_range(hint.span.unwrap().range(), hint.replacement.as_deref().unwrap());
        assert!(built_lists(&text).is_empty(), "{text}");
        text
    }

    #[test]
    fn a_list_built_by_a_loop_with_more_than_an_append_becomes_a_collect() {
        let source = "proc enabled(units: List[Unit]) -> List[Str] {\n  var names = []\n\n  for unit in units {\n    continue unless unit.enabled\n\n    print $unit.name\n    names += [unit.name]\n  }\n\n  names\n}\n";
        assert_eq!(
            rewritten(source),
            "proc enabled(units: List[Unit]) -> List[Str] {\n  let names = collect {\n    for unit in units {\n      continue unless unit.enabled\n\n      print $unit.name\n      yield unit.name\n    }\n  }\n\n  names\n}\n"
        );
    }

    #[test]
    fn every_append_spelling_and_position_becomes_a_yield() {
        let source = "proc gather(rows: List[Row], extra: List[Str]) -> List[Str] {\n  var out: List[Str] = []\n  let limit = 3\n  out = out.push(\"head\")\n  for row in rows {\n    match row.kind {\n      \"a\" => out += [row.name]\n      _ => {\n        out += [row.name, row.kind]\n      }\n    }\n    if row.size > limit {\n      out += extra\n    } else {\n      out = out.push(f\"{row.name}!\")\n    }\n  }\n  print \"done\"\n  out.len()\n}\n";
        assert_eq!(
            rewritten(source),
            "proc gather(rows: List[Row], extra: List[Str]) -> List[Str] {\n  let limit = 3\n  let out: List[Str] = collect {\n    yield \"head\"\n    for row in rows {\n      match row.kind {\n        \"a\" => yield row.name\n        _ => {\n          yield @[row.name, row.kind]\n        }\n      }\n      if row.size > limit {\n        yield @extra\n      } else {\n        yield f\"{row.name}!\"\n      }\n    }\n  }\n  print \"done\"\n  out.len()\n}\n"
        );
    }

    #[test]
    fn a_guarded_append_becomes_a_guarded_yield() {
        let source = "var big = []\nfor n in [1, 2] {\n  print $n\n  big += [n] when n > 1\n}\nprint f\"{big.len()}\"\n";
        assert_eq!(
            rewritten(source),
            "let big = collect {\n  for n in [1, 2] {\n    print $n\n    yield n when n > 1\n  }\n}\nprint f\"{big.len()}\"\n"
        );
    }

    #[test]
    fn a_value_on_lines_of_its_own_moves_out_from_under_the_brackets() {
        let source = "var rows = []\nfor n in [1, 2] {\n  print $n\n  rows += [\n    {\n      id: n,\n      label: f\"{n}\",\n    },\n  ]\n}\nprint f\"{rows.len()}\"\n";
        assert_eq!(
            rewritten(source),
            "let rows = collect {\n  for n in [1, 2] {\n    print $n\n    yield {\n      id: n,\n      label: f\"{n}\",\n    }\n  }\n}\nprint f\"{rows.len()}\"\n"
        );
    }

    #[test]
    fn the_block_ends_at_the_last_append_before_the_first_read() {
        let source = "var xs = []\nfor n in [1, 2] {\n  print $n\n  xs += [n]\n}\nprint \"between\"\nxs += [3]\nprint \"after\"\nprint f\"{xs.len()}\"\n";
        assert_eq!(
            rewritten(source),
            "let xs = collect {\n  for n in [1, 2] {\n    print $n\n    yield n\n  }\n  print \"between\"\n  yield 3\n}\nprint \"after\"\nprint f\"{xs.len()}\"\n"
        );
    }

    #[test]
    fn a_plain_comprehension_is_left_to_its_own_lint() {
        for source in [
            "var xs = []\nfor n in [1, 2] {\n  xs += [n * 2]\n}\nprint f\"{xs.len()}\"\n",
            "var xs = []\nfor n in [1, 2] {\n  continue unless n > 1\n  xs += [n * 2]\n}\nprint f\"{xs.len()}\"\n",
        ] {
            assert!(built_lists(source).is_empty(), "{source}");
        }
    }

    #[test]
    fn lists_the_rewrite_could_change_are_left_alone() {
        for source in [
            // The list is read while it is built.
            "var xs = []\nfor n in [1, 2] {\n  print f\"{xs.len()}\"\n  xs += [n]\n}\nprint f\"{xs.len()}\"\n",
            // An appended value names the list.
            "var xs = []\nfor n in [1, 2] {\n  print $n\n  xs += [xs.len() + n]\n}\nprint f\"{xs.len()}\"\n",
            // The append is inside a `try`, which a yield may not leave.
            "var xs = []\nfor n in [1, 2] {\n  let r = try {\n    xs += [n]\n  }\n}\nprint f\"{xs.len()}\"\n",
            // The append is inside a stage block.
            "var xs = []\n[1, 2] |> each {\n  xs += [.]\n}\nprint f\"{xs.len()}\"\n",
            // A statement of the block declares a name the rest reads.
            "var xs = []\nxs += [1]\nlet total = 2\nxs += [total]\nprint f\"{xs.len()} {total}\"\n",
            // A `yield` in the statements would become the block's.
            "stream both(items: List[Int]) -> Stream[Int] {\n  var xs = []\n  for item in items {\n    yield item\n    xs += [item]\n  }\n  yield xs.len()\n}\n",
            // Two lists are built by the same loop.
            "var odd = []\nvar even = []\nfor n in [1, 2] {\n  if n % 2 == 0 {\n    even += [n]\n  } else {\n    odd += [n]\n  }\n}\nprint f\"{odd.len()} {even.len()}\"\n",
            // No fix is applied across a comment.
            "var xs = []\nfor n in [1, 2] {\n  # each one\n  print $n\n  xs += [n]\n}\nprint f\"{xs.len()}\"\n",
            // A triple-quoted string would be indented.
            "var xs = []\nfor n in [1, 2] {\n  print $n\n  xs += [\"\"\"a\nb\"\"\"]\n}\nprint f\"{xs.len()}\"\n",
            // The list is replaced, not appended to.
            "var xs = []\nfor n in [1, 2] {\n  print $n\n  xs = [n]\n}\nprint f\"{xs.len()}\"\n",
            // The list is appended to again after it is read.
            "var seen = []\nfor n in [1, 2] {\n  print $n\n  seen += [n]\n}\nprint f\"{seen.len()}\"\nseen += [3]\n",
            // Every append is a statement of the block: a list literal.
            "var xs = []\nxs += [1]\nprint \"between\"\nxs += [2]\nprint f\"{xs.len()}\"\n",
            // Nothing is appended.
            "var xs = []\nprint f\"{xs.len()}\"\n",
            // The list does not start empty.
            "var xs = [0]\nfor n in [1, 2] {\n  print $n\n  xs += [n]\n}\nprint f\"{xs.len()}\"\n",
        ] {
            let diagnostics = built_lists(source);
            assert!(diagnostics.is_empty(), "{source}\n{diagnostics:?}");
        }
    }
}
