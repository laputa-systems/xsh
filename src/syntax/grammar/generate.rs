//! Random sentences of the grammar, for proving that the productions accept
//! nothing the parser rejects.
//!
//! Generation is deterministic for a seed and bounded by a rule-expansion
//! depth: past the bound every choice takes an alternative of least height,
//! so every derivation finishes. Lookaheads are honored by regenerating the
//! item they guard; a candidate that still misses one is not a sentence, and
//! the caller filters candidates through the recognizer before parsing them.

use super::{Class, Grammar, GrammarToken, Item, Term};
use crate::syntax::lexer::tokens_stay_separate;
use crate::syntax::token::{Keyword, TokenTag};
use rustc_hash::{FxHashMap, FxHashSet};

/// Identifiers that no production spells as a contextual word, so a
/// generated name never changes how the parser dispatches.
pub const NAMES: [&str; 8] = ["a", "b", "c", "x", "y", "value", "items", "Thing"];

/// One emitted terminal.
#[derive(Clone, Debug)]
struct Emitted {
    tag: TokenTag,
    keyword: Option<Keyword>,
    text: String,
    glued: bool,
}

impl Emitted {
    fn token(&self) -> GrammarToken<'_> {
        GrammarToken {
            tag: self.tag,
            keyword: self.keyword,
            text: &self.text,
            glued: self.glued,
        }
    }
}

pub struct Generator<'g> {
    rules: FxHashMap<&'static str, &'g Item>,
    heights: FxHashMap<&'static str, u32>,
    state: u64,
    /// A glued terminal could not be spelled apart from the token before
    /// it, so the candidate is not a sentence.
    merged: bool,
    /// Inside an item written on one line: optional line breaks are left out.
    single_line: u32,
    /// The rules the last candidate expanded.
    expanded: FxHashSet<&'static str>,
    /// For a targeted candidate, each rule's least number of expansions to
    /// reach the target rule; cleared once the target is expanded.
    toward: Option<FxHashMap<&'static str, u32>>,
    /// Whether the item being expanded is on the shortest way to the target;
    /// the other items of a sequence are generated freely.
    steering: bool,
}

impl<'g> Generator<'g> {
    pub fn new(grammar: &'g Grammar) -> Self {
        let rules: FxHashMap<&'static str, &'g Item> = grammar
            .rules
            .iter()
            .map(|rule| (rule.name, &rule.body))
            .collect();
        let mut heights: FxHashMap<&'static str, u32> = FxHashMap::default();
        loop {
            let mut changed = false;
            for rule in &grammar.rules {
                if let Some(height) = item_height(&rule.body, &heights).map(|height| height + 1)
                    && heights.get(rule.name).is_none_or(|known| height < *known)
                {
                    heights.insert(rule.name, height);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Self {
            rules,
            heights,
            state: 0,
            merged: false,
            single_line: 0,
            expanded: FxHashSet::default(),
            toward: None,
            steering: false,
        }
    }

    /// Rules with no finite derivation, which generation could never finish.
    pub fn underivable_rules(&self) -> Vec<&'static str> {
        let mut missing: Vec<&'static str> = self
            .rules
            .keys()
            .filter(|name| !self.heights.contains_key(*name))
            .copied()
            .collect();
        missing.sort_unstable();
        missing
    }

    fn next(&mut self) -> u64 {
        // SplitMix64.
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    /// A candidate like [`Generator::sentence`] that takes the shortest way to
    /// expand `target` before choosing freely, so every production can be
    /// exercised from the start rule.
    pub fn targeted_sentence(
        &mut self,
        start: &str,
        target: &str,
        seed: u64,
        depth: u32,
    ) -> Option<String> {
        let mut distance: FxHashMap<&'static str, u32> = FxHashMap::default();
        let (target, _) = self
            .rules
            .get_key_value(target)
            .unwrap_or_else(|| panic!("no rule `{target}`"));
        distance.insert(*target, 0);
        loop {
            let mut changed = false;
            for (name, body) in &self.rules {
                if let Some(steps) = item_distance(body, &distance).map(|steps| steps + 1)
                    && distance.get(name).is_none_or(|known| steps < *known)
                {
                    distance.insert(name, steps);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        self.toward = Some(distance);
        self.steering = true;
        let sentence = self.sentence(start, seed, depth);
        self.steering = false;
        let reached = self.toward.is_none();
        self.toward = None;
        sentence.filter(|_| reached)
    }

    /// A candidate program of the start rule for `seed`, expanding at most
    /// `depth` rules deep before taking only the shortest alternatives, or
    /// `None` when two glued terminals would lex as one token.
    pub fn sentence(&mut self, start: &str, seed: u64, depth: u32) -> Option<String> {
        self.state = seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0x5851_F42D_4C95_7F2D;
        self.merged = false;
        self.single_line = 0;
        self.expanded.clear();
        let mut out = Vec::new();
        let (name, body) = self
            .rules
            .get_key_value(start)
            .map(|(name, body)| (*name, *body))
            .unwrap_or_else(|| panic!("no rule `{start}`"));
        self.expanded.insert(name);
        if self
            .toward
            .as_ref()
            .is_some_and(|distance| distance.get(name) == Some(&0))
        {
            self.toward = None;
        }
        self.expand(body, depth, &mut out);
        (!self.merged).then(|| render(&out))
    }

    /// The rules the last candidate expanded.
    pub fn expanded_rules(&self) -> &FxHashSet<&'static str> {
        &self.expanded
    }

    /// Whether a targeted candidate still needs `item` to reach its target.
    fn leads_toward(&self, item: &Item) -> bool {
        self.steering
            && self
                .toward
                .as_ref()
                .is_some_and(|distance| item_distance(item, distance).is_some())
    }

    fn rule(&self, name: &str) -> &'g Item {
        self.rules
            .get(name)
            .unwrap_or_else(|| panic!("grammar refers to undefined rule `{name}`"))
    }

    fn height(&self, item: &Item) -> u32 {
        item_height(item, &self.heights).unwrap_or(u32::MAX)
    }

    fn expand(&mut self, item: &Item, depth: u32, out: &mut Vec<Emitted>) {
        match item {
            Item::Term(term) => self.emit(*term, out),
            Item::Rule(name) => {
                self.expanded.insert(name);
                if self
                    .toward
                    .as_ref()
                    .is_some_and(|distance| distance.get(name) == Some(&0))
                {
                    self.toward = None;
                }
                let body = self.rule(name);
                self.expand(body, depth.saturating_sub(1), out);
            }
            Item::Seq(items) => {
                let steering = self.steering;
                let closest = self
                    .toward
                    .as_ref()
                    .filter(|_| steering)
                    .and_then(|distance| {
                        let steps: Vec<Option<u32>> = items
                            .iter()
                            .map(|item| item_distance(item, distance))
                            .collect();
                        let least = steps.iter().flatten().min()?;
                        steps.iter().position(|step| step == &Some(*least))
                    });
                let mut index = 0;
                while index < items.len() {
                    // A guarded item is the one after its lookahead.
                    let guarded = usize::from(
                        matches!(items[index], Item::Not(_) | Item::Peek(_))
                            && index + 1 < items.len(),
                    );
                    self.steering = steering && closest == Some(index + guarded);
                    match (&items[index], items.get(index + 1)) {
                        (Item::Not(sequences), Some(next)) => {
                            self.guarded(next, depth, out, |emitted| {
                                !begins_with_any(emitted, sequences)
                            });
                            index += 2;
                        }
                        (Item::Peek(sequences), Some(next)) => {
                            self.guarded(next, depth, out, |emitted| {
                                begins_with_any(emitted, sequences)
                            });
                            index += 2;
                        }
                        (item, _) => {
                            self.expand(item, depth, out);
                            index += 1;
                        }
                    }
                }
                self.steering = steering;
            }
            Item::Alt(items) => {
                let toward: Option<Vec<usize>> = self
                    .toward
                    .as_ref()
                    .filter(|_| self.steering)
                    .and_then(|distance| {
                        let steps: Vec<Option<u32>> = items
                            .iter()
                            .map(|item| item_distance(item, distance))
                            .collect();
                        let least = steps.iter().flatten().min()?;
                        Some(
                            (0..items.len())
                                .filter(|index| steps[*index] == Some(*least))
                                .collect(),
                        )
                    });
                let choice = if let Some(closest) = toward {
                    closest[self.below(closest.len())]
                } else if depth == 0 {
                    let least = items
                        .iter()
                        .map(|item| self.height(item))
                        .min()
                        .unwrap_or(0);
                    let shortest: Vec<usize> = (0..items.len())
                        .filter(|index| self.height(&items[*index]) == least)
                        .collect();
                    shortest[self.below(shortest.len())]
                } else {
                    self.below(items.len())
                };
                self.expand(&items[choice], depth, out);
            }
            Item::Opt(inner) => {
                if self.leads_toward(inner) || (depth > 0 && self.chance(50)) {
                    self.expand(inner, depth, out);
                }
            }
            Item::Star(inner) | Item::Plus(inner) => {
                let mut count =
                    usize::from(matches!(item, Item::Plus(_)) || self.leads_toward(inner));
                let line_break = matches!(
                    **inner,
                    Item::Term(Term {
                        class: Class::Tag(TokenTag::Newline),
                        ..
                    })
                );
                if depth > 0 && !(line_break && self.single_line > 0) {
                    while count < 3 && self.chance(45) {
                        count += 1;
                    }
                }
                for _ in 0..count {
                    self.expand(inner, depth, out);
                }
            }
            Item::List {
                item: inner,
                lines,
                min_one,
            } => {
                let mut count = usize::from(*min_one || self.leads_toward(inner));
                if depth > 0 {
                    while count < 3 && self.chance(50) {
                        count += 1;
                    }
                }
                for index in 0..count {
                    if index > 0 {
                        self.emit(comma(), out);
                    }
                    if *lines && self.single_line == 0 && self.chance(10) {
                        self.emit(newline(), out);
                    }
                    self.expand(inner, depth, out);
                }
                if count > 0 && self.chance(20) {
                    self.emit(comma(), out);
                }
                if *lines && self.single_line == 0 && self.chance(10) {
                    self.emit(newline(), out);
                }
            }
            Item::Line(inner) => {
                self.single_line += 1;
                self.expand(inner, depth, out);
                self.single_line -= 1;
            }
            // A lookahead at the end of a sequence constrains what follows
            // the enclosing rule; the recognizer filter enforces it.
            Item::Not(_) | Item::Peek(_) => {}
        }
    }

    /// Expands `item` until its tokens satisfy `accept`, keeping the last
    /// attempt if none does.
    fn guarded(
        &mut self,
        item: &Item,
        depth: u32,
        out: &mut Vec<Emitted>,
        accept: impl Fn(&[Emitted]) -> bool,
    ) {
        let mut attempt = Vec::new();
        for _ in 0..64 {
            attempt.clear();
            let mut scratch: Vec<Emitted> = out.last().cloned().into_iter().collect();
            let before = scratch.len();
            self.expand(item, depth, &mut scratch);
            attempt.extend(scratch.drain(before..));
            if accept(&attempt) {
                break;
            }
        }
        out.extend(attempt);
    }

    fn emit(&mut self, term: Term, out: &mut Vec<Emitted>) {
        let previous = out.last().map(|emitted| emitted.text.clone());
        for _ in 0..16 {
            let (tag, keyword, text) = self.sample(term.class);
            let separate = match (&previous, term.glued) {
                (Some(previous), true) => tokens_stay_separate(previous, &text),
                _ => true,
            };
            if separate {
                out.push(Emitted {
                    tag,
                    keyword,
                    text,
                    glued: term.glued,
                });
                return;
            }
        }
        self.merged = true;
        let (tag, keyword, text) = self.sample(term.class);
        out.push(Emitted {
            tag,
            keyword,
            text,
            glued: term.glued,
        });
    }

    fn sample(&mut self, class: Class) -> (TokenTag, Option<Keyword>, String) {
        let pick = |generator: &mut Self, options: &[&str]| {
            options[generator.below(options.len())].to_string()
        };
        match class {
            Class::Tag(tag) => {
                let text = match tag {
                    TokenTag::Ident => pick(self, &NAMES),
                    TokenTag::ProcIdent => "a-b".to_string(),
                    TokenTag::Int => pick(self, &["0", "1", "42", "0o7"]),
                    TokenTag::Float => pick(self, &["1.5", "2e3"]),
                    TokenTag::Duration => pick(self, &["5s", "10ms", "2m", "1h"]),
                    TokenTag::String => pick(self, &["\"s\"", "\"\"", "r\"raw\"", "\"\"\"t\"\"\""]),
                    TokenTag::FmtString => pick(self, &["f\"x\"", "f\"{a}\""]),
                    TokenTag::PathString => "p\"p\"".to_string(),
                    TokenTag::GlobString => "g\"*.x\"".to_string(),
                    TokenTag::PathFmtString => "fp\"{a}/x\"".to_string(),
                    TokenTag::Bytes => "b\"x\"".to_string(),
                    TokenTag::Regex => "rx\"a+\"".to_string(),
                    TokenTag::Newline => "\n".to_string(),
                    TokenTag::DollarIdent => "$a".to_string(),
                    other => other
                        .fixed_text()
                        .expect("punctuation has fixed text")
                        .to_string(),
                };
                (tag, None, text)
            }
            Class::Keyword(keyword) => (
                TokenTag::Keyword,
                Some(keyword),
                keyword.as_str().to_string(),
            ),
            Class::Word(word) => {
                let tag = if word.bytes().all(|byte| byte.is_ascii_digit()) {
                    TokenTag::Int
                } else if word.contains('-') {
                    TokenTag::ProcIdent
                } else {
                    TokenTag::Ident
                };
                (tag, None, word.to_string())
            }
            Class::Name => {
                if self.chance(15) {
                    (TokenTag::ProcIdent, None, "a-b".to_string())
                } else {
                    (TokenTag::Ident, None, pick(self, &NAMES))
                }
            }
            Class::Label | Class::Member => {
                if self.chance(10) {
                    let keyword = [Keyword::Type, Keyword::If, Keyword::Match][self.below(3)];
                    (
                        TokenTag::Keyword,
                        Some(keyword),
                        keyword.as_str().to_string(),
                    )
                } else if class == Class::Member && self.chance(10) {
                    (TokenTag::ProcIdent, None, "a-b".to_string())
                } else {
                    (TokenTag::Ident, None, pick(self, &NAMES))
                }
            }
            Class::WordPart => {
                let options: [(TokenTag, &str); 9] = [
                    (TokenTag::Ident, "w"),
                    (TokenTag::Int, "1"),
                    (TokenTag::Minus, "-"),
                    (TokenTag::Equals, "="),
                    (TokenTag::Colon, ":"),
                    (TokenTag::Slash, "/"),
                    (TokenTag::Dot, "."),
                    (TokenTag::Plus, "+"),
                    (TokenTag::ProcIdent, "x-y"),
                ];
                let (tag, text) = options[self.below(options.len())];
                (tag, None, text.to_string())
            }
            Class::PathPart => {
                let options: [(TokenTag, &str); 5] = [
                    (TokenTag::Ident, "usr"),
                    (TokenTag::Ident, "bin"),
                    (TokenTag::Slash, "/"),
                    (TokenTag::Int, "1"),
                    (TokenTag::Dot, "."),
                ];
                let (tag, text) = options[self.below(options.len())];
                (tag, None, text.to_string())
            }
        }
    }
}

fn comma() -> Term {
    Term {
        class: Class::Tag(TokenTag::Comma),
        glued: false,
    }
}

fn newline() -> Term {
    Term {
        class: Class::Tag(TokenTag::Newline),
        glued: false,
    }
}

fn begins_with_any(emitted: &[Emitted], sequences: &[Vec<Term>]) -> bool {
    sequences.iter().any(|sequence| {
        sequence.len() <= emitted.len()
            && sequence
                .iter()
                .zip(emitted)
                .all(|(term, emitted)| term.matches(&emitted.token()))
    })
}

/// The least number of rule expansions from `item` to the rule at distance
/// zero, if it can reach it.
fn item_distance(item: &Item, distance: &FxHashMap<&'static str, u32>) -> Option<u32> {
    match item {
        Item::Rule(name) => distance.get(name).copied(),
        Item::Seq(items) | Item::Alt(items) => items
            .iter()
            .filter_map(|item| item_distance(item, distance))
            .min(),
        Item::Opt(inner) | Item::Star(inner) | Item::Plus(inner) | Item::Line(inner) => {
            item_distance(inner, distance)
        }
        Item::List { item, .. } => item_distance(item, distance),
        Item::Term(_) | Item::Not(_) | Item::Peek(_) => None,
    }
}

/// The least number of rule expansions that derive `item`, if any.
fn item_height(item: &Item, heights: &FxHashMap<&'static str, u32>) -> Option<u32> {
    match item {
        Item::Term(_) | Item::Opt(_) | Item::Star(_) | Item::Not(_) | Item::Peek(_) => Some(0),
        Item::Rule(name) => heights.get(name).copied(),
        Item::Seq(items) => items.iter().try_fold(0, |height, item| {
            item_height(item, heights).map(|item| height.max(item))
        }),
        Item::Alt(items) => items
            .iter()
            .filter_map(|item| item_height(item, heights))
            .min(),
        Item::Plus(inner) | Item::Line(inner) => item_height(inner, heights),
        Item::List { item, min_one, .. } => {
            if *min_one {
                item_height(item, heights)
            } else {
                Some(0)
            }
        }
    }
}

/// Joins emitted terminals with one space, or none before a glued terminal
/// and around line breaks.
fn render(emitted: &[Emitted]) -> String {
    let mut source = String::new();
    for (index, token) in emitted.iter().enumerate() {
        let after_newline = index == 0 || emitted[index - 1].tag == TokenTag::Newline;
        if !token.glued && !after_newline && token.tag != TokenTag::Newline {
            source.push(' ');
        }
        source.push_str(&token.text);
    }
    source.push('\n');
    source
}
