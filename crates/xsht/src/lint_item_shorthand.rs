use xsh::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use xsh::frontend::source::Span;
use xsh::frontend::symbols::Name;
use xsh::frontend::syntax::arena::{
    ArenaExprKind, ArenaExprTag, ArenaPipeStageKind, ArenaProgram, AstArena, BlockId, ExprId,
};
use xsh::frontend::syntax::node::{BinaryOp, StreamStageKind};
use xsh::frontend::syntax::parser::Parser;

/// A one-parameter callback that reads its parameter as the receiver of field
/// accesses or method calls can leave it implicit: `{ |hit| hit.url }` is
/// `{ .url }`. The fix is offered only when the rewritten block reparses to
/// the original tree with every use of the parameter read as `.`.
pub(super) fn lint_item_shorthand(program: &ArenaProgram, source: &str) -> Vec<Diagnostic> {
    let arena = &program.arena;
    // A workspace lints each module against one arena holding every module.
    let Some(source_id) = program
        .statement_ids()
        .next()
        .map(|stmt| arena.stmt(stmt).span.source_id)
    else {
        return Vec::new();
    };
    callback_blocks(arena)
        .into_iter()
        .filter(|&(block, _)| arena.span(arena.block(block).span).source_id == source_id)
        .filter_map(|(block, callback)| item_shorthand(arena, source, block, callback))
        .collect()
}

fn named_parameter(arena: &AstArena, block: BlockId) -> Option<Name> {
    match arena.block_params(arena.block(block).params) {
        [param] if param.name.as_str() != "_" => Some(param.name),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Callback {
    Stage,
    Handler,
}

impl Callback {
    /// A program whose next token is a block of this kind.
    fn prefix(self) -> &'static str {
        match self {
            Self::Stage => "let _ = [] |> map ",
            Self::Handler => "let _ = 1 ?? ",
        }
    }
}

fn callback_blocks(arena: &AstArena) -> Vec<(BlockId, Callback)> {
    let one_item = |kind: StreamStageKind| !matches!(kind, StreamStageKind::Fold | StreamStageKind::Reduce);
    let mut blocks = Vec::new();
    // Tags are read first: a workspace arena holds every module's rows.
    for (index, tag) in arena.expr_tags.iter().enumerate() {
        if !matches!(
            tag,
            ArenaExprTag::StructuredPipeline | ArenaExprTag::Pipeline | ArenaExprTag::BinaryResultFallback
        ) {
            continue;
        }
        match arena.expr(ExprId::from_index(index)).kind {
            ArenaExprKind::StructuredPipeline { stages, .. } => blocks.extend(
                arena
                    .stream_stages(stages)
                    .iter()
                    .filter(|stage| one_item(stage.kind))
                    .filter_map(|stage| stage.block)
                    .map(|block| (block, Callback::Stage)),
            ),
            ArenaExprKind::Pipeline { stages, .. } => {
                blocks.extend(arena.pipe_stages(stages).iter().filter_map(|stage| {
                    match &stage.kind {
                        ArenaPipeStageKind::Stream(stage) if one_item(stage.kind) => {
                            stage.block.map(|block| (block, Callback::Stage))
                        }
                        _ => None,
                    }
                }));
            }
            ArenaExprKind::Binary {
                op: BinaryOp::ResultFallback,
                right,
                ..
            } => {
                if let ArenaExprKind::ValueBlock(block) = arena.expr(right).kind {
                    blocks.push((block, Callback::Handler));
                }
            }
            _ => {}
        }
    }
    blocks
}

/// The block is analyzed in a parse of its own text, so the work is bounded
/// by the block and not by the arena, which a workspace shares between
/// modules.
fn item_shorthand(
    arena: &AstArena,
    source: &str,
    block: BlockId,
    callback: Callback,
) -> Option<Diagnostic> {
    let name = named_parameter(arena, block)?;
    let param_span = arena.span(arena.block_params(arena.block(block).params)[0].span);
    let block_span = arena.span(arena.block(block).span);
    let text = source.get(block_span.range())?;
    // Every spelling of the name in the block must be a use of the parameter:
    // a binder, label, field name, string, or comment that repeats it means a
    // rewrite could change what a use refers to.
    let spellings = word_count(text, name.as_str().as_str());
    if spellings < 2 {
        return None;
    }
    let snippet = CallbackSnippet::parse(callback, text)?;
    let local = &snippet.parsed.arena.arena;
    let inside = |span: Span, outer: Span| outer.start() <= span.start() && span.end() <= outer.end();
    let root_span = local.span(local.block(snippet.block).span);
    let uses = (0..local.expr_tags.len())
        .map(ExprId::from_index)
        .filter(|&expr| {
            matches!(local.expr(expr).kind, ArenaExprKind::Ident(found) if found == name)
                && inside(local.expr(expr).span, root_span)
        })
        .collect::<Vec<_>>();
    if uses.len() + 1 != spellings {
        return None;
    }
    // Inside a nested callback `.` is that callback's own item.
    let nested = callback_blocks(local)
        .into_iter()
        .filter(|&(other, _)| other != snippet.block)
        .map(|(other, _)| local.span(local.block(other).span))
        .collect::<Vec<_>>();
    if uses.iter().any(|&expr| {
        nested
            .iter()
            .any(|&outer| inside(local.expr(expr).span, outer))
    }) {
        return None;
    }
    let receivers = (0..local.expr_tags.len())
        .filter_map(|index| match local.expr(ExprId::from_index(index)).kind {
            ArenaExprKind::Field { base, .. } if uses.contains(&base) => Some(base),
            _ => None,
        })
        .collect::<Vec<_>>();
    // A parameter only ever passed whole is the stage-callable lint's case.
    if receivers.is_empty() {
        return None;
    }
    let offset = root_span.start();
    let local_param = local.span(local.block_params(local.block(snippet.block).params)[0].span);
    let header = parameter_header(&snippet.source, root_span, local_param)?;
    let mut edits = vec![(header, "")];
    for &expr in &uses {
        edits.push((
            local.expr(expr).span,
            if receivers.contains(&expr) { "" } else { "." },
        ));
    }
    edits.sort_by_key(|(span, _)| span.start());
    let mut replacement = text.to_owned();
    for (span, edit) in edits.iter().rev() {
        replacement.replace_range(span.start() - offset..span.end() - offset, edit);
    }
    let expected = super::super::format::canonical_block_with_implicit_item(
        local,
        &snippet.source,
        snippet.block,
        name,
    );
    let rewritten = CallbackSnippet::parse(callback, &replacement)?;
    let actual = super::super::format::canonical_subtree(
        &rewritten.parsed.arena.arena,
        &rewritten.source,
        Ok(rewritten.block),
    );
    if expected != actual {
        return None;
    }
    // Include the complete callback in the edit so comments and nested
    // callbacks move together with the header and its item reads.
    let diagnostic = Diagnostic::warning(format!(
        "`{name}` is only read through its fields; leave the parameter implicit and write `.`"
    ))
    .with_code(DiagnosticCode::LintPreferItemShorthand)
    .with_label(Label::primary(param_span, "this parameter can be the item `.`"))
    // Header removal and every item read form one rewrite. Keeping them in a
    // single edit prevents an overlapping fix from leaving an unbound name.
    .with_fix_hint(FixHint::replacement(
        block_span,
        "rewrite this callback with an implicit item",
        replacement,
    ));
    Some(diagnostic)
}

/// A block's text parsed alone in a position of its kind, so the original and
/// rewritten text are compared in the same context.
struct CallbackSnippet {
    source: String,
    parsed: xsh::frontend::syntax::parser::ArenaParseOutput,
    block: BlockId,
}

impl CallbackSnippet {
    fn parse(callback: Callback, block: &str) -> Option<Self> {
        let prefix = callback.prefix();
        let source = format!("{prefix}{block}\n");
        let parsed =
            Parser::parse_source_arena_only(xsh::frontend::source::SourceId::new(0), &source);
        if !parsed.diagnostics.is_empty() {
            return None;
        }
        let arena = &parsed.arena.arena;
        let block = (0..arena.blocks.len())
            .map(BlockId::from_index)
            .find(|&id| arena.span(arena.block(id).span).start() == prefix.len())?;
        Some(Self {
            source,
            parsed,
            block,
        })
    }
}

/// The `|name|` header and the space on one side of it, so `{ |x| x.f }`
/// becomes `{ .f }` and `{ |x|` before a line break becomes `{`.
fn parameter_header(source: &str, block: Span, param: Span) -> Option<Span> {
    let open = source.get(block.start()..param.start())?.rfind('|')? + block.start();
    let close = source.get(param.end()..block.end())?.find('|')? + param.end() + 1;
    let (start, end) = if source.get(close..close + 1) == Some(" ") {
        (open, close + 1)
    } else {
        let start = source.get(block.start()..open)?.trim_end().len() + block.start();
        (start, close)
    };
    Some(Span::new(block.source_id, start, end))
}

fn word_count(text: &str, word: &str) -> usize {
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(word)
        .filter(|(at, _)| {
            !text[..*at].chars().next_back().is_some_and(is_word)
                && !text[at + word.len()..].chars().next().is_some_and(is_word)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use crate::xsht::lint::{LintOptions, Linter};
    use xsh::diagnostic::{Diagnostic, DiagnosticCode};
    use xsh::frontend::check::Checker;
    use xsh::frontend::source::SourceId;
    use xsh::frontend::syntax::parser::Parser;

    fn diagnostics(source: &str) -> Vec<Diagnostic> {
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
        .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::LintPreferItemShorthand))
        .collect()
    }

    fn fixed(source: &str) -> String {
        let output = diagnostics(source);
        let mut fixed = source.to_owned();
        let mut fixes = output
            .iter()
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .collect::<Vec<_>>();
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.span.unwrap().start()));
        for fix in fixes {
            fixed.replace_range(fix.span.unwrap().range(), fix.replacement.as_deref().unwrap());
        }
        fixed
    }

    const HITS: &str = "type Hit = {url: Str, size: Int}\nlet hits = [Hit(url: \"/a\", size: 1), Hit(url: \"/b\", size: 2)]\n";

    #[test]
    fn field_and_method_receivers_become_the_item() {
        for (body, expected) in [
            ("let urls = hits |> count { |hit| hit.url }\n", "let urls = hits |> count { .url }\n"),
            (
                "let sizes = hits |> map { |hit| hit.url.byte_len() + hit.size }\n",
                "let sizes = hits |> map { .url.byte_len() + .size }\n",
            ),
            (
                "let big = hits |> where { |hit| hit.size > 1 and hit.url != \"/\" }\n",
                "let big = hits |> where { .size > 1 and .url != \"/\" }\n",
            ),
            (
                "let text = \"8080\".parse_int() ?? { |failure| failure.message.byte_len() }\n",
                "let text = \"8080\".parse_int() ?? { .message.byte_len() }\n",
            ),
            (
                "let both = hits |> map { |hit| [hit.size, hit.url.byte_len()] |> sum() }\n",
                "let both = hits |> map { [.size, .url.byte_len()] |> sum() }\n",
            ),
        ] {
            let source = format!("{HITS}{body}");
            let output = fixed(&source);
            assert_eq!(output, format!("{HITS}{expected}"), "{body}");
            let checked = Checker::check_arena(
                &Parser::parse_source_arena_only(SourceId::new(0), &output).arena,
                &output,
            );
            assert!(checked.diagnostics.is_empty(), "{output}: {:?}", checked.diagnostics);
            assert!(diagnostics(&output).is_empty(), "{output}");
        }
    }

    #[test]
    fn a_header_before_a_line_break_leaves_the_brace_alone() {
        let source = format!("{HITS}let labels = hits\n  |> map {{ |hit|\n    f\"{{hit.url}}:{{hit.size}}\"\n  }}\n");
        assert!(
            fixed(&source).ends_with("  |> map {\n    f\"{.url}:{.size}\"\n  }\n"),
            "{}",
            fixed(&source)
        );
    }

    #[test]
    fn whole_parameter_uses_become_the_item_beside_field_reads() {
        let source = format!("{HITS}pure weight(hit: Hit) -> Int {{ hit.size }}\nlet total = hits |> map {{ |hit| hit.size + weight(hit) }}\n");
        assert!(
            fixed(&source).ends_with("let total = hits |> map { .size + weight(.) }\n"),
            "{}",
            fixed(&source)
        );
    }

    #[test]
    fn parameters_the_item_cannot_spell_keep_their_name() {
        for body in [
            // Only passed whole: the stage-callable lint owns `map(f)`.
            "pure weight(hit: Hit) -> Int { hit.size }\nlet total = hits |> map { |hit| weight(hit) }\n",
            // A nested callback has its own item.
            "let pairs = hits |> map { |hit| hits |> where { .size == hit.size } |> count() }\n",
            // A shadowing binder, and the name reused as a field or label.
            "let shadowed = hits |> map { |hit| [hit.size for hit in hits] |> sum() }\n",
            "let labelled = hits |> map { |hit| {hit: hit.size} }\n",
            // Two parameters, and a discarded one.
            "let folded = hits |> fold(0) { |acc, hit| acc + hit.size }\n",
            "let ignored = \"x\".parse_int() ?? { |_| 0 }\n",
            // `. in` would read as the field `.in`.
            "let member = hits |> where { |hit| hit.size > 0 and hit in hits }\n",
        ] {
            let source = format!("{HITS}{body}");
            assert!(diagnostics(&source).is_empty(), "{body}");
        }
    }
}
