//! `atomically replace DEST as NAME { BODY }`: produce a file beside its
//! destination and rename it into place once the body has finished.
//!
//! The statement is sugar. Its meaning is the expansion `expand_atomically`
//! builds, under the rules every expansion keeps (`super::sugar`).

use super::{Name, Parser, TokenTag};
use crate::diagnostic::DiagnosticCode;
use crate::source::Span;
use crate::syntax::arena::{
    ArenaCallArgInput, ArenaExprOrRun, ArenaProgramBuilder, ArenaSugarOperand, BindingTargetId,
    BlockId, DeferTrigger, ExprId, StmtId, SugarForm,
};

/// The word that begins the statement. It stays an ordinary identifier
/// everywhere else.
const ATOMICALLY_WORD: &str = "atomically";
/// The word that follows it and names what is done to the destination.
const REPLACE_WORD: &str = "replace";
/// The word between the destination and the name of the temporary path.
const AS_WORD: &str = "as";
/// The local that holds the destination, which the expansion reads twice.
/// No identifier can spell it, so the body cannot see or shadow it.
const DEST_LOCAL: &str = "%dest";

impl Parser<'_> {
    /// Whether the statement at the cursor is `atomically replace DEST as
    /// NAME {`.
    ///
    /// Neither word is reserved. The statement is recognized by the two
    /// words that begin it, on one line: no other statement begins with the
    /// name `atomically` directly followed by the name `replace`.
    pub(super) fn lookahead_is_atomically(&self) -> bool {
        self.current_name()
            .is_some_and(|name| name == ATOMICALLY_WORD)
            && self.peek_tag(1) == Some(TokenTag::Ident)
            && self.peek_name(1).is_some_and(|name| name == REPLACE_WORD)
    }

    /// Whether the cursor is on the `as` that ends the destination of the
    /// `atomically replace` head being parsed.
    pub(super) fn at_head_as(&self) -> bool {
        self.head_as_index == Some(self.index)
    }

    /// The token index of the `as` that ends the destination which starts at
    /// the cursor: the first `as` outside every bracket and brace the
    /// destination opens that stands directly before a name and `{`.
    ///
    /// A pattern test may end in `as NAME` too (`x is Shape as s`), but never
    /// with a `{` after the name, so outside brackets that shape is always
    /// the end of the head. Inside brackets `as` names the pattern as usual.
    fn head_as_index(&self) -> Option<usize> {
        let mut depth = 0usize;
        let mut offset = 0;
        loop {
            match self.peek_tag(offset)? {
                TokenTag::Ident
                    if depth == 0
                        && self.peek_name(offset).is_some_and(|name| name == AS_WORD)
                        && self.peek_tag(offset + 1) == Some(TokenTag::Ident)
                        && self.peek_tag(offset + 2) == Some(TokenTag::LBrace) =>
                {
                    return Some(self.index + offset);
                }
                TokenTag::LParen
                | TokenTag::LBracket
                | TokenTag::LBrace
                | TokenTag::DollarLBrace => depth += 1,
                TokenTag::RParen | TokenTag::RBracket | TokenTag::RBrace => {
                    depth = depth.checked_sub(1)?;
                }
                TokenTag::Semicolon if depth == 0 => return None,
                TokenTag::Eof => return None,
                _ => {}
            }
            offset += 1;
        }
    }

    pub(super) fn parse_atomically_arena_only(
        &mut self,
        start: usize,
        arena: &mut ArenaProgramBuilder<'_>,
    ) -> Option<()> {
        let keyword = self.bump();
        let replace = self.bump();
        let ends_at = self.head_as_index();
        let outer = std::mem::replace(&mut self.head_as_index, ends_at);
        let dest = self.parse_head_expr_arena_only(arena);
        self.head_as_index = outer;
        let dest = dest?.id;
        let dest_span = arena.expr_span(dest);
        if !self.current_name().is_some_and(|name| name == AS_WORD) {
            self.diagnostic_here(
                "expected `as NAME` after the destination of `atomically replace`",
                DiagnosticCode::ParseExpectedKeyword,
            );
            return None;
        }
        let as_word = self.bump();
        let name_start = self.current_start();
        let name = self.expect_ident("expected a name for the temporary path after `as`")?;
        let name_span = self.span(name_start, self.previous_end());
        let body_start = self.current_start();
        let body = self.parse_block_arena_only(arena)?;
        let span = self.span(start, self.previous_end());
        let target = arena.push_binding_target_name(name);
        let operands = AtomicallyOperands {
            keyword,
            replace,
            dest,
            dest_span,
            as_word,
            name,
            name_span,
            target,
            body,
            body_span: self.span(body_start, span.end()),
        };
        arena.push_sugar(
            SugarForm::Atomically,
            &[
                ArenaSugarOperand::Expr(dest),
                ArenaSugarOperand::BindingTarget(target),
                ArenaSugarOperand::Block(body),
            ],
            span,
            |arena| expand_atomically(arena, operands, span),
        );
        Some(())
    }
}

struct AtomicallyOperands {
    /// The `atomically` word.
    keyword: Span,
    /// The `replace` word.
    replace: Span,
    dest: ExprId,
    dest_span: Span,
    /// The `as` word.
    as_word: Span,
    name: Name,
    name_span: Span,
    target: BindingTargetId,
    body: BlockId,
    body_span: Span,
}

/// `atomically replace DEST as NAME { BODY }` is a block that binds `NAME`
/// to a fresh hidden path beside `DEST`, defers its removal, runs `BODY` as
/// an inner block, and then renames the path over `DEST`:
///
/// ```text
/// {
///   let %dest: Path = DEST
///   let NAME: Path = fs.temp_sibling(%dest)?
///   defer NAME.remove(missing_ok: true)
///   { BODY }
///   NAME.rename(to: %dest, overwrite: true)?
/// }
/// ```
///
/// The rename is the last statement, so it runs only when the body ran to
/// its end. Its `?` makes a failed rename the statement's failure wherever
/// the statement stands: without it the rename's `Result` would be the value
/// of the block, which the tail of a `try` block keeps as data. The removal is a
/// plain `defer`, not an `errdefer`: a body that leaves early without an
/// error (`return`, `break`, `continue`) must not leave the temporary path
/// behind either, and after a rename there is nothing at the path, which
/// `missing_ok` accepts.
///
/// The expansion adds seventeen expressions over a head of five parts, and
/// each needs a span no other expression has. The nodes a diagnostic can
/// name sit on the part of the head that says what they do: the call that
/// names the temporary path on `atomically replace` and its propagation on
/// `as NAME`, the deferred removal on `NAME`, the rename on `replace DEST`,
/// and its propagation on `replace DEST as`. The rest take distinct prefixes
/// and suffixes of the ten-byte `atomically` word.
fn expand_atomically(
    arena: &mut ArenaProgramBuilder<'_>,
    operands: AtomicallyOperands,
    span: Span,
) -> StmtId {
    let AtomicallyOperands {
        keyword,
        replace,
        dest,
        dest_span,
        as_word,
        name,
        name_span,
        target,
        body,
        body_span,
    } = operands;
    let within = |start: usize, end: usize| Span::new(span.source_id, start, end);
    // Nine proper prefixes and nine proper suffixes, none of them the word.
    let prefix = |len: usize| within(keyword.start(), keyword.start() + len);
    let suffix = |skip: usize| within(keyword.start() + skip, keyword.end());
    let dest_local = Name::intern(DEST_LOCAL);

    arena.begin_block();

    let dest_type = arena.push_named_type_expr(Name::intern("Path"), dest_span);
    let dest_target = arena.push_binding_target_name(dest_local);
    arena.push_binding_parts(
        true,
        dest_target,
        Some(dest_type),
        ArenaExprOrRun::Expr(dest),
        within(keyword.start(), dest_span.end()),
    );

    // The path is in the destination's directory, so the rename never
    // crosses a filesystem, and its name is drawn anew by every execution,
    // so two writers to one destination never share it.
    let module = arena.push_ident_expr(Name::intern("fs"), prefix(1));
    let callee = arena.push_field_expr(module, Name::intern("temp_sibling"), keyword);
    let beside = arena.push_ident_expr(dest_local, prefix(2));
    arena.begin_call_args();
    arena.push_call_arg_input(ArenaCallArgInput::Positional(beside));
    let args = arena.finish_call_args();
    let sibling = arena.push_call_expr(callee, args, within(keyword.start(), replace.end()));
    let temporary_span = within(as_word.start(), name_span.end());
    let temporary = arena.push_try_expr(sibling, temporary_span);
    let temporary_type = arena.push_named_type_expr(Name::intern("Path"), name_span);
    arena.push_binding_parts(
        true,
        target,
        Some(temporary_type),
        ArenaExprOrRun::Expr(temporary),
        temporary_span,
    );

    // Both operations are methods of the temporary path, which is declared
    // a `Path` above, so no name the body's file binds can change them.
    let removed = arena.push_ident_expr(name, suffix(3));
    let callee = arena.push_field_expr(removed, Name::intern("remove"), suffix(2));
    let missing_ok = arena.push_bool_expr(true, suffix(4));
    arena.begin_call_args();
    // A named argument starts before its value, as a written `name: value`
    // does; one that starts where its value starts is the shorthand `name:`.
    arena.push_call_arg_input(ArenaCallArgInput::Named {
        name: Name::intern("missing_ok"),
        value: missing_ok,
        span: keyword,
    });
    let args = arena.finish_call_args();
    let discard = arena.push_call_expr(callee, args, name_span);
    arena.push_defer(ArenaExprOrRun::Expr(discard), DeferTrigger::Exit, name_span);

    let inner = arena.push_value_block_expr(body, body_span);
    arena.push_expr_statement(inner, body_span);

    let source = arena.push_ident_expr(name, suffix(6));
    let callee = arena.push_field_expr(source, Name::intern("rename"), replace);
    let destination = arena.push_ident_expr(dest_local, suffix(7));
    let overwrite = arena.push_bool_expr(true, suffix(8));
    arena.begin_call_args();
    arena.push_call_arg_input(ArenaCallArgInput::Named {
        name: Name::intern("to"),
        value: destination,
        span: keyword,
    });
    arena.push_call_arg_input(ArenaCallArgInput::Named {
        name: Name::intern("overwrite"),
        value: overwrite,
        span: keyword,
    });
    let args = arena.finish_call_args();
    let rename = arena.push_call_expr(callee, args, within(replace.start(), dest_span.end()));
    let propagate_span = within(replace.start(), as_word.end());
    let propagate = arena.push_try_expr(rename, propagate_span);
    arena.push_expr_statement(propagate, propagate_span);

    // The block starts at `replace`: the whole statement's span may already
    // belong to a block, since a `match` arm that is one statement is a block
    // with that statement's span.
    let scope = within(replace.start(), span.end());
    let block = arena.finish_block(&[], scope);
    let outer = arena.push_value_block_expr(block, scope);
    arena.push_expr_statement(outer, span)
}

#[cfg(test)]
mod tests {
    use crate::source::SourceId;
    use crate::syntax::grammar::earley::Recognizer;
    use crate::syntax::grammar::{grammar, lex_grammar_tokens};
    use crate::syntax::parser::Parser;

    /// The grammar and the parser read the same texts as statements, and
    /// reject the same ones. `sentence` says which.
    fn assert_agree(cases: &[(&str, bool)]) {
        let recognizer = Recognizer::new(grammar());
        for (source, sentence) in cases {
            let tokens = lex_grammar_tokens(source).expect("lexes");
            assert_eq!(
                recognizer.recognize(&tokens).is_ok(),
                *sentence,
                "grammar: {source}"
            );
            let parsed = Parser::parse_source_arena_only(SourceId::new(0), source);
            assert_eq!(parsed.diagnostics.is_empty(), *sentence, "parser: {source}");
        }
    }

    /// A pattern test may end in `as NAME`, and so does the head. The `as`
    /// directly before a name and the `{` of the body ends the destination,
    /// wherever the destination's own lines break; an `as` inside brackets or
    /// before anything else names the pattern.
    #[test]
    fn the_destination_ends_at_the_as_before_the_name_and_the_body() {
        assert_agree(&[
            ("atomically replace x is Thing as b { }\n", true),
            ("atomically replace x is Thing as y as b { }\n", true),
            ("atomically replace y or x is Thing as b { }\n", true),
            ("atomically replace (x is Thing as y) as b { }\n", true),
            ("atomically replace { if v is Thing as t { print \"x\" }\nv } as b { }\n", true),
            ("atomically replace x +\n\ny is Thing as b { }\n", true),
            ("atomically replace x |> sort-by .size or\ny <= c as b { }\n", true),
            ("atomically replace as as as { }\n", true),
            ("if x is Thing as b { }\n", true),
            // The head breaks its line before the word that would end it.
            ("atomically replace x is Thing as y\nas b { }\n", false),
            ("atomically replace x as b\n", false),
            ("atomically replace x\n", false),
        ]);
    }

    /// The parser decides that a statement is an `atomically replace` or a
    /// `tempdir NAME at PATH` on the words that begin it, so the grammar has
    /// no command that begins with them: what follows is the statement's
    /// head or an error, never a command's arguments.
    #[test]
    fn the_words_that_begin_a_statement_never_begin_a_command() {
        assert_agree(&[
            ("atomically replace\n", false),
            ("atomically replace x y\n", false),
            ("tempdir a at\n", false),
            ("tempdir a at b\n", false),
            // A head that ends its line on an operator goes on, so the block
            // of the stage is not the body.
            ("tempdir a at b -\nc |> d |> any ( ) {\n\n| Thing | }\n", false),
            ("atomically replace b -\nc |> d |> any ( ) {\n\n| Thing | }\n", false),
            // Any other words are a command.
            ("atomically x\n", true),
            ("tempdir a b\n", true),
            ("replace x y\n", true),
        ]);
    }
}
