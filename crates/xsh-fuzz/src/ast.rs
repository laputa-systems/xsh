//! The typed program model shared by the generator, the reference evaluator,
//! and the XSH printer.
//!
//! Every node is built for a known type, so a printed program is well typed by
//! construction. The printer emits the minimal parentheses the XSH grammar
//! needs (including the explicit groupings the grammar requires for mixed
//! `and`/`or`/`??` and for Bool operands of comparisons).

use std::fmt::Write as _;
use xsh::diagnostic::DiagnosticCode;
use xsh::frontend::syntax::grammar;
use xsh::frontend::syntax::node::BinaryOp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fam {
    /// The program's own nominal family, `FzErr`.
    Fz,
    /// The common `Error` type.
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Str,
    Bool,
    Path,
    Bytes,
    /// Nonnegative milliseconds.
    Duration,
    List(Box<Ty>),
    /// Keys are `Str` or `Int`.
    Map(Box<Ty>, Box<Ty>),
    Rec(usize),
    Enum(usize),
    Opt(Box<Ty>),
    Res(Box<Ty>, Fam),
}

impl Ty {
    pub fn list(element: Ty) -> Ty {
        Ty::List(Box::new(element))
    }

    pub fn map(key: Ty, value: Ty) -> Ty {
        Ty::Map(Box::new(key), Box::new(value))
    }

    pub fn opt(inner: Ty) -> Ty {
        Ty::Opt(Box::new(inner))
    }

    pub fn res(inner: Ty, fam: Fam) -> Ty {
        Ty::Res(Box::new(inner), fam)
    }

    pub fn is_scalar(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float | Ty::Str | Ty::Bool | Ty::Path | Ty::Bytes)
    }

    /// Displayable directly inside an f-string interpolation.
    pub fn is_displayable(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float | Ty::Str | Ty::Bool | Ty::Path)
    }

    /// Supports `==`/`!=` in generated programs.
    pub fn is_equatable(&self) -> bool {
        match self {
            Ty::Int | Ty::Str | Ty::Bool | Ty::Path | Ty::Bytes | Ty::Float => true,
            Ty::List(element) => element.is_equatable() && element.is_scalar(),
            _ => false,
        }
    }

    pub fn render(&self, program: &Program) -> String {
        match self {
            Ty::Int => "Int".into(),
            Ty::Float => "Float".into(),
            Ty::Str => "Str".into(),
            Ty::Bool => "Bool".into(),
            Ty::Path => "Path".into(),
            Ty::Bytes => "Bytes".into(),
            Ty::Duration => "Duration".into(),
            Ty::List(element) => format!("List[{}]", element.render(program)),
            Ty::Map(key, value) if **key == Ty::Str => format!("Map[{}]", value.render(program)),
            Ty::Map(key, value) => format!("Map[{}, {}]", key.render(program), value.render(program)),
            Ty::Rec(index) => program.records[*index].name.clone(),
            Ty::Enum(index) => program.enums[*index].name.clone(),
            Ty::Opt(inner) => format!("{}?", inner.render(program)),
            Ty::Res(inner, Fam::Fz) => format!("Result[{}, FzErr]", inner.render(program)),
            Ty::Res(inner, Fam::Error) => format!("Result[{}]", inner.render(program)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecDecl {
    pub name: String,
    pub fields: Vec<(String, Ty)>,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<(String, Vec<Ty>)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FnKind {
    Pure,
    Proc,
    /// `stream name(...) [] -> Stream[T]`; `tail` is the item type.
    Stream,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Ty,
    pub default: Option<Expr>,
    /// Omit the annotation of a defaulted parameter (its type is inferred).
    pub infer_type: bool,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub kind: FnKind,
    pub params: Vec<Param>,
    /// The type callers observe. Unannotated procs observe `Result[T]`.
    pub ret: Ty,
    /// The type of the body's value tail.
    pub tail: Ty,
    pub annotate: bool,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Expr>,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Let { name: String, annot: Option<Ty>, value: Expr, mutable: bool },
    Assign { target: Target, op: AssignOp, value: Expr },
    If { cond: Expr, then: Block, otherwise: Option<Block> },
    For { var: String, iter: Expr, body: Block },
    /// `var counter = 0; while counter < limit { body; counter += 1 }`
    While { counter: String, limit: i64, body: Block },
    Match { subject: Expr, arms: Vec<(Pat, Block)> },
    /// `assert expr == <value>`; the evaluator fills in the value it first
    /// observes, so the assertion holds when it runs.
    AssertEq { expr: Expr, expected: Option<Expr> },
    Assert(Expr),
    /// `out += [expr]`
    Out(Expr),
    ContinueWhen(Expr),
    BreakWhen(Expr),
    /// `return Err(FzErr.Bad(message: ...)) when cond`
    ReturnErrWhen { cond: Expr, message: String },
    Block(Block),
    /// `defer { ... }`
    Defer(Block),
    /// `guard cond else { continue | break | return Err(...) }`
    Guard { cond: Expr, exit: Exit },
    /// `if let pat = subject { ... } else { ... }`
    IfLet { pat: Pat, subject: Expr, then: Block, otherwise: Option<Block> },
    /// `yield expr` in a stream producer.
    Yield(Expr),
}

#[derive(Clone, Debug)]
pub enum Exit {
    Continue,
    Break,
    ReturnErr(String),
}

#[derive(Clone, Debug)]
pub enum Target {
    Var(String),
    Field(String, String),
    Key(String, Expr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
}

#[derive(Clone, Debug)]
pub enum Pat {
    Wild,
    Bind(String),
    Int(i64),
    Str(String),
    Bool(bool),
    Variant { name: String, binds: Vec<String> },
    Ok(String),
    Err(String),
    /// `[]`
    ListEmpty,
    /// `[head, ..rest]`
    ListCons { head: String, rest: String },
}

/// The non-binding right side of `value is ...`.
#[derive(Clone, Debug)]
pub enum IsPat {
    Variant { name: String, arity: usize },
    Ok,
    Err,
}

#[derive(Clone, Debug)]
pub enum Stage {
    Map { var: String, body: Expr },
    Where { var: String, body: Expr },
    Take(i64),
    Drop(i64),
    Sort,
    Repeat(i64),
    Collect,
    Sum,
    Count,
    Min,
    Max,
    First,
    Last,
    Any { var: String, body: Expr },
    All { var: String, body: Expr },
    Fold { init: Expr, acc: String, item: String, body: Expr },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    In,
    NotIn,
}

#[derive(Clone, Debug)]
pub enum Arg {
    Pos(Expr),
    Named(String, Expr),
    /// `...{name: value, ...}`
    Spread(Vec<(String, Expr)>),
}

#[derive(Clone, Debug)]
pub enum FmtPart {
    Lit(String),
    Interp(Expr),
    /// `{expr:>N}`, `{expr:<N}`, or `{expr:0N}`, with the alignment byte.
    Width(Expr, char, usize),
}

#[derive(Clone, Debug)]
pub enum Expr {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Path(String),
    Bytes(Vec<u8>),
    Null,
    Var(String),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Block>, Box<Block>),
    Match(Box<Expr>, Vec<(Pat, Expr)>),
    Call { func: usize, args: Vec<Arg> },
    Method { recv: Box<Expr>, name: &'static str, args: Vec<Expr> },
    /// `recv?.name(args)` on an Optional receiver.
    OptMethod { recv: Box<Expr>, name: &'static str, args: Vec<Expr> },
    List(Vec<Elem>),
    Comp { proj: Box<Expr>, var: String, iter: Box<Expr>, filter: Option<Box<Expr>> },
    /// Computed-key Map literal, `{[k]: v, ...}`; empty maps print as `map.empty()`.
    MapLit(Vec<(Expr, Expr)>),
    MapComp { key: Box<Expr>, value: Box<Expr>, var: String, iter: Box<Expr> },
    RecCtor { rec: usize, fields: Vec<(String, Expr)> },
    RecUpdate { base: Box<Expr>, updates: Vec<(String, Expr)> },
    Field(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Slice(Box<Expr>, Option<Box<Expr>>, Option<Box<Expr>>),
    Variant { en: usize, variant: usize, args: Vec<Expr> },
    Ok(Box<Expr>),
    Err(String),
    Try(Box<Block>),
    /// `left ?? right` on a Result or Optional.
    Fallback(Box<Expr>, Box<Expr>),
    /// `result ?? { |name| block }`
    FallbackBlock(Box<Expr>, String, Box<Block>),
    Propagate(Box<Expr>),
    Fmt(Vec<FmtPart>),
    /// A value block `{ (stmts) tail }`.
    BlockValue(Box<Block>),
    Duration(u64),
    /// `source |> stage |> ...`
    Pipeline { source: Box<Expr>, stages: Vec<Stage> },
    Is(Box<Expr>, IsPat),
    /// `retry [] { ... }`
    Retry(Box<Block>),
    /// `ctx "description" { ... }`
    Ctx(String, Box<Block>),
    /// A call through an immutable callable alias of `func`.
    AliasCall { alias: String, func: usize, args: Vec<Arg> },
    /// `recv?.field` on an Optional record.
    OptField(Box<Expr>, String),
}

#[derive(Clone, Debug)]
pub enum Elem {
    Item(Expr),
    Splice(Expr),
}

#[derive(Clone, Debug)]
pub struct Program {
    /// Top-level `const` declarations, visible everywhere.
    pub consts: Vec<(String, Ty, Expr)>,
    pub records: Vec<RecDecl>,
    pub enums: Vec<EnumDecl>,
    pub functions: Vec<FnDecl>,
    pub body: Block,
}

/// The fixed text around the generated body. The body runs under an empty
/// effect clause, so the checker proves it performs no host effect; only this
/// shim prints.
pub const ENTRY: &str = "fuzz_main";

impl Program {
    pub fn print(&self) -> String {
        let mut printer = Printer { program: self, out: String::new(), indent: 0 };
        printer.program_text();
        strip_redundant_parens(printer.out)
    }
}

/// The printer groups conservatively, and the checker rejects grouping that
/// does not change the parse (`check.redundant-parens`), so remove exactly the
/// parentheses it reports.
fn strip_redundant_parens(mut source: String) -> String {
    for _ in 0..16 {
        let parsed = xsh::frontend::syntax::parser::Parser::parse_source_arena_only(xsh::frontend::source::SourceId::new(0), &source);
        if !parsed.diagnostics.is_empty() {
            return source;
        }
        let checked = xsh::frontend::check::Checker::check_arena(&parsed.arena, &source);
        let mut edits: Vec<(usize, usize, String)> = checked
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == Some(DiagnosticCode::CheckRedundantParens))
            .flat_map(|diagnostic| &diagnostic.fix_hints)
            .filter_map(|hint| Some((hint.span?.start(), hint.span?.end(), hint.replacement.clone()?)))
            .collect();
        if edits.is_empty() {
            return source;
        }
        edits.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
        let mut kept_end = 0;
        edits.retain(|(start, end, _)| {
            let keep = *start >= kept_end;
            if keep {
                kept_end = *end;
            }
            keep
        });
        for (start, end, replacement) in edits.into_iter().rev() {
            source.replace_range(start..end, &replacement);
        }
    }
    source
}

struct Printer<'a> {
    program: &'a Program,
    out: String,
    indent: usize,
}

// Binding strengths come from the XSH grammar's operator table, so a printed
// program groups exactly as the parser reads it.
const PREC_FALLBACK: u8 = grammar::binary_precedence(BinaryOp::ResultFallback);
const PREC_EQ: u8 = grammar::binary_precedence(BinaryOp::Eq);
const PREC_CMP: u8 = grammar::binary_precedence(BinaryOp::Lt);
const PREC_TERM: u8 = grammar::binary_precedence(BinaryOp::Add);
const PREC_UNARY: u8 = grammar::PREFIX;
const PREC_POSTFIX: u8 = grammar::PREFIX_OPERAND;

impl BinOp {
    const fn grammar_op(self) -> BinaryOp {
        match self {
            BinOp::Add => BinaryOp::Add,
            BinOp::Sub => BinaryOp::Sub,
            BinOp::Mul => BinaryOp::Mul,
            BinOp::Div => BinaryOp::Div,
            BinOp::Rem => BinaryOp::Rem,
            BinOp::Eq => BinaryOp::Eq,
            BinOp::Ne => BinaryOp::Ne,
            BinOp::Lt => BinaryOp::Lt,
            BinOp::Le => BinaryOp::Le,
            BinOp::Gt => BinaryOp::Gt,
            BinOp::Ge => BinaryOp::Ge,
            BinOp::And => BinaryOp::And,
            BinOp::Or => BinaryOp::Or,
            BinOp::In => BinaryOp::In,
            BinOp::NotIn => BinaryOp::NotIn,
        }
    }
}

fn binop_prec(op: BinOp) -> u8 {
    grammar::binary_precedence(op.grammar_op())
}

fn binop_text(op: BinOp) -> &'static str {
    grammar::binary_operator(op.grammar_op()).spelling
}

/// The binding strength of an expression's outermost operator.
fn expr_prec(expr: &Expr) -> u8 {
    match expr {
        Expr::Binary(op, ..) => binop_prec(*op),
        Expr::Fallback(..) | Expr::FallbackBlock(..) => PREC_FALLBACK,
        Expr::Not(_) | Expr::Neg(_) => PREC_UNARY,
        Expr::Int(value) if *value < 0 => PREC_UNARY,
        Expr::Float(value) if value.is_sign_negative() => PREC_UNARY,
        // Keyword-introduced expressions extend as far as their braces; they
        // need grouping before a postfix operator or a following operand.
        Expr::If(..) | Expr::Match(..) | Expr::Try(..) | Expr::BlockValue(_) | Expr::Pipeline { .. } | Expr::Retry(_) | Expr::Ctx(..) => 0,
        Expr::Is(..) => grammar::PATTERN_TEST,
        _ => PREC_POSTFIX,
    }
}

pub fn quote_str(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            // `${` in an expression string is an error; `\$` is always text.
            '$' => out.push_str("\\$"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

/// Literal f-string text: braces doubled, and `\$` wherever a bare `$`
/// would start `${` or a `$name` that may name a binding. `next` is the
/// first character written after the text.
fn fmt_text(text: &str, next: Option<char>) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        let following = chars.get(index + 1).copied().or(next);
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '{' => out.push_str("{{"),
            '}' => out.push_str("}}"),
            '$' if following.is_some_and(|c| c == '{' || c == '_' || c.is_ascii_alphabetic()) => out.push_str("\\$"),
            _ => out.push(*ch),
        }
    }
    out
}

/// `{expr}` with an optional `:spec`; `{{` is a brace escape, so an
/// expression that begins with `{` is set off by spaces.
fn interpolation(expr: &str, spec: &str) -> String {
    if expr.starts_with('{') { format!("{{ {expr}{spec} }}") } else { format!("{{{expr}{spec}}}") }
}

pub fn float_literal(value: f64) -> String {
    let text = format!("{value:?}");
    if text.contains('.') || text.contains('e') || text.contains("inf") || text.contains("NaN") {
        text
    } else {
        format!("{text}.0")
    }
}

impl Printer<'_> {
    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.out.push_str("  ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn program_text(&mut self) {
        let program = self.program;
        self.line("error FzErr = Bad(message: Str)");
        for (name, ty, value) in &program.consts {
            let value = self.expr_text(value, 0);
            self.line(&format!("const {name}: {} = {value}", ty.render(program)));
        }
        for record in &program.records {
            let fields: Vec<String> = record
                .fields
                .iter()
                .map(|(name, ty)| format!("{name}: {}", ty.render(program)))
                .collect();
            self.line(&format!("type {} = {{{}}}", record.name, fields.join(", ")));
        }
        for decl in &program.enums {
            let variants: Vec<String> = decl
                .variants
                .iter()
                .map(|(name, payload)| {
                    if payload.is_empty() {
                        name.clone()
                    } else {
                        let tys: Vec<String> = payload.iter().map(|ty| ty.render(program)).collect();
                        format!("{name}({})", tys.join(", "))
                    }
                })
                .collect();
            self.line(&format!("enum {} {{ {} }}", decl.name, variants.join(", ")));
        }
        for function in &program.functions {
            self.function(function);
        }
        self.line(&format!("proc {ENTRY}() [] -> Result[List[Str]] {{"));
        self.indent += 1;
        self.line("try {");
        self.indent += 1;
        self.line("var out: List[Str] = []");
        self.block_stmts(&program.body);
        self.line("out");
        self.indent -= 1;
        self.line("}");
        self.indent -= 1;
        self.line("}");
        self.line(&format!("for line in {ENTRY}()? {{ print ${{line}} }}"));
    }

    fn function(&mut self, function: &FnDecl) {
        let program = self.program;
        let params: Vec<String> = function
            .params
            .iter()
            .map(|param| match (&param.default, param.infer_type) {
                (Some(default), true) => format!("{} = {}", param.name, self.expr_text(default, 0)),
                (Some(default), false) => format!(
                    "{}: {} = {}",
                    param.name,
                    param.ty.render(program),
                    self.expr_text(default, 0)
                ),
                (None, _) => format!("{}: {}", param.name, param.ty.render(program)),
            })
            .collect();
        let keyword = match function.kind {
            FnKind::Pure => "pure",
            FnKind::Proc => "proc",
            FnKind::Stream => "stream",
        };
        let ret = if function.kind == FnKind::Stream {
            format!(" [] -> Stream[{}]", function.tail.render(program))
        } else if function.annotate {
            format!(" -> {}", function.ret.render(program))
        } else {
            String::new()
        };
        self.line(&format!("{keyword} {}({}){ret} {{", function.name, params.join(", ")));
        self.indent += 1;
        self.block_stmts(&function.body);
        if let Some(tail) = &function.body.tail {
            let text = self.expr_text(tail, 0);
            self.tail_line(&text);
        }
        self.indent -= 1;
        self.line("}");
    }

    fn tail_line(&mut self, text: &str) {
        self.line(text);
    }

    fn block_stmts(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
    }

    fn nested_block(&mut self, header: &str, block: &Block) {
        self.line(&format!("{header} {{"));
        self.indent += 1;
        self.block_stmts(block);
        if let Some(tail) = &block.tail {
            let text = self.expr_text(tail, 0);
            self.tail_line(&text);
        }
        self.indent -= 1;
    }

    fn stmt(&mut self, stmt: &Stmt) {
        let program = self.program;
        match stmt {
            Stmt::Let { name, annot, value, mutable } => {
                let keyword = if *mutable { "var" } else { "let" };
                let annot = annot
                    .as_ref()
                    .map(|ty| format!(": {}", ty.render(program)))
                    .unwrap_or_default();
                let value = self.expr_text(value, 0);
                self.line(&format!("{keyword} {name}{annot} = {value}"));
            }
            Stmt::Assign { target, op, value } => {
                let target = match target {
                    Target::Var(name) => name.clone(),
                    Target::Field(name, field) => format!("{name}.{field}"),
                    Target::Key(name, key) => format!("{name}[{}]", self.expr_text(key, 0)),
                };
                let op = match op {
                    AssignOp::Set => "=",
                    AssignOp::Add => "+=",
                    AssignOp::Sub => "-=",
                    AssignOp::Mul => "*=",
                };
                let value = self.expr_text(value, 0);
                self.line(&format!("{target} {op} {value}"));
            }
            Stmt::If { cond, then, otherwise } => {
                let cond = self.header_text(cond);
                self.nested_block(&format!("if {cond}"), then);
                if let Some(otherwise) = otherwise {
                    self.nested_block("} else", otherwise);
                }
                self.line("}");
            }
            Stmt::For { var, iter, body } => {
                let iter = self.header_text(iter);
                self.nested_block(&format!("for {var} in {iter}"), body);
                self.line("}");
            }
            Stmt::While { counter, limit, body } => {
                // The increment comes first so `continue` cannot skip it.
                self.line(&format!("var {counter} = 0"));
                self.line(&format!("while {counter} < {limit} {{"));
                self.indent += 1;
                self.line(&format!("{counter} += 1"));
                self.block_stmts(body);
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Match { subject, arms } => {
                let subject = self.header_text(subject);
                self.line(&format!("match {subject} {{"));
                self.indent += 1;
                for (pat, body) in arms {
                    let pat = pat_text(pat);
                    self.nested_block(&format!("{pat} =>"), body);
                    self.line("}");
                }
                self.indent -= 1;
                self.line("}");
            }
            Stmt::AssertEq { expr, expected } => {
                let right = expected.clone().unwrap_or_else(|| expr.clone());
                let text = self.expr_raw(&Expr::Binary(BinOp::Eq, Box::new(expr.clone()), Box::new(right)));
                self.line(&format!("assert {text}"));
            }
            Stmt::Assert(expr) => {
                let text = self.expr_text(expr, 0);
                self.line(&format!("assert {text}"));
            }
            Stmt::Out(expr) => {
                let text = self.expr_text(expr, 0);
                self.line(&format!("out += [{text}]"));
            }
            Stmt::ContinueWhen(cond) => {
                let text = self.header_text(cond);
                self.line(&format!("continue when {text}"));
            }
            Stmt::BreakWhen(cond) => {
                let text = self.header_text(cond);
                self.line(&format!("break when {text}"));
            }
            Stmt::ReturnErrWhen { cond, message } => {
                let text = self.header_text(cond);
                self.line(&format!(
                    "return Err(FzErr.Bad(message: {})) when {text}",
                    quote_str(message)
                ));
            }
            Stmt::Block(block) => {
                self.line("{");
                self.indent += 1;
                self.block_stmts(block);
                self.indent -= 1;
                self.line("}");
            }
            Stmt::Defer(block) => {
                self.nested_block("defer", block);
                self.line("}");
            }
            Stmt::Guard { cond, exit } => {
                let cond = self.header_text(cond);
                let exit = match exit {
                    Exit::Continue => "continue".to_string(),
                    Exit::Break => "break".to_string(),
                    Exit::ReturnErr(message) => format!("return Err(FzErr.Bad(message: {}))", quote_str(message)),
                };
                self.line(&format!("guard {cond} else {{ {exit} }}"));
            }
            Stmt::IfLet { pat, subject, then, otherwise } => {
                let subject = self.header_text(subject);
                self.nested_block(&format!("if let {} = {subject}", pat_text(pat)), then);
                if let Some(otherwise) = otherwise {
                    self.nested_block("} else", otherwise);
                }
                self.line("}");
            }
            Stmt::Yield(value) => {
                let text = self.expr_text(value, 0);
                self.line(&format!("yield {text}"));
            }
        }
    }

    fn block_inline(&self, block: &Block) -> String {
        let mut printer = Printer { program: self.program, out: String::new(), indent: self.indent + 1 };
        printer.block_stmts(block);
        if let Some(tail) = &block.tail {
            let text = printer.expr_text(tail, 0);
            printer.tail_line(&text);
        }
        let mut indent = String::new();
        for _ in 0..self.indent {
            indent.push_str("  ");
        }
        format!("{{\n{}{indent}}}", printer.out)
    }

    fn args_text(&self, args: &[Expr]) -> String {
        let parts: Vec<String> = args.iter().map(|arg| self.expr_text(arg, 0)).collect();
        parts.join(", ")
    }

    /// Prints `expr` for a context that binds at least `min` tightly.
    fn expr_text(&self, expr: &Expr, min: u8) -> String {
        let text = self.expr_raw(expr);
        if expr_prec(expr) < min {
            format!("({text})")
        } else {
            text
        }
    }

    /// An expression followed by a `{` (a condition, subject, or iterable)
    /// must not itself start or end with braces.
    fn header_text(&self, expr: &Expr) -> String {
        let text = self.expr_raw(expr);
        if expr_prec(expr) == 0 || text.starts_with('{') {
            format!("({text})")
        } else {
            text
        }
    }

    fn operand(&self, op: BinOp, operand: &Expr, right: bool) -> String {
        let prec = binop_prec(op);
        let operand_prec = expr_prec(operand);
        let needs = match operand {
            // Mixing `and`, `or`, and `??` always needs explicit grouping.
            Expr::Binary(inner, ..) if matches!(op, BinOp::And | BinOp::Or) && matches!(inner, BinOp::And | BinOp::Or) && *inner != op => true,
            Expr::Fallback(..) | Expr::FallbackBlock(..) if matches!(op, BinOp::And | BinOp::Or) => true,
            // Comparisons and equality never chain implicitly here.
            Expr::Binary(inner, ..) if (PREC_EQ..=PREC_CMP).contains(&prec) && (PREC_EQ..=PREC_CMP).contains(&binop_prec(*inner)) => true,
            _ if right => operand_prec <= prec,
            _ => operand_prec < prec,
        };
        let text = self.expr_raw(operand);
        if needs { format!("({text})") } else { text }
    }

    fn expr_raw(&self, expr: &Expr) -> String {
        let program = self.program;
        match expr {
            Expr::Int(value) => value.to_string(),
            Expr::Float(value) => float_literal(*value),
            Expr::Str(text) => quote_str(text),
            Expr::Bool(value) => value.to_string(),
            Expr::Path(text) => format!("p{}", quote_str(text)),
            Expr::Bytes(bytes) => {
                let mut out = String::from("b\"");
                for byte in bytes {
                    if byte.is_ascii_alphanumeric() || *byte == b' ' {
                        out.push(*byte as char);
                    } else {
                        let _ = write!(out, "\\x{byte:02x}");
                    }
                }
                out.push('"');
                out
            }
            Expr::Null => "null".into(),
            Expr::Var(name) => name.clone(),
            Expr::Not(inner) => format!("!{}", self.expr_text(inner, PREC_UNARY)),
            Expr::Neg(inner) => format!("-{}", self.expr_text(inner, PREC_POSTFIX)),
            Expr::Binary(op, left, right) => format!(
                "{} {} {}",
                self.operand(*op, left, false),
                binop_text(*op),
                self.operand(*op, right, true)
            ),
            Expr::If(cond, then, otherwise) => format!(
                "if {} {} else {}",
                self.header_text(cond),
                self.block_inline(then),
                self.block_inline(otherwise)
            ),
            Expr::Match(subject, arms) => {
                let arms: Vec<String> = arms
                    .iter()
                    .map(|(pat, body)| {
                        let body = self.expr_text(body, 0);
                        format!("{} => {body}", pat_text(pat))
                    })
                    .collect();
                format!("match {} {{ {} }}", self.header_text(subject), arms.join(", "))
            }
            Expr::Call { func, args } => {
                let args: Vec<String> = args
                    .iter()
                    .map(|arg| match arg {
                        Arg::Pos(value) => self.expr_text(value, 0),
                        Arg::Named(name, value) => format!("{name}: {}", self.expr_text(value, 0)),
                        Arg::Spread(fields) => {
                            let fields: Vec<String> = fields
                                .iter()
                                .map(|(name, value)| format!("{name}: {}", self.expr_text(value, 0)))
                                .collect();
                            format!("...{{{}}}", fields.join(", "))
                        }
                    })
                    .collect();
                format!("{}({})", program.functions[*func].name, args.join(", "))
            }
            Expr::Method { recv, name, args } => {
                format!("{}.{name}({})", self.expr_text(recv, PREC_POSTFIX), self.args_text(args))
            }
            Expr::OptMethod { recv, name, args } => {
                format!("{}?.{name}({})", self.expr_text(recv, PREC_POSTFIX), self.args_text(args))
            }
            Expr::List(elems) => {
                let parts: Vec<String> = elems
                    .iter()
                    .map(|elem| match elem {
                        Elem::Item(value) => self.expr_text(value, 0),
                        Elem::Splice(value) => format!("@{}", self.expr_text(value, PREC_POSTFIX)),
                    })
                    .collect();
                format!("[{}]", parts.join(", "))
            }
            Expr::Comp { proj, var, iter, filter } => {
                let filter = filter
                    .as_ref()
                    .map(|filter| format!(" if {}", self.header_text(filter)))
                    .unwrap_or_default();
                format!(
                    "[{} for {var} in {}{filter}]",
                    self.expr_text(proj, 0),
                    self.header_text(iter)
                )
            }
            Expr::MapLit(entries) if entries.is_empty() => "map.empty()".into(),
            Expr::MapLit(entries) => {
                let parts: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("[{}]: {}", self.expr_text(key, 0), self.expr_text(value, 0)))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            Expr::MapComp { key, value, var, iter } => format!(
                "{{[{}]: {} for {var} in {}}}",
                self.expr_text(key, 0),
                self.expr_text(value, 0),
                self.header_text(iter)
            ),
            Expr::RecCtor { rec, fields } => {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|(name, value)| format!("{name}: {}", self.expr_text(value, 0)))
                    .collect();
                format!("{}({})", program.records[*rec].name, parts.join(", "))
            }
            Expr::RecUpdate { base, updates } => {
                let parts: Vec<String> = updates
                    .iter()
                    .map(|(name, value)| format!("{name}: {}", self.expr_text(value, 0)))
                    .collect();
                format!("{{...{}, {}}}", self.expr_text(base, 0), parts.join(", "))
            }
            Expr::Field(base, field) => format!("{}.{field}", self.expr_text(base, PREC_POSTFIX)),
            Expr::Index(base, index) => {
                format!("{}[{}]", self.expr_text(base, PREC_POSTFIX), self.expr_text(index, 0))
            }
            Expr::Slice(base, start, end) => format!(
                "{}[{}..{}]",
                self.expr_text(base, PREC_POSTFIX),
                start.as_ref().map(|start| self.expr_text(start, PREC_TERM)).unwrap_or_default(),
                end.as_ref().map(|end| self.expr_text(end, PREC_TERM)).unwrap_or_default()
            ),
            Expr::Variant { en, variant, args } => {
                let name = &program.enums[*en].variants[*variant].0;
                if args.is_empty() {
                    name.clone()
                } else {
                    format!("{name}({})", self.args_text(args))
                }
            }
            Expr::Ok(inner) => format!("Ok({})", self.expr_text(inner, 0)),
            Expr::Err(message) => format!("Err(FzErr.Bad(message: {}))", quote_str(message)),
            Expr::Try(block) => format!("try {}", self.block_inline(block)),
            Expr::Fallback(left, right) => format!(
                "{} ?? {}",
                self.fallback_operand(left, false),
                self.fallback_operand(right, true)
            ),
            Expr::FallbackBlock(left, name, block) => format!(
                "{} ?? {{ |{name}|{}",
                self.fallback_operand(left, false),
                &self.block_inline(block)[1..]
            ),
            Expr::Propagate(inner) => format!("{}?", self.expr_text(inner, PREC_POSTFIX)),
            Expr::Fmt(parts) => {
                let mut out = String::new();
                for (index, part) in parts.iter().enumerate() {
                    match part {
                        FmtPart::Lit(text) => {
                            let next = match parts.get(index + 1) {
                                Some(FmtPart::Lit(text)) => text.chars().next(),
                                Some(_) => Some('{'),
                                None => None,
                            };
                            out.push_str(&fmt_text(text, next));
                        }
                        FmtPart::Interp(value) => out.push_str(&interpolation(&self.expr_text(value, 0), "")),
                        FmtPart::Width(value, align, width) => {
                            out.push_str(&interpolation(&self.expr_text(value, 0), &format!(":{align}{width}")));
                        }
                    }
                }
                // Only a triple-quoted f-string may break lines inside `{...}`;
                // text never starts with a raw line break, so no block layout
                // applies.
                if out.contains('\n') { format!("f\"\"\"{out}\"\"\"") } else { format!("f\"{out}\"") }
            }
            Expr::BlockValue(block) => self.block_inline(block),
            Expr::Duration(millis) => {
                if millis % 1000 == 0 && *millis > 0 { format!("{}s", millis / 1000) } else { format!("{millis}ms") }
            }
            Expr::Pipeline { source, stages } => {
                let mut text = self.expr_text(source, PREC_POSTFIX);
                for stage in stages {
                    text.push_str(" |> ");
                    text.push_str(&self.stage_text(stage));
                }
                text
            }
            Expr::Is(subject, pat) => {
                let pat = match pat {
                    IsPat::Variant { name, arity: 0 } => name.clone(),
                    IsPat::Variant { name, arity } => format!("{name}({})", vec!["_"; *arity].join(", ")),
                    IsPat::Ok => "Ok(_)".into(),
                    IsPat::Err => "Err(_)".into(),
                };
                format!("{} is {pat}", self.expr_text(subject, PREC_CMP + 1))
            }
            Expr::Retry(block) => format!("retry [] {}", self.block_inline(block)),
            Expr::Ctx(description, block) => format!("ctx {} {}", quote_str(description), self.block_inline(block)),
            Expr::AliasCall { alias, args, .. } => {
                let args: Vec<String> = args
                    .iter()
                    .map(|arg| match arg {
                        Arg::Pos(value) => self.expr_text(value, 0),
                        Arg::Named(name, value) => format!("{name}: {}", self.expr_text(value, 0)),
                        Arg::Spread(fields) => {
                            let fields: Vec<String> = fields
                                .iter()
                                .map(|(name, value)| format!("{name}: {}", self.expr_text(value, 0)))
                                .collect();
                            format!("...{{{}}}", fields.join(", "))
                        }
                    })
                    .collect();
                format!("{alias}({})", args.join(", "))
            }
            Expr::OptField(recv, field) => format!("{}?.{field}", self.expr_text(recv, PREC_POSTFIX)),
        }
    }

    fn stage_text(&self, stage: &Stage) -> String {
        let body = |var: &str, body: &Expr| format!("{{ |{var}| {} }}", self.expr_text(body, 0));
        match stage {
            Stage::Map { var, body: value } => format!("map {}", body(var, value)),
            Stage::Where { var, body: value } => format!("where {}", body(var, value)),
            Stage::Take(count) => format!("take({count})"),
            Stage::Drop(count) => format!("drop({count})"),
            Stage::Sort => "sort".into(),
            Stage::Repeat(count) => format!("repeat({count})"),
            Stage::Collect => "collect()".into(),
            Stage::Sum => "sum".into(),
            Stage::Count => "count".into(),
            Stage::Min => "min".into(),
            Stage::Max => "max".into(),
            Stage::First => "first()".into(),
            Stage::Last => "last()".into(),
            Stage::Any { var, body: value } => format!("any {}", body(var, value)),
            Stage::All { var, body: value } => format!("all {}", body(var, value)),
            Stage::Fold { init, acc, item, body: value } => {
                format!("fold({}) {{ |{acc}, {item}| {} }}", self.expr_text(init, 0), self.expr_text(value, 0))
            }
        }
    }

    fn fallback_operand(&self, operand: &Expr, right: bool) -> String {
        let text = self.expr_raw(operand);
        let prec = expr_prec(operand);
        let needs = match operand {
            Expr::Binary(BinOp::And | BinOp::Or, ..) => true,
            Expr::Fallback(..) | Expr::FallbackBlock(..) => !right,
            _ => prec < PREC_FALLBACK,
        };
        if needs { format!("({text})") } else { text }
    }
}

pub fn pat_text(pat: &Pat) -> String {
    match pat {
        Pat::Wild => "_".into(),
        Pat::Bind(name) => name.clone(),
        Pat::Int(value) => value.to_string(),
        Pat::Str(text) => quote_str(text),
        Pat::Bool(value) => value.to_string(),
        Pat::Variant { name, binds } if binds.is_empty() => name.clone(),
        Pat::Variant { name, binds } => format!("{name}({})", binds.join(", ")),
        Pat::Ok(name) => format!("Ok({name})"),
        Pat::Err(name) => format!("Err({name})"),
        Pat::ListEmpty => "[]".into(),
        Pat::ListCons { head, rest } => format!("[{head}, ..{rest}]"),
    }
}
