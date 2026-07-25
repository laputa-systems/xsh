use crate::diagnostic::Diagnostic;
use crate::source::{SourceId, Span};
use crate::syntax::lexer::Lexer;
use crate::syntax::token::{TokenId, TokenTable, TokenTag};
use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SyntaxNodeId(NonZeroU32);

impl SyntaxNodeId {
    const fn new(index: usize) -> Self {
        assert!(index < u32::MAX as usize);
        Self(NonZeroU32::new(index as u32 + 1).expect("non-zero syntax node id"))
    }

    pub const fn raw(self) -> usize {
        self.0.get() as usize - 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SyntaxTokenId(NonZeroU32);

impl SyntaxTokenId {
    const fn new(index: usize) -> Self {
        assert!(index < u32::MAX as usize);
        Self(NonZeroU32::new(index as u32 + 1).expect("non-zero syntax token id"))
    }

    pub const fn raw(self) -> usize {
        self.0.get() as usize - 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct SyntaxTriviaId(NonZeroU32);

impl SyntaxTriviaId {
    const fn new(index: usize) -> Self {
        assert!(index < u32::MAX as usize);
        Self(NonZeroU32::new(index as u32 + 1).expect("non-zero syntax trivia id"))
    }

    pub const fn raw(self) -> usize {
        self.0.get() as usize - 1
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxKind {
    Root,
    Group(SyntaxGroupKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxGroupKind {
    Paren,
    Brace,
    Bracket,
    Interpolation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxElement {
    Node(SyntaxNodeId),
    Token(SyntaxTokenId),
    Trivia(SyntaxTriviaId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxNode {
    pub kind: SyntaxKind,
    pub span: Span,
    pub children: Vec<SyntaxElement>,
    pub open: Option<SyntaxTokenId>,
    pub close: Option<SyntaxTokenId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxToken {
    pub token: TokenId,
    pub len: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxTrivia {
    pub kind: TriviaKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriviaKind {
    Whitespace,
    Newline,
    Comment,
    Skipped,
}

/// A deferred `SyntaxTree` built from a captured token table and source text.
///
/// The parser always lexes and needs the token table, but building the full
/// syntax tree (one entry per token for `tokens`/`trivia`, one node per
/// bracket/paren/brace group) is a second full pass over every token that
/// only tooling consumers (the `xsht` formatter/editor) actually read. The
/// interpreter's own script-loading and lowering paths hold onto this value
/// only long enough to drop it, so building it eagerly was pure waste on the
/// hot path. `get()` builds and caches the tree on first access; clones share
/// the same cache via the inner `Arc`, so cloning a parsed unit never forces
/// (or duplicates) the syntax-tree build.
#[derive(Clone, Debug)]
pub struct LazyCst {
    source_id: SourceId,
    source: Arc<str>,
    token_table: TokenTable,
    cell: Arc<OnceLock<SyntaxTree>>,
}

impl LazyCst {
    pub fn new(source_id: SourceId, source: &str, token_table: TokenTable) -> Self {
        Self {
            source_id,
            source: Arc::from(source),
            token_table,
            cell: Arc::new(OnceLock::new()),
        }
    }

    pub fn empty(source_id: SourceId) -> Self {
        Self {
            source_id,
            source: Arc::from(""),
            token_table: TokenTable::default(),
            cell: Arc::new(OnceLock::from(SyntaxTree::empty(source_id))),
        }
    }

    pub fn get(&self) -> &SyntaxTree {
        self.cell.get_or_init(|| {
            SyntaxTree::from_token_table(self.source_id, &self.source, self.token_table.clone())
        })
    }

    pub fn token_table(&self) -> &TokenTable {
        &self.token_table
    }

    /// Bytes retained by the deferred CST handle. Does not force tree construction.
    pub fn retained_bytes(&self) -> usize {
        let mut total = std::mem::size_of::<Self>()
            + self.source.len()
            + self.token_table.retained_bytes();
        if let Some(tree) = self.cell.get() {
            total = total.saturating_add(tree.retained_bytes_without_token_table());
        }
        total
    }

    pub fn cst_built(&self) -> bool {
        self.cell.get().is_some()
    }
}

impl Default for LazyCst {
    fn default() -> Self {
        Self::empty(SourceId::new(0))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxTree {
    source_id: SourceId,
    source: Arc<str>,
    root: SyntaxNodeId,
    token_table: TokenTable,
    nodes: Vec<SyntaxNode>,
    tokens: Vec<SyntaxToken>,
    trivia: Vec<SyntaxTrivia>,
}

impl Default for SyntaxTree {
    fn default() -> Self {
        Self::empty(SourceId::new(0))
    }
}

impl SyntaxTree {
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn element_token_count(&self) -> usize {
        self.tokens.len()
    }

    pub fn trivia_count(&self) -> usize {
        self.trivia.len()
    }

    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes_without_token_table() + self.token_table.retained_bytes()
    }

    pub fn retained_bytes_without_token_table(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>()
            + self.source.len()
            + self.nodes.capacity() * size_of::<SyntaxNode>()
            + self.tokens.capacity() * size_of::<SyntaxToken>()
            + self.trivia.capacity() * size_of::<SyntaxTrivia>()
            + self
                .nodes
                .iter()
                .map(|node| node.children.capacity() * size_of::<SyntaxElement>())
                .sum::<usize>()
    }

    pub fn parse(source_id: SourceId, source: &str) -> (Self, Vec<Diagnostic>) {
        let lexed = Lexer::new(source_id, source).lex_compact();
        let tree = Self::from_token_table(source_id, source, lexed.token_table);
        (tree, lexed.diagnostics)
    }

    pub fn empty(source_id: SourceId) -> Self {
        let root = SyntaxNodeId::new(0);
        Self {
            source_id,
            source: Arc::from(""),
            root,
            token_table: TokenTable::default(),
            nodes: vec![SyntaxNode {
                kind: SyntaxKind::Root,
                span: Span::new(source_id, 0, 0),
                children: Vec::new(),
                open: None,
                close: None,
            }],
            tokens: Vec::new(),
            trivia: Vec::new(),
        }
    }

    pub fn from_token_table(source_id: SourceId, source: &str, token_table: TokenTable) -> Self {
        // Every lexed token becomes exactly one `tokens` or `trivia` entry, so the
        // full token count is a safe (if occasionally loose) upper bound on `tokens`
        // that avoids growth reallocations while walking the table below.
        let token_capacity = token_table.len();
        // `nodes` always holds the root, so growing its initial capacity carries no
        // risk of allocating for a feature a file never uses (unlike a fresh
        // collection); every open-paren/brace/bracket/interpolation group pushes
        // one more entry, so a modest token-proportional reservation avoids most
        // growth reallocations for typical, group-heavy scripts.
        let mut nodes = Vec::with_capacity(token_capacity / 8 + 1);
        nodes.push(SyntaxNode {
            kind: SyntaxKind::Root,
            span: Span::new(source_id, 0, source.len()),
            // The root holds every top-level token/group as a direct child, so it
            // is always populated for a non-empty file; reserve like a group's
            // children to skip the same early growth reallocations.
            children: Vec::with_capacity(token_capacity / 4 + 1),
            open: None,
            close: None,
        });
        let mut tree = Self {
            source_id,
            source: Arc::from(source),
            root: SyntaxNodeId::new(0),
            token_table,
            nodes,
            tokens: Vec::with_capacity(token_capacity),
            // Comments, newlines, and whitespace gaps average ~14% of tokens across
            // the checked-in corpus; every file has at least newline trivia, so this
            // reservation is safe to make unconditionally.
            trivia: Vec::with_capacity(token_capacity / 6 + 1),
        };

        let mut stack = vec![tree.root];
        let mut offset = 0usize;
        for token_index in 0..tree.token_table.len() {
            let tag = tree
                .token_table
                .tag_at(token_index)
                .expect("token index comes from token table length");
            let span = tree
                .token_table
                .span_at(token_index, source_id, source)
                .expect("token index comes from token table length");
            if span.start() > offset {
                tree.push_gap_trivia(*stack.last().expect("root stack"), offset, span.start());
            }
            offset = offset.max(span.end());

            match tag {
                TokenTag::Comment => {
                    tree.push_trivia(
                        *stack.last().expect("root stack"),
                        TriviaKind::Comment,
                        span,
                    );
                }
                TokenTag::Newline => {
                    tree.push_trivia(
                        *stack.last().expect("root stack"),
                        TriviaKind::Newline,
                        span,
                    );
                }
                TokenTag::Eof => {
                    stack.truncate(1);
                    let token_id = tree.push_token(token_index, span);
                    tree.push_child(tree.root, SyntaxElement::Token(token_id));
                }
                _ => tree.push_syntax_token(token_index, tag, span, &mut stack),
            }
        }

        if source.len() > offset {
            tree.push_gap_trivia(tree.root, offset, source.len());
        }
        tree.close_unclosed_groups(source.len());
        tree
    }

    pub fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn root(&self) -> SyntaxNodeId {
        self.root
    }

    pub fn token_table(&self) -> &TokenTable {
        &self.token_table
    }

    pub fn node(&self, id: SyntaxNodeId) -> &SyntaxNode {
        &self.nodes[id.raw()]
    }

    pub fn token(&self, id: SyntaxTokenId) -> &SyntaxToken {
        &self.tokens[id.raw()]
    }

    pub fn trivia(&self, id: SyntaxTriviaId) -> &SyntaxTrivia {
        &self.trivia[id.raw()]
    }

    pub fn nodes(&self) -> &[SyntaxNode] {
        &self.nodes
    }

    pub fn tokens(&self) -> &[SyntaxToken] {
        &self.tokens
    }

    pub fn trivia_items(&self) -> &[SyntaxTrivia] {
        &self.trivia
    }

    pub fn comment_trivia(&self) -> impl Iterator<Item = (SyntaxTriviaId, &SyntaxTrivia)> {
        self.trivia
            .iter()
            .enumerate()
            .filter_map(|(index, trivia)| {
                (trivia.kind == TriviaKind::Comment).then_some((SyntaxTriviaId::new(index), trivia))
            })
    }

    pub fn token_text(&self, id: SyntaxTokenId) -> &str {
        self.span_text(self.token_span(id))
    }

    pub fn token_span(&self, id: SyntaxTokenId) -> Span {
        let token = self.token(id);
        let start = self.token_table.start(token.token);
        Span::new(self.source_id, start, start + token.len as usize)
    }

    pub fn trivia_text(&self, id: SyntaxTriviaId) -> &str {
        self.span_text(self.trivia(id).span)
    }

    pub fn span_text(&self, span: Span) -> &str {
        if span.source_id != self.source_id {
            return "";
        }
        self.source.get(span.start()..span.end()).unwrap_or("")
    }

    pub fn exact_text(&self) -> String {
        let mut output = String::new();
        self.write_node(self.root, &mut output);
        output
    }

    pub fn tokens_in_span(&self, span: Span) -> Vec<SyntaxTokenId> {
        self.tokens
            .iter()
            .enumerate()
            .filter_map(|(index, _)| {
                let token_span = self.token_span(SyntaxTokenId::new(index));
                (token_span.source_id == span.source_id
                    && token_span.start() >= span.start()
                    && token_span.end() <= span.end())
                .then_some(SyntaxTokenId::new(index))
            })
            .collect()
    }

    pub fn trivia_in_span(&self, span: Span) -> Vec<SyntaxTriviaId> {
        self.trivia
            .iter()
            .enumerate()
            .filter_map(|(index, trivia)| {
                (trivia.span.source_id == span.source_id
                    && trivia.span.start() >= span.start()
                    && trivia.span.end() <= span.end())
                .then_some(SyntaxTriviaId::new(index))
            })
            .collect()
    }

    pub fn contains_comment(&self, span: Span) -> bool {
        self.trivia_in_span(span)
            .into_iter()
            .any(|id| self.trivia(id).kind == TriviaKind::Comment)
    }

    pub fn covering_node(&self, span: Span) -> Option<SyntaxNodeId> {
        if span.source_id != self.source_id {
            return None;
        }
        self.covering_node_from(self.root, span)
    }

    fn push_syntax_token(
        &mut self,
        token_index: usize,
        tag: TokenTag,
        span: Span,
        stack: &mut Vec<SyntaxNodeId>,
    ) {
        let current = *stack.last().expect("root stack");
        let token_id = self.push_token(token_index, span);
        match opening_group(tag) {
            Some(group_kind) => {
                // Most groups (call args, blocks, list/record literals) hold more
                // than the opening token before they close; starting at capacity 4
                // instead of 1 avoids the first one or two growth reallocations for
                // the common case.
                let mut children = Vec::with_capacity(8);
                children.push(SyntaxElement::Token(token_id));
                let node_id = self.push_node(SyntaxNode {
                    kind: SyntaxKind::Group(group_kind),
                    span,
                    children,
                    open: Some(token_id),
                    close: None,
                });
                self.push_child(current, SyntaxElement::Node(node_id));
                stack.push(node_id);
            }
            None => {
                let current = *stack.last().expect("root stack");
                self.push_child(current, SyntaxElement::Token(token_id));
                if closing_matches(self.nodes[current.raw()].kind, tag) {
                    self.nodes[current.raw()].close = Some(token_id);
                    self.nodes[current.raw()].span.set_end(span.end());
                    if stack.len() > 1 {
                        stack.pop();
                    }
                }
            }
        }
    }

    fn push_token(&mut self, token_index: usize, span: Span) -> SyntaxTokenId {
        let id = SyntaxTokenId::new(self.tokens.len());
        self.tokens.push(SyntaxToken {
            token: TokenId::new(token_index),
            len: u32::try_from(span.end() - span.start())
                .expect("syntax token length exceeded u32"),
        });
        id
    }

    fn push_node(&mut self, node: SyntaxNode) -> SyntaxNodeId {
        let id = SyntaxNodeId::new(self.nodes.len());
        self.nodes.push(node);
        id
    }

    fn push_trivia(&mut self, parent: SyntaxNodeId, kind: TriviaKind, span: Span) {
        let id = SyntaxTriviaId::new(self.trivia.len());
        self.trivia.push(SyntaxTrivia { kind, span });
        self.push_child(parent, SyntaxElement::Trivia(id));
    }

    fn push_gap_trivia(&mut self, parent: SyntaxNodeId, start: usize, end: usize) {
        let text = self.source.get(start..end).unwrap_or("");
        if text.is_empty() {
            return;
        }
        let kind = if text.bytes().all(|byte| matches!(byte, b' ' | b'\t')) {
            TriviaKind::Whitespace
        } else if text.bytes().all(|byte| matches!(byte, b'\r' | b'\n')) {
            TriviaKind::Newline
        } else {
            TriviaKind::Skipped
        };
        self.push_trivia(parent, kind, Span::new(self.source_id, start, end));
    }

    fn push_child(&mut self, parent: SyntaxNodeId, child: SyntaxElement) {
        self.nodes[parent.raw()].children.push(child);
    }

    fn close_unclosed_groups(&mut self, end: usize) {
        for node in &mut self.nodes {
            if matches!(node.kind, SyntaxKind::Group(_)) && node.close.is_none() {
                node.close = None;
                node.span.set_end(end);
            }
        }
    }

    fn write_node(&self, id: SyntaxNodeId, output: &mut String) {
        for child in &self.node(id).children {
            match *child {
                SyntaxElement::Node(node) => self.write_node(node, output),
                SyntaxElement::Token(token) => output.push_str(self.token_text(token)),
                SyntaxElement::Trivia(trivia) => output.push_str(self.trivia_text(trivia)),
            }
        }
    }

    fn covering_node_from(&self, node_id: SyntaxNodeId, span: Span) -> Option<SyntaxNodeId> {
        let node = self.node(node_id);
        if !span_contains(node.span, span) {
            return None;
        }
        node.children
            .iter()
            .filter_map(|child| match *child {
                SyntaxElement::Node(child_id) => self.covering_node_from(child_id, span),
                SyntaxElement::Token(_) | SyntaxElement::Trivia(_) => None,
            })
            .next()
            .or(Some(node_id))
    }
}

fn opening_group(tag: TokenTag) -> Option<SyntaxGroupKind> {
    Some(match tag {
        TokenTag::LParen => SyntaxGroupKind::Paren,
        TokenTag::LBrace => SyntaxGroupKind::Brace,
        TokenTag::LBracket => SyntaxGroupKind::Bracket,
        TokenTag::DollarLBrace => SyntaxGroupKind::Interpolation,
        _ => return None,
    })
}

fn closing_matches(node_kind: SyntaxKind, tag: TokenTag) -> bool {
    matches!(
        (node_kind, tag),
        (SyntaxKind::Group(SyntaxGroupKind::Paren), TokenTag::RParen)
            | (SyntaxKind::Group(SyntaxGroupKind::Brace), TokenTag::RBrace)
            | (
                SyntaxKind::Group(SyntaxGroupKind::Bracket),
                TokenTag::RBracket
            )
            | (
                SyntaxKind::Group(SyntaxGroupKind::Interpolation),
                TokenTag::RBrace
            )
    )
}

fn span_contains(outer: Span, inner: Span) -> bool {
    outer.source_id == inner.source_id
        && outer.start() <= inner.start()
        && outer.end() >= inner.end()
}
