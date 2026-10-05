//! The productions of the XSH grammar, built from the tables in the parent
//! module so that operators, stages, run forms, and effects are spelled once.

use super::{
    BINARY_OPERATORS, BUILDER_APIS, BUILDER_STATEMENT_KEYWORDS, Class, EXPORTABLE_KEYWORDS, Item,
    OperatorFamily, OperatorToken, RUN_FORMS, Rule, RunOption, STATEMENT_KEYWORDS, STREAM_STAGES,
    Section, SignalHookOption, StatementForm, StreamStage, Term, effect_names,
};
use crate::syntax::node::CoreCommand;
use crate::syntax::token::{Keyword, TokenTag};

fn term(class: Class, glued: bool) -> Term {
    Term { class, glued }
}

/// A token of this kind, with any spacing before it.
fn t(tag: TokenTag) -> Item {
    Item::Term(term(Class::Tag(tag), false))
}

/// A token of this kind written directly after the previous token.
fn g(tag: TokenTag) -> Item {
    Item::Term(term(Class::Tag(tag), true))
}

fn kw(keyword: Keyword) -> Item {
    Item::Term(term(Class::Keyword(keyword), false))
}

fn w(word: &'static str) -> Item {
    Item::Term(word_term(word, false))
}

fn gw(word: &'static str) -> Item {
    Item::Term(word_term(word, true))
}

/// A contextual word, or the keyword with that spelling (`run.stream`).
fn word_term(word: &'static str, glued: bool) -> Term {
    match Keyword::from_ident(word) {
        Some(keyword) => term(Class::Keyword(keyword), glued),
        None => term(Class::Word(word), glued),
    }
}

fn class(class: Class) -> Item {
    Item::Term(term(class, false))
}

fn gclass(class: Class) -> Item {
    Item::Term(term(class, true))
}

fn r(name: &'static str) -> Item {
    Item::Rule(name)
}

fn seq(items: impl IntoIterator<Item = Item>) -> Item {
    Item::Seq(items.into_iter().collect())
}

fn alt(items: impl IntoIterator<Item = Item>) -> Item {
    Item::Alt(items.into_iter().collect())
}

fn opt(item: Item) -> Item {
    Item::Opt(Box::new(item))
}

fn star(item: Item) -> Item {
    Item::Star(Box::new(item))
}

fn plus(item: Item) -> Item {
    Item::Plus(Box::new(item))
}

/// Comma-separated items with line breaks allowed around them.
fn list(item: Item) -> Item {
    Item::List {
        item: Box::new(item),
        lines: true,
        min_one: false,
    }
}

fn list1(item: Item) -> Item {
    Item::List {
        item: Box::new(item),
        lines: true,
        min_one: true,
    }
}

/// Zero or more line breaks.
fn nl() -> Item {
    star(t(TokenTag::Newline))
}

/// The item contains no line break.
fn line(item: Item) -> Item {
    Item::Line(Box::new(item))
}

fn not(sequences: impl IntoIterator<Item = Vec<Term>>) -> Item {
    Item::Not(sequences.into_iter().collect())
}

fn tag_term(tag: TokenTag) -> Term {
    term(Class::Tag(tag), false)
}

fn glued_tag_term(tag: TokenTag) -> Term {
    term(Class::Tag(tag), true)
}

fn keyword_term(keyword: Keyword) -> Term {
    term(Class::Keyword(keyword), false)
}

fn operator_term(token: OperatorToken) -> Term {
    match token {
        OperatorToken::Tag(tag) => tag_term(tag),
        OperatorToken::Keyword(keyword) => keyword_term(keyword),
    }
}

/// The spelling of each binary operator of `family`, from the operator table.
fn operators(family: OperatorFamily) -> Item {
    alt(BINARY_OPERATORS
        .iter()
        .filter(|operator| operator.family == family)
        .map(|operator| {
            let first = Item::Term(operator_term(operator.first));
            match operator.second {
                Some(second) => seq([first, kw(second)]),
                None => first,
            }
        }))
}

/// Every token sequence that begins a binary operator, `is`, or `|>`: a
/// statement whose name is followed by one of these is an expression.
fn operator_leads() -> Vec<Vec<Term>> {
    let mut leads: Vec<Vec<Term>> = BINARY_OPERATORS
        .iter()
        .map(|operator| vec![operator_term(operator.first)])
        .collect();
    leads.push(vec![word_term("is", false)]);
    leads.push(vec![tag_term(TokenTag::PipeGt)]);
    leads
}

fn stage_name(stage: &StreamStage) -> Item {
    match stage.name.split_once('.') {
        Some((namespace, member)) => seq([w(namespace), g(TokenTag::Dot), gw(member)]),
        None => w(stage.name),
    }
}

fn stage_name_terms(stage: &StreamStage) -> Vec<Term> {
    match stage.name.split_once('.') {
        Some((namespace, member)) => vec![
            word_term(namespace, false),
            glued_tag_term(TokenTag::Dot),
            word_term(member, true),
        ],
        None => vec![word_term(stage.name, false)],
    }
}

/// The stream stages, grouped by what follows their arguments. After a stage
/// that ends without an inline expression, the pipeline is the left operand
/// of whatever suffixes and operators follow. A stage that takes a block
/// reads a `{` right after its arguments as that block, and a stage that
/// takes an inline expression but has none must end the stage.
fn stream_stages(context: ExpressionContext) -> Item {
    let mut groups: Vec<((bool, bool), Vec<Item>)> = Vec::new();
    for stage in &STREAM_STAGES {
        let key = (stage.block, stage.inline);
        match groups.iter_mut().find(|(group, _)| *group == key) {
            Some((_, names)) => names.push(stage_name(stage)),
            None => groups.push((key, vec![stage_name(stage)])),
        }
    }
    alt(groups.into_iter().map(|((block, inline), names)| {
        let arguments = opt(seq([
            t(TokenTag::LParen),
            r("call_arguments"),
            t(TokenTag::RParen),
        ]));
        let continued = || seq([star(r("postfix")), r(context.rule("logical_tail"))]);
        let no_block = || not([vec![tag_term(TokenTag::LBrace)]]);
        let tail = match (block, inline) {
            // A `(` after the stage name is always its argument list.
            (true, true) => alt([
                seq([r("block"), continued()]),
                seq([
                    not([
                        vec![tag_term(TokenTag::LBrace)],
                        vec![tag_term(TokenTag::LParen)],
                    ]),
                    r(context.rule("stage_expression")),
                ]),
                r("stage_end"),
            ]),
            (true, false) => alt([
                seq([r("block"), continued()]),
                seq([no_block(), continued()]),
            ]),
            (false, _) => continued(),
        };
        seq([alt(names), arguments, tail])
    }))
}

fn run_option(option: RunOption) -> Item {
    seq([
        t(TokenTag::Minus),
        g(TokenTag::Minus),
        gw(option.name()),
        g(TokenTag::Equals),
        r("run_option_value"),
    ])
}

/// Every non-empty ordering of distinct options.
fn option_orders(options: &[RunOption]) -> Vec<Vec<RunOption>> {
    let mut orders = Vec::new();
    for (index, option) in options.iter().enumerate() {
        orders.push(vec![*option]);
        let rest: Vec<RunOption> = options
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, option)| *option)
            .collect();
        for mut tail in option_orders(&rest) {
            tail.insert(0, *option);
            orders.push(tail);
        }
    }
    orders
}

/// A shell `[ -f x ]` or `[[ $x ]]` test where a condition begins, which the
/// parser reports instead of reading a list.
fn shell_test_leads() -> Vec<Vec<Term>> {
    let mut leads = Vec::new();
    for open in [
        vec![tag_term(TokenTag::LBracket)],
        vec![tag_term(TokenTag::LBracket), tag_term(TokenTag::LBracket)],
    ] {
        for word in [
            vec![tag_term(TokenTag::DollarIdent)],
            vec![tag_term(TokenTag::DollarLBrace)],
            vec![tag_term(TokenTag::Minus), glued_tag_term(TokenTag::Ident)],
        ] {
            leads.push(open.iter().chain(&word).copied().collect());
        }
    }
    leads
}

/// The compound or simple declarations `export` publishes, from the parser's
/// table of exportable keywords. An exported binding is written with its own
/// keyword (`export let`, `export const`).
fn exported(compound: bool) -> Vec<Item> {
    let mut items = Vec::new();
    let bindings: Vec<Item> = EXPORTABLE_KEYWORDS
        .into_iter()
        .filter(|keyword| super::statement_form(*keyword) == Some(StatementForm::Binding))
        .map(kw)
        .collect();
    if !compound && !bindings.is_empty() {
        items.push(seq([alt(bindings), r("binding_rest")]));
    }
    let forms = keyword_forms(
        EXPORTABLE_KEYWORDS
            .into_iter()
            .filter(|keyword| super::statement_form(*keyword) != Some(StatementForm::Binding)),
    );
    items.extend(statement_rules(&forms, compound));
    items
}

fn core_word(command: CoreCommand) -> Item {
    w(command.as_str())
}

/// The statement forms that `keywords` begin, each once, in table order.
fn keyword_forms(keywords: impl IntoIterator<Item = Keyword>) -> Vec<StatementForm> {
    let mut forms: Vec<StatementForm> = Vec::new();
    for keyword in keywords {
        let form = super::statement_form(keyword).expect("dispatch keywords begin statement forms");
        if !forms.contains(&form) {
            forms.push(form);
        }
    }
    forms
}

/// The productions of the compound or simple forms among `forms`.
fn statement_rules(forms: &[StatementForm], compound: bool) -> Vec<Item> {
    forms
        .iter()
        .filter(|form| form.is_compound() == compound)
        .filter_map(|form| form.rule().map(r))
        .collect()
}

fn rule(section: Section, name: &'static str, body: Item) -> Rule {
    Rule {
        section,
        name,
        body,
    }
}

/// The primary patterns, with the payload rule that follows a qualified
/// name.
fn pattern_primary(payload: &'static str) -> Item {
    alt([
        seq([
            t(TokenTag::LParen),
            nl(),
            r("pattern"),
            nl(),
            t(TokenTag::RParen),
        ]),
        // A name or `_` followed by `is` is a typed pattern.
        seq([w("_"), not([vec![word_term("is", false)]])]),
        seq([
            w("is"),
            t(TokenTag::Ident),
            opt(seq([t(TokenTag::Dot), t(TokenTag::Ident)])),
        ]),
        seq([
            class(Class::Name),
            t(TokenTag::Dot),
            t(TokenTag::Ident),
            t(TokenTag::LParen),
            opt(r("pattern")),
            t(TokenTag::RParen),
        ]),
        seq([
            class(Class::Name),
            t(TokenTag::Dot),
            t(TokenTag::Ident),
            opt(seq([t(TokenTag::Dot), t(TokenTag::Ident)])),
            opt(r(payload)),
        ]),
        seq([
            class(Class::Name),
            t(TokenTag::LParen),
            opt(seq([
                r("pattern"),
                star(seq([t(TokenTag::Comma), r("pattern")])),
                opt(t(TokenTag::Comma)),
            ])),
            t(TokenTag::RParen),
        ]),
        // A variant of the matched value's enum or error family.
        seq([
            t(TokenTag::Dot),
            t(TokenTag::Ident),
            t(TokenTag::LParen),
            opt(seq([
                r("pattern"),
                star(seq([t(TokenTag::Comma), r("pattern")])),
                opt(t(TokenTag::Comma)),
            ])),
            t(TokenTag::RParen),
        ]),
        seq([t(TokenTag::Dot), t(TokenTag::Ident), opt(r(payload))]),
        seq([
            not([vec![word_term("is", false)], vec![word_term("_", false)]]),
            class(Class::Name),
            not([vec![word_term("is", false)]]),
        ]),
        kw(Keyword::Null),
        kw(Keyword::True),
        kw(Keyword::False),
        seq([
            t(TokenTag::Minus),
            alt([g(TokenTag::Int), g(TokenTag::Float)]),
        ]),
        t(TokenTag::Int),
        t(TokenTag::Float),
        t(TokenTag::Duration),
        t(TokenTag::String),
        t(TokenTag::Bytes),
        r("list_pattern"),
        r("record_pattern"),
    ])
}

/// Where an expression is read. A condition is followed by its body block:
/// an `if`, `while`, or `guard` condition, a `for` iterable, a `match`
/// subject, a `with` value, or a `ctx` message. There a brace after a
/// qualified pattern test is the test's payload only when it looks like one
/// (`{name: P}`, `{..}`); any other brace opens the body. Every bracket,
/// brace, and parenthesis inside a condition returns to the general rules,
/// so only the rules between a condition and its pattern tests have a
/// condition form.
#[derive(Clone, Copy, Eq, PartialEq)]
enum ExpressionContext {
    General,
    Condition,
}

impl ExpressionContext {
    /// The name of a context-dependent rule in this context.
    fn rule(self, general: &'static str) -> &'static str {
        if self == Self::General {
            return general;
        }
        match general {
            "expression" => "condition_expression",
            "logical" => "condition_logical",
            "conjunction" => "condition_conjunction",
            "equality" => "condition_equality",
            "pipe_stage" => "condition_pipe_stage",
            "value_stage" => "condition_value_stage",
            "stage_expression" => "condition_stage_expression",
            "stream_stage" => "condition_stream_stage",
            "logical_tail" => "condition_logical_tail",
            "conjunction_tail" => "condition_conjunction_tail",
            "equality_tail" => "condition_equality_tail",
            "test_pattern" => "condition_test_pattern",
            _ => panic!("`{general}` does not depend on the expression context"),
        }
    }
}

/// The rules from an expression down to its pattern tests, in `context`.
fn expression_rules(context: ExpressionContext) -> Vec<Rule> {
    use Section::*;
    use TokenTag as T;
    let c = |name| r(context.rule(name));
    let logical_operators = || alt([operators(OperatorFamily::Fallback), kw(Keyword::Or)]);
    let equality_suffixes = || {
        star(alt([
            seq([
                operators(OperatorFamily::Equality),
                nl(),
                r("equality_operand"),
            ]),
            seq([w("is"), nl(), c("test_pattern")]),
        ]))
    };
    let payload_pattern = match context {
        ExpressionContext::General => "primary_pattern",
        ExpressionContext::Condition => "condition_primary_pattern",
    };
    vec![
        rule(
            Expressions,
            context.rule("expression"),
            seq([c("logical"), star(seq([t(T::PipeGt), c("pipe_stage")]))]),
        ),
        rule(
            Expressions,
            context.rule("logical"),
            seq([
                c("conjunction"),
                star(seq([logical_operators(), nl(), c("conjunction")])),
            ]),
        ),
        rule(
            Expressions,
            context.rule("conjunction"),
            seq([
                c("equality"),
                star(seq([kw(Keyword::And), nl(), c("equality")])),
            ]),
        ),
        rule(
            Expressions,
            context.rule("equality"),
            alt([
                r("ordering"),
                seq([r("equality_operand"), equality_suffixes()]),
            ]),
        ),
        rule(
            Expressions,
            context.rule("pipe_stage"),
            alt([c("stream_stage"), c("value_stage")]),
        ),
        rule(
            Expressions,
            context.rule("value_stage"),
            seq([
                not(STREAM_STAGES.iter().map(stage_name_terms)),
                c("stage_expression"),
            ]),
        ),
        rule(Expressions, context.rule("stage_expression"), c("logical")),
        rule(
            Expressions,
            context.rule("stream_stage"),
            stream_stages(context),
        ),
        rule(
            Expressions,
            context.rule("equality_tail"),
            alt([
                seq([
                    r("additive_tail"),
                    plus(seq([
                        operators(OperatorFamily::Ordering),
                        nl(),
                        r("additive"),
                    ])),
                ]),
                seq([
                    r("additive_tail"),
                    star(seq([
                        operators(OperatorFamily::Membership),
                        nl(),
                        r("additive"),
                    ])),
                    equality_suffixes(),
                ]),
            ]),
        ),
        rule(
            Expressions,
            context.rule("conjunction_tail"),
            seq([
                c("equality_tail"),
                star(seq([kw(Keyword::And), nl(), c("equality")])),
            ]),
        ),
        rule(
            Expressions,
            context.rule("logical_tail"),
            seq([
                c("conjunction_tail"),
                star(seq([logical_operators(), nl(), c("conjunction")])),
            ]),
        ),
        rule(
            Patterns,
            context.rule("test_pattern"),
            alt([
                // A name followed by `[` or `?` is read as a type.
                seq([
                    r("builtin_type"),
                    opt(r("type_arguments")),
                    opt(t(T::Question)),
                ]),
                seq([
                    r("named_type"),
                    alt([
                        seq([r("type_arguments"), opt(t(T::Question))]),
                        t(T::Question),
                    ]),
                ]),
                seq([
                    alt([
                        seq([
                            alt([
                                w("_"),
                                seq([not([vec![word_term("is", false)]]), class(Class::Name)]),
                            ]),
                            w("is"),
                            r("type_expr"),
                        ]),
                        r(payload_pattern),
                    ]),
                    star(seq([
                        w("as"),
                        not([vec![word_term("_", false)]]),
                        t(T::Ident),
                    ])),
                ]),
            ]),
        ),
    ]
}

pub(super) fn rules() -> Vec<super::Rule> {
    use Section::*;
    use TokenTag as T;
    let ident = || t(T::Ident);
    let name = || class(Class::Name);
    let label = || class(Class::Label);
    let member = || class(Class::Member);
    let sep = || r("separator");
    let dot_dot = || seq([t(T::Dot), g(T::Dot)]);
    let ellipsis = || seq([t(T::Dot), g(T::Dot), g(T::Dot)]);
    let parens = |inner: Item| seq([t(T::LParen), inner, t(T::RParen)]);
    let typed = || seq([t(T::Colon), r("type_expr")]);
    let returns = || seq([t(T::Arrow), r("type_expr")]);
    let signature = || seq([t(T::LParen), r("parameters"), t(T::RParen)]);
    let block = || r("block");
    // A `{` that opens a record whose first field is written out, possibly
    // on the next line: in a `match` statement arm any other `{` opens the
    // arm's block.
    let field_record_leads = || {
        let brace = tag_term(T::LBrace);
        let mut leads = Vec::new();
        for lines in [false, true] {
            let open = if lines {
                vec![brace, tag_term(T::Newline)]
            } else {
                vec![brace]
            };
            let lead = |rest: &[Term]| open.iter().chain(rest).copied().collect::<Vec<_>>();
            leads.extend([
                lead(&[tag_term(T::Dot), tag_term(T::Dot)]),
                lead(&[tag_term(T::LBracket)]),
                lead(&[term(Class::Label, false), tag_term(T::Colon)]),
                lead(&[term(Class::Label, false), tag_term(T::Dot)]),
                lead(&[tag_term(T::String), tag_term(T::Colon)]),
            ]);
        }
        leads
    };
    let mut rules = vec![
        // Programs and blocks.
        rule(
            Programs,
            "program",
            seq([
                star(alt([
                    sep(),
                    r("top_level_declaration"),
                    r("compound_statement"),
                    seq([r("simple_statement"), sep()]),
                ])),
                opt(r("simple_statement")),
            ]),
        ),
        rule(
            Programs,
            "statements",
            seq([
                star(alt([
                    sep(),
                    r("compound_statement"),
                    seq([r("simple_statement"), sep()]),
                ])),
                opt(r("simple_statement")),
            ]),
        ),
        rule(Programs, "separator", alt([t(T::Newline), t(T::Semicolon)])),
        rule(
            Programs,
            "block",
            seq([
                t(T::LBrace),
                star(sep()),
                opt(r("block_parameters")),
                r("statements"),
                t(T::RBrace),
            ]),
        ),
        rule(
            Programs,
            "block_parameters",
            seq([t(T::Pipe), list1(ident()), t(T::Pipe)]),
        ),
        rule(
            Programs,
            "top_level_declaration",
            alt([r("test_declaration"), r("cli_main")]),
        ),
        // Keyword statements come from the parser's dispatch table.
        rule(
            Programs,
            "compound_statement",
            alt(statement_rules(
                &keyword_forms(STATEMENT_KEYWORDS.map(|(keyword, _)| keyword)),
                true,
            )
            .into_iter()
            .chain([
                r("signal_hook"),
                seq([
                    kw(Keyword::Export),
                    alt(exported(true).into_iter().chain([r("signal_hook")])),
                ]),
                r("repeat_statement"),
                r("without_statement"),
                r("tempdir_statement"),
            ])),
        ),
        rule(
            Programs,
            "simple_statement",
            alt(statement_rules(
                &keyword_forms(STATEMENT_KEYWORDS.map(|(keyword, _)| keyword)),
                false,
            )
            .into_iter()
            .chain([
                r("assignment"),
                r("error_declaration"),
                seq([
                    kw(Keyword::Export),
                    alt(exported(false).into_iter().chain([r("error_declaration")])),
                ]),
                seq([r("context_scope"), opt(t(T::Question))]),
                seq([r("named_command"), opt(t(T::Question))]),
                r("expression_statement"),
                r("exit_statement"),
            ])),
        ),
        // Declarations.
        rule(
            Declarations,
            "use_statement",
            seq([
                kw(Keyword::Use),
                name(),
                star(seq([t(T::Dot), name()])),
                opt(seq([w("as"), ident()])),
            ]),
        ),
        rule(
            Declarations,
            "binding",
            seq([
                alt([kw(Keyword::Let), kw(Keyword::Const), kw(Keyword::Var)]),
                r("binding_rest"),
            ]),
        ),
        rule(
            Declarations,
            "binding_rest",
            seq([
                r("binding_target"),
                opt(typed()),
                t(T::Equals),
                r("expression_or_run"),
            ]),
        ),
        rule(
            Declarations,
            "binding_target",
            alt([
                ident(),
                seq([t(T::LBrace), list(r("destructure_field")), t(T::RBrace)]),
            ]),
        ),
        rule(
            Declarations,
            "destructure_field",
            alt([
                dot_dot(),
                ident(),
                seq([label(), t(T::Colon), nl(), r("binding_target")]),
            ]),
        ),
        rule(
            Declarations,
            "type_declaration",
            seq([
                kw(Keyword::Type),
                ident(),
                opt(seq([
                    t(T::LBracket),
                    ident(),
                    star(seq([t(T::Comma), ident()])),
                    t(T::RBracket),
                ])),
                t(T::Equals),
                alt([
                    seq([opt(w("exact")), w("module"), r("module_contract")]),
                    r("record_schema"),
                    r("type_expr"),
                ]),
            ]),
        ),
        rule(
            Declarations,
            "record_schema",
            seq([t(T::LBrace), list(r("schema_field")), t(T::RBrace)]),
        ),
        rule(
            Declarations,
            "schema_field",
            seq([
                label(),
                t(T::Colon),
                r("type_expr"),
                opt(seq([t(T::Equals), r("expression")])),
            ]),
        ),
        rule(
            Declarations,
            "module_contract",
            seq([
                t(T::LBrace),
                nl(),
                star(seq([
                    r("contract_entry"),
                    star(sep()),
                    opt(seq([t(T::Comma), star(sep())])),
                ])),
                t(T::RBrace),
            ]),
        ),
        rule(
            Declarations,
            "contract_entry",
            seq([
                kw(Keyword::Export),
                opt(w("optional")),
                alt([
                    seq([
                        kw(Keyword::Proc),
                        name(),
                        signature(),
                        opt(r("effects")),
                        opt(returns()),
                    ]),
                    seq([kw(Keyword::Pure), ident(), signature(), returns()]),
                    seq([opt(kw(Keyword::Let)), ident(), typed()]),
                ]),
            ]),
        ),
        rule(
            Declarations,
            "enum_declaration",
            seq([
                kw(Keyword::Enum),
                ident(),
                alt([
                    seq([
                        t(T::LBrace),
                        list1(seq([
                            ident(),
                            opt(seq([t(T::LParen), list(r("type_expr")), t(T::RParen)])),
                        ])),
                        t(T::RBrace),
                    ]),
                    seq([
                        t(T::Colon),
                        w("Str"),
                        t(T::LBrace),
                        list1(seq([ident(), t(T::Equals), r("expression")])),
                        t(T::RBrace),
                    ]),
                ]),
            ]),
        ),
        rule(
            Declarations,
            "error_declaration",
            seq([
                w("error"),
                ident(),
                alt([
                    seq([
                        t(T::Equals),
                        nl(),
                        opt(seq([t(T::Pipe), nl()])),
                        r("error_variant"),
                        star(seq([t(T::Pipe), nl(), r("error_variant")])),
                    ]),
                    // One variant per line: a line break is the separator.
                    seq([
                        t(T::LBrace),
                        nl(),
                        r("error_variant"),
                        star(seq([t(T::Newline), r("error_variant")])),
                        nl(),
                        t(T::RBrace),
                    ]),
                ]),
            ]),
        ),
        rule(
            Declarations,
            "error_variant",
            seq([
                name(),
                opt(seq([
                    t(T::LParen),
                    list(seq([label(), typed()])),
                    t(T::RParen),
                ])),
                opt(seq([
                    t(T::Colon),
                    ident(),
                    star(seq([t(T::Comma), ident()])),
                ])),
            ]),
        ),
        rule(
            Declarations,
            "proc_declaration",
            seq([
                kw(Keyword::Proc),
                name(),
                signature(),
                opt(r("effects")),
                opt(returns()),
                block(),
            ]),
        ),
        rule(
            Declarations,
            "pure_declaration",
            seq([
                kw(Keyword::Pure),
                ident(),
                signature(),
                opt(returns()),
                block(),
            ]),
        ),
        rule(
            Declarations,
            "stream_declaration",
            seq([
                kw(Keyword::Stream),
                ident(),
                signature(),
                opt(r("effects")),
                returns(),
                block(),
            ]),
        ),
        rule(
            Declarations,
            "cli_main",
            seq([
                w("cli"),
                w("main"),
                signature(),
                opt(r("effects")),
                opt(returns()),
                block(),
            ]),
        ),
        rule(
            Declarations,
            "test_declaration",
            seq([
                w("test"),
                ident(),
                opt(r("effects")),
                t(T::LBrace),
                star(sep()),
                opt(seq([t(T::Pipe), ident(), opt(t(T::Comma)), t(T::Pipe)])),
                r("statements"),
                t(T::RBrace),
            ]),
        ),
        rule(Declarations, "parameters", list(r("parameter"))),
        rule(
            Declarations,
            "parameter",
            alt([
                seq([ident(), typed(), opt(seq([t(T::Equals), r("expression")]))]),
                seq([ident(), t(T::Equals), r("expression")]),
                seq([ellipsis(), ident(), typed()]),
            ]),
        ),
        rule(
            Declarations,
            "effects",
            seq([
                t(T::LBracket),
                list(alt(effect_names().into_iter().map(w))),
                t(T::RBracket),
            ]),
        ),
        rule(
            Declarations,
            "signal_hook",
            seq([
                w("on"),
                alt([name(), t(T::Int)]),
                star(seq([
                    t(T::Minus),
                    g(T::Minus),
                    alt(SignalHookOption::ALL.map(|option| gw(option.name()))),
                    g(T::Equals),
                    g(T::Duration),
                ])),
                r("effects"),
                block(),
            ]),
        ),
        // Statements.
        rule(
            Statements,
            "assignment",
            alt([
                seq([
                    ident(),
                    star(alt([
                        seq([t(T::Dot), member()]),
                        seq([t(T::LBracket), r("expression"), t(T::RBracket)]),
                    ])),
                    alt([
                        t(T::Equals),
                        seq([
                            alt([
                                t(T::Plus),
                                t(T::Minus),
                                t(T::Star),
                                t(T::Slash),
                                t(T::Percent),
                            ]),
                            g(T::Equals),
                        ]),
                    ]),
                    r("expression_or_run"),
                ]),
                // Setting an environment variable takes only plain `=`.
                seq([t(T::EnvString), t(T::Equals), r("expression_or_run")]),
            ]),
        ),
        rule(
            Statements,
            "postfix_guard",
            seq([
                alt([kw(Keyword::When), kw(Keyword::Unless)]),
                r("expression"),
            ]),
        ),
        // A run form reads every following word up to `?` or the end of the
        // statement, so a guard after one needs the `?`.
        rule(
            Statements,
            "guarded_value",
            alt([
                seq([r("expression_item"), opt(r("postfix_guard"))]),
                seq([
                    r("run_form"),
                    opt(seq([t(T::Question), opt(r("postfix_guard"))])),
                ]),
            ]),
        ),
        rule(
            Statements,
            "return_statement",
            seq([
                kw(Keyword::Return),
                opt(alt([r("postfix_guard"), r("guarded_value")])),
            ]),
        ),
        rule(
            Statements,
            "yield_statement",
            seq([
                kw(Keyword::Yield),
                alt([
                    seq([t(T::At), r("expression"), opt(r("postfix_guard"))]),
                    r("guarded_value"),
                ]),
            ]),
        ),
        rule(
            Statements,
            "break_statement",
            seq([
                kw(Keyword::Break),
                opt(alt([
                    r("postfix_guard"),
                    seq([r("expression"), opt(r("postfix_guard"))]),
                ])),
            ]),
        ),
        // `exit` is a contextual word: it begins the statement where a
        // command named `exit` would be read, with its status on the line.
        rule(
            Statements,
            "exit_statement",
            seq([w("exit"), line(r("expression")), opt(r("postfix_guard"))]),
        ),
        rule(
            Statements,
            "continue_statement",
            seq([kw(Keyword::Continue), opt(r("postfix_guard"))]),
        ),
        rule(
            Statements,
            "defer_statement",
            seq([
                alt([kw(Keyword::Defer), kw(Keyword::Errdefer)]),
                alt([
                    block(),
                    seq([not([vec![tag_term(T::LBrace)]]), r("expression_or_run")]),
                ]),
            ]),
        ),
        rule(
            Statements,
            "assert_statement",
            seq([
                kw(Keyword::Assert),
                r("expression"),
                opt(seq([t(T::Comma), r("expression")])),
            ]),
        ),
        rule(
            Statements,
            "expression_statement",
            seq([
                // These begin statements of their own: keyword statements,
                // scope statements, and `process.command {`, which reads as a
                // dotted command.
                not({
                    let mut leads: Vec<Vec<Term>> = STATEMENT_KEYWORDS
                        .iter()
                        .map(|(keyword, _)| vec![keyword_term(*keyword)])
                        .collect();
                    leads.push(vec![
                        word_term(CoreCommand::Env.as_str(), false),
                        tag_term(T::LParen),
                    ]);
                    // Contextual words that begin declarations and commands.
                    leads.push(vec![word_term(CoreCommand::Cd.as_str(), false)]);
                    leads.extend([
                        vec![word_term("test", false), term(Class::Name, false)],
                        vec![word_term("on", false), term(Class::Name, false)],
                    ]);
                    leads.extend([
                        vec![word_term("on", false), tag_term(T::Int)],
                        vec![
                            word_term("cli", false),
                            tag_term(T::Ident),
                            tag_term(T::LParen),
                        ],
                        vec![
                            word_term("error", false),
                            term(Class::Name, false),
                            tag_term(T::Equals),
                        ],
                        vec![
                            word_term("error", false),
                            term(Class::Name, false),
                            tag_term(T::LBrace),
                        ],
                    ]);
                    leads.extend(BUILDER_APIS.map(|(module, function)| {
                        vec![
                            word_term(module, false),
                            glued_tag_term(T::Dot),
                            word_term(function, true),
                            tag_term(T::LBrace),
                        ]
                    }));
                    leads
                }),
                r("expression"),
                opt(t(T::Question)),
            ]),
        ),
        rule(
            Statements,
            "if_statement",
            seq([
                kw(Keyword::If),
                r("condition"),
                block(),
                star(seq([
                    kw(Keyword::Else),
                    kw(Keyword::If),
                    r("condition"),
                    block(),
                ])),
                opt(seq([kw(Keyword::Else), block()])),
            ]),
        ),
        rule(
            Statements,
            "condition",
            seq([
                not(shell_test_leads()),
                alt([
                    seq([
                        kw(Keyword::Let),
                        nl(),
                        r("pattern"),
                        nl(),
                        t(T::Equals),
                        nl(),
                        r("condition_expression"),
                    ]),
                    r("condition_expression"),
                ]),
            ]),
        ),
        rule(
            Statements,
            "while_statement",
            seq([kw(Keyword::While), r("condition"), block()]),
        ),
        rule(
            Statements,
            "for_statement",
            seq([
                kw(Keyword::For),
                r("binding_target"),
                kw(Keyword::In),
                r("condition_expression"),
                block(),
            ]),
        ),
        rule(
            Statements,
            "loop_statement",
            seq([kw(Keyword::Loop), block()]),
        ),
        // `repeat` and `times` are contextual words: the statement is the
        // whole head `repeat COUNT times {`, written on one line.
        rule(
            Statements,
            "repeat_statement",
            seq([
                w("repeat"),
                line(r("condition_expression")),
                w("times"),
                block(),
            ]),
        ),
        // `without` is a contextual word: the statement is the whole head
        // `without EFFECT, ... {`, written on one line.
        rule(
            Statements,
            "without_statement",
            seq([
                w("without"),
                line(seq([
                    alt(effect_names().into_iter().map(w)),
                    star(seq([
                        t(T::Comma),
                        alt(effect_names().into_iter().map(w)),
                    ])),
                ])),
                block(),
            ]),
        ),
        // `tempdir` and `at` are contextual words: the statement is recognized
        // by the three words that begin it.
        rule(
            Statements,
            "tempdir_statement",
            seq([
                w("tempdir"),
                t(T::Ident),
                w("at"),
                r("condition_expression"),
                block(),
            ]),
        ),
        rule(
            Statements,
            "match_statement",
            seq([
                kw(Keyword::Match),
                r("condition_expression"),
                t(T::LBrace),
                star(alt([sep(), seq([r("arm_head"), r("arm_body")])])),
                // The catch-all `else` arm is the last arm.
                opt(alt([
                    seq([r("arm_head"), r("arm_statement")]),
                    seq([
                        r("else_arm_head"),
                        alt([r("arm_body"), r("arm_statement")]),
                        star(sep()),
                    ]),
                ])),
                t(T::RBrace),
            ]),
        ),
        rule(
            Statements,
            "arm_head",
            seq([
                // A line beginning with `.name` continues the line before
                // it, so a head never begins with a target-typed variant.
                not([vec![tag_term(T::Dot), tag_term(T::Ident)]]),
                r("pattern"),
                opt(seq([kw(Keyword::If), r("expression")])),
                t(T::FatArrow),
            ]),
        ),
        // `else` takes no guard: a guarded catch-all is `_ if cond =>`.
        rule(
            Statements,
            "else_arm_head",
            seq([kw(Keyword::Else), t(T::FatArrow)]),
        ),
        // A statement arm's body with what ends it before a following arm.
        rule(
            Statements,
            "arm_body",
            alt([
                seq([
                    alt([block(), r("compound_statement")]),
                    opt(t(T::Comma)),
                ]),
                seq([r("arm_statement"), sep()]),
                // `assert` and an error family read a following `,` as
                // part of the statement.
                seq([
                    not([
                        vec![keyword_term(Keyword::Assert)],
                        vec![
                            word_term("error", false),
                            tag_term(T::Ident),
                            tag_term(T::Equals),
                        ],
                        vec![keyword_term(Keyword::Export), word_term("error", false)],
                    ]),
                    r("arm_statement"),
                    t(T::Comma),
                ]),
            ]),
        ),
        rule(
            Statements,
            "arm_statement",
            alt([
                seq([not([vec![tag_term(T::LBrace)]]), r("simple_statement")]),
                seq([
                    Item::Peek(field_record_leads()),
                    r("record_expression"),
                    opt(t(T::Question)),
                ]),
            ]),
        ),
        rule(
            Statements,
            "guard_statement",
            seq([
                kw(Keyword::Guard),
                alt([
                    seq([
                        not([vec![keyword_term(Keyword::Let)]]),
                        r("guard_condition"),
                        kw(Keyword::Else),
                        block(),
                    ]),
                    seq([
                        kw(Keyword::Let),
                        r("binding_target"),
                        opt(typed()),
                        t(T::Equals),
                        alt([r("expression_item"), seq([r("run_form"), t(T::Question)])]),
                        kw(Keyword::Else),
                        nl(),
                        block(),
                    ]),
                ]),
            ]),
        ),
        rule(
            Statements,
            "guard_condition",
            seq([not(shell_test_leads()), r("condition_expression")]),
        ),
        rule(
            Statements,
            "with_statement",
            seq([
                kw(Keyword::With),
                list(seq([ident(), t(T::Equals), r("condition_expression")])),
                block(),
                nl(),
                kw(Keyword::Else),
                nl(),
                block(),
            ]),
        ),
        // Expressions.
        rule(
            Expressions,
            "expression_or_run",
            alt([
                r("expression_item"),
                seq([r("run_form"), opt(t(T::Question))]),
            ]),
        ),
        rule(
            Expressions,
            "equality_operand",
            alt([r("membership"), r("additive")]),
        ),
        rule(
            Expressions,
            "ordering",
            seq([
                r("additive"),
                plus(seq([
                    operators(OperatorFamily::Ordering),
                    nl(),
                    r("additive"),
                ])),
            ]),
        ),
        rule(
            Expressions,
            "membership",
            seq([
                r("additive"),
                plus(seq([
                    operators(OperatorFamily::Membership),
                    nl(),
                    r("additive"),
                ])),
            ]),
        ),
        rule(
            Expressions,
            "additive",
            seq([
                r("multiplicative"),
                star(seq([
                    operators(OperatorFamily::Additive),
                    nl(),
                    r("multiplicative"),
                ])),
            ]),
        ),
        rule(
            Expressions,
            "multiplicative",
            seq([
                r("unary"),
                star(seq([
                    operators(OperatorFamily::Multiplicative),
                    nl(),
                    r("unary"),
                ])),
            ]),
        ),
        rule(
            Expressions,
            "unary",
            alt([
                seq([alt([t(T::Bang), t(T::Minus)]), r("unary")]),
                seq([r("primary"), star(r("postfix"))]),
            ]),
        ),
        rule(
            Expressions,
            "postfix",
            alt([
                seq([g(T::Dot), r("member_access")]),
                seq([g(T::Question), g(T::Dot), r("member_access")]),
                seq([g(T::LBracket), r("index"), t(T::RBracket)]),
                seq([g(T::Question), g(T::LBracket), r("index"), t(T::RBracket)]),
                seq([g(T::LParen), r("call_arguments"), t(T::RParen)]),
                g(T::Question),
            ]),
        ),
        rule(
            Expressions,
            "member_access",
            alt([
                seq([
                    w("require"),
                    t(T::LParen),
                    nl(),
                    opt(r("type_expr")),
                    nl(),
                    t(T::RParen),
                ]),
                member(),
            ]),
        ),
        rule(
            Expressions,
            "index",
            alt([
                seq([dot_dot(), opt(r("expression"))]),
                seq([r("expression"), opt(seq([dot_dot(), opt(r("expression"))]))]),
            ]),
        ),
        rule(Expressions, "call_arguments", list(r("argument"))),
        rule(
            Expressions,
            "argument",
            alt([
                seq([ellipsis(), nl(), r("expression")]),
                seq([t(T::At), r("expression")]),
                seq([label(), t(T::Colon), nl(), r("expression_item")]),
                seq([ident(), t(T::Colon)]),
                r("expression_item"),
            ]),
        ),
        // A spaced `?` after a whole expression tries it only where the
        // expression ends.
        rule(
            Expressions,
            "expression_item",
            seq([
                r("expression"),
                opt(seq([
                    t(T::Question),
                    Item::Peek(
                        [
                            T::Newline,
                            T::Semicolon,
                            T::Comma,
                            T::RParen,
                            T::RBracket,
                            T::RBrace,
                        ]
                        .map(|tag| vec![tag_term(tag)])
                        .to_vec(),
                    ),
                ])),
            ]),
        ),
        rule(
            Expressions,
            "primary",
            alt([
                r("literal"),
                t(T::LastStatus),
                ident(),
                r("list_literal"),
                r("record_literal"),
                r("map_comprehension"),
                block(),
                seq([t(T::LParen), nl(), r("expression_item"), nl(), t(T::RParen)]),
                seq([
                    t(T::LParen),
                    nl(),
                    r("run_form"),
                    opt(t(T::Question)),
                    nl(),
                    t(T::RParen),
                ]),
                r("if_expression"),
                r("match_expression"),
                seq([kw(Keyword::Loop), block()]),
                seq([kw(Keyword::Try), block()]),
                r("retry_expression"),
                // The `?` before a `|>` belongs to the run form.
                seq([
                    r("run_form"),
                    alt([
                        seq([t(T::Question), Item::Peek(vec![vec![tag_term(T::PipeGt)]])]),
                        r("run_end"),
                    ]),
                ]),
                seq([
                    kw(Keyword::Spawn),
                    alt([seq([r("run_form"), r("run_end")]), r("operand")]),
                ]),
                seq([kw(Keyword::Wait), r("operand")]),
                seq([w("ctx"), line(r("condition_expression")), block()]),
                r("context_scope"),
                r("builder_call"),
                r("item_expression"),
                r("bare_path"),
            ]),
        ),
        rule(
            Expressions,
            "literal",
            alt([
                kw(Keyword::Null),
                kw(Keyword::True),
                kw(Keyword::False),
                t(T::Int),
                t(T::Float),
                t(T::Duration),
                t(T::String),
                t(T::FmtString),
                t(T::Regex),
                t(T::Bytes),
                t(T::PathString),
                t(T::PathFmtString),
                t(T::GlobString),
                t(T::EnvString),
            ]),
        ),
        rule(
            Expressions,
            "list_literal",
            alt([
                seq([
                    t(T::LBracket),
                    list(seq([opt(seq([t(T::At), nl()])), r("expression_item")])),
                    t(T::RBracket),
                ]),
                seq([
                    t(T::LBracket),
                    nl(),
                    r("expression"),
                    nl(),
                    r("comprehension"),
                    t(T::RBracket),
                ]),
            ]),
        ),
        rule(
            Expressions,
            "comprehension",
            seq([
                kw(Keyword::For),
                r("binding_target"),
                kw(Keyword::In),
                r("expression"),
                star(seq([
                    nl(),
                    alt([
                        seq([
                            kw(Keyword::For),
                            r("binding_target"),
                            kw(Keyword::In),
                            r("expression"),
                        ]),
                        seq([kw(Keyword::If), r("expression")]),
                    ]),
                ])),
                nl(),
            ]),
        ),
        rule(
            Expressions,
            "record_literal",
            seq([t(T::LBrace), list(r("record_field")), t(T::RBrace)]),
        ),
        rule(
            Expressions,
            "record_field",
            alt([
                seq([ellipsis(), r("expression")]),
                seq([
                    t(T::LBracket),
                    nl(),
                    r("expression"),
                    nl(),
                    t(T::RBracket),
                    t(T::Colon),
                    r("expression_item"),
                ]),
                seq([
                    alt([label(), t(T::String)]),
                    star(seq([t(T::Dot), label()])),
                    t(T::Colon),
                    r("expression_item"),
                ]),
                ident(),
            ]),
        ),
        rule(
            Expressions,
            "map_comprehension",
            seq([
                t(T::LBrace),
                nl(),
                alt([
                    seq([t(T::LBracket), nl(), r("expression"), nl(), t(T::RBracket)]),
                    seq([
                        alt([label(), t(T::String)]),
                        star(seq([t(T::Dot), label()])),
                    ]),
                ]),
                t(T::Colon),
                r("expression"),
                nl(),
                r("comprehension"),
                t(T::RBrace),
            ]),
        ),
        rule(
            Expressions,
            "if_expression",
            seq([
                kw(Keyword::If),
                r("condition"),
                block(),
                star(seq([
                    kw(Keyword::Else),
                    kw(Keyword::If),
                    r("condition"),
                    block(),
                ])),
                kw(Keyword::Else),
                block(),
            ]),
        ),
        rule(
            Expressions,
            "match_expression",
            seq([
                kw(Keyword::Match),
                r("condition_expression"),
                t(T::LBrace),
                star(alt([
                    sep(),
                    seq([r("match_expression_arm"), alt([t(T::Comma), sep()])]),
                ])),
                // The catch-all `else` arm is the last arm.
                opt(alt([
                    r("match_expression_arm"),
                    seq([
                        r("else_arm_head"),
                        r("arm_value"),
                        opt(t(T::Comma)),
                        star(sep()),
                    ]),
                ])),
                t(T::RBrace),
            ]),
        ),
        rule(
            Expressions,
            "match_expression_arm",
            seq([r("arm_head"), r("arm_value")]),
        ),
        rule(
            Expressions,
            "arm_value",
            alt([
                block(),
                seq([not([vec![tag_term(T::LBrace)]]), r("expression_item")]),
                seq([r("record_expression"), opt(t(T::Question))]),
            ]),
        ),
        // An arm value that starts with a record, which the parser tells
        // from the arm's block by the record's first field.
        rule(
            Expressions,
            "record_expression",
            seq([
                alt([r("record_literal"), r("map_comprehension")]),
                star(r("postfix")),
                r("logical_tail"),
                star(seq([t(T::PipeGt), r("pipe_stage")])),
            ]),
        ),
        rule(
            Expressions,
            "retry_expression",
            seq([
                kw(Keyword::Retry),
                t(T::LBracket),
                list(r("expression")),
                t(T::RBracket),
                opt(seq([
                    w("on"),
                    t(T::LParen),
                    nl(),
                    r("pattern"),
                    nl(),
                    t(T::RParen),
                    star(seq([w("as"), ident()])),
                ])),
                block(),
            ]),
        ),
        // The operand of `spawn` and `wait` takes every following `.name`,
        // index, and call, however it is spaced.
        rule(
            Expressions,
            "operand",
            seq([
                alt([
                    seq([alt([t(T::Bang), t(T::Minus)]), r("unary")]),
                    seq([
                        r("primary"),
                        star(alt([
                            seq([g(T::Dot), member()]),
                            seq([g(T::LBracket), r("index"), t(T::RBracket)]),
                            seq([g(T::LParen), r("call_arguments"), t(T::RParen)]),
                        ])),
                    ]),
                ]),
                not([
                    vec![tag_term(T::Dot), term(Class::Member, false)],
                    vec![tag_term(T::LBracket)],
                    vec![tag_term(T::LParen)],
                ]),
            ]),
        ),
        rule(
            Expressions,
            "context_scope",
            seq([
                alt([core_word(CoreCommand::Cd), core_word(CoreCommand::Env)]),
                t(T::LParen),
                r("expression"),
                t(T::RParen),
                nl(),
                block(),
            ]),
        ),
        rule(
            Expressions,
            "builder_call",
            seq([
                alt(BUILDER_APIS
                    .map(|(module, function)| seq([w(module), g(T::Dot), gw(function)]))),
                opt(seq([g(T::LParen), r("call_arguments"), t(T::RParen)])),
                r("builder_block"),
            ]),
        ),
        rule(
            Expressions,
            "builder_block",
            seq([
                t(T::LBrace),
                star(alt([
                    sep(),
                    r("builder_compound_entry"),
                    seq([r("builder_entry"), sep()]),
                ])),
                opt(r("builder_entry")),
                t(T::RBrace),
            ]),
        ),
        rule(
            Expressions,
            "builder_compound_entry",
            alt(
                statement_rules(&keyword_forms(BUILDER_STATEMENT_KEYWORDS), true)
                    .into_iter()
                    .chain([
                        seq([w("task"), name(), opt(signature()), block()]),
                        seq([name(), star(r("command_argument")), r("builder_block")]),
                    ]),
            ),
        ),
        rule(
            Expressions,
            "builder_entry",
            alt(
                statement_rules(&keyword_forms(BUILDER_STATEMENT_KEYWORDS), false)
                    .into_iter()
                    .chain([
                        seq([ident(), t(T::Equals), r("expression")]),
                        seq([
                            name(),
                            not([vec![tag_term(T::Equals)]]),
                            star(r("command_argument")),
                        ]),
                    ]),
            ),
        ),
        rule(
            Expressions,
            "item_expression",
            seq([
                t(T::Dot),
                alt([
                    member(),
                    not([
                        vec![term(Class::Member, false)],
                        vec![tag_term(T::Dot)],
                        vec![glued_tag_term(T::Slash)],
                    ]),
                ]),
            ]),
        ),
        rule(
            Expressions,
            "bare_path",
            seq([
                alt([
                    t(T::Slash),
                    seq([t(T::Dot), g(T::Slash)]),
                    seq([t(T::Dot), g(T::Dot), g(T::Slash)]),
                ]),
                star(gclass(Class::PathPart)),
                not([vec![term(Class::PathPart, true)]]),
            ]),
        ),
        rule(
            Expressions,
            "multiplicative_tail",
            star(seq([
                operators(OperatorFamily::Multiplicative),
                nl(),
                r("unary"),
            ])),
        ),
        rule(
            Expressions,
            "additive_tail",
            seq([
                r("multiplicative_tail"),
                star(seq([
                    operators(OperatorFamily::Additive),
                    nl(),
                    r("multiplicative"),
                ])),
            ]),
        ),
        rule(
            Expressions,
            "stage_end",
            Item::Peek(
                [
                    T::Newline,
                    T::Semicolon,
                    T::RBrace,
                    T::PipeGt,
                    T::RParen,
                    T::RBracket,
                    T::Comma,
                ]
                .map(|tag| vec![tag_term(tag)])
                .to_vec(),
            ),
        ),
        // Patterns.
        rule(
            Patterns,
            "pattern",
            seq([
                r("alias_pattern"),
                star(seq([t(T::Pipe), r("alias_pattern")])),
            ]),
        ),
        rule(
            Patterns,
            "alias_pattern",
            seq([
                r("typed_pattern"),
                star(seq([w("as"), not([vec![word_term("_", false)]]), ident()])),
            ]),
        ),
        rule(
            Patterns,
            "typed_pattern",
            alt([
                seq([
                    alt([w("_"), seq([not([vec![word_term("is", false)]]), name()])]),
                    w("is"),
                    r("type_expr"),
                ]),
                r("primary_pattern"),
            ]),
        ),
        rule(
            Patterns,
            "primary_pattern",
            pattern_primary("record_pattern"),
        ),
        rule(
            Patterns,
            "list_pattern",
            seq([
                t(T::LBracket),
                nl(),
                opt(seq([
                    alt([
                        seq([
                            r("pattern"),
                            star(seq([nl(), t(T::Comma), nl(), r("pattern")])),
                            opt(seq([nl(), t(T::Comma), nl(), r("list_rest")])),
                        ]),
                        r("list_rest"),
                    ]),
                    opt(seq([nl(), t(T::Comma)])),
                ])),
                nl(),
                t(T::RBracket),
            ]),
        ),
        rule(Patterns, "list_rest", seq([dot_dot(), opt(name())])),
        rule(
            Patterns,
            "record_pattern",
            seq([t(T::LBrace), list(r("record_pattern_field")), t(T::RBrace)]),
        ),
        rule(
            Patterns,
            "record_pattern_field",
            alt([
                dot_dot(),
                seq([label(), t(T::Colon), r("pattern")]),
                ident(),
            ]),
        ),
        rule(
            Patterns,
            "condition_primary_pattern",
            pattern_primary("condition_payload"),
        ),
        rule(
            Patterns,
            "condition_payload",
            seq([
                t(T::LBrace),
                nl(),
                alt([dot_dot(), seq([ident(), t(T::Colon), r("pattern")])]),
                star(seq([nl(), t(T::Comma), nl(), r("record_pattern_field")])),
                opt(seq([nl(), t(T::Comma)])),
                nl(),
                t(T::RBrace),
            ]),
        ),
        // Types.
        rule(
            Types,
            "type_expr",
            seq([
                alt([r("builtin_type"), r("named_type")]),
                opt(r("type_arguments")),
                opt(t(T::Question)),
            ]),
        ),
        rule(
            Types,
            "builtin_type",
            alt([
                seq([w("List"), t(T::LBracket), r("type_expr"), t(T::RBracket)]),
                seq([
                    w("Map"),
                    t(T::LBracket),
                    r("type_expr"),
                    opt(seq([t(T::Comma), r("type_expr")])),
                    t(T::RBracket),
                ]),
                seq([w("Stream"), t(T::LBracket), r("type_expr"), t(T::RBracket)]),
                seq([w("Module"), t(T::LBracket), r("type_expr"), t(T::RBracket)]),
                seq([
                    w("Result"),
                    t(T::LBracket),
                    r("type_expr"),
                    opt(seq([t(T::Comma), r("type_expr")])),
                    t(T::RBracket),
                ]),
                seq([
                    w("Union"),
                    t(T::LBracket),
                    r("type_expr"),
                    star(seq([t(T::Comma), r("type_expr")])),
                    t(T::RBracket),
                ]),
            ]),
        ),
        rule(
            Types,
            "named_type",
            seq([
                not(["List", "Map", "Stream", "Module", "Result", "Union"]
                    .map(|word| vec![word_term(word, false)])),
                ident(),
                opt(seq([t(T::Dot), ident()])),
            ]),
        ),
        rule(
            Types,
            "type_arguments",
            seq([
                t(T::LBracket),
                opt(seq([
                    r("type_expr"),
                    star(seq([t(T::Comma), r("type_expr")])),
                ])),
                t(T::RBracket),
            ]),
        ),
        // Commands and processes.
        // A run form followed by `|>` heads a pipeline, as it does in value
        // position.
        rule(
            Commands,
            "run_statement",
            seq([
                r("run_form"),
                opt(t(T::Question)),
                star(seq([t(T::PipeGt), r("pipe_stage")])),
            ]),
        ),
        rule(
            Commands,
            "named_command",
            alt([
                seq([
                    alt([
                        core_word(CoreCommand::Print),
                        core_word(CoreCommand::Eprint),
                    ]),
                    opt(seq([r("lead_argument"), star(r("command_argument"))])),
                ]),
                seq([
                    core_word(CoreCommand::Cd),
                    not([vec![tag_term(T::Equals)]]),
                    r("command_argument"),
                    block(),
                ]),
                seq([
                    core_word(CoreCommand::Env),
                    line(plus(r("env_assignment"))),
                    block(),
                ]),
                seq([
                    not(CoreCommand::ALL.map(|command| vec![word_term(command.as_str(), false)])),
                    name(),
                    opt(seq([r("lead_argument"), star(r("command_argument"))])),
                ]),
                seq([
                    name(),
                    plus(seq([g(T::Dot), gclass(Class::Name)])),
                    r("dotted_lead_argument"),
                    star(r("command_argument")),
                ]),
            ]),
        ),
        rule(
            Commands,
            "lead_argument",
            alt([
                seq([
                    not({
                        let mut stops = operator_leads();
                        stops.extend([vec![tag_term(T::Equals)], vec![tag_term(T::Dot)]]);
                        stops
                    }),
                    r("command_argument"),
                ]),
                seq([
                    t(T::Minus),
                    not([vec![glued_tag_term(T::Equals)]]),
                    plus(seq([g_word_part()])),
                ]),
            ]),
        ),
        rule(
            Commands,
            "dotted_lead_argument",
            seq([
                not({
                    let mut stops = operator_leads();
                    stops.retain(|lead| lead != &vec![keyword_term(Keyword::Not)]);
                    stops.extend([
                        vec![keyword_term(Keyword::Not), keyword_term(Keyword::In)],
                        vec![tag_term(T::Equals)],
                        vec![tag_term(T::Dot)],
                    ]);
                    stops
                }),
                r("command_argument"),
            ]),
        ),
        rule(
            Commands,
            "env_assignment",
            seq([ident(), t(T::Equals), r("command_argument")]),
        ),
        rule(
            Commands,
            "command_argument",
            alt([
                r("call_argument_chain"),
                seq([
                    t(T::At),
                    alt([
                        g(T::Ident),
                        g(T::GlobString),
                        seq([g(T::LParen), r("expression"), t(T::RParen)]),
                    ]),
                ]),
                parens(r("expression")),
                seq([
                    alt([
                        t(T::PathString),
                        t(T::GlobString),
                        t(T::EnvString),
                        t(T::PathFmtString),
                        t(T::FmtString),
                    ]),
                    star(r("glued_postfix")),
                ]),
                r("word"),
            ]),
        ),
        rule(
            Commands,
            "call_argument_chain",
            seq([
                alt([t(T::Ident), t(T::String)]),
                star(seq([opt(g(T::Question)), g(T::Dot), gclass(Class::Label)])),
                alt([
                    seq([g(T::LParen), r("call_arguments"), t(T::RParen)]),
                    seq([
                        opt(g(T::Question)),
                        g(T::LBracket),
                        r("index"),
                        t(T::RBracket),
                    ]),
                ]),
                star(r("glued_postfix")),
            ]),
        ),
        rule(
            Commands,
            "glued_postfix",
            alt([
                seq([g(T::Dot), r("member_access")]),
                seq([g(T::Question), g(T::Dot), r("member_access")]),
                seq([g(T::LBracket), r("index"), t(T::RBracket)]),
                seq([g(T::Question), g(T::LBracket), r("index"), t(T::RBracket)]),
                seq([g(T::LParen), r("call_arguments"), t(T::RParen)]),
                g(T::Question),
            ]),
        ),
        rule(
            Commands,
            "word",
            seq([
                alt([
                    seq([
                        not(
                            [
                                T::PathString,
                                T::GlobString,
                                T::EnvString,
                                T::PathFmtString,
                                T::FmtString,
                            ]
                            .map(|tag| vec![tag_term(tag)]),
                        ),
                        class(Class::WordPart),
                    ]),
                    t(T::String),
                    r("dollar_name"),
                    seq([t(T::DollarLBrace), r("expression"), t(T::RBrace)]),
                ]),
                star(alt([
                    gclass(Class::WordPart),
                    g(T::String),
                    seq([g(T::DollarIdent), star(r("dollar_suffix"))]),
                    seq([g(T::DollarLBrace), r("expression"), t(T::RBrace)]),
                ])),
            ]),
        ),
        rule(
            Commands,
            "dollar_name",
            seq([t(T::DollarIdent), star(r("dollar_suffix"))]),
        ),
        rule(
            Commands,
            "dollar_suffix",
            alt([
                seq([g(T::Dot), gclass(Class::Label)]),
                seq([g(T::LParen), r("call_arguments"), t(T::RParen)]),
            ]),
        ),
        rule(
            Commands,
            "run_form",
            seq([r("run_segment"), star(seq([t(T::Pipe), r("run_segment")]))]),
        ),
        // A run form inside an expression reads command words until one of
        // these tokens.
        rule(
            Commands,
            "run_end",
            Item::Peek(
                [
                    T::Newline,
                    T::Semicolon,
                    T::RBrace,
                    T::Question,
                    T::LBrace,
                    T::PipeGt,
                ]
                .map(|tag| vec![tag_term(tag)])
                .to_vec(),
            ),
        ),
        rule(
            Commands,
            "run_segment",
            seq([
                kw(Keyword::Run),
                // A `.` touching `run` always selects a run form.
                alt([
                    seq([g(T::Dot), r("run_member")]),
                    not([vec![glued_tag_term(T::Dot)]]),
                ]),
                opt(r("run_options")),
                // A repeated option is a mistake, not a command word.
                not(RunOption::ALL.map(|option| {
                    vec![
                        tag_term(T::Minus),
                        glued_tag_term(T::Minus),
                        word_term(option.name(), true),
                    ]
                })),
                star(r("env_assignment")),
                alt([
                    seq([
                        t(T::LParen),
                        t(T::Newline),
                        r("command_argument"),
                        star(alt([t(T::Newline), r("redirection"), r("run_argument")])),
                        t(T::RParen),
                    ]),
                    seq([
                        r("run_argument"),
                        star(alt([r("redirection"), r("run_argument")])),
                    ]),
                ]),
            ]),
        ),
        // Each option at most once, in any order.
        rule(
            Commands,
            "run_options",
            alt(option_orders(&RunOption::ALL)
                .into_iter()
                .map(|order| seq(order.into_iter().map(|option| r(option.rule()))))),
        ),
        rule(
            Commands,
            RunOption::Timeout.rule(),
            run_option(RunOption::Timeout),
        ),
        rule(
            Commands,
            RunOption::CpuMax.rule(),
            run_option(RunOption::CpuMax),
        ),
        rule(
            Commands,
            RunOption::Accept.rule(),
            run_option(RunOption::Accept),
        ),
        rule(
            Commands,
            "run_member",
            alt(RUN_FORMS.iter().filter_map(|form| {
                let member = gw(form.member?);
                Some(match form.mode {
                    Some(mode) => seq([member, t(T::Minus), g(T::Minus), gw(mode)]),
                    None => member,
                })
            })),
        ),
        rule(
            Commands,
            "run_option_value",
            alt([
                t(T::Int),
                t(T::Duration),
                seq([
                    ident(),
                    not([
                        vec![glued_tag_term(T::Dot)],
                        vec![glued_tag_term(T::LParen)],
                        vec![glued_tag_term(T::LBracket)],
                    ]),
                ]),
                seq([
                    ident(),
                    Item::Peek(vec![
                        vec![glued_tag_term(T::Dot)],
                        vec![glued_tag_term(T::LParen)],
                        vec![glued_tag_term(T::LBracket)],
                    ]),
                    plus(r("glued_postfix")),
                ]),
                // `spawn`, `wait`, and `run` would read past the whitespace that
                // ends the option.
                seq([
                    not([T::Int, T::Duration, T::Ident]
                        .map(|tag| vec![tag_term(tag)])
                        .into_iter()
                        .chain(
                            [Keyword::Spawn, Keyword::Wait, Keyword::Run]
                                .map(|keyword| vec![keyword_term(keyword)]),
                        )),
                    r("primary"),
                    star(r("glued_postfix")),
                ]),
            ]),
        ),
        rule(
            Commands,
            "run_argument",
            seq([
                not([
                    vec![tag_term(T::Gt)],
                    vec![tag_term(T::GtGt)],
                    vec![tag_term(T::Lt)],
                    vec![tag_term(T::ErrorGt)],
                    vec![tag_term(T::ErrorGtGt)],
                    vec![word_term("2", false), glued_tag_term(T::Gt)],
                    vec![word_term("2", false), glued_tag_term(T::GtGt)],
                ]),
                r("command_argument"),
            ]),
        ),
        rule(
            Commands,
            "redirection",
            seq([
                alt([
                    t(T::Gt),
                    t(T::GtGt),
                    t(T::Lt),
                    seq([w("2"), g(T::Gt)]),
                    seq([w("2"), g(T::GtGt)]),
                    seq([t(T::Gt), t(T::Amp)]),
                    seq([t(T::Lt), t(T::Amp)]),
                ]),
                alt([
                    seq([t(T::Bytes), star(r("glued_postfix"))]),
                    r("command_argument"),
                ]),
            ]),
        ),
    ];
    let general_at = rules
        .iter()
        .position(|rule| rule.name == "expression_or_run")
        .expect("expression_or_run rule")
        + 1;
    rules.splice(
        general_at..general_at,
        expression_rules(ExpressionContext::General),
    );
    rules.extend(expression_rules(ExpressionContext::Condition));
    rules.sort_by_key(|rule| rule.section);
    rules
}

fn g_word_part() -> Item {
    alt([
        gclass(Class::WordPart),
        g(TokenTag::String),
        seq([g(TokenTag::DollarIdent), star(r("dollar_suffix"))]),
        seq([
            g(TokenTag::DollarLBrace),
            r("expression"),
            t(TokenTag::RBrace),
        ]),
    ])
}
