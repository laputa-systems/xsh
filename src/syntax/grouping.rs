//! Where an expression must be parenthesized.
//!
//! `needs_parens` derives from the grammar's operator table the one answer
//! shared by the printer and the checker: `xsht fmt` emits exactly these
//! parentheses, and `check.redundant-parens` rejects every other source
//! parenthesis. A parenthesis is required only when removing it changes the
//! parse or breaks a grouping rule.

use crate::diagnostic::{Diagnostic, DiagnosticCode, FixHint, Label};
use crate::source::Span;
use crate::syntax::arena::{
    ArenaExprKind, ArenaFmtPart, ArenaProgram, ArenaRecordFieldKind, ArenaSpawnTarget, ArenaStmtKind,
    AstArena, ExprId,
};
use crate::syntax::grammar::{
    self, OperatorFamily, PATTERN_TEST, PREFIX, PREFIX_OPERAND, binary_precedence,
    binary_right_operand_precedence,
};
use crate::syntax::lexer::{lex_spellings, tokens_stay_separate};
use crate::syntax::node::BinaryOp;
use crate::syntax::token::{Keyword, TokenTag};
use rustc_hash::FxHashMap;

/// Binding power of a postfix form, and of a primary: tighter than every
/// operator in the grammar's precedence table.
const POSTFIX: u8 = PREFIX_OPERAND;
const PRIMARY: u8 = PREFIX_OPERAND + 1;

pub const fn is_ordering(op: BinaryOp) -> bool {
    matches!(
        grammar::binary_operator(op).family,
        OperatorFamily::Ordering
    )
}

pub const fn is_comparison(op: BinaryOp) -> bool {
    matches!(
        grammar::binary_operator(op).family,
        OperatorFamily::Equality | OperatorFamily::Ordering | OperatorFamily::Membership
    )
}

/// Whether an operand of this kind is an ordering (`Some(true)`) or an
/// equality, membership, or pattern test (`Some(false)`). The two families
/// never mix without explicit grouping.
pub fn comparison_family(kind: &ArenaExprKind) -> Option<bool> {
    match kind {
        ArenaExprKind::ComparisonChain(_) => Some(true),
        ArenaExprKind::PatternTest { .. } => Some(false),
        ArenaExprKind::Binary { op, .. } if is_comparison(*op) => Some(is_ordering(*op)),
        _ => None,
    }
}

const fn is_logical(op: BinaryOp) -> bool {
    matches!(
        grammar::binary_operator(op).family,
        OperatorFamily::Logical | OperatorFamily::Fallback
    )
}

/// Whether `child`, written as an ungrouped operand of `parent`, mixes `and`,
/// `or`, and `??`, which requires explicit grouping.
pub fn mixes_logical(parent: BinaryOp, child: &ArenaExprKind) -> bool {
    matches!(child, ArenaExprKind::Binary { op, .. } if is_logical(parent) && is_logical(*op) && *op != parent)
}

/// The token that follows an expression where it is written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FollowToken {
    /// A newline, `;`, `}`, comment, or the end of input.
    End,
    RParen,
    /// `]`, `,`, `=>`, or another delimiter that does not end a command.
    Close,
    /// A colon, which an unfinished pipeline stage would otherwise take as
    /// the start of its callback expression.
    Colon,
    /// `{` opening a block.
    Brace,
    /// `and`, `or`, `in`, `not`, or `is`.
    WordOperator,
    /// Any other keyword or name, such as `for` or `when`.
    Word,
    /// A symbolic binary operator.
    Operator,
    PipeGt,
    Pipe,
    Question,
    /// An adjacent `?[`.
    QuestionBracket,
    /// An adjacent `?.` chain, which a command's typed final argument takes
    /// as its own.
    QuestionDot,
    /// An adjacent `?.` chain that ends in a call or index, which a command's
    /// final name or quoted word also takes (`Parser::at_call_or_index_chain`).
    QuestionDotCall,
    Dot,
    /// `.require(`.
    Require,
    DotDot,
    Bracket,
    Paren,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Follow {
    pub token: FollowToken,
    pub adjacent: bool,
}

impl Follow {
    pub const END: Self = Self::spaced(FollowToken::End);
    pub const CLOSE: Self = Self::spaced(FollowToken::Close);
    pub const BRACE: Self = Self::spaced(FollowToken::Brace);
    pub const WORD: Self = Self::spaced(FollowToken::Word);

    /// An adjacent token that extends the expression before it.
    pub const fn is_suffix(self) -> bool {
        self.adjacent
            && matches!(
                self.token,
                FollowToken::Dot
                    | FollowToken::Require
                    | FollowToken::DotDot
                    | FollowToken::Bracket
                    | FollowToken::Paren
                    | FollowToken::Question
                    | FollowToken::QuestionBracket
                    | FollowToken::QuestionDot
                    | FollowToken::QuestionDotCall
            )
    }

    pub const fn spaced(token: FollowToken) -> Self {
        Self {
            token,
            adjacent: false,
        }
    }

    pub const fn adjacent(token: FollowToken) -> Self {
        Self {
            token,
            adjacent: true,
        }
    }

    /// Classifies the first token of `text`, which begins `adjacent` to the
    /// expression or after whitespace.
    pub fn of_source(text: &str, adjacent: bool) -> Self {
        let tokens = lex_spellings(text);
        let Some(&(tag, spelling)) = tokens.first() else {
            return Self::END;
        };
        let next = tokens.get(1).map(|(tag, _)| *tag);
        let token = match tag {
            TokenTag::Newline | TokenTag::Semicolon | TokenTag::RBrace | TokenTag::Comment => {
                FollowToken::End
            }
            TokenTag::RParen => FollowToken::RParen,
            TokenTag::Colon => FollowToken::Colon,
            TokenTag::LBrace => FollowToken::Brace,
            TokenTag::Keyword
                if matches!(
                    Keyword::from_ident(spelling),
                    Some(Keyword::And | Keyword::Or | Keyword::In)
                ) =>
            {
                FollowToken::WordOperator
            }
            TokenTag::Ident if spelling == "is" => FollowToken::WordOperator,
            TokenTag::Keyword if spelling == "not" => FollowToken::WordOperator,
            TokenTag::Ident | TokenTag::ProcIdent | TokenTag::Keyword => FollowToken::Word,
            TokenTag::PipeGt => FollowToken::PipeGt,
            TokenTag::Pipe => FollowToken::Pipe,
            TokenTag::Question if next == Some(TokenTag::LBracket) => FollowToken::QuestionBracket,
            TokenTag::Question => FollowToken::Question,
            TokenTag::Dot if next == Some(TokenTag::Dot) => FollowToken::DotDot,
            TokenTag::Dot
                if tokens.get(1).is_some_and(|(_, name)| *name == "require")
                    && tokens
                        .get(2)
                        .is_some_and(|(tag, _)| *tag == TokenTag::LParen) =>
            {
                FollowToken::Require
            }
            TokenTag::Dot => FollowToken::Dot,
            TokenTag::LBracket => FollowToken::Bracket,
            TokenTag::LParen => FollowToken::Paren,
            _ if grammar::binary_operator_at(tag, None, None).is_some() => FollowToken::Operator,
            _ => FollowToken::Close,
        };
        Self { token, adjacent }
    }
}

/// Where a statement-start or arm-body expression begins: there the parser
/// dispatches on the first tokens before it parses an expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lead {
    /// `after_expression`: the previous statement ends in an expression that a
    /// line starting with `.name` would continue.
    Statement {
        after_expression: bool,
    },
    /// The unbraced statement of a `match` statement arm, where a leading
    /// `{` always opens the arm's block.
    ArmStatement,
    ArmBody,
    /// A `let`, assignment, `return`, `yield`, or `defer` value, where a
    /// leading `run` is a run form rather than an expression.
    Initializer,
}

/// The operator or construct that holds an expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Slot {
    /// Delimited, or at statement level: no operator binds the expression.
    Open,
    Left(BinaryOp),
    Right(BinaryOp),
    /// The operand of `is`.
    PatternTestValue,
    /// The operand of `!` or unary `-`.
    Prefix,
    /// The receiver of `.name`, `?.name`, `[...]`, a call, `.require(...)`, or a
    /// builder block. `dotted` is set on the base of `.name`.
    Postfix {
        dotted: bool,
    },
    /// The operand of `?`.
    Try,
    /// A `spawn` or `wait` target or the receiver chain inside one: a prefix
    /// form or a primary with `.name`, `[...]`, and call suffixes only.
    /// `spawn` is set on the target of `spawn` itself, where `run` starts the
    /// `spawn run` form; `receiver` on the receiver of a suffix inside the
    /// target, which must itself be a primary or suffix form.
    CommandTarget {
        spawn: bool,
        receiver: bool,
    },
    /// The input of `|>`; `structured` when every stage is a stream stage.
    PipelineInput {
        structured: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Context {
    pub slot: Slot,
    pub follow: Follow,
    pub lead: Option<Lead>,
    /// For the base of a `.name` chain, what follows the whole chain.
    pub chain_follow: Follow,
    /// The precedence at which the parser reads the expression's first token;
    /// `is` applies only where it is at most `PATTERN_TEST`.
    pub level: u8,
}

impl Context {
    pub const fn open(follow: Follow) -> Self {
        Self {
            slot: Slot::Open,
            follow,
            lead: None,
            chain_follow: follow,
            level: 0,
        }
    }

    pub const fn statement(follow: Follow, after_expression: bool) -> Self {
        Self {
            lead: Some(Lead::Statement { after_expression }),
            ..Self::open(follow)
        }
    }

    pub const fn arm_statement() -> Self {
        Self {
            lead: Some(Lead::ArmStatement),
            ..Self::open(Follow::END)
        }
    }

    pub const fn initializer(follow: Follow) -> Self {
        Self {
            lead: Some(Lead::Initializer),
            ..Self::open(follow)
        }
    }

    pub const fn arm_body(follow: Follow) -> Self {
        Self {
            lead: Some(Lead::ArmBody),
            ..Self::open(follow)
        }
    }

    /// The context of a parenthesized expression's contents.
    pub const fn group(self) -> Self {
        Self::open(Follow::spaced(FollowToken::RParen))
    }
}

fn binding_power(kind: &ArenaExprKind) -> u8 {
    match kind {
        ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::ValuePipelineCall { .. }
        | ArenaExprKind::PatternCondition { .. } => 0,
        ArenaExprKind::Binary { op, .. } => binary_precedence(*op),
        ArenaExprKind::PatternTest { .. } => PATTERN_TEST,
        ArenaExprKind::ComparisonChain(_) => binary_precedence(BinaryOp::Lt),
        ArenaExprKind::Unary { .. }
        | ArenaExprKind::Spawn(crate::syntax::arena::ArenaSpawnForm {
            target: ArenaSpawnTarget::Command(_),
            ..
        })
        | ArenaExprKind::Wait(_) => PREFIX,
        ArenaExprKind::Call { .. }
        | ArenaExprKind::Field { .. }
        | ArenaExprKind::NullSafeField { .. }
        | ArenaExprKind::Index { .. }
        | ArenaExprKind::Slice { .. }
        | ArenaExprKind::Try(_)
        | ArenaExprKind::Require { .. }
        | ArenaExprKind::BuilderCall { .. } => POSTFIX,
        _ => PRIMARY,
    }
}

fn is_right_associative(kind: &ArenaExprKind) -> bool {
    matches!(
        kind,
        ArenaExprKind::Binary {
            op: BinaryOp::ResultFallback,
            ..
        }
    )
}

/// How the last stage of a pipeline ends where it is written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LastStage {
    /// A stage that still takes a block or an inline expression.
    Awaiting,
    /// An expression stage, or an inline stage expression after the name.
    Expression,
    /// A stage complete in itself; `bare_name` when written as its name alone.
    Complete { bare_name: bool },
}

fn last_stage(arena: &AstArena, kind: &ArenaExprKind) -> LastStage {
    use crate::syntax::arena::ArenaPipeStageKind;
    let stage = match kind {
        ArenaExprKind::StructuredPipeline { stages, .. } => {
            arena.stream_stages(*stages).last().cloned()
        }
        ArenaExprKind::Pipeline { stages, .. } => {
            match arena.pipe_stages(*stages).last().map(|stage| &stage.kind) {
                Some(ArenaPipeStageKind::Stream(stage)) => Some(stage.clone()),
                _ => None,
            }
        }
        _ => None,
    };
    let Some(stage) = stage else {
        return LastStage::Expression;
    };
    if inline_stage_expr(arena, &stage).is_some() {
        LastStage::Expression
    } else if stage.block.is_none()
        && (grammar::stream_stage(stage.kind).block || grammar::stream_stage(stage.kind).inline)
    {
        LastStage::Awaiting
    } else {
        LastStage::Complete {
            bare_name: stage.args.is_empty()
                && !(stage.block.is_none() && stage.kind.canonical_parens_when_empty()),
        }
    }
}

/// The one expression of a stage block that is written after the stage name
/// without braces, which then takes in what follows the pipeline.
pub fn inline_stage_expr(
    arena: &AstArena,
    stage: &crate::syntax::arena::ArenaStreamStage,
) -> Option<ExprId> {
    use crate::syntax::node::StreamStageKind as Kind;
    if !matches!(
        stage.kind,
        Kind::Where
            | Kind::Map
            | Kind::ParMap
            | Kind::Each
            | Kind::SortBy
            | Kind::UniqueBy
            | Kind::Tee
            | Kind::GroupBy
            | Kind::FlatMap
            | Kind::Any
            | Kind::All
    ) {
        return None;
    }
    let block = arena.block(stage.block?);
    let mut statements = arena.stmt_ids(block.statements);
    let (Some(statement), None) = (statements.next(), statements.next()) else {
        return None;
    };
    match arena.stmt(statement).kind {
        ArenaStmtKind::Expr(expr) if block.params.is_empty() && reads_inline(arena, expr) => {
            Some(expr)
        }
        _ => None,
    }
}

/// Whether an expression reads the same after a stage name as inside braces.
/// One that starts with `{` would be read as the stage's block, and a pipeline
/// would end the inline argument at its first `|>`.
fn reads_inline(arena: &AstArena, expr: ExprId) -> bool {
    let mut current = expr;
    loop {
        current = match arena.expr(current).kind {
            ArenaExprKind::Record(_)
            | ArenaExprKind::MapComp { .. }
            | ArenaExprKind::ValueBlock(_)
            | ArenaExprKind::Pipeline { .. }
            | ArenaExprKind::StructuredPipeline { .. }
            | ArenaExprKind::ValuePipelineCall { .. } => return false,
            ArenaExprKind::Binary { left, .. } => left,
            ArenaExprKind::Field { base, .. }
            | ArenaExprKind::NullSafeField { base, .. }
            | ArenaExprKind::Index { base, .. }
            | ArenaExprKind::Slice { base, .. } => base,
            ArenaExprKind::Call { callee, .. } => callee,
            ArenaExprKind::Try(inner) => inner,
            _ => return true,
        };
    }
}

fn is_command_target_form(kind: &ArenaExprKind) -> bool {
    matches!(
        kind,
        ArenaExprKind::Spawn(crate::syntax::arena::ArenaSpawnForm {
            target: ArenaSpawnTarget::Command(_),
            ..
        }) | ArenaExprKind::Wait(_)
    )
}

fn command_run(kind: &ArenaExprKind) -> Option<crate::syntax::arena::RunFormId> {
    match kind {
        ArenaExprKind::Run(run)
        | ArenaExprKind::Spawn(crate::syntax::arena::ArenaSpawnForm {
            target: ArenaSpawnTarget::Run(run),
            ..
        }) => Some(*run),
        _ => None,
    }
}

fn run_ends_with_typed_arg(arena: &AstArena, run: crate::syntax::arena::RunFormId) -> bool {
    use crate::syntax::arena::{ArenaCommandArgKind, ArenaRedirectionTarget};
    let Some(segment) = arena.run_segments(arena.run_form(run).segments).last() else {
        return false;
    };
    let last = match arena.redirections(segment.redirections).last() {
        Some(redirection) => match &redirection.target {
            ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => arg,
        },
        None => arena
            .command_args(segment.args)
            .last()
            .unwrap_or(&segment.target),
    };
    matches!(last.kind, ArenaCommandArgKind::Typed(_))
}

fn is_command_form(kind: &ArenaExprKind) -> bool {
    command_run(kind).is_some()
}

/// Command forms take words up to a terminator, `?`, `{`, `|`, or `|>`.
/// Inside parentheses a `)` also ends one, but a command is always grouped
/// before a `)` so that removing an enclosing pair cannot extend it.
fn command_ends_before(follow: Follow) -> bool {
    matches!(
        follow.token,
        FollowToken::End
            | FollowToken::Question
            | FollowToken::QuestionDot
            | FollowToken::QuestionDotCall
            | FollowToken::Brace
            | FollowToken::Pipe
            | FollowToken::PipeGt
    )
}

/// Whether the final word of `run` takes in an adjacent `?.` chain: a typed
/// argument always does, and a quoted word or a `.`-separated name does when
/// the chain ends in a call or index.
fn run_takes_null_safe_chain(
    arena: &AstArena,
    source: &str,
    run: crate::syntax::arena::RunFormId,
    follow: Follow,
) -> bool {
    use crate::syntax::arena::{ArenaCommandArgKind, ArenaRedirectionTarget, ArenaWordPart};
    if !matches!(
        follow.token,
        FollowToken::QuestionDot | FollowToken::QuestionDotCall
    ) {
        return false;
    }
    let Some(segment) = arena.run_segments(arena.run_form(run).segments).last() else {
        return false;
    };
    let last = match arena.redirections(segment.redirections).last() {
        Some(redirection) => match &redirection.target {
            ArenaRedirectionTarget::Path(arg) | ArenaRedirectionTarget::Fd(arg) => arg,
        },
        None => arena
            .command_args(segment.args)
            .last()
            .unwrap_or(&segment.target),
    };
    match last.kind {
        ArenaCommandArgKind::Typed(_) => true,
        ArenaCommandArgKind::Word(parts) if follow.token == FollowToken::QuestionDotCall => {
            let mut parts = arena.word_parts(parts);
            match (parts.next(), parts.next()) {
                (Some(ArenaWordPart::Quoted(_)), None) => true,
                (Some(ArenaWordPart::Bare(text)), None) => {
                    arena.text_value(&text, source).is_none_or(|text| {
                        let tokens = lex_spellings(text);
                        tokens
                            .first()
                            .is_some_and(|(tag, _)| *tag == TokenTag::Ident)
                            && tokens[1..].chunks(2).all(|pair| {
                                matches!(
                                    pair,
                                    [(TokenTag::Dot, _), (TokenTag::Ident | TokenTag::Keyword, _)]
                                )
                            })
                    })
                }
                _ => false,
            }
        }
        _ => false,
    }
}

/// The follow of a `?.` chain whose receiver is written where `context`
/// holds the chain's first link.
fn null_safe_follow(context: Context) -> Follow {
    let ends_chain = |follow: Follow| {
        follow.adjacent
            && matches!(
                follow.token,
                FollowToken::Paren
                    | FollowToken::Bracket
                    | FollowToken::QuestionBracket
                    | FollowToken::QuestionDotCall
                    | FollowToken::Require
            )
    };
    let call = ends_chain(context.follow)
        || (context.follow.adjacent
            && context.follow.token == FollowToken::Dot
            && ends_chain(context.chain_follow));
    Follow::adjacent(if call {
        FollowToken::QuestionDotCall
    } else {
        FollowToken::QuestionDot
    })
}

/// A call or field that the parser extends with a following `{` builder block.
fn accepts_builder_block(arena: &AstArena, kind: &ArenaExprKind) -> bool {
    let field = match kind {
        ArenaExprKind::Call { callee, .. } => arena.expr(*callee).kind,
        other => other.clone(),
    };
    matches!(field, ArenaExprKind::Field { base, name }
        if matches!(arena.expr(base).kind, ArenaExprKind::Ident(module)
            if grammar::builder_api_accepts_block(&module.as_str(), &name.as_str())))
}

/// Whether a record or map comprehension written first in a statement or arm
/// body reads as a record rather than a block (`Parser::brace_starts_record_value`).
fn brace_reads_as_record(arena: &AstArena, kind: &ArenaExprKind) -> bool {
    match kind {
        ArenaExprKind::Record(fields) => match arena
            .record_fields(*fields)
            .first()
            .map(|field| &field.kind)
        {
            Some(ArenaRecordFieldKind::Shorthand { name, .. }) => {
                let name = name.as_str();
                let name = name.as_str();
                !name.contains('-')
                    && !matches!(
                        Keyword::from_ident(name),
                        Some(
                            Keyword::True
                                | Keyword::False
                                | Keyword::Null
                                | Keyword::Return
                                | Keyword::Break
                                | Keyword::Continue
                        )
                    )
            }
            _ => true,
        },
        // The key is a name path or `[key]`.
        ArenaExprKind::MapComp { .. } => true,
        _ => true,
    }
}

/// Whether `expr`, written in `context` with only the parentheses its own
/// children need, must be parenthesized for the parser to rebuild it.
pub fn needs_parens(arena: &AstArena, source: &str, expr: ExprId, context: Context) -> bool {
    let kind = arena.expr(expr).kind;
    // A pipeline whose last stage is complete continues like a primary at
    // the level where `|>` applies; elsewhere it binds below every operator.
    let closed_pipeline = matches!(
        kind,
        ArenaExprKind::Pipeline { .. } | ArenaExprKind::StructuredPipeline { .. }
    ) && matches!(last_stage(arena, &kind), LastStage::Complete { .. });
    let power = if closed_pipeline {
        PRIMARY
    } else {
        binding_power(&kind)
    };
    let slot_needs = match context.slot {
        Slot::Open => false,
        Slot::Left(op) if matches!(kind, ArenaExprKind::PatternTest { .. }) => {
            comparison_mixes(op, &kind) || mixes_logical(op, &kind)
        }
        Slot::Left(op) => {
            power < binary_precedence(op)
                || (power == binary_precedence(op) && is_right_associative(&kind))
                || (is_ordering(op) && comparison_family(&kind) == Some(true))
                || comparison_mixes(op, &kind)
                || mixes_logical(op, &kind)
        }
        Slot::Right(op) => {
            power < binary_right_operand_precedence(op)
                || comparison_mixes(op, &kind)
                || mixes_logical(op, &kind)
        }
        Slot::PatternTestValue => {
            power < PATTERN_TEST
                || comparison_family(&kind) == Some(true)
                || matches!(kind, ArenaExprKind::PatternTest { .. })
        }
        Slot::Prefix => power < PREFIX,
        // A `spawn` or `wait` target stops before `?`, `?.`, and `?[`.
        Slot::Postfix { .. } | Slot::Try => {
            (power < POSTFIX
                && !(matches!(context.follow.token, FollowToken::Question | FollowToken::QuestionBracket | FollowToken::QuestionDot | FollowToken::QuestionDotCall) && is_command_target_form(&kind)))
                // A typed final command argument takes an adjacent `?` as its own.
                || (context.slot == Slot::Try && command_run(&kind).is_some_and(|run| run_ends_with_typed_arg(arena, run)))
        }
        Slot::CommandTarget { spawn, receiver } => {
            power < if receiver { POSTFIX } else { PREFIX }
                || matches!(
                    kind,
                    ArenaExprKind::Try(_)
                        | ArenaExprKind::NullSafeField { .. }
                        | ArenaExprKind::Require { .. }
                        | ArenaExprKind::BuilderCall { .. }
                        | ArenaExprKind::Index { guarded: true, .. }
                        | ArenaExprKind::Slice { guarded: true, .. }
                )
                || (spawn && matches!(kind, ArenaExprKind::Run(_)))
        }
        // A following stage would join an ungrouped pipeline's stages.
        Slot::PipelineInput { structured } => match kind {
            ArenaExprKind::Pipeline { .. } | ArenaExprKind::PatternCondition { .. } => true,
            ArenaExprKind::StructuredPipeline { .. } => structured,
            _ => false,
        },
    };
    let follow = context.follow;
    let follow_needs = match kind {
        // `x?.`, `x?[`, `x??`, and `x?..` read as one postfix or operator.
        ArenaExprKind::Try(_) => {
            follow.adjacent
                && matches!(
                    follow.token,
                    FollowToken::Dot
                        | FollowToken::Bracket
                        | FollowToken::Question
                        | FollowToken::QuestionBracket
                        | FollowToken::QuestionDot
                        | FollowToken::QuestionDotCall
                        | FollowToken::DotDot
                )
        }
        _ if is_command_form(&kind) => {
            !command_ends_before(follow)
                || command_run(&kind)
                    .is_some_and(|run| run_takes_null_safe_chain(arena, source, run, follow))
        }
        // `. in x` reads as the field `.in`.
        // A type pattern takes in an adjacent `.`, `[`, `(`, or `?`.
        ArenaExprKind::PatternTest { .. } => follow.is_suffix(),
        ArenaExprKind::Item => {
            matches!(follow.token, FollowToken::Word | FollowToken::WordOperator)
                || (follow.adjacent && follow.token == FollowToken::DotDot)
        }
        // Before a block `{`, a last stage that takes a block, or a builder
        // call, would take the block as its own.
        // A last stage that still takes a block or an inline expression runs
        // to a terminator, `|>`, or a closing delimiter; any stage takes an
        // adjacent suffix.
        ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::ValuePipelineCall { .. } => match last_stage(arena, &kind) {
            LastStage::Awaiting => !matches!(
                follow.token,
                FollowToken::End | FollowToken::PipeGt | FollowToken::RParen | FollowToken::Close
            ),
            LastStage::Expression => {
                follow.is_suffix()
                    || matches!(
                        follow.token,
                        FollowToken::Operator | FollowToken::WordOperator
                    )
            }
            // A stage written as a bare name would extend it with `.name` or call it.
            LastStage::Complete { bare_name } => {
                bare_name
                    && follow.adjacent
                    && matches!(
                        follow.token,
                        FollowToken::Dot | FollowToken::Require | FollowToken::Paren
                    )
            }
        },
        _ => {
            context.slot == Slot::Open
                && follow.token == FollowToken::Brace
                && accepts_builder_block(arena, &kind)
        }
    };
    let level_needs = (matches!(kind, ArenaExprKind::PatternTest { .. })
        && context.level > PATTERN_TEST)
        || (closed_pipeline && context.level > 0);
    slot_needs
        || follow_needs
        || level_needs
        || held_ambiguously(context.slot, &kind)
        || context
            .lead
            .is_some_and(|lead| lead_needs_parens(arena, &kind, lead, context))
}

/// Whether `kind`, held by an operator or suffix in `slot`, must be grouped
/// because a reader cannot see its extent (`check.ambiguous-grouping`): an
/// `if` or `match` anywhere an operator, suffix, or `|>` holds it, and a
/// pipeline that an operator, prefix, or `is` applies to. A suffix after a
/// complete last stage chains left to right like a method chain, so
/// `xs |> drop(1).join("")` applies `.join` to the pipeline's result.
pub fn held_ambiguously(slot: Slot, kind: &ArenaExprKind) -> bool {
    match kind {
        ArenaExprKind::If { .. } | ArenaExprKind::Match { .. } => slot != Slot::Open,
        ArenaExprKind::Pipeline { .. }
        | ArenaExprKind::StructuredPipeline { .. }
        | ArenaExprKind::ValuePipelineCall { .. } => {
            matches!(
                slot,
                Slot::Left(_) | Slot::Right(_) | Slot::Prefix | Slot::PatternTestValue
            )
        }
        _ => false,
    }
}

fn comparison_mixes(op: BinaryOp, kind: &ArenaExprKind) -> bool {
    is_comparison(op) && comparison_family(kind).is_some_and(|family| family != is_ordering(op))
}

/// Statement and arm-body dispatch on the expression's own first token.
fn lead_needs_parens(arena: &AstArena, kind: &ArenaExprKind, lead: Lead, context: Context) -> bool {
    let statement = matches!(lead, Lead::Statement { .. } | Lead::ArmStatement);
    match kind {
        ArenaExprKind::ValueBlock(_) if lead == Lead::ArmStatement => true,
        ArenaExprKind::Record(fields) if lead == Lead::ArmStatement => !matches!(
            arena
                .record_fields(*fields)
                .first()
                .map(|field| &field.kind),
            Some(
                ArenaRecordFieldKind::Named { .. }
                    | ArenaRecordFieldKind::Path { .. }
                    | ArenaRecordFieldKind::Computed { .. }
                    | ArenaRecordFieldKind::Spread { .. }
            )
        ),
        ArenaExprKind::Record(_) | ArenaExprKind::MapComp { .. } if lead != Lead::Initializer => {
            !brace_reads_as_record(arena, kind)
        }
        // A statement-position scope has no value body.
        ArenaExprKind::If { .. }
        | ArenaExprKind::Match { .. }
        | ArenaExprKind::Loop { .. }
        | ArenaExprKind::ContextScope {
            value_body: true, ..
        }
        | ArenaExprKind::TempDirScope {
            value_body: true, ..
        } => statement,
        // A run form that heads a pipeline reads as a value at any lead, and
        // so does one written under `try`, which begins with that word.
        ArenaExprKind::Run(run) => {
            !arena.run_form(*run).captured
                && lead != Lead::ArmBody
                && !matches!(context.slot, Slot::PipelineInput { .. })
        }
        ArenaExprKind::Field { base, .. }
            if matches!(arena.expr(*base).kind, ArenaExprKind::Item) =>
        {
            lead == Lead::Statement {
                after_expression: true,
            }
        }
        // A name alone is a command; a `.name` chain followed by a word is a
        // dotted command (`Parser::lookahead_is_dotted_command`).
        ArenaExprKind::Ident(_) if statement => match context.slot {
            Slot::Postfix { dotted: true } => matches!(
                context.chain_follow.token,
                FollowToken::Word | FollowToken::Brace
            ),
            _ => context.follow.token == FollowToken::End,
        },
        _ => false,
    }
}

/// The context of `child` where `parent` is written in `context` (already
/// `Context::group` when `parent` is parenthesized).
pub fn child_context(arena: &AstArena, parent: ExprId, context: Context, child: ExprId) -> Context {
    let inherit = |slot, level| Context {
        slot,
        lead: None,
        chain_follow: context.follow,
        level,
        ..context
    };
    let left = |slot, follow| Context {
        slot,
        follow,
        chain_follow: follow,
        ..context
    };
    let open = Context::open;
    match arena.expr(parent).kind {
        ArenaExprKind::Binary {
            op, left: operand, ..
        } if operand == child => left(
            Slot::Left(op),
            Follow::spaced(
                if matches!(
                    op,
                    BinaryOp::And | BinaryOp::Or | BinaryOp::In | BinaryOp::NotIn
                ) {
                    FollowToken::WordOperator
                } else {
                    FollowToken::Operator
                },
            ),
        ),
        ArenaExprKind::Binary { op, .. } => {
            inherit(Slot::Right(op), binary_right_operand_precedence(op))
        }
        ArenaExprKind::PatternTest { .. } => left(
            Slot::PatternTestValue,
            Follow::spaced(FollowToken::WordOperator),
        ),
        ArenaExprKind::Unary { .. } => inherit(Slot::Prefix, PREFIX_OPERAND),
        ArenaExprKind::Spawn(_) => inherit(
            Slot::CommandTarget {
                spawn: true,
                receiver: false,
            },
            PREFIX_OPERAND,
        ),
        ArenaExprKind::Wait(_) => inherit(
            Slot::CommandTarget {
                spawn: false,
                receiver: false,
            },
            PREFIX_OPERAND,
        ),
        ArenaExprKind::Field { base, .. } if base == child => {
            let chain_follow = match context.slot {
                Slot::Postfix { dotted: true } => context.chain_follow,
                _ => context.follow,
            };
            let slot = if matches!(context.slot, Slot::CommandTarget { .. }) {
                Slot::CommandTarget {
                    spawn: false,
                    receiver: true,
                }
            } else {
                Slot::Postfix { dotted: true }
            };
            Context {
                chain_follow,
                ..left(slot, Follow::adjacent(FollowToken::Dot))
            }
        }
        ArenaExprKind::Require { .. } => left(
            Slot::Postfix { dotted: false },
            Follow::adjacent(FollowToken::Require),
        ),
        ArenaExprKind::NullSafeField { .. } => {
            left(Slot::Postfix { dotted: false }, null_safe_follow(context))
        }
        ArenaExprKind::Index { base, guarded, .. } | ArenaExprKind::Slice { base, guarded, .. }
            if base == child =>
        {
            let slot = if matches!(context.slot, Slot::CommandTarget { .. }) {
                Slot::CommandTarget {
                    spawn: false,
                    receiver: true,
                }
            } else {
                Slot::Postfix { dotted: false }
            };
            left(
                slot,
                Follow::adjacent(if guarded {
                    FollowToken::QuestionBracket
                } else {
                    FollowToken::Bracket
                }),
            )
        }
        ArenaExprKind::Slice {
            start: Some(start), ..
        } if start == child => open(Follow::adjacent(FollowToken::DotDot)),
        ArenaExprKind::Call { callee, .. } if callee == child => {
            let slot = if matches!(context.slot, Slot::CommandTarget { .. }) {
                Slot::CommandTarget {
                    spawn: false,
                    receiver: true,
                }
            } else {
                Slot::Postfix { dotted: false }
            };
            left(slot, Follow::adjacent(FollowToken::Paren))
        }
        // `x?.require(` continues as a `?.` chain.
        ArenaExprKind::Try(_)
            if context.follow.adjacent && context.follow.token == FollowToken::Require =>
        {
            left(Slot::Try, Follow::adjacent(FollowToken::QuestionDotCall))
        }
        ArenaExprKind::Try(_) => left(Slot::Try, Follow::adjacent(FollowToken::Question)),
        ArenaExprKind::BuilderCall { .. } => left(Slot::Postfix { dotted: false }, Follow::BRACE),
        ArenaExprKind::Pipeline { input, .. } | ArenaExprKind::ValuePipelineCall { input, .. }
            if input == child =>
        {
            left(
                Slot::PipelineInput { structured: false },
                Follow::spaced(FollowToken::PipeGt),
            )
        }
        ArenaExprKind::StructuredPipeline { input, .. } if input == child => left(
            Slot::PipelineInput { structured: true },
            Follow::spaced(FollowToken::PipeGt),
        ),
        // Each ordering pair is written as an operand pair of the chain.
        ArenaExprKind::ComparisonChain(_) => context,
        ArenaExprKind::ValuePipelineCall { .. } => inherit(Slot::Open, 0),
        ArenaExprKind::If { .. } | ArenaExprKind::Match { .. }
            if is_condition(arena, parent, child) =>
        {
            open(Follow::BRACE)
        }
        ArenaExprKind::Match { arms, .. }
            if arena
                .match_expr_arms(arms)
                .iter()
                .any(|arm| arm.value == child) =>
        {
            Context::arm_body(Follow::CLOSE)
        }
        ArenaExprKind::ErrorContext { .. } | ArenaExprKind::PatternCondition { .. } => {
            open(Follow::BRACE)
        }
        ArenaExprKind::ListComp { expr, .. } if expr == child => open(Follow::WORD),
        ArenaExprKind::MapComp { value, .. } if value == child => open(Follow::WORD),
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            let has_spec = arena.fmt_parts(parts).any(|part| {
                matches!(part, ArenaFmtPart::Expr(expr, Some(_)) if expr == child)
            });
            open(if has_spec {
                Follow::adjacent(FollowToken::Colon)
            } else {
                Follow::END
            })
        }
        _ => open(Follow::CLOSE),
    }
}

fn is_condition(arena: &AstArena, parent: ExprId, child: ExprId) -> bool {
    match arena.expr(parent).kind {
        ArenaExprKind::If { branches, .. } => arena
            .if_expr_branches(branches)
            .iter()
            .any(|branch| branch.condition == child),
        ArenaExprKind::Match { value, .. } => value == child,
        _ => false,
    }
}

/// The children of `parent` that an operator or suffix holds, whose context
/// `child_context` derives from `parent`'s.
fn for_each_operand(arena: &AstArena, parent: ExprId, mut visit: impl FnMut(ExprId)) {
    match arena.expr(parent).kind {
        ArenaExprKind::Binary { left, right, .. } => {
            visit(left);
            visit(right);
        }
        ArenaExprKind::ComparisonChain(pairs) => arena.expr_ids(pairs).for_each(visit),
        ArenaExprKind::PatternTest { value, .. } | ArenaExprKind::Require { value, .. } => {
            visit(value)
        }
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => visit(expr),
        ArenaExprKind::Field { base, .. }
            if matches!(arena.expr(base).kind, ArenaExprKind::Item) => {}
        ArenaExprKind::Field { base, .. }
        | ArenaExprKind::NullSafeField { base, .. }
        | ArenaExprKind::Index { base, .. }
        | ArenaExprKind::Slice { base, .. } => visit(base),
        ArenaExprKind::Call { callee, .. } => visit(callee),
        ArenaExprKind::BuilderCall { call, .. } => visit(call),
        ArenaExprKind::Pipeline { input, .. } | ArenaExprKind::StructuredPipeline { input, .. } => {
            visit(input)
        }
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            visit(input);
            visit(call);
        }
        ArenaExprKind::Spawn(form) => {
            if let ArenaSpawnTarget::Command(target) = form.target {
                visit(target)
            }
        }
        ArenaExprKind::Wait(form) => visit(form.target),
        _ => {}
    }
}

/// Every expression child of `parent`.
fn for_each_child(arena: &AstArena, parent: ExprId, mut visit: impl FnMut(ExprId)) {
    use crate::syntax::arena::{ArenaCallArgKind, ArenaFmtPart};
    match arena.expr(parent).kind {
        ArenaExprKind::Binary { left, right, .. } => {
            visit(left);
            visit(right);
        }
        ArenaExprKind::ComparisonChain(pairs) => arena.expr_ids(pairs).for_each(visit),
        ArenaExprKind::PatternTest { value, .. }
        | ArenaExprKind::PatternCondition { value, .. } => visit(value),
        ArenaExprKind::Unary { expr, .. } | ArenaExprKind::Try(expr) => visit(expr),
        // `.name` on the item is one token pair, not a receiver and suffix.
        ArenaExprKind::Field { base, .. }
            if matches!(arena.expr(base).kind, ArenaExprKind::Item) => {}
        ArenaExprKind::Field { base, .. } | ArenaExprKind::NullSafeField { base, .. } => {
            visit(base)
        }
        ArenaExprKind::Require { value, .. } => visit(value),
        ArenaExprKind::Index { base, index, .. } => {
            visit(base);
            visit(index);
        }
        ArenaExprKind::Slice {
            base, start, end, ..
        } => {
            visit(base);
            start.into_iter().chain(end).for_each(visit);
        }
        ArenaExprKind::Call { callee, args } => {
            visit(callee);
            for arg in arena.call_args(args) {
                match arg.kind {
                    ArenaCallArgKind::Positional(value)
                    | ArenaCallArgKind::NamedSpread { value, .. }
                    | ArenaCallArgKind::Splice { value, .. } => visit(value),
                    ArenaCallArgKind::Named { value, .. } => visit(value),
                }
            }
        }
        ArenaExprKind::ValuePipelineCall { input, call, .. } => {
            visit(input);
            visit(call);
        }
        ArenaExprKind::Pipeline { input, .. } | ArenaExprKind::StructuredPipeline { input, .. } => {
            visit(input)
        }
        ArenaExprKind::Spawn(form) => {
            if let ArenaSpawnTarget::Command(target) = form.target {
                visit(target)
            }
        }
        ArenaExprKind::Wait(form) => visit(form.target),
        ArenaExprKind::BuilderCall { call, .. } => visit(call),
        ArenaExprKind::List(items) => arena
            .list_elements(items)
            .for_each(|item| visit(item.value)),
        ArenaExprKind::ListComp { expr, qualifiers } => {
            visit(expr);
            arena
                .comp_qualifiers(qualifiers)
                .iter()
                .for_each(|q| visit(q.expr()));
        }
        ArenaExprKind::MapComp {
            key,
            value,
            qualifiers,
        } => {
            visit(key);
            visit(value);
            arena
                .comp_qualifiers(qualifiers)
                .iter()
                .for_each(|q| visit(q.expr()));
        }
        ArenaExprKind::Record(fields) => {
            for field in arena.record_fields(fields) {
                match field.kind {
                    ArenaRecordFieldKind::Computed { key, value, .. } => {
                        visit(key);
                        visit(value);
                    }
                    ArenaRecordFieldKind::Path { value, .. }
                    | ArenaRecordFieldKind::Named { value, .. } => visit(value),
                    ArenaRecordFieldKind::Spread { expr, .. } => visit(expr),
                    ArenaRecordFieldKind::Shorthand { .. } => {}
                }
            }
        }
        ArenaExprKind::If {
            branches,
            else_value,
        } => {
            arena.if_expr_branches(branches).iter().for_each(|branch| {
                visit(branch.condition);
                visit(branch.value);
            });
            visit(else_value);
        }
        ArenaExprKind::Match { value, arms } => {
            visit(value);
            for arm in arena.match_expr_arms(arms) {
                arm.guard.into_iter().for_each(&mut visit);
                visit(arm.value);
            }
        }
        ArenaExprKind::FmtString(parts) | ArenaExprKind::PathFmtString(parts) => {
            for part in arena.fmt_parts(parts) {
                if let ArenaFmtPart::Expr(expr, _) = part {
                    visit(expr)
                }
            }
        }
        ArenaExprKind::ErrorContext { message, .. } => visit(message),
        ArenaExprKind::ContextScope { input, .. } => visit(input),
        ArenaExprKind::Retry { delays, .. } => arena.expr_ids(delays).for_each(visit),
        _ => {}
    }
}

/// Whether a statement of this kind ends in an expression that a following
/// line starting with `.name` continues.
pub fn statement_may_continue(kind: &ArenaStmtKind) -> bool {
    matches!(
        kind,
        ArenaStmtKind::Let { .. }
            | ArenaStmtKind::Var { .. }
            | ArenaStmtKind::Const { .. }
            | ArenaStmtKind::Assign { .. }
            | ArenaStmtKind::Expr(_)
            | ArenaStmtKind::Return(Some(_))
            | ArenaStmtKind::Yield(_)
            | ArenaStmtKind::YieldDelegate(_)
            | ArenaStmtKind::Exit(_)
            | ArenaStmtKind::Defer(..)
            | ArenaStmtKind::Assert { .. }
            | ArenaStmtKind::Break { value: Some(_) }
            | ArenaStmtKind::Sugar {
                form: crate::syntax::arena::SugarForm::When
                    | crate::syntax::arena::SugarForm::Unless
                    | crate::syntax::arena::SugarForm::Fail,
                ..
            }
            | ArenaStmtKind::Export(_)
    )
}

/// `check.redundant-parens`, `check.mixed-logical`, and
/// `check.ambiguous-grouping` for the root source.
pub fn grouping_diagnostics(program: &ArenaProgram, source: &str) -> Vec<Diagnostic> {
    let arena = &program.arena;
    let Some(source_id) = program
        .statement_ids()
        .next()
        .map(|id| arena.stmt(id).span.source_id)
    else {
        return Vec::new();
    };
    // Only a check that holds the root source text can judge its spelling.
    if program
        .statement_ids()
        .any(|id| source.get(arena.stmt(id).span.range()).is_none())
    {
        return Vec::new();
    }
    let mut grouped: FxHashMap<ExprId, Vec<Span>> = FxHashMap::default();
    for (expr, span) in &program.paren_groups {
        if span.source_id == source_id
            && source
                .get(span.range())
                .is_some_and(|text| text.starts_with('(') && text.ends_with(')'))
        {
            grouped.entry(*expr).or_default().push(*span);
        }
    }
    let mut parents: FxHashMap<ExprId, ExprId> = FxHashMap::default();
    let mut arm_bodies = Vec::new();
    let mut diagnostics = Vec::new();
    for (index, tag) in arena.expr_tags.iter().enumerate() {
        use crate::syntax::arena::ArenaExprTag as Tag;
        // An expansion's own expressions are not spelled in the source, so
        // they hold nothing and no source text follows them.
        if arena.expr_is_synthetic(ExprId::from_index(index)) {
            continue;
        }
        let logical = matches!(
            tag,
            Tag::BinaryResultFallback | Tag::BinaryOr | Tag::BinaryAnd
        );
        let holds_operand = !matches!(
            tag,
            Tag::Null
                | Tag::BoolFalse
                | Tag::BoolTrue
                | Tag::Int
                | Tag::Float
                | Tag::Duration
                | Tag::Str
                | Tag::PathStr
                | Tag::GlobStr
                | Tag::FmtString
                | Tag::PathFmtString
                | Tag::Bytes
                | Tag::Regex
                | Tag::Ident
                | Tag::Item
                | Tag::LastStatus
                | Tag::List
                | Tag::ListComp
                | Tag::MapComp
                | Tag::Record
                | Tag::If
                | Tag::Run
                | Tag::SpawnRun
                | Tag::Capture
                | Tag::Loop
                | Tag::Retry
                | Tag::ValueBlock
                | Tag::EnvString
                | Tag::EnvPathList
        );
        if !logical && !holds_operand {
            continue;
        }
        let parent = ExprId::from_index(index);
        let expr = arena.expr(parent);
        if expr.span.source_id != source_id {
            continue;
        }
        // Delimited children are judged against the source that follows
        // them, which only the source layout decides.
        if holds_operand {
            for_each_operand(arena, parent, |child| {
                if !grouped.is_empty() {
                    parents.insert(child, parent);
                }
                if !grouped.contains_key(&child)
                    && held_ambiguously(
                        spelled_slot(arena, parent, child, source),
                        &arena.expr(child).kind,
                    )
                {
                    diagnostics.push(ambiguous_grouping(arena, child, source));
                }
            });
            if let ArenaExprKind::Match { arms, .. } = expr.kind
                && !grouped.is_empty()
            {
                arm_bodies.extend(arena.match_expr_arms(arms).iter().map(|arm| arm.value));
            }
        }
        if let ArenaExprKind::Binary { op, left, right } = expr.kind {
            for operand in [left, right] {
                if !grouped.contains_key(&operand) && mixes_logical(op, &arena.expr(operand).kind) {
                    diagnostics.push(mixed_logical(arena, operand, source));
                }
            }
        }
    }
    let mut redundant: Vec<Span> = Vec::new();
    if !grouped.is_empty() {
        let mut groups = Groups {
            arena,
            source,
            grouped: &grouped,
            parents: &parents,
            statements: FxHashMap::default(),
            contexts: FxHashMap::default(),
        };
        groups
            .statements
            .extend(arm_bodies.into_iter().map(|body| (body, Lead::ArmBody)));
        for index in 0..arena.stmt_tags.len() {
            if let ArenaStmtKind::Let {
                initializer: value, ..
            }
            | ArenaStmtKind::Var {
                initializer: value, ..
            }
            | ArenaStmtKind::Const {
                initializer: value, ..
            }
            | ArenaStmtKind::Guard {
                initializer: value, ..
            }
            | ArenaStmtKind::Assign { value, .. }
            | ArenaStmtKind::Return(Some(value))
            | ArenaStmtKind::Yield(value)
            | ArenaStmtKind::Defer(value, _) = arena
                .stmt(crate::syntax::arena::StmtId::from_index(index))
                .kind
                && let crate::syntax::arena::ArenaExprOrRun::Expr(expr) = value
            {
                groups.statements.insert(expr, Lead::Initializer);
            }
        }
        groups.mark_statements(program.statement_ids());
        for block in &arena.blocks {
            groups.mark_statements(arena.stmt_ids(block.statements));
        }
        for index in 0..arena.stmt_tags.len() {
            if let ArenaStmtKind::Match { arms, .. } = arena
                .stmt(crate::syntax::arena::StmtId::from_index(index))
                .kind
            {
                for arm in arena.match_arms(arms) {
                    // An unbraced arm's block is its one statement.
                    let block = arena.block(arm.block);
                    if let Some(stmt) = arena.stmt_ids(block.statements).next()
                        && arena.span(block.span).start() == arena.stmt(stmt).span.start()
                        && let ArenaStmtKind::Expr(expr) = arena.stmt(stmt).kind
                    {
                        groups.statements.insert(expr, Lead::ArmStatement);
                    }
                }
            }
        }
        for (expr, spans) in &grouped {
            let mut spans = spans.clone();
            spans.sort_by_key(|span| std::cmp::Reverse(span.end() - span.start()));
            let mut context = groups.context(*expr);
            // A `|>` method stage is rebuilt as a call on this receiver, but
            // the source spells the receiver as a pipeline input.
            let after = &source[spans[0].end()..];
            if matches!(context.slot, Slot::Postfix { .. })
                && Follow::of_source(&after[..after.find('\n').unwrap_or(after.len())], false).token
                    == FollowToken::PipeGt
            {
                context = Context {
                    slot: Slot::PipelineInput { structured: false },
                    follow: Follow::spaced(FollowToken::PipeGt),
                    ..context
                };
            }
            let required = needs_parens(arena, source, *expr, context)
                || removal_breaks_children(arena, source, *expr, context, &grouped)
                || removal_merges_tokens(source, spans[0], arena.expr(*expr).span)
                || ((context.slot != Slot::Open || context.follow.is_suffix())
                    && !matches!(arena.expr(*expr).kind, ArenaExprKind::Pipeline { .. } | ArenaExprKind::StructuredPipeline { .. } | ArenaExprKind::ValuePipelineCall { .. })
                    && spells_pipeline(&source[spans[0].start() + 1..spans[0].end() - 1]))
                || bare_path_absorbs_suffix(source, spans[0])
                // A spaced `?` applies to a command only as the last token
                // before a delimiter; the printer writes `(command)?` instead.
                || (matches!(arena.expr(*expr).kind, ArenaExprKind::Try(_))
                    && source[spans[0].start() + 1..spans[0].end() - 1].trim_end().strip_suffix('?').is_some_and(|text| text.ends_with([' ', '\t'])));
            redundant.extend(if required { &spans[1..] } else { &spans[..] });
        }
    }
    // Pairs that are not nested inside one another can be removed together;
    // an inner pair is judged again once the outer one is gone.
    redundant.sort_by_key(|span| (span.start(), std::cmp::Reverse(span.end())));
    let mut outer_end = 0;
    for span in redundant {
        if span.start() >= outer_end {
            diagnostics.push(redundant_parens(span, source));
            outer_end = span.end();
        }
    }
    diagnostics.sort_by_key(|diagnostic| diagnostic.labels.first().map(|label| label.span.start()));
    diagnostics
}

/// Removes every redundant pair of parentheses inside `within` from `source`,
/// for tools that splice text whose grouping was chosen without context.
/// Returns the text and `within` moved to match it.
pub fn remove_redundant_parens(
    source: &str,
    within: &[std::ops::Range<usize>],
) -> (String, Vec<std::ops::Range<usize>>) {
    try_remove_redundant_parens(source, within)
        .unwrap_or_else(|| (source.to_string(), within.to_vec()))
}

/// `remove_redundant_parens`, or `None` when `source` does not parse and so
/// nothing in it could be judged.
pub fn try_remove_redundant_parens(
    source: &str,
    within: &[std::ops::Range<usize>],
) -> Option<(String, Vec<std::ops::Range<usize>>)> {
    let mut text = source.to_string();
    let mut ranges = within.to_vec();
    for round in 0..32 {
        let parsed = crate::syntax::parser::Parser::parse_source_arena_only(
            crate::source::SourceId::new(0),
            &text,
        );
        if !parsed.diagnostics.is_empty() {
            if round == 0 {
                return None;
            }
            break;
        }
        let mut fixes: Vec<(usize, usize, String)> = grouping_diagnostics(&parsed.arena, &text)
            .into_iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckRedundantParens))
            .flat_map(|diagnostic| diagnostic.fix_hints)
            .filter_map(|hint| Some((hint.span?.start(), hint.span?.end(), hint.replacement?)))
            .filter(|(start, end, _)| {
                ranges
                    .iter()
                    .any(|range| range.start <= *start && *end <= range.end)
            })
            .collect();
        if fixes.is_empty() {
            break;
        }
        fixes.sort_by_key(|fix| std::cmp::Reverse(fix.0));
        for (start, end, replacement) in fixes {
            text.replace_range(start..end, &replacement);
            let removed = end - start - replacement.len();
            for range in &mut ranges {
                if range.start >= end {
                    range.start -= removed;
                }
                if range.end >= end {
                    range.end -= removed;
                }
            }
        }
    }
    Some((text, ranges))
}

struct Groups<'a> {
    arena: &'a AstArena,
    source: &'a str,
    grouped: &'a FxHashMap<ExprId, Vec<Span>>,
    parents: &'a FxHashMap<ExprId, ExprId>,
    /// Expressions where the parser dispatches on the leading tokens.
    statements: FxHashMap<ExprId, Lead>,
    contexts: FxHashMap<ExprId, Context>,
}

impl Groups<'_> {
    fn mark_statements(&mut self, statements: impl Iterator<Item = crate::syntax::arena::StmtId>) {
        let mut previous_continues = false;
        for stmt in statements {
            let kind = self.arena.stmt(stmt).kind;
            if let ArenaStmtKind::Expr(expr) = kind {
                self.statements.insert(
                    expr,
                    Lead::Statement {
                        after_expression: previous_continues,
                    },
                );
            }
            previous_continues = statement_may_continue(&kind);
        }
    }

    fn outer_span(&self, expr: ExprId) -> Span {
        self.grouped
            .get(&expr)
            .and_then(|spans| {
                spans
                    .iter()
                    .max_by_key(|span| span.end() - span.start())
                    .copied()
            })
            .unwrap_or(self.arena.expr(expr).span)
    }

    fn context(&mut self, expr: ExprId) -> Context {
        if let Some(context) = self.contexts.get(&expr) {
            return *context;
        }
        let context = match self.parents.get(&expr).copied() {
            Some(parent) => {
                let outer = self.context(parent);
                let inside = if self.grouped.contains_key(&parent) {
                    outer.group()
                } else {
                    outer
                };
                child_context(self.arena, parent, inside, expr)
            }
            None => {
                let span = self.outer_span(expr);
                let after = &self.source[span.end()..];
                let line = &after[..after.find('\n').map_or(after.len(), |end| end + 1)];
                let follow = Follow::of_source(line, !after.starts_with([' ', '\t', '\n', '\r']));
                Context {
                    lead: self.statements.get(&expr).copied(),
                    ..Context::open(follow)
                }
            }
        };
        self.contexts.insert(expr, context);
        context
    }
}

/// Without its parentheses, `expr`'s context reaches the children along its
/// edges, which may then need parentheses of their own.
fn removal_breaks_children(
    arena: &AstArena,
    source: &str,
    expr: ExprId,
    context: Context,
    grouped: &FxHashMap<ExprId, Vec<Span>>,
) -> bool {
    let mut broken = false;
    for_each_child(arena, expr, |child| {
        if broken || grouped.contains_key(&child) {
            return;
        }
        let ungrouped = child_context(arena, expr, context, child);
        if ungrouped != child_context(arena, expr, context.group(), child) {
            broken = needs_parens(arena, source, child, ungrouped)
                || removal_breaks_children(arena, source, child, ungrouped, grouped);
        }
    });
    broken
}

/// Whether a bare path spelled in `group` would take in the text after it once
/// the parentheses are gone; the printer quotes such a path instead.
fn bare_path_absorbs_suffix(source: &str, group: Span) -> bool {
    let inner = &source[group.start() + 1..group.end() - 1];
    let after = &source[group.end()..];
    let joined = format!(
        "{inner}{}",
        &after[..after.find('\n').unwrap_or(after.len())]
    );
    crate::syntax::literal::scan_bare_path_at(inner, 0) == Some(inner.len())
        && crate::syntax::literal::scan_bare_path_at(&joined, 0)
            .is_some_and(|end| end > inner.len())
}

/// Whether `text` has an ungrouped `|>`. Method-call stages are rebuilt as
/// calls, but their source spelling still binds below every operator.
fn spells_pipeline(text: &str) -> bool {
    let mut depth = 0usize;
    lex_spellings(text).iter().any(|(tag, _)| {
        match tag {
            TokenTag::LParen | TokenTag::LBracket | TokenTag::LBrace | TokenTag::DollarLBrace => {
                depth += 1
            }
            TokenTag::RParen | TokenTag::RBracket | TokenTag::RBrace => {
                depth = depth.saturating_sub(1)
            }
            _ => {}
        }
        depth == 0 && *tag == TokenTag::PipeGt
    })
}

/// Whether deleting the parentheses of `group` around `inner` would join the
/// neighboring tokens into different ones.
fn removal_merges_tokens(source: &str, group: Span, inner: Span) -> bool {
    // Neighbors are read only up to whitespace or a delimiter, which never
    // merge, so a group inside a string interpolation is judged by its own text.
    let before = &source[..group.start()];
    let previous = &before[before
        .rfind([' ', '\t', '\n', '\r', '{', '(', '[', ',', ';'])
        .map_or(0, |at| at + 1)..];
    let after = &source[group.end()..];
    let next = &after[..after
        .find([' ', '\t', '\n', '\r', '}', ')', ']', ',', ';'])
        .unwrap_or(after.len())];
    let inner_tokens = lex_spellings(&source[inner.range()]);
    let first_merges = lex_spellings(previous)
        .last()
        .zip(inner_tokens.first())
        .is_some_and(|((_, previous), (_, first))| !tokens_stay_separate(previous, first));
    let last_merges = lex_spellings(next)
        .first()
        .zip(inner_tokens.last())
        .is_some_and(|((_, next), (_, last))| !tokens_stay_separate(last, next));
    // In f-string text `{{` is a brace escape, so an interpolation whose
    // expression begins with `{` keeps the parentheses that separate them.
    let brace_escape = before.ends_with('{') && source[inner.range()].starts_with('{');
    first_merges || last_merges || brace_escape
}

fn redundant_parens(span: Span, source: &str) -> Diagnostic {
    let inner = source[span.start() + 1..span.end() - 1].trim();
    // `(.).name` is the item's field `.name`, not `..name`.
    let fix = if inner == "."
        && source[span.end()..].starts_with('.')
        && !source[span.end()..].starts_with(".require(")
    {
        FixHint::replacement(
            Span::new(span.source_id, span.start(), span.end() + 1),
            "remove the parentheses",
            ".",
        )
    } else {
        FixHint::replacement(span, "remove the parentheses", inner)
    };
    Diagnostic::error("parentheses do not change how this expression parses")
        .with_code(DiagnosticCode::CheckRedundantParens)
        .with_label(Label::primary(span, "remove these parentheses"))
        .with_fix_hint(fix)
}

/// The slot `child` fills as the source spells it: a receiver followed by
/// `|>` is the input of a method stage that the parser rebuilt as a call.
fn spelled_slot(arena: &AstArena, parent: ExprId, child: ExprId, source: &str) -> Slot {
    let after = source
        .get(arena.expr(child).span.end()..)
        .unwrap_or_default();
    match child_context(arena, parent, Context::open(Follow::END), child).slot {
        Slot::Postfix { .. } if after.trim_start().starts_with("|>") => {
            Slot::PipelineInput { structured: false }
        }
        slot => slot,
    }
}

fn ambiguous_grouping(arena: &AstArena, operand: ExprId, source: &str) -> Diagnostic {
    let span = arena.expr(operand).span;
    let text = &source[span.range()];
    let message = if matches!(
        arena.expr(operand).kind,
        ArenaExprKind::If { .. } | ArenaExprKind::Match { .. }
    ) {
        "group an `if` or `match` expression that an operator or suffix applies to"
    } else {
        "group a pipeline that an operator applies to"
    };
    Diagnostic::error(message)
        .with_code(DiagnosticCode::CheckAmbiguousGrouping)
        .with_label(Label::primary(span, "add parentheses around this operand"))
        .with_fix_hint(FixHint::replacement(
            span,
            "add parentheses",
            format!("({text})"),
        ))
}

fn mixed_logical(arena: &AstArena, operand: ExprId, source: &str) -> Diagnostic {
    let span = arena.expr(operand).span;
    let text = &source[span.range()];
    Diagnostic::error("group `and`, `or`, and `??` explicitly when mixing them")
        .with_code(DiagnosticCode::CheckMixedLogical)
        .with_label(Label::primary(
            span,
            "add parentheses around the intended operand",
        ))
        .with_fix_hint(FixHint::replacement(
            span,
            "add parentheses",
            format!("({text})"),
        ))
}
