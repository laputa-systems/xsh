//! An Earley recognizer over the grammar productions.
//!
//! It proves the second direction of the grammar contract: every source the
//! parser accepts is a sentence of the productions. The extended-BNF rules
//! are compiled to plain BNF once; prediction is filtered by each rule's
//! FIRST set against the next token, which keeps the item sets small enough
//! to recognize the whole repository in seconds.

use super::{Grammar, GrammarToken, Item, Term};
use crate::syntax::token::{Keyword, TokenTag};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Symbol {
    Terminal(u32),
    Rule(u32),
    Look(u32),
    NameStatement(super::NameStatementForm),
    End,
}

#[derive(Debug)]
struct Lookahead {
    negative: bool,
    sequences: Vec<Vec<u32>>,
}

/// Why a token stream is not a sentence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rejection {
    /// The index of the first token no item could scan, or the token count
    /// when the input ended too early.
    pub token: usize,
    /// Terminals some item could have scanned there.
    pub expected: Vec<String>,
}

pub struct Recognizer {
    terminals: Vec<Term>,
    lookaheads: Vec<Lookahead>,
    names: Vec<String>,
    /// Flattened right-hand sides; each production ends with `Symbol::End`.
    symbols: Vec<Symbol>,
    /// The production that owns each position of `symbols`.
    owner: Vec<u32>,
    production_lhs: Vec<u32>,
    production_start: Vec<u32>,
    productions_of: Vec<Vec<u32>>,
    /// Derives the empty string without passing a lookahead.
    nullable: Vec<bool>,
    /// May derive the empty string, possibly through lookaheads.
    production_may_be_empty: Vec<bool>,
    production_first: Vec<u64>,
    /// Rules whose sentences contain no `NEWLINE`.
    single_line: Vec<bool>,
    words: usize,
    start: u32,
}

struct Compiler<'g> {
    grammar: &'g Grammar,
    terminals: Vec<Term>,
    terminal_ids: FxHashMap<Term, u32>,
    lookaheads: Vec<Lookahead>,
    names: Vec<String>,
    rule_ids: FxHashMap<&'g str, u32>,
    productions: Vec<(u32, Vec<Symbol>)>,
    single_line: Vec<u32>,
}

impl<'g> Compiler<'g> {
    fn terminal(&mut self, term: Term) -> u32 {
        if let Some(id) = self.terminal_ids.get(&term) {
            return *id;
        }
        let id = self.terminals.len() as u32;
        self.terminals.push(term);
        self.terminal_ids.insert(term, id);
        id
    }

    fn fresh(&mut self, name: String) -> u32 {
        self.names.push(name);
        (self.names.len() - 1) as u32
    }

    fn rule_id(&self, name: &str) -> u32 {
        *self
            .rule_ids
            .get(name)
            .unwrap_or_else(|| panic!("grammar refers to undefined rule `{name}`"))
    }

    /// Appends the symbols of `item` to `out`, introducing helper rules for
    /// alternation and repetition.
    fn flatten(&mut self, owner: &str, item: &Item, out: &mut Vec<Symbol>) {
        match item {
            Item::Term(term) => {
                let id = self.terminal(*term);
                out.push(Symbol::Terminal(id));
            }
            Item::NameStatement(form) => out.push(Symbol::NameStatement(*form)),
            Item::Rule(name) => out.push(Symbol::Rule(self.rule_id(name))),
            Item::Seq(items) => {
                for item in items {
                    self.flatten(owner, item, out);
                }
            }
            Item::Alt(items) => {
                let id = self.fresh(format!("{owner}#alt"));
                for item in items {
                    let mut rhs = Vec::new();
                    self.flatten(owner, item, &mut rhs);
                    self.productions.push((id, rhs));
                }
                out.push(Symbol::Rule(id));
            }
            Item::Opt(inner) => {
                let id = self.fresh(format!("{owner}#opt"));
                self.productions.push((id, Vec::new()));
                let mut rhs = Vec::new();
                self.flatten(owner, inner, &mut rhs);
                self.productions.push((id, rhs));
                out.push(Symbol::Rule(id));
            }
            Item::Star(inner) | Item::Plus(inner) => {
                let id = self.fresh(format!("{owner}#rep"));
                let mut once = Vec::new();
                self.flatten(owner, inner, &mut once);
                let mut again = vec![Symbol::Rule(id)];
                again.extend(once.iter().copied());
                if matches!(item, Item::Star(_)) {
                    self.productions.push((id, Vec::new()));
                } else {
                    self.productions.push((id, once));
                }
                self.productions.push((id, again));
                out.push(Symbol::Rule(id));
            }
            Item::List {
                item: inner,
                lines,
                min_one,
            } => {
                let expanded = list_items(inner, *lines, *min_one);
                self.flatten(owner, &expanded, out);
            }
            Item::Line(inner) => {
                let id = self.fresh(format!("{owner}#line"));
                let mut rhs = Vec::new();
                self.flatten(owner, inner, &mut rhs);
                self.productions.push((id, rhs));
                self.single_line.push(id);
                out.push(Symbol::Rule(id));
            }
            Item::Not(sequences) | Item::Peek(sequences) => {
                let sequences = sequences
                    .iter()
                    .map(|sequence| sequence.iter().map(|term| self.terminal(*term)).collect())
                    .collect();
                self.lookaheads.push(Lookahead {
                    negative: matches!(item, Item::Not(_)),
                    sequences,
                });
                out.push(Symbol::Look((self.lookaheads.len() - 1) as u32));
            }
        }
    }
}

/// The extended-BNF meaning of a list: `item ("," item)* ","?`, with line
/// breaks around items and commas when `lines`.
pub(crate) fn list_items(item: &Item, lines: bool, min_one: bool) -> Item {
    let newlines = || {
        if lines {
            Item::Star(Box::new(Item::Term(Term {
                class: super::Class::Tag(TokenTag::Newline),
                glued: false,
            })))
        } else {
            Item::Seq(Vec::new())
        }
    };
    let comma = || {
        Item::Term(Term {
            class: super::Class::Tag(TokenTag::Comma),
            glued: false,
        })
    };
    let body = Item::Seq(vec![
        item.clone(),
        Item::Star(Box::new(Item::Seq(vec![
            newlines(),
            comma(),
            newlines(),
            item.clone(),
        ]))),
        Item::Opt(Box::new(Item::Seq(vec![newlines(), comma()]))),
    ]);
    let body = if min_one {
        body
    } else {
        Item::Opt(Box::new(body))
    };
    Item::Seq(vec![newlines(), body, newlines()])
}

impl Recognizer {
    pub fn new(grammar: &Grammar) -> Self {
        let mut compiler = Compiler {
            grammar,
            terminals: Vec::new(),
            terminal_ids: FxHashMap::default(),
            lookaheads: Vec::new(),
            names: Vec::new(),
            rule_ids: FxHashMap::default(),
            productions: Vec::new(),
            single_line: Vec::new(),
        };
        for rule in &grammar.rules {
            let id = compiler.fresh(rule.name.to_string());
            assert!(
                compiler.rule_ids.insert(rule.name, id).is_none(),
                "rule `{}` is defined twice",
                rule.name
            );
        }
        for rule in &compiler.grammar.rules {
            let id = compiler.rule_id(rule.name);
            let mut rhs = Vec::new();
            compiler.flatten(rule.name, &rule.body, &mut rhs);
            compiler.productions.push((id, rhs));
        }
        let program = compiler.rule_id(Grammar::START);
        let start = compiler.fresh("start".to_string());
        compiler
            .productions
            .push((start, vec![Symbol::Rule(program)]));

        let rule_count = compiler.names.len();
        let mut symbols = Vec::new();
        let mut owner = Vec::new();
        let mut production_lhs = Vec::new();
        let mut production_start = Vec::new();
        let mut productions_of = vec![Vec::new(); rule_count];
        for (index, (lhs, rhs)) in compiler.productions.iter().enumerate() {
            production_lhs.push(*lhs);
            production_start.push(symbols.len() as u32);
            productions_of[*lhs as usize].push(index as u32);
            for symbol in rhs.iter().copied().chain([Symbol::End]) {
                symbols.push(symbol);
                owner.push(index as u32);
            }
        }

        let derives_empty = |strict: bool| {
            let mut empty = vec![false; rule_count];
            loop {
                let mut changed = false;
                for (lhs, rhs) in &compiler.productions {
                    if empty[*lhs as usize] {
                        continue;
                    }
                    let all = rhs.iter().all(|symbol| match symbol {
                        Symbol::Terminal(_) | Symbol::End => false,
                        Symbol::Rule(id) => empty[*id as usize],
                        Symbol::Look(_) | Symbol::NameStatement(_) => !strict,
                    });
                    if all {
                        empty[*lhs as usize] = true;
                        changed = true;
                    }
                }
                if !changed {
                    return empty;
                }
            }
        };
        let nullable = derives_empty(true);
        let may_be_empty = derives_empty(false);

        let words = compiler.terminals.len().div_ceil(64).max(1);
        let mut first = vec![0u64; rule_count * words];
        loop {
            let mut changed = false;
            for (lhs, rhs) in &compiler.productions {
                let mut add = vec![0u64; words];
                for symbol in rhs {
                    match symbol {
                        Symbol::Terminal(id) => {
                            add[*id as usize / 64] |= 1 << (id % 64);
                            break;
                        }
                        Symbol::Rule(id) => {
                            let from = *id as usize * words;
                            for word in 0..words {
                                add[word] |= first[from + word];
                            }
                            if !may_be_empty[*id as usize] {
                                break;
                            }
                        }
                        Symbol::Look(_) | Symbol::NameStatement(_) | Symbol::End => {}
                    }
                }
                let to = *lhs as usize * words;
                for word in 0..words {
                    if first[to + word] | add[word] != first[to + word] {
                        first[to + word] |= add[word];
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut production_first = vec![0u64; compiler.productions.len() * words];
        let mut production_may_be_empty = vec![false; compiler.productions.len()];
        for (index, (_, rhs)) in compiler.productions.iter().enumerate() {
            let mut empty = true;
            for symbol in rhs {
                match symbol {
                    Symbol::Terminal(id) => {
                        production_first[index * words + *id as usize / 64] |= 1 << (id % 64);
                        empty = false;
                        break;
                    }
                    Symbol::Rule(id) => {
                        for word in 0..words {
                            production_first[index * words + word] |=
                                first[*id as usize * words + word];
                        }
                        if !may_be_empty[*id as usize] {
                            empty = false;
                            break;
                        }
                    }
                    Symbol::Look(_) | Symbol::NameStatement(_) | Symbol::End => {}
                }
            }
            production_may_be_empty[index] = empty;
        }

        let mut single_line = vec![false; rule_count];
        for id in &compiler.single_line {
            single_line[*id as usize] = true;
        }
        Self {
            single_line,
            terminals: compiler.terminals,
            lookaheads: compiler.lookaheads,
            names: compiler.names,
            symbols,
            owner,
            production_lhs,
            production_start,
            productions_of,
            nullable,
            production_may_be_empty,
            production_first,
            words,
            start,
        }
    }

    /// The number of BNF productions the grammar compiles to.
    pub fn production_count(&self) -> usize {
        self.production_lhs.len()
    }

    fn token_terminals(&self, tokens: &[GrammarToken<'_>]) -> Vec<u64> {
        let mut cache: FxHashMap<(TokenTag, Option<Keyword>, bool, &str), usize> =
            FxHashMap::default();
        let mut rows: Vec<u64> = Vec::new();
        let mut matches = Vec::with_capacity(tokens.len() * self.words);
        for token in tokens {
            let text = if matches!(
                token.tag,
                TokenTag::Ident | TokenTag::ProcIdent | TokenTag::Int
            ) || token.text.len() <= 3
            {
                token.text
            } else {
                // Only identifier-like words and short tokens are spelled in
                // the grammar; other text never changes which terminals match
                // except for bare-path parts, which are decided by character.
                if token
                    .text
                    .chars()
                    .all(crate::syntax::literal::is_bare_path_literal_char)
                {
                    "/"
                } else {
                    "\""
                }
            };
            let key = (token.tag, token.keyword, token.glued, text);
            let row = *cache.entry(key).or_insert_with(|| {
                let start = rows.len();
                rows.resize(start + self.words, 0);
                for (id, term) in self.terminals.iter().enumerate() {
                    if term.matches(token) {
                        rows[start + id / 64] |= 1 << (id % 64);
                    }
                }
                start
            });
            matches.extend_from_slice(&rows[row..row + self.words]);
        }
        matches
    }

    fn describe(&self, terminal: u32) -> String {
        let term = self.terminals[terminal as usize];
        let text = super::reference::terminal_text(&term.class);
        if term.glued { format!("~{text}") } else { text }
    }

    /// Whether `tokens` is a sentence of the grammar's start rule.
    pub fn recognize(&self, tokens: &[GrammarToken<'_>]) -> Result<(), Rejection> {
        let words = self.words;
        let matches = self.token_terminals(tokens);
        let has = |position: usize, terminal: u32| -> bool {
            position < tokens.len()
                && matches[position * words + terminal as usize / 64] & (1 << (terminal % 64)) != 0
        };
        let look_passes = |look: u32, position: usize| -> bool {
            let lookahead = &self.lookaheads[look as usize];
            let found = lookahead.sequences.iter().any(|sequence| {
                sequence
                    .iter()
                    .enumerate()
                    .all(|(offset, terminal)| has(position + offset, *terminal))
            });
            if lookahead.negative {
                !found
            } else {
                found || position == tokens.len()
            }
        };
        // Line breaks before each position, for single-line rules.
        let mut breaks = Vec::with_capacity(tokens.len() + 1);
        breaks.push(0u32);
        for token in tokens {
            breaks.push(
                breaks.last().copied().unwrap_or(0) + u32::from(token.tag == TokenTag::Newline),
            );
        }
        let rule_count = self.names.len();
        let mut predicted = vec![u32::MAX; rule_count];
        let mut empty_completed = vec![u32::MAX; rule_count];
        // For every finished set: (waiting rule, position, origin), sorted.
        let mut waiting: Vec<Vec<(u32, u32, u32)>> = Vec::with_capacity(tokens.len() + 1);
        let mut current: Vec<(u32, u32)> = vec![(
            self.production_start[self.productions_of[self.start as usize][0] as usize],
            0,
        )];
        let mut seen: FxHashSet<u64> = FxHashSet::default();
        let mut next: Vec<(u32, u32)> = Vec::new();
        let mut next_seen: FxHashSet<u64> = FxHashSet::default();
        seen.insert(u64::from(current[0].0) << 32);

        for position in 0..=tokens.len() {
            let set = position as u32;
            let mut index = 0;
            while index < current.len() {
                let (dot, origin) = current[index];
                index += 1;
                let mut add = |item: (u32, u32), current: &mut Vec<(u32, u32)>| {
                    if seen.insert(u64::from(item.0) << 32 | u64::from(item.1)) {
                        current.push(item);
                    }
                };
                match self.symbols[dot as usize] {
                    Symbol::End => {
                        let lhs = self.production_lhs[self.owner[dot as usize] as usize];
                        if self.single_line[lhs as usize]
                            && breaks[position] != breaks[origin as usize]
                        {
                            continue;
                        }
                        if origin == set {
                            if !self.nullable[lhs as usize] {
                                empty_completed[lhs as usize] = set;
                                let mut scan = 0;
                                while scan < current.len() {
                                    let (waiting_dot, waiting_origin) = current[scan];
                                    if self.symbols[waiting_dot as usize] == Symbol::Rule(lhs) {
                                        add((waiting_dot + 1, waiting_origin), &mut current);
                                    }
                                    scan += 1;
                                }
                            }
                        } else {
                            let items = &waiting[origin as usize];
                            let from = items.partition_point(|entry| entry.0 < lhs);
                            for &(_, waiting_dot, waiting_origin) in
                                items[from..].iter().take_while(|entry| entry.0 == lhs)
                            {
                                add((waiting_dot + 1, waiting_origin), &mut current);
                            }
                        }
                    }
                    Symbol::Terminal(terminal) => {
                        if has(position, terminal)
                            && next_seen.insert(u64::from(dot + 1) << 32 | u64::from(origin))
                        {
                            next.push((dot + 1, origin));
                        }
                    }
                    Symbol::Look(look) => {
                        if look_passes(look, position) {
                            add((dot + 1, origin), &mut current);
                        }
                    }
                    Symbol::NameStatement(form) => {
                        if super::name_statement_matches(form, |offset| tokens.get(position + offset).copied())
                        {
                            add((dot + 1, origin), &mut current);
                        }
                    }
                    Symbol::Rule(rule) => {
                        if predicted[rule as usize] != set {
                            predicted[rule as usize] = set;
                            for &production in &self.productions_of[rule as usize] {
                                let viable = self.production_may_be_empty[production as usize]
                                    || (position < tokens.len()
                                        && (0..words).any(|word| {
                                            self.production_first
                                                [production as usize * words + word]
                                                & matches[position * words + word]
                                                != 0
                                        }));
                                if viable {
                                    add(
                                        (self.production_start[production as usize], set),
                                        &mut current,
                                    );
                                }
                            }
                        }
                        if self.nullable[rule as usize] || empty_completed[rule as usize] == set {
                            add((dot + 1, origin), &mut current);
                        }
                    }
                }
            }

            if position == tokens.len() {
                let accepted = current.iter().any(|&(dot, origin)| {
                    origin == 0
                        && self.symbols[dot as usize] == Symbol::End
                        && self.production_lhs[self.owner[dot as usize] as usize] == self.start
                });
                return if accepted {
                    Ok(())
                } else {
                    Err(self.rejection(position, &current))
                };
            }
            if next.is_empty() {
                return Err(self.rejection(position, &current));
            }
            let mut finished: Vec<(u32, u32, u32)> = current
                .iter()
                .filter_map(|&(dot, origin)| match self.symbols[dot as usize] {
                    Symbol::Rule(rule) => Some((rule, dot, origin)),
                    _ => None,
                })
                .collect();
            finished.sort_unstable();
            waiting.push(finished);
            current.clear();
            std::mem::swap(&mut current, &mut next);
            seen.clear();
            std::mem::swap(&mut seen, &mut next_seen);
        }
        unreachable!("the loop returns at the end of input")
    }

    fn rejection(&self, position: usize, items: &[(u32, u32)]) -> Rejection {
        let mut expected: Vec<String> = items
            .iter()
            .filter_map(|&(dot, _)| match self.symbols[dot as usize] {
                Symbol::Terminal(terminal) => Some(self.describe(terminal)),
                _ => None,
            })
            .collect();
        expected.sort();
        expected.dedup();
        Rejection {
            token: position,
            expected,
        }
    }
}

/// Splits a program's terminals before top-level declarations so each part
/// can be recognized on its own. Joining sentences of `program` with a
/// `NEWLINE` is again a sentence, so a split never accepts a non-sentence; a
/// split inside a statement would only reject.
pub fn top_level_parts<'a, 's>(tokens: &'a [GrammarToken<'s>]) -> Vec<&'a [GrammarToken<'s>]> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (index, token) in tokens.iter().enumerate() {
        match token.tag {
            TokenTag::LParen | TokenTag::LBracket | TokenTag::LBrace | TokenTag::DollarLBrace => {
                depth += 1
            }
            TokenTag::RParen | TokenTag::RBracket | TokenTag::RBrace => {
                depth = depth.saturating_sub(1)
            }
            TokenTag::Newline if depth == 0 && index > start => {
                let begins_declaration = tokens.get(index + 1).is_some_and(|next| {
                    matches!(
                        next.keyword,
                        Some(
                            Keyword::Proc
                                | Keyword::Pure
                                | Keyword::Stream
                                | Keyword::Let
                                | Keyword::Const
                                | Keyword::Use
                                | Keyword::Type
                                | Keyword::Enum
                                | Keyword::Export
                        )
                    ) || (next.tag == TokenTag::Ident && matches!(next.text, "test" | "cli"))
                });
                if begins_declaration {
                    parts.push(&tokens[start..index]);
                    start = index + 1;
                }
            }
            _ => {}
        }
    }
    parts.push(&tokens[start..]);
    parts
}
