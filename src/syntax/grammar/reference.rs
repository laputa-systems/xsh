//! The grammar as reference data: EBNF text for each production and the
//! tables behind them, as one JSON document that `make docs` renders into
//! `docs/reference/grammar.md`.

use super::{
    Associativity, BINARY_OPERATORS, Class, DURATION_SUFFIXES, Grammar, Item, OperatorFamily,
    QUOTED_LITERALS, RUN_FORMS, RunOption, STREAM_STAGES, Section, line_continuation_spellings,
};
use crate::syntax::token::{Keyword, TokenTag};
use std::fmt::Write as _;

/// The name of a terminal class in EBNF text.
pub fn terminal_text(class: &Class) -> String {
    match class {
        Class::Tag(tag) => tag_name(*tag),
        Class::Keyword(keyword) => format!("\"{}\"", keyword.as_str()),
        Class::Word(word) => format!("\"{word}\""),
        Class::Name => "NAME".to_string(),
        Class::Label => "LABEL".to_string(),
        Class::Member => "MEMBER".to_string(),
        Class::WordPart => "WORD_PART".to_string(),
        Class::PathPart => "PATH_PART".to_string(),
    }
}

fn tag_name(tag: TokenTag) -> String {
    let name = match tag {
        TokenTag::Ident => "IDENT",
        TokenTag::ProcIdent => "PROC_IDENT",
        TokenTag::Int => "INT",
        TokenTag::Float => "FLOAT",
        TokenTag::Duration => "DURATION",
        TokenTag::String => "STRING",
        TokenTag::PathString => "PATH",
        TokenTag::GlobString => "GLOB",
        TokenTag::EnvString => "ENV_STRING",
        TokenTag::FmtString => "FMT_STRING",
        TokenTag::PathFmtString => "PATH_FMT",
        TokenTag::Bytes => "BYTES",
        TokenTag::Regex => "REGEX",
        TokenTag::Newline => "NEWLINE",
        TokenTag::DollarIdent => "DOLLAR_NAME",
        TokenTag::Keyword => "KEYWORD",
        TokenTag::Comment => "COMMENT",
        TokenTag::Eof => "EOF",
        other => {
            return format!(
                "\"{}\"",
                other.fixed_text().expect("punctuation has fixed text")
            );
        }
    };
    name.to_string()
}

/// The terminals named in EBNF text, with what each stands for.
pub const TERMINAL_DESCRIPTIONS: [(&str, &str); 19] = [
    (
        "IDENT",
        "an identifier: `[A-Za-z_][A-Za-z0-9_]*`, not a keyword",
    ),
    (
        "NAME",
        "an identifier that may also contain `-` after its first character",
    ),
    ("LABEL", "an identifier or keyword used as a field label"),
    ("MEMBER", "a label or a hyphenated name after `.`"),
    ("INT", "a decimal or `0o` octal integer"),
    ("FLOAT", "a decimal number with a fraction or exponent"),
    ("DURATION", "an integer followed by a duration suffix"),
    (
        "STRING",
        "a `\"...\"`, `\"\"\"...\"\"\"`, or raw `r\"...\"` string",
    ),
    ("FMT_STRING", "an interpolating `f\"...\"` string"),
    ("PATH", "a `p\"...\"` path"),
    ("PATH_FMT", "an interpolating `fp\"...\"` path"),
    ("GLOB", "a `g\"...\"` glob"),
    ("BYTES", "a `b\"...\"` byte string"),
    ("REGEX", "an `rx\"...\"` regular expression"),
    ("DOLLAR_NAME", "`$name` in a command word"),
    (
        "NEWLINE",
        "a line break that ends a statement (see Line continuation)",
    ),
    (
        "WORD_PART",
        "any other token that can be part of a bare command word",
    ),
    ("PATH_PART", "a token made only of bare-path characters"),
    ("KEYWORD", "a reserved keyword"),
];

/// Renders `item` as EBNF; `nested` parenthesizes alternatives and sequences.
pub fn render_item(item: &Item, nested: bool) -> String {
    match item {
        Item::Term(term) => {
            let text = terminal_text(&term.class);
            if term.glued { format!("~{text}") } else { text }
        }
        Item::Rule(name) => (*name).to_string(),
        Item::Seq(items) if items.is_empty() => "()".to_string(),
        Item::Seq(items) if items.len() == 1 => render_item(&items[0], nested),
        Item::Seq(items) => {
            let text = items
                .iter()
                .map(render_sequence_item)
                .collect::<Vec<_>>()
                .join(" ");
            if nested { format!("( {text} )") } else { text }
        }
        Item::Alt(items) => {
            let text = items
                .iter()
                .map(|item| render_item(item, false))
                .collect::<Vec<_>>()
                .join(" | ");
            if nested { format!("( {text} )") } else { text }
        }
        Item::Opt(inner) => format!("{}?", render_item(inner, true)),
        Item::Star(inner) => format!("{}*", render_item(inner, true)),
        Item::Plus(inner) => format!("{}+", render_item(inner, true)),
        Item::List {
            item,
            lines,
            min_one,
        } => {
            let name = match (lines, min_one) {
                (true, false) => "list",
                (true, true) => "list1",
                (false, false) => "inline_list",
                (false, true) => "inline_list1",
            };
            format!("{name}({})", render_item(item, false))
        }
        Item::Not(sequences) => format!("!{}", render_lookahead(sequences)),
        Item::Peek(sequences) => format!("&{}", render_lookahead(sequences)),
        Item::Line(inner) => format!("line({})", render_item(inner, false)),
    }
}

/// An item of a sequence; a nested sequence needs no parentheses there.
fn render_sequence_item(item: &Item) -> String {
    match item {
        Item::Seq(items) => items
            .iter()
            .map(render_sequence_item)
            .collect::<Vec<_>>()
            .join(" "),
        item => render_item(item, true),
    }
}

fn render_lookahead(sequences: &[Vec<super::Term>]) -> String {
    let text = sequences
        .iter()
        .map(|sequence| {
            sequence
                .iter()
                .map(|term| {
                    let text = terminal_text(&term.class);
                    if term.glued { format!("~{text}") } else { text }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" | ");
    if sequences.len() == 1 && sequences[0].len() == 1 {
        text
    } else {
        format!("( {text} )")
    }
}

/// The production `name = body ;` wrapped at top-level alternatives.
pub fn render_rule(name: &str, body: &Item) -> String {
    let indent = " ".repeat(name.len() + 1);
    match body {
        Item::Alt(items) if items.len() > 3 || render_item(body, false).len() > 100 => {
            let mut text = format!("{name} = {}", render_item(&items[0], false));
            for item in &items[1..] {
                let _ = write!(text, "\n{indent}| {}", render_item(item, false));
            }
            text.push_str(" ;");
            text
        }
        _ => format!("{name} = {} ;", render_item(body, false)),
    }
}

/// Every production as EBNF, by section.
pub fn ebnf(grammar: &Grammar) -> String {
    let mut text = String::new();
    for section in Section::ALL {
        let _ = writeln!(text, "(* {} *)\n", section.title());
        for rule in grammar.rules.iter().filter(|rule| rule.section == section) {
            let _ = writeln!(text, "{}", render_rule(rule.name, &rule.body));
        }
        text.push('\n');
    }
    text
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn json_array(items: impl IntoIterator<Item = String>) -> String {
    format!("[{}]", items.into_iter().collect::<Vec<_>>().join(","))
}

fn json_object(fields: &[(&str, String)]) -> String {
    format!(
        "{{{}}}",
        fields
            .iter()
            .map(|(key, value)| format!("{}:{value}", json_string(key)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

const fn family_name(family: OperatorFamily) -> &'static str {
    match family {
        OperatorFamily::Fallback => "fallback",
        OperatorFamily::Logical => "logical",
        OperatorFamily::Equality => "equality",
        OperatorFamily::Ordering => "ordering",
        OperatorFamily::Membership => "membership",
        OperatorFamily::Additive => "additive",
        OperatorFamily::Multiplicative => "multiplicative",
    }
}

/// The grammar reference document: sections of EBNF productions and the
/// operator, keyword, literal, stage, and run-form tables.
pub fn reference_json(grammar: &Grammar) -> String {
    let sections = Section::ALL.iter().map(|section| {
        let rules = grammar
            .rules
            .iter()
            .filter(|rule| rule.section == *section)
            .map(|rule| render_rule(rule.name, &rule.body));
        json_object(&[
            ("title", json_string(section.title())),
            ("ebnf", json_string(&rules.collect::<Vec<_>>().join("\n"))),
        ])
    });
    let mut levels: Vec<u8> = BINARY_OPERATORS
        .iter()
        .map(|operator| operator.precedence)
        .collect();
    levels.dedup();
    let operators = levels.iter().rev().map(|level| {
        let row: Vec<_> = BINARY_OPERATORS
            .iter()
            .filter(|operator| operator.precedence == *level)
            .collect();
        json_object(&[
            ("precedence", level.to_string()),
            (
                "operators",
                json_string(
                    &row.iter()
                        .map(|operator| format!("`{}`", operator.spelling))
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
            ),
            (
                "associativity",
                json_string(
                    if row
                        .iter()
                        .any(|operator| operator.associativity == Associativity::Right)
                    {
                        "right (`??`), left (others)"
                    } else {
                        "left"
                    },
                ),
            ),
            (
                "families",
                json_string(&{
                    let mut families: Vec<&str> = row
                        .iter()
                        .map(|operator| family_name(operator.family))
                        .collect();
                    families.dedup();
                    families.join(", ")
                }),
            ),
        ])
    });
    let keywords = Keyword::ALL
        .iter()
        .map(|keyword| keyword.as_str())
        .collect::<Vec<_>>();
    let mut sorted_keywords = keywords.clone();
    sorted_keywords.sort_unstable();
    let literals = QUOTED_LITERALS.iter().map(|form| {
        json_object(&[
            ("prefix", json_string(&format!("{}\"", form.prefix))),
            ("terminal", json_string(&tag_name(form.token))),
            ("raw", json_string(if form.raw { "yes" } else { "no" })),
        ])
    });
    let stages = STREAM_STAGES.iter().map(|stage| {
        json_object(&[
            ("name", json_string(stage.name)),
            ("block", json_string(if stage.block { "yes" } else { "no" })),
            (
                "inline",
                json_string(if stage.inline { "yes" } else { "no" }),
            ),
        ])
    });
    let run_forms = RUN_FORMS.iter().map(|form| {
        let mut spelling = "run".to_string();
        if let Some(member) = form.member {
            let _ = write!(spelling, ".{member}");
        }
        if let Some(mode) = form.mode {
            let _ = write!(spelling, " --{mode}");
        }
        json_string(&spelling)
    });
    let terminals = TERMINAL_DESCRIPTIONS.iter().map(|(name, description)| {
        json_object(&[
            ("name", json_string(name)),
            ("description", json_string(description)),
        ])
    });
    json_object(&[
        ("sections", json_array(sections)),
        ("operators", json_array(operators)),
        ("prefix_precedence", super::PREFIX.to_string()),
        (
            "continuation",
            json_string(
                &line_continuation_spellings()
                    .iter()
                    .map(|spelling| format!("`{spelling}`"))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        ),
        ("keywords", json_string(&sorted_keywords.join(" "))),
        ("terminals", json_array(terminals)),
        ("literals", json_array(literals)),
        (
            "duration_suffixes",
            json_string(
                &DURATION_SUFFIXES
                    .map(|suffix| format!("`{suffix}`"))
                    .join(" "),
            ),
        ),
        ("stages", json_array(stages)),
        ("run_forms", json_array(run_forms)),
        (
            "run_options",
            json_string(
                &RunOption::ALL
                    .map(|option| format!("`--{}=`", option.name()))
                    .join(" "),
            ),
        ),
    ])
}
