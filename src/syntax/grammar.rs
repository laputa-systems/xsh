//! The XSH grammar as data: the one definition of the language's syntax.
//!
//! The hand-written recursive-descent parser keeps its error recovery and
//! diagnostics, but every table it dispatches on lives here: binary operators
//! with their precedence, associativity, and line-continuation behavior; the
//! keywords that begin statements and expression forms; quoted-literal
//! prefixes; stream stages; and run forms. The productions describe the same
//! language over the lexer's tokens, and `make docs` renders them as
//! `docs/reference/grammar.md`.
//!
//! The productions are checked against the parser in both directions on
//! every test run: sentences generated from them (`generate`) must parse
//! without diagnostics, and an Earley recognizer over them (`earley`) must
//! accept every checked-in source file and every fuzz-generated program.
//!
//! Productions read a token stream prepared by [`grammar_tokens`]: comments
//! are dropped, a line break before a line-continuation token (see
//! [`line_continuation`]) is removed exactly as the parser joins the lines,
//! and every remaining run of line breaks is one `NEWLINE` terminal. A `\`
//! that ends a line never reaches the productions: the lexer reads it and
//! its line break as whitespace, and the parser rejects one written outside
//! a command (`TokenTable::line_continuations`). A
//! terminal can require that its token is written directly after the
//! previous one ([`Term::glued`]), which is how the grammar states the
//! parser's spacing rules: `f(x)` is a call while `f (x)` passes a typed
//! command argument.

use crate::syntax::literal::QuotedLiteralKind;
use crate::syntax::node::{BinaryOp, Effect, RunKind, StreamStageKind};
use crate::syntax::token::{Keyword, TokenTable, TokenTag};
use std::sync::OnceLock;

pub mod earley;
pub mod generate;
pub mod reference;

/// Binding power of `!` and unary `-`: their operand is parsed at
/// [`PREFIX_OPERAND`], so prefix forms bind tighter than every binary operator.
pub const PREFIX: u8 = 8;
/// Minimum binding power of a prefix operand.
pub const PREFIX_OPERAND: u8 = 9;
/// Binding power of the conversion `value as TYPE`: tighter than every binary
/// operator and looser than a prefix, so `-n as UInt` converts `-n` and
/// `a * b as Int` converts `b`. A conversion chains to the left.
pub const CONVERSION: u8 = 7;
/// `is` applies to an operand built at this precedence or tighter: it shares
/// the equality level.
pub const PATTERN_TEST: u8 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Associativity {
    Left,
    Right,
}

/// Operators of one family chain without grouping; ordering never mixes with
/// equality, membership, or pattern tests (`parse.mixed-comparison`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorFamily {
    Fallback,
    Logical,
    Equality,
    Ordering,
    Membership,
    Additive,
    Multiplicative,
}

/// The first token of an operator spelling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperatorToken {
    Tag(TokenTag),
    Keyword(Keyword),
}

impl OperatorToken {
    const fn matches(self, tag: TokenTag, keyword: Option<Keyword>) -> bool {
        match self {
            Self::Tag(expected) => expected as u8 == tag as u8,
            Self::Keyword(expected) => {
                matches!(tag, TokenTag::Keyword)
                    && matches!(keyword, Some(found) if found as u8 == expected as u8)
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BinaryOperator {
    pub op: BinaryOp,
    pub spelling: &'static str,
    pub first: OperatorToken,
    /// The second keyword of a two-word operator (`not in`).
    pub second: Option<Keyword>,
    pub precedence: u8,
    pub associativity: Associativity,
    pub family: OperatorFamily,
    /// Whether a line that begins with this operator continues the previous
    /// line's expression. `-` (negation) and `/` (an absolute path) can begin
    /// a statement, so a line starting with them never continues.
    pub continues_line: bool,
}

const fn operator(
    op: BinaryOp,
    spelling: &'static str,
    first: OperatorToken,
    precedence: u8,
    family: OperatorFamily,
) -> BinaryOperator {
    BinaryOperator {
        op,
        spelling,
        first,
        second: None,
        precedence,
        associativity: Associativity::Left,
        family,
        continues_line: true,
    }
}

/// Every binary operator, loosest first.
pub const BINARY_OPERATORS: [BinaryOperator; 18] = [
    BinaryOperator {
        associativity: Associativity::Right,
        ..operator(
            BinaryOp::ResultFallback,
            "??",
            OperatorToken::Tag(TokenTag::QuestionQuestion),
            1,
            OperatorFamily::Fallback,
        )
    },
    operator(
        BinaryOp::Or,
        "or",
        OperatorToken::Keyword(Keyword::Or),
        1,
        OperatorFamily::Logical,
    ),
    operator(
        BinaryOp::And,
        "and",
        OperatorToken::Keyword(Keyword::And),
        2,
        OperatorFamily::Logical,
    ),
    operator(
        BinaryOp::Eq,
        "==",
        OperatorToken::Tag(TokenTag::EqEq),
        3,
        OperatorFamily::Equality,
    ),
    operator(
        BinaryOp::Ne,
        "!=",
        OperatorToken::Tag(TokenTag::BangEq),
        3,
        OperatorFamily::Equality,
    ),
    operator(
        BinaryOp::Lt,
        "<",
        OperatorToken::Tag(TokenTag::Lt),
        4,
        OperatorFamily::Ordering,
    ),
    operator(
        BinaryOp::Le,
        "<=",
        OperatorToken::Tag(TokenTag::Le),
        4,
        OperatorFamily::Ordering,
    ),
    operator(
        BinaryOp::Gt,
        ">",
        OperatorToken::Tag(TokenTag::Gt),
        4,
        OperatorFamily::Ordering,
    ),
    operator(
        BinaryOp::Ge,
        ">=",
        OperatorToken::Tag(TokenTag::Ge),
        4,
        OperatorFamily::Ordering,
    ),
    operator(
        BinaryOp::In,
        "in",
        OperatorToken::Keyword(Keyword::In),
        4,
        OperatorFamily::Membership,
    ),
    BinaryOperator {
        second: Some(Keyword::In),
        ..operator(
            BinaryOp::NotIn,
            "not in",
            OperatorToken::Keyword(Keyword::Not),
            4,
            OperatorFamily::Membership,
        )
    },
    operator(
        BinaryOp::Add,
        "+",
        OperatorToken::Tag(TokenTag::Plus),
        5,
        OperatorFamily::Additive,
    ),
    BinaryOperator {
        continues_line: false,
        ..operator(
            BinaryOp::Sub,
            "-",
            OperatorToken::Tag(TokenTag::Minus),
            5,
            OperatorFamily::Additive,
        )
    },
    operator(
        BinaryOp::Mul,
        "*",
        OperatorToken::Tag(TokenTag::Star),
        6,
        OperatorFamily::Multiplicative,
    ),
    BinaryOperator {
        continues_line: false,
        ..operator(
            BinaryOp::Div,
            "/",
            OperatorToken::Tag(TokenTag::Slash),
            6,
            OperatorFamily::Multiplicative,
        )
    },
    operator(
        BinaryOp::Rem,
        "%",
        OperatorToken::Tag(TokenTag::Percent),
        6,
        OperatorFamily::Multiplicative,
    ),
    // The set operators sit with the arithmetic they resemble: union with
    // `+`, intersection with `*`. A line never starts with one, since `|`
    // also opens a block's parameters.
    BinaryOperator {
        continues_line: false,
        ..operator(
            BinaryOp::Union,
            "|",
            OperatorToken::Tag(TokenTag::Pipe),
            5,
            OperatorFamily::Additive,
        )
    },
    BinaryOperator {
        continues_line: false,
        ..operator(
            BinaryOp::Intersect,
            "&",
            OperatorToken::Tag(TokenTag::Amp),
            6,
            OperatorFamily::Multiplicative,
        )
    },
];

pub const fn binary_operator(op: BinaryOp) -> &'static BinaryOperator {
    let mut index = 0;
    while index < BINARY_OPERATORS.len() {
        if BINARY_OPERATORS[index].op as u8 == op as u8 {
            return &BINARY_OPERATORS[index];
        }
        index += 1;
    }
    panic!("every binary operator has a grammar row")
}

pub const fn binary_precedence(op: BinaryOp) -> u8 {
    binary_operator(op).precedence
}

/// The minimum binding power of the right operand: a left-associative
/// operator's right operand binds tighter than the operator itself.
pub const fn binary_right_operand_precedence(op: BinaryOp) -> u8 {
    let operator = binary_operator(op);
    match operator.associativity {
        Associativity::Right => operator.precedence,
        Associativity::Left => operator.precedence + 1,
    }
}

/// The operator that a token (and, for `not in`, the keyword after it)
/// spells, if any.
pub fn binary_operator_at(
    tag: TokenTag,
    keyword: Option<Keyword>,
    next_keyword: Option<Keyword>,
) -> Option<&'static BinaryOperator> {
    BINARY_OPERATORS.iter().find(|operator| {
        operator.first.matches(tag, keyword)
            && operator
                .second
                .is_none_or(|second| next_keyword == Some(second))
    })
}

/// What a line that begins with a continuation token continues.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineContinuation {
    /// A binary operator whose row allows it ([`BinaryOperator::continues_line`]).
    Operator,
    /// `.name` applies to the previous line's expression.
    Member,
    /// `|>` adds a pipeline stage.
    Pipeline,
}

/// Whether a line beginning with `first` (followed by `second`) continues the
/// expression on the line before it. No continuation token can begin a
/// statement, so a line break never silently joins two statements; the item
/// expression `.name` is the one exception, and it is always postfix at the
/// start of a line.
pub fn line_continuation(
    first: TokenTag,
    first_keyword: Option<Keyword>,
    second: Option<TokenTag>,
    second_keyword: Option<Keyword>,
) -> Option<LineContinuation> {
    match first {
        TokenTag::PipeGt => Some(LineContinuation::Pipeline),
        TokenTag::Dot
            if matches!(
                second,
                Some(TokenTag::Ident | TokenTag::ProcIdent | TokenTag::Keyword)
            ) =>
        {
            Some(LineContinuation::Member)
        }
        _ => binary_operator_at(first, first_keyword, second_keyword)
            .filter(|operator| operator.continues_line)
            .map(|_| LineContinuation::Operator),
    }
}

/// The spellings that continue an expression onto a new line.
pub fn line_continuation_spellings() -> Vec<&'static str> {
    let mut spellings: Vec<&'static str> = BINARY_OPERATORS
        .iter()
        .filter(|operator| operator.continues_line)
        .map(|operator| operator.spelling)
        .collect();
    spellings.extend([".", "|>"]);
    spellings
}

/// A form that a keyword begins in statement position. The parser dispatches
/// on this table, and the `statement` productions begin each form's rule
/// with its keyword.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatementForm {
    Binding,
    Assert,
    If,
    While,
    For,
    Loop,
    Return,
    Yield,
    Defer,
    Break,
    Continue,
    Match,
    Proc,
    Pure,
    Stream,
    Use,
    Guard,
    With,
    Enum,
    Type,
    Export,
    /// A command statement: `run ...`.
    Run,
}

impl StatementForm {
    /// The production for this form. `export` publishes several kinds of
    /// declaration, so its productions are part of `compound_statement` and
    /// `simple_statement` directly.
    pub const fn rule(self) -> Option<&'static str> {
        Some(match self {
            Self::Binding => "binding",
            Self::Assert => "assert_statement",
            Self::If => "if_statement",
            Self::While => "while_statement",
            Self::For => "for_statement",
            Self::Loop => "loop_statement",
            Self::Return => "return_statement",
            Self::Yield => "yield_statement",
            Self::Defer => "defer_statement",
            Self::Break => "break_statement",
            Self::Continue => "continue_statement",
            Self::Match => "match_statement",
            Self::Proc => "proc_declaration",
            Self::Pure => "pure_declaration",
            Self::Stream => "stream_declaration",
            Self::Use => "use_statement",
            Self::Guard => "guard_statement",
            Self::With => "with_statement",
            Self::Enum => "enum_declaration",
            Self::Type => "type_declaration",
            Self::Run => "run_statement",
            Self::Export => return None,
        })
    }

    /// A compound statement ends with a block and needs no terminator.
    pub const fn is_compound(self) -> bool {
        matches!(
            self,
            Self::If
                | Self::While
                | Self::For
                | Self::Loop
                | Self::Match
                | Self::Proc
                | Self::Pure
                | Self::Stream
                | Self::Guard
                | Self::With
        )
    }
}

pub const STATEMENT_KEYWORDS: [(Keyword, StatementForm); 25] = [
    (Keyword::Let, StatementForm::Binding),
    (Keyword::Const, StatementForm::Binding),
    (Keyword::Var, StatementForm::Binding),
    (Keyword::Assert, StatementForm::Assert),
    (Keyword::If, StatementForm::If),
    (Keyword::While, StatementForm::While),
    (Keyword::For, StatementForm::For),
    (Keyword::Loop, StatementForm::Loop),
    (Keyword::Return, StatementForm::Return),
    (Keyword::Yield, StatementForm::Yield),
    (Keyword::Defer, StatementForm::Defer),
    (Keyword::Errdefer, StatementForm::Defer),
    (Keyword::Break, StatementForm::Break),
    (Keyword::Continue, StatementForm::Continue),
    (Keyword::Match, StatementForm::Match),
    (Keyword::Proc, StatementForm::Proc),
    (Keyword::Pure, StatementForm::Pure),
    (Keyword::Stream, StatementForm::Stream),
    (Keyword::Use, StatementForm::Use),
    (Keyword::Guard, StatementForm::Guard),
    (Keyword::With, StatementForm::With),
    (Keyword::Enum, StatementForm::Enum),
    (Keyword::Type, StatementForm::Type),
    (Keyword::Export, StatementForm::Export),
    (Keyword::Run, StatementForm::Run),
];

/// The statement form a keyword begins, if any.
pub fn statement_form(keyword: Keyword) -> Option<StatementForm> {
    STATEMENT_KEYWORDS
        .iter()
        .find(|(candidate, _)| *candidate == keyword)
        .map(|(_, form)| *form)
}

/// Keyword statements a builder block accepts as entries.
pub const BUILDER_STATEMENT_KEYWORDS: [Keyword; 15] = [
    Keyword::Let,
    Keyword::Const,
    Keyword::Var,
    Keyword::Return,
    Keyword::Defer,
    Keyword::Errdefer,
    Keyword::If,
    Keyword::While,
    Keyword::For,
    Keyword::Loop,
    Keyword::Guard,
    Keyword::Break,
    Keyword::Continue,
    Keyword::Match,
    Keyword::Run,
];

/// `(module, function)` calls that take a builder block: `process.command { ... }`.
pub const BUILDER_APIS: [(&str, &str); 1] = [("process", "command")];

pub fn builder_api_accepts_block(module: &str, function: &str) -> bool {
    BUILDER_APIS.contains(&(module, function))
}

/// Keyword-led declarations that `export` publishes. `export` also accepts a
/// signal hook (`on`) and an error family (`error`), which begin with
/// contextual words.
pub const EXPORTABLE_KEYWORDS: [Keyword; 7] = [
    Keyword::Let,
    Keyword::Const,
    Keyword::Proc,
    Keyword::Pure,
    Keyword::Stream,
    Keyword::Enum,
    Keyword::Type,
];

/// Keywords that begin a primary expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimaryForm {
    Null,
    True,
    False,
    If,
    Match,
    Loop,
    Try,
    Retry,
    Run,
    Spawn,
    Wait,
}

pub const PRIMARY_KEYWORDS: [(Keyword, PrimaryForm); 11] = [
    (Keyword::Null, PrimaryForm::Null),
    (Keyword::True, PrimaryForm::True),
    (Keyword::False, PrimaryForm::False),
    (Keyword::If, PrimaryForm::If),
    (Keyword::Match, PrimaryForm::Match),
    (Keyword::Loop, PrimaryForm::Loop),
    (Keyword::Try, PrimaryForm::Try),
    (Keyword::Retry, PrimaryForm::Retry),
    (Keyword::Run, PrimaryForm::Run),
    (Keyword::Spawn, PrimaryForm::Spawn),
    (Keyword::Wait, PrimaryForm::Wait),
];

pub fn primary_form(keyword: Keyword) -> Option<PrimaryForm> {
    PRIMARY_KEYWORDS
        .iter()
        .find(|(candidate, _)| *candidate == keyword)
        .map(|(_, form)| *form)
}

/// Keywords that a record shorthand field can never be: `{true}` and
/// `{return}` open blocks.
pub const BLOCK_ONLY_KEYWORDS: [Keyword; 6] = [
    Keyword::True,
    Keyword::False,
    Keyword::Null,
    Keyword::Return,
    Keyword::Break,
    Keyword::Continue,
];

/// A quoted literal form, selected by the bytes before its opening quote.
#[derive(Clone, Copy, Debug)]
pub struct QuotedLiteralForm {
    pub prefix: &'static str,
    pub(crate) kind: QuotedLiteralKind,
    /// Escapes are not decoded.
    pub raw: bool,
    pub token: TokenTag,
}

/// Every quoted literal prefix, longest first so the first match wins.
pub const QUOTED_LITERALS: [QuotedLiteralForm; 9] = [
    QuotedLiteralForm {
        prefix: "rx",
        kind: QuotedLiteralKind::Regex,
        raw: true,
        token: TokenTag::Regex,
    },
    QuotedLiteralForm {
        prefix: "fp",
        kind: QuotedLiteralKind::PathFmt,
        raw: false,
        token: TokenTag::PathFmtString,
    },
    QuotedLiteralForm {
        prefix: "b",
        kind: QuotedLiteralKind::Bytes,
        raw: false,
        token: TokenTag::Bytes,
    },
    QuotedLiteralForm {
        prefix: "p",
        kind: QuotedLiteralKind::Path,
        raw: false,
        token: TokenTag::PathString,
    },
    QuotedLiteralForm {
        prefix: "g",
        kind: QuotedLiteralKind::Glob,
        raw: false,
        token: TokenTag::GlobString,
    },
    // An environment variable named by its literal identifier. The contents
    // are kept raw because a valid name never needs an escape.
    QuotedLiteralForm {
        prefix: "e",
        kind: QuotedLiteralKind::Env,
        raw: true,
        token: TokenTag::EnvString,
    },
    QuotedLiteralForm {
        prefix: "f",
        kind: QuotedLiteralKind::Fmt,
        raw: false,
        token: TokenTag::FmtString,
    },
    QuotedLiteralForm {
        prefix: "r",
        kind: QuotedLiteralKind::Str,
        raw: true,
        token: TokenTag::String,
    },
    QuotedLiteralForm {
        prefix: "",
        kind: QuotedLiteralKind::Str,
        raw: false,
        token: TokenTag::String,
    },
];

/// The quoted literal form whose prefix and opening quote start at `start`.
pub fn quoted_literal_at(bytes: &[u8], start: usize) -> Option<&'static QuotedLiteralForm> {
    let rest = bytes.get(start..)?;
    QUOTED_LITERALS.iter().find(|form| {
        rest.starts_with(form.prefix.as_bytes()) && rest.get(form.prefix.len()) == Some(&b'"')
    })
}

/// Duration literal suffixes, longest first.
pub const DURATION_SUFFIXES: [&str; 4] = ["ms", "s", "m", "h"];

/// The duration suffix written at `offset`, right after an integer's digits.
pub fn duration_suffix_at(bytes: &[u8], offset: usize) -> Option<&'static str> {
    let rest = bytes.get(offset..)?;
    DURATION_SUFFIXES
        .into_iter()
        .find(|suffix| rest.starts_with(suffix.as_bytes()))
}

/// Size literal suffixes with the number of bytes each stands for: binary
/// units are powers of 1024 and decimal units powers of 1000.
pub const SIZE_SUFFIXES: [(&str, u64); 6] = [
    ("KiB", 1 << 10),
    ("MiB", 1 << 20),
    ("GiB", 1 << 30),
    ("KB", 1_000),
    ("MB", 1_000_000),
    ("GB", 1_000_000_000),
];

/// The size suffix written at `offset`, right after an integer's digits. A
/// suffix that runs on into more name characters is not one: `1KBps` is not
/// a size.
pub fn size_suffix_at(bytes: &[u8], offset: usize) -> Option<&'static str> {
    let rest = bytes.get(offset..)?;
    SIZE_SUFFIXES
        .into_iter()
        .map(|(suffix, _)| suffix)
        .find(|suffix| {
            rest.starts_with(suffix.as_bytes())
                && !rest
                    .get(suffix.len())
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        })
}

/// A `|>` stage that the parser reads as a structured stream stage rather
/// than a value expression.
#[derive(Clone, Copy, Debug)]
pub struct StreamStage {
    pub kind: StreamStageKind,
    /// The stage name; a dotted name such as `text.lines` is written with
    /// the dot touching both words.
    pub name: &'static str,
    /// The stage takes a `{ ... }` block after its arguments.
    pub block: bool,
    /// The stage takes an inline expression instead of a block.
    pub inline: bool,
}

const fn stage(
    kind: StreamStageKind,
    name: &'static str,
    block: bool,
    inline: bool,
) -> StreamStage {
    StreamStage {
        kind,
        name,
        block,
        inline,
    }
}

/// Every stream stage, in `StreamStageKind` declaration order.
pub const STREAM_STAGES: [StreamStage; 35] = [
    stage(StreamStageKind::Where, "where", true, true),
    stage(StreamStageKind::Map, "map", true, true),
    stage(StreamStageKind::ParMap, "par-map", true, true),
    stage(StreamStageKind::Each, "each", true, true),
    stage(StreamStageKind::Batch, "batch", true, false),
    stage(StreamStageKind::Sort, "sort", false, false),
    stage(StreamStageKind::SortBy, "sort-by", true, true),
    stage(StreamStageKind::Take, "take", false, false),
    stage(StreamStageKind::Drop, "drop", false, false),
    stage(StreamStageKind::First, "first", false, false),
    stage(StreamStageKind::Last, "last", false, false),
    stage(StreamStageKind::UniqueBy, "unique-by", true, true),
    stage(StreamStageKind::Enumerate, "enumerate", false, false),
    stage(StreamStageKind::Zip, "zip", false, false),
    stage(StreamStageKind::Range, "range", false, false),
    stage(StreamStageKind::Repeat, "repeat", false, false),
    stage(StreamStageKind::Tee, "tee", true, true),
    stage(StreamStageKind::Sum, "sum", false, false),
    stage(StreamStageKind::Min, "min", false, false),
    stage(StreamStageKind::Max, "max", false, false),
    stage(StreamStageKind::GroupBy, "group-by", true, true),
    stage(StreamStageKind::Fold, "fold", true, false),
    stage(StreamStageKind::Reduce, "reduce", true, false),
    stage(StreamStageKind::FlatMap, "flat-map", true, true),
    stage(StreamStageKind::Any, "any", true, true),
    stage(StreamStageKind::All, "all", true, true),
    stage(StreamStageKind::Shuffle, "shuffle", false, false),
    stage(StreamStageKind::TablePrint, "table.print", false, false),
    stage(StreamStageKind::TextStreamLines, "text.lines", false, false),
    stage(StreamStageKind::BytesChunks, "bytes.chunks", false, false),
    stage(StreamStageKind::JsonLines, "json.lines", false, false),
    stage(StreamStageKind::JsonStream, "json.stream", false, false),
    stage(StreamStageKind::Count, "count", true, false),
    stage(StreamStageKind::Collect, "collect", true, false),
    stage(StreamStageKind::ReduceBy, "reduce-by", true, false),
];

pub const fn stream_stage(kind: StreamStageKind) -> &'static StreamStage {
    &STREAM_STAGES[kind as usize]
}

/// The stage spelled `name` or, for a dotted stage, `name.member`.
pub fn stream_stage_named(name: &str, member: Option<&str>) -> Option<&'static StreamStage> {
    STREAM_STAGES
        .iter()
        .find(|stage| match (stage.name.split_once('.'), member) {
            (None, None) => stage.name == name,
            (Some((namespace, stage_member)), Some(member)) => {
                namespace == name && stage_member == member
            }
            _ => false,
        })
}

/// Whether `name` begins a dotted stage name such as `text.lines`.
pub fn is_stream_stage_namespace(name: &str) -> bool {
    STREAM_STAGES.iter().any(|stage| {
        stage
            .name
            .split_once('.')
            .is_some_and(|(namespace, _)| namespace == name)
    })
}

/// A `run` form: `run`, or `run.MEMBER` with an optional capture mode.
#[derive(Clone, Copy, Debug)]
pub struct RunForm {
    pub kind: RunKind,
    pub member: Option<&'static str>,
    /// The mode word after the form (`--text` or `--bytes`).
    pub mode: Option<&'static str>,
}

pub const RUN_FORMS: [RunForm; 8] = [
    RunForm {
        kind: RunKind::Plain,
        member: None,
        mode: None,
    },
    RunForm {
        kind: RunKind::Status,
        member: Some("status"),
        mode: None,
    },
    RunForm {
        kind: RunKind::CaptureText,
        member: Some("text"),
        mode: None,
    },
    RunForm {
        kind: RunKind::CaptureBytes,
        member: Some("bytes"),
        mode: None,
    },
    RunForm {
        kind: RunKind::CaptureTextRecord,
        member: Some("capture"),
        mode: Some("text"),
    },
    RunForm {
        kind: RunKind::CaptureBytesRecord,
        member: Some("capture"),
        mode: Some("bytes"),
    },
    RunForm {
        kind: RunKind::StreamText,
        member: Some("stream"),
        mode: Some("text"),
    },
    RunForm {
        kind: RunKind::StreamBytes,
        member: Some("stream"),
        mode: Some("bytes"),
    },
];

/// The run form spelled `run.member --mode`.
pub fn run_form(member: &str, mode: Option<&str>) -> Option<&'static RunForm> {
    RUN_FORMS
        .iter()
        .find(|form| form.member == Some(member) && form.mode == mode)
}

/// Whether `run.member` takes a capture mode word.
pub fn run_form_takes_mode(member: &str) -> bool {
    RUN_FORMS
        .iter()
        .any(|form| form.member == Some(member) && form.mode.is_some())
}

/// A `--name=value` option of a run segment. Each is written at most once,
/// in any order, before the segment's environment assignments.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunOption {
    Timeout,
    CpuMax,
    Accept,
}

impl RunOption {
    pub const ALL: [RunOption; 3] = [Self::Timeout, Self::CpuMax, Self::Accept];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::CpuMax => "cpumax",
            Self::Accept => "accept",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|option| option.name() == name)
    }

    /// The production for this option.
    pub const fn rule(self) -> &'static str {
        match self {
            Self::Timeout => "timeout_option",
            Self::CpuMax => "cpumax_option",
            Self::Accept => "accept_option",
        }
    }
}

/// An option of an `on SIGNAL` hook.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignalHookOption {
    PreCancel,
}

impl SignalHookOption {
    pub const ALL: [SignalHookOption; 1] = [Self::PreCancel];

    pub const fn name(self) -> &'static str {
        match self {
            Self::PreCancel => "pre-cancel",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|option| option.name() == name)
    }
}

/// The token classes a terminal can match.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Class {
    Tag(TokenTag),
    Keyword(Keyword),
    /// A contextual word: an identifier (or `2` in `2>`) spelled exactly so.
    Word(&'static str),
    /// An identifier, which may contain `-` after its first character.
    Name,
    /// A field label: an identifier or a keyword.
    Label,
    /// A member name after `.`: a label or a hyphenated identifier.
    Member,
    /// A token that can be part of a bare command word.
    WordPart,
    /// A token made only of bare-path characters.
    PathPart,
}

impl Class {
    pub fn matches(self, token: &GrammarToken<'_>) -> bool {
        match self {
            Self::Tag(tag) => token.tag == tag,
            Self::Keyword(keyword) => token.keyword == Some(keyword),
            Self::Word(word) => {
                matches!(
                    token.tag,
                    TokenTag::Ident | TokenTag::ProcIdent | TokenTag::Int
                ) && token.text == word
            }
            Self::Name => matches!(token.tag, TokenTag::Ident | TokenTag::ProcIdent),
            Self::Label => matches!(token.tag, TokenTag::Ident | TokenTag::Keyword),
            Self::Member => matches!(
                token.tag,
                TokenTag::Ident | TokenTag::ProcIdent | TokenTag::Keyword
            ),
            Self::WordPart => is_word_part(token.tag),
            Self::PathPart => {
                !token.text.is_empty()
                    && token
                        .text
                        .chars()
                        .all(crate::syntax::literal::is_bare_path_literal_char)
            }
        }
    }
}

/// Whether a token can be part of a bare command word. Quoted strings,
/// `$name`, and `${` are word parts with their own productions. A `[` or `(`
/// touching a name makes it an index or call argument, so brackets are never
/// part of a bare word.
pub const fn is_word_part(tag: TokenTag) -> bool {
    !matches!(
        tag,
        TokenTag::LBracket
            | TokenTag::RBracket
            | TokenTag::Eof
            | TokenTag::Newline
            | TokenTag::Comment
            | TokenTag::Semicolon
            | TokenTag::LBrace
            | TokenTag::RBrace
            | TokenTag::At
            | TokenTag::Question
            | TokenTag::Pipe
            | TokenTag::PipeGt
            | TokenTag::Amp
            | TokenTag::LParen
            | TokenTag::RParen
            | TokenTag::String
            | TokenTag::DollarIdent
            | TokenTag::DollarLBrace
    )
}

/// A grammar terminal.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Term {
    pub class: Class,
    /// The token must be written directly after the previous one, with no
    /// whitespace, line break, or comment between them.
    pub glued: bool,
}

impl Term {
    pub fn matches(&self, token: &GrammarToken<'_>) -> bool {
        (!self.glued || token.glued) && self.class.matches(token)
    }
}

/// Binding names exclude the hyphenated names used to invoke procedures.
pub const BINDING_NAME: TokenTag = TokenTag::Ident;
pub const TRAILING_TRY_END: [TokenTag; 7] = [
    TokenTag::Newline, TokenTag::Semicolon, TokenTag::Comma, TokenTag::RParen,
    TokenTag::RBracket, TokenTag::RBrace, TokenTag::Eof,
];

const fn token_term(tag: TokenTag, glued: bool) -> Term {
    Term { class: Class::Tag(tag), glued }
}

/// A range marker's dots touch; a qualified pattern continues with a name.
pub const RANGE_MARKER: [Term; 2] = [token_term(TokenTag::Dot, false), token_term(TokenTag::Dot, true)];
pub const COMMAND_MEMBER: [Term; 2] = [token_term(TokenTag::Dot, false), Term { class: Class::Name, glued: false }];
pub const PATTERN_MEMBER: [Term; 2] = [token_term(TokenTag::Dot, false), token_term(BINDING_NAME, false)];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostfixForm { Member, SafeMember, Index, SafeIndex, Call, Propagation }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NameStatementForm { Expression, Command, ProcCommand }

impl PostfixForm {
    pub const ALL: [Self; 6] = [Self::Member, Self::SafeMember, Self::Index, Self::SafeIndex, Self::Call, Self::Propagation];

    /// Expression suffixes permit spaces before member, call, and index
    /// openers. Guarded indices and propagation attach to the receiver;
    /// command arguments require every opener to attach to it.
    pub fn lead(self, command_argument: bool) -> impl Iterator<Item = Term> {
        let terms: &'static [Term] = match self {
            Self::Member => const { &[token_term(TokenTag::Dot, false)] },
            Self::SafeMember => const { &[token_term(TokenTag::Question, false), token_term(TokenTag::Dot, true)] },
            Self::Index => const { &[token_term(TokenTag::LBracket, false)] },
            Self::SafeIndex => const { &[token_term(TokenTag::Question, true), token_term(TokenTag::LBracket, true)] },
            Self::Call => const { &[token_term(TokenTag::LParen, false)] },
            Self::Propagation => const { &[token_term(TokenTag::Question, true)] },
        };
        terms.iter().copied().enumerate().map(move |(index, mut term)| {
            term.glued |= command_argument && index == 0;
            term
        })
    }
}

/// Name-led statements select expression parsing only for an attached suffix
/// or an operator. A spaced argument after a qualified name selects a command.
pub fn name_has_postfix<'s>(mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> bool {
    peek(1).is_some_and(|token| PostfixForm::ALL.into_iter().any(|form| {
        form.lead(true).next().expect("postfix opener").matches(&token)
    }))
}

pub fn name_has_binary<'s>(mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> bool {
    if let Some(next) = peek(1) {
        if next.tag != TokenTag::Minus
            && (next.keyword == Some(Keyword::Not)
                || binary_operator_at(next.tag, next.keyword, peek(2).and_then(|token| token.keyword)).is_some())
            || next.tag == TokenTag::Minus && (next.glued || !next.next_glued)
            // Dollar-prefixed words carry these names but remain command arguments.
            || next.tag == TokenTag::Ident && matches!(next.text, "is" | "as")
        {
            return true;
        }
    }
    let mut offset = 1;
    while peek(offset).is_some_and(|token| token.tag == TokenTag::Newline) { offset += 1; }
    peek(offset).is_some_and(|token| token.tag == TokenTag::PipeGt)
}

pub fn name_is_dotted_command<'s>(mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> bool {
    let mut offset = 0;
    while peek(offset + 1).is_some_and(|token| token.tag == TokenTag::Dot && token.glued)
        && peek(offset + 2).is_some_and(|token| matches!(token.tag, TokenTag::Ident | TokenTag::ProcIdent))
    {
        offset += 2;
    }
    offset > 0 && peek(offset + 1).is_some_and(|token| {
        !token.glued
            && !matches!(token.tag, TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Eof | TokenTag::PipeGt)
            && binary_operator_at(token.tag, token.keyword, peek(offset + 2).and_then(|next| next.keyword)).is_none()
            && !(token.tag == TokenTag::Ident && matches!(token.text, "is" | "as"))
            // A postfix guard follows an expression statement, as an operator would.
            && !matches!(token.keyword, Some(Keyword::When | Keyword::Unless))
    })
}

pub fn name_starts_context<'s>(mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> bool {
    if !peek(0).is_some_and(|token| token.tag == TokenTag::Ident && token.text == "ctx")
        || peek(1).is_some_and(|token| token.glued)
    {
        return false;
    }
    let mut depth = 0;
    for offset in 1.. {
        match peek(offset).map(|token| token.tag) {
            Some(TokenTag::LParen | TokenTag::LBracket) => depth += 1,
            Some(TokenTag::RParen | TokenTag::RBracket) if depth > 0 => depth -= 1,
            Some(TokenTag::LBrace) if depth == 0 => return true,
            Some(TokenTag::Newline | TokenTag::Semicolon | TokenTag::Equals | TokenTag::RBrace) | None if depth == 0 => return false,
            Some(TokenTag::Dot) if offset == 1 => return false,
            None => return false,
            _ => {}
        }
    }
    unreachable!()
}

pub fn name_starts_collect<'s>(mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> bool {
    peek(0).is_some_and(|token| token.tag == TokenTag::Ident && token.text == "collect")
        && peek(1).is_some_and(|token| token.tag == TokenTag::LBrace)
}

pub fn name_statement_is_expression<'s>(mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> bool {
    let Some(first) = peek(0) else { return true };
    !matches!(first.tag, TokenTag::Ident | TokenTag::ProcIdent)
        || name_starts_collect(&mut peek)
        || name_starts_context(&mut peek)
        || !name_is_dotted_command(&mut peek) && (name_has_postfix(&mut peek) || name_has_binary(&mut peek))
}

/// A block on a command's line selects its scoped form. Interpolation
/// braces belong to an argument and do not open that scope.
pub fn command_line_has_block<'s>(mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> bool {
    let mut interpolations = 0usize;
    for offset in 1.. {
        match peek(offset).map(|token| token.tag) {
            Some(TokenTag::DollarLBrace) => interpolations += 1,
            Some(TokenTag::RBrace) if interpolations > 0 => interpolations -= 1,
            Some(TokenTag::LBrace) if interpolations == 0 => return true,
            None | Some(TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Eof) => return false,
            _ => {}
        }
    }
    unreachable!()
}

/// An attached first namespace dot escapes a core command's spelling.
/// `env` selects its core form only with a scope block on the line.
pub fn core_command_at<'s>(mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> Option<crate::syntax::node::CoreCommand> {
    use crate::syntax::node::CoreCommand;
    let command = CoreCommand::from_name(peek(0)?.text)?;
    if peek(1).is_some_and(|token| token.tag == TokenTag::Dot && token.glued)
        || command == CoreCommand::Env && !command_line_has_block(peek)
    { None } else { Some(command) }
}

pub fn name_statement_matches<'s>(form: NameStatementForm, mut peek: impl FnMut(usize) -> Option<GrammarToken<'s>>) -> bool {
    let expression = name_statement_is_expression(&mut peek);
    match form {
        NameStatementForm::Expression => expression,
        NameStatementForm::Command => !expression,
        NameStatementForm::ProcCommand => !expression && core_command_at(peek).is_none(),
    }
}

/// A production body in extended BNF.
#[derive(Clone, Debug)]
pub enum Item {
    Term(Term),
    /// Zero-width: the statement head selects the requested name form.
    NameStatement(NameStatementForm),
    Rule(&'static str),
    Seq(Vec<Item>),
    Alt(Vec<Item>),
    Opt(Box<Item>),
    Star(Box<Item>),
    Plus(Box<Item>),
    /// `item ("," item)* ","?`. With `lines`, line breaks may surround each
    /// item and comma; `min_one` requires at least one item.
    List {
        item: Box<Item>,
        lines: bool,
        min_one: bool,
    },
    /// Zero-width: the following tokens do not begin with any of these
    /// sequences.
    Not(Vec<Vec<Term>>),
    /// Zero-width: the following tokens begin with one of these sequences,
    /// or the input ends.
    Peek(Vec<Vec<Term>>),
    /// The item is written on one line: it contains no `NEWLINE`.
    Line(Box<Item>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Section {
    Programs,
    Declarations,
    Statements,
    Expressions,
    Patterns,
    Types,
    Commands,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Self::Programs,
        Self::Declarations,
        Self::Statements,
        Self::Expressions,
        Self::Patterns,
        Self::Types,
        Self::Commands,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Programs => "Programs and blocks",
            Self::Declarations => "Declarations",
            Self::Statements => "Statements",
            Self::Expressions => "Expressions",
            Self::Patterns => "Patterns",
            Self::Types => "Types",
            Self::Commands => "Commands and processes",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub section: Section,
    pub name: &'static str,
    pub body: Item,
}

#[derive(Debug)]
pub struct Grammar {
    pub rules: Vec<Rule>,
}

impl Grammar {
    pub const START: &'static str = "program";

    pub fn rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.name == name)
    }
}

/// The XSH grammar.
pub fn grammar() -> &'static Grammar {
    static GRAMMAR: OnceLock<Grammar> = OnceLock::new();
    GRAMMAR.get_or_init(|| Grammar {
        rules: productions::rules(),
    })
}

/// A lexer token as a grammar terminal sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GrammarToken<'s> {
    pub tag: TokenTag,
    pub keyword: Option<Keyword>,
    pub text: &'s str,
    /// Written directly after the previous token, or the first token of a
    /// line that continues the previous one.
    pub glued: bool,
    /// The next lexer token touches this one, before comments and line breaks
    /// are normalized. A standalone command flag depends on that boundary.
    pub next_glued: bool,
}

/// The terminals the productions read for `source`: comments are dropped, a
/// continuation line is joined to the line before it, and each other run of
/// line breaks becomes one `NEWLINE`. A command line continued with `\` has
/// no line break to join: the lexer emits no token for the backslash or its
/// line break. The end-of-input token is not included.
pub fn grammar_tokens<'s>(source: &'s str, table: &TokenTable) -> Vec<GrammarToken<'s>> {
    let length = table.len().saturating_sub(usize::from(
        table.tag_at(table.len().saturating_sub(1)) == Some(TokenTag::Eof),
    ));
    let text = |index: usize| -> &'s str {
        let start = table.start_at(index).expect("token start");
        let end = table.end_at(index, source).expect("token end");
        &source[start..end]
    };
    let mut tokens: Vec<GrammarToken<'s>> = Vec::with_capacity(length);
    let mut continued = false;
    let mut index = 0;
    while index < length {
        let tag = table.tag_at(index).expect("token tag");
        match tag {
            TokenTag::Comment => index += 1,
            TokenTag::Newline => {
                let mut next = index;
                while next < length
                    && matches!(
                        table.tag_at(next),
                        Some(TokenTag::Newline | TokenTag::Comment)
                    )
                {
                    next += 1;
                }
                let joins = next < length
                    && line_continuation(
                        table.tag_at(next).expect("token tag"),
                        table.keyword_at(next),
                        table.tag_at(next + 1).filter(|_| next + 1 < length),
                        table.keyword_at(next + 1),
                    )
                    .is_some();
                if joins {
                    continued = true;
                } else if tokens
                    .last()
                    .is_some_and(|token| token.tag != TokenTag::Newline)
                {
                    tokens.push(GrammarToken {
                        tag,
                        keyword: None,
                        text: "\n",
                        glued: false,
                        next_glued: table.end_at(index, source) == table.start_at(index + 1),
                    });
                }
                index = next;
            }
            _ => {
                let start = table.start_at(index).expect("token start");
                let adjacent = index > 0
                    && !matches!(
                        table.tag_at(index - 1),
                        Some(TokenTag::Newline | TokenTag::Comment)
                    )
                    && table.end_at(index - 1, source) == Some(start);
                tokens.push(GrammarToken {
                    tag,
                    keyword: table.keyword_at(index),
                    text: text(index),
                    glued: adjacent || continued,
                    next_glued: table.end_at(index, source) == table.start_at(index + 1),
                });
                continued = false;
                index += 1;
            }
        }
    }
    tokens
}

/// Lexes `source` and returns its grammar terminals, or `None` when the lexer
/// reports a diagnostic.
pub fn lex_grammar_tokens(source: &str) -> Option<Vec<GrammarToken<'_>>> {
    let lexed =
        crate::syntax::lexer::Lexer::new(crate::source::SourceId::new(0), source).lex_compact();
    lexed
        .diagnostics
        .is_empty()
        .then(|| grammar_tokens(source, &lexed.token_table))
}

/// Effect names, from the effect vocabulary.
pub fn effect_names() -> Vec<&'static str> {
    Effect::ALL.iter().map(Effect::as_str).collect()
}

mod productions;
