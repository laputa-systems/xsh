//! A grammar- and type-directed generator of well-typed XSH programs.
//!
//! Every expression is built for a requested type from productions whose
//! typing rules the generator knows, so the printed program must pass the
//! checker; a rejection is a generator bug or a checker bug, never noise.
//! Programs use no `Any` and no host API: the body runs under an empty effect
//! clause, so the checker itself proves the program has no host effect.

use crate::ast::{
    Arg, AssignOp, BinOp, Block, Elem, EnumDecl, Exit, Expr, Fam, FmtPart, FnDecl, FnKind, IsPat,
    Param, Pat, Program, RecDecl, Stage, Stmt, Target, Ty,
};
use crate::eval::{Evaluator, Val};
use crate::methods::{ORACLE_METHODS, OracleMethod, Shape};
use crate::rng::Rng;
use rustc_hash::FxHashMap as HashMap;
use xsh_registry::signature::MethodReceiver;

#[derive(Clone, Debug)]
pub struct GenConfig {
    pub max_body_stmts: usize,
    pub max_block_stmts: usize,
    pub max_expr_depth: usize,
    pub max_functions: usize,
    pub max_records: usize,
    pub max_enums: usize,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self {
            max_body_stmts: 18,
            max_block_stmts: 4,
            max_expr_depth: 3,
            max_functions: 4,
            max_records: 2,
            max_enums: 2,
        }
    }
}

/// A generated program with its expected stdout.
#[derive(Clone, Debug)]
pub struct Generated {
    pub seed: u64,
    pub program: Program,
    pub source: String,
    pub expected: String,
}

/// Larger draws are rejected, so one program cannot dominate a run.
pub const MAX_SOURCE_BYTES: usize = 48 << 10;

/// Generates the program for `seed`. Draws that the reference evaluator
/// rejects (overflow, a missing index) are retried with a derived seed, so the
/// result is still a pure function of `seed`.
pub fn generate(seed: u64, config: &GenConfig) -> Generated {
    for attempt in 0..1000u64 {
        let rng = Rng::new(seed.wrapping_mul(0x0100_0000_01B3).wrapping_add(attempt));
        let program = Gen::new(rng, config).program();
        if let Some(generated) = finish(seed, program) {
            return generated;
        }
    }
    panic!("seed {seed}: no evaluable program in 1000 attempts")
}

/// Evaluates `program`, fills its assertion holes, and prints it. `None` when
/// the program would fail for a domain reason.
pub fn finish(seed: u64, mut program: Program) -> Option<Generated> {
    let (lines, observed) = {
        let mut evaluator = Evaluator::new(&program);
        let lines = evaluator.run().ok()?;
        (lines, std::mem::take(&mut evaluator.assert_values))
    };
    fill_asserts(&mut program.body, &observed);
    for function in &mut program.functions {
        fill_asserts(&mut function.body, &observed);
    }
    let source = program.print();
    if source.len() > MAX_SOURCE_BYTES {
        return None;
    }
    let mut expected = String::new();
    for line in lines {
        expected.push_str(&line);
        expected.push('\n');
    }
    Some(Generated {
        seed,
        program,
        source,
        expected,
    })
}

fn literal_of(value: &Val) -> Option<Expr> {
    Some(match value {
        Val::Int(value) => Expr::Int(*value),
        Val::Float(value) => Expr::Float(*value),
        Val::Str(text) => Expr::Str(text.clone()),
        Val::Bool(value) => Expr::Bool(*value),
        Val::Path(text) => Expr::Path(text.clone()),
        Val::Bytes(bytes) => Expr::Bytes(bytes.clone()),
        Val::Duration(millis) => Expr::Duration(*millis),
        _ => return None,
    })
}

fn fill_asserts(block: &mut Block, observed: &HashMap<usize, Option<Val>>) {
    for stmt in &mut block.stmts {
        let key = std::ptr::from_ref(&*stmt) as usize;
        match stmt {
            Stmt::AssertEq { expr, expected } => match observed.get(&key) {
                Some(Some(value)) if literal_of(value).is_some() => *expected = literal_of(value),
                // Values that differ between executions: `expr == expr` would
                // run the expression twice, so discard it instead.
                Some(_) => {
                    let value = expr.clone();
                    *stmt = Stmt::Let {
                        name: "_".into(),
                        annot: None,
                        value,
                        mutable: false,
                    };
                }
                // Never executed: `expr == expr` is a well-typed dead assertion.
                None => {}
            },
            Stmt::If {
                then, otherwise, ..
            } => {
                fill_asserts(then, observed);
                if let Some(otherwise) = otherwise {
                    fill_asserts(otherwise, observed);
                }
            }
            Stmt::For { body, .. }
            | Stmt::While { body, .. }
            | Stmt::Block(body)
            | Stmt::Defer(body) => fill_asserts(body, observed),
            Stmt::IfLet {
                then, otherwise, ..
            } => {
                fill_asserts(then, observed);
                if let Some(otherwise) = otherwise {
                    fill_asserts(otherwise, observed);
                }
            }
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    fill_asserts(body, observed);
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone, Debug)]
struct Local {
    name: String,
    ty: Ty,
    mutable: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Main,
    Pure,
    Proc,
    Stream,
}

#[derive(Clone, Copy, Debug)]
struct Ctx {
    kind: Kind,
    /// Which Result families `?` may propagate here.
    prop: Option<Fam>,
    in_loop: bool,
    /// `return Err(...) when ...` is allowed.
    return_err: bool,
    /// Statements may assert and observe (`out += [...]`).
    observe: bool,
}

struct Gen<'c> {
    rng: Rng,
    config: &'c GenConfig,
    records: Vec<RecDecl>,
    enums: Vec<EnumDecl>,
    functions: Vec<FnDecl>,
    scopes: Vec<Vec<Local>>,
    ctx: Ctx,
    next: usize,
    nesting: usize,
    /// Top-level constants, visible in every body.
    globals: Vec<Local>,
    consts: Vec<(String, Ty, Expr)>,
    /// Callable aliases declared at the entry body's top level.
    aliases: Vec<(String, usize)>,
    /// The item type while generating a stream producer body.
    yield_ty: Option<Ty>,
    /// Value blocks being generated; assertion holes are filled only in
    /// statement blocks, so none are generated inside expressions.
    value_depth: usize,
}

const ALPHABET: &[&str] = &[
    "a", "b", "c", "x", "y", "z", "A", "Q", "0", "7", " ", "-", "_", ".", ",", ":", "/", "é", "ß",
    "🦀", "ab", "xy",
];

fn fam_allows(prop: Option<Fam>, fam: Fam) -> bool {
    match (prop, fam) {
        (None, _) => false,
        (Some(Fam::Error), _) => true,
        (Some(Fam::Fz), Fam::Fz) => true,
        (Some(Fam::Fz), Fam::Error) => false,
    }
}

/// Values of this type cannot be written as a literal without an expected
/// type (`null`, `Ok(...)`, `[]` of Optionals, ...).
fn needs_cx(ty: &Ty) -> bool {
    match ty {
        Ty::Opt(_) | Ty::Res(..) => true,
        Ty::List(element) => needs_cx(element),
        Ty::Map(_, value) => needs_cx(value),
        _ => false,
    }
}

/// The printed expression starts with `-`. A newline before a binary
/// operator continues the previous line, so a statement or tail must not
/// start with one.
fn starts_with_minus(expr: &Expr) -> bool {
    match expr {
        Expr::Neg(_) => true,
        Expr::Int(value) => *value < 0,
        Expr::Float(value) => value.is_sign_negative(),
        Expr::Binary(_, left, _) | Expr::Fallback(left, _) | Expr::FallbackBlock(left, ..) => {
            starts_with_minus(left)
        }
        Expr::Method { recv, .. }
        | Expr::OptMethod { recv, .. }
        | Expr::Field(recv, _)
        | Expr::Index(recv, _)
        | Expr::Slice(recv, ..)
        | Expr::Propagate(recv) => starts_with_minus(recv),
        _ => false,
    }
}

/// The printed expression starts with a computed-key or spread brace.
fn starts_with_brace(expr: &Expr) -> bool {
    match expr {
        Expr::MapComp { .. } | Expr::RecUpdate { .. } | Expr::BlockValue(_) => true,
        Expr::MapLit(entries) => !entries.is_empty(),
        Expr::Binary(_, left, _) | Expr::Fallback(left, _) | Expr::FallbackBlock(left, ..) => {
            starts_with_brace(left)
        }
        Expr::Method { recv, .. }
        | Expr::OptMethod { recv, .. }
        | Expr::Field(recv, _)
        | Expr::Index(recv, _)
        | Expr::Slice(recv, ..)
        | Expr::Propagate(recv) => starts_with_brace(recv),
        _ => false,
    }
}

/// A statement-position `match` (a block tail, or an arm of one) reads arm
/// bodies starting with a computed key or a spread as blocks, so such a
/// match cannot be a block tail. Parser defect, reported separately.
fn brace_arm_match(expr: &Expr) -> bool {
    match expr {
        Expr::Match(_, arms) => arms
            .iter()
            .any(|(_, body)| starts_with_brace(body) || brace_arm_match(body)),
        _ => false,
    }
}

/// A statement starting with a field path followed by `not in` or `is`
/// parses as a command. Parser defect, reported separately.
fn field_not_in(expr: &Expr) -> bool {
    let field_path = |expr: &Expr| {
        matches!(expr, Expr::Field(..)) || matches!(expr, Expr::Var(name) if name.contains('.'))
    };
    match expr {
        Expr::Binary(BinOp::NotIn, left, _) | Expr::Is(left, _) => field_path(left),
        Expr::Binary(_, left, _) | Expr::Fallback(left, _) => field_not_in(left),
        _ => false,
    }
}

impl<'c> Gen<'c> {
    fn new(rng: Rng, config: &'c GenConfig) -> Self {
        Self {
            rng,
            config,
            records: Vec::new(),
            enums: Vec::new(),
            functions: Vec::new(),
            scopes: Vec::new(),
            ctx: Ctx {
                kind: Kind::Main,
                prop: None,
                in_loop: false,
                return_err: false,
                observe: false,
            },
            next: 0,
            nesting: 0,
            globals: Vec::new(),
            consts: Vec::new(),
            aliases: Vec::new(),
            yield_ty: None,
            value_depth: 0,
        }
    }

    fn fresh(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}{}", self.next)
    }

    fn program(mut self) -> Program {
        let records = self.rng.below(self.config.max_records + 1);
        for index in 0..records {
            let count = 1 + self.rng.below(3);
            let mut fields = Vec::new();
            for field in 0..count {
                let ty = self.field_ty(index);
                let name = if self.rng.chance(15) {
                    ["type", "in", "match"][field % 3].to_string()
                } else {
                    format!("f{field}")
                };
                if fields.iter().any(|(existing, _)| *existing == name) {
                    fields.push((format!("f{field}"), ty));
                } else {
                    fields.push((name, ty));
                }
            }
            self.records.push(RecDecl {
                name: format!("R{index}"),
                fields,
            });
        }
        let enums = self.rng.below(self.config.max_enums + 1);
        for index in 0..enums {
            let count = 2 + self.rng.below(3);
            let mut variants = Vec::new();
            for variant in 0..count {
                let payload = match self.rng.below(4) {
                    0 | 1 => Vec::new(),
                    2 => vec![self.scalar_ty()],
                    _ => vec![self.scalar_ty(), self.scalar_ty()],
                };
                variants.push((format!("E{index}V{variant}"), payload));
            }
            self.enums.push(EnumDecl {
                name: format!("E{index}"),
                variants,
            });
        }
        for index in 0..self.rng.below(3) {
            let ty = self
                .rng
                .pick(&[
                    Ty::Int,
                    Ty::Str,
                    Ty::Bool,
                    Ty::Float,
                    Ty::Path,
                    Ty::Duration,
                ])
                .clone();
            let value = self.literal(&ty, true, 0);
            let name = format!("C{index}");
            self.consts.push((name.clone(), ty.clone(), value));
            self.globals.push(Local {
                name,
                ty,
                mutable: false,
            });
        }
        let functions = self.rng.below(self.config.max_functions + 1);
        for index in 0..functions {
            let function = self.function(index);
            self.functions.push(function);
        }
        self.ctx = Ctx {
            kind: Kind::Main,
            prop: Some(Fam::Error),
            in_loop: false,
            return_err: false,
            observe: true,
        };
        self.scopes = vec![self.globals.clone(), Vec::new()];
        let count = 4 + self.rng.below(self.config.max_body_stmts);
        let mut stmts = Vec::new();
        for _ in 0..count {
            self.stmt(&mut stmts);
        }
        // Observe every top-level local at the end.
        let locals: Vec<Local> = self.scopes[1].clone();
        for local in locals {
            if self.rng.chance(60) {
                stmts.push(Stmt::Out(
                    self.render(&Expr::Var(local.name.clone()), &local.ty),
                ));
            }
        }
        self.scopes.clear();
        Program {
            consts: self.consts,
            records: self.records,
            enums: self.enums,
            functions: self.functions,
            body: Block { stmts, tail: None },
        }
    }

    fn scalar_ty(&mut self) -> Ty {
        match self.rng.below(10) {
            0..=3 => Ty::Int,
            4..=6 => Ty::Str,
            7 => Ty::Bool,
            8 => Ty::Float,
            _ => Ty::Path,
        }
    }

    fn field_ty(&mut self, record: usize) -> Ty {
        match self.rng.below(10) {
            0..=5 => self.scalar_ty(),
            6 => Ty::list(self.scalar_ty()),
            7 => Ty::opt(self.scalar_ty()),
            8 if record > 0 => Ty::Rec(self.rng.below(record)),
            _ => Ty::Bytes,
        }
    }

    fn ty(&mut self, depth: usize) -> Ty {
        let roll = self.rng.below(100);
        if depth >= 2 {
            return self.scalar_ty();
        }
        match roll {
            0..=24 => Ty::Int,
            25..=42 => Ty::Str,
            43..=50 => Ty::Bool,
            51..=55 => Ty::Float,
            56..=58 => Ty::Path,
            59..=60 => Ty::Bytes,
            61 => Ty::Duration,
            62..=73 => Ty::list(self.elem_ty(depth + 1)),
            74..=78 => {
                let key = if self.rng.chance(70) {
                    Ty::Str
                } else {
                    Ty::Int
                };
                Ty::map(key, self.elem_ty(depth + 1))
            }
            79..=84 if !self.records.is_empty() => Ty::Rec(self.rng.below(self.records.len())),
            85..=89 if !self.enums.is_empty() => Ty::Enum(self.rng.below(self.enums.len())),
            90..=94 => Ty::opt(self.scalar_ty()),
            95 if !self.records.is_empty() => Ty::opt(Ty::Rec(self.rng.below(self.records.len()))),
            96..=99 => {
                let fam = if self.rng.chance(50) {
                    Fam::Fz
                } else {
                    Fam::Error
                };
                Ty::res(self.elem_ty(depth + 1), fam)
            }
            _ => Ty::Int,
        }
    }

    /// Collection elements: no Result layers.
    fn elem_ty(&mut self, depth: usize) -> Ty {
        loop {
            let ty = self.ty(depth);
            if !matches!(ty, Ty::Res(..)) {
                return ty;
            }
        }
    }

    fn function(&mut self, index: usize) -> FnDecl {
        let kind = match self.rng.below(20) {
            0..=2 => FnKind::Stream,
            3..=9 => FnKind::Proc,
            _ => FnKind::Pure,
        };
        let mut params = Vec::new();
        let count = self.rng.below(4);
        let mut defaulted = false;
        for position in 0..count {
            let ty = self.ty(1);
            defaulted |= self.rng.chance(35);
            let name = format!("p{position}");
            if defaulted {
                let infer_type = !needs_cx(&ty) && self.rng.chance(30) && ty.is_scalar();
                let default = self.literal(&ty, !infer_type, 0);
                params.push(Param {
                    name,
                    ty,
                    default: Some(default),
                    infer_type,
                });
            } else {
                params.push(Param {
                    name,
                    ty,
                    default: None,
                    infer_type: false,
                });
            }
        }
        let tail = self.elem_ty(0);
        if kind == FnKind::Stream {
            return self.stream_function(index, params, tail);
        }
        // Annotated Result returns admit `?` and early `Err` returns.
        // An inferred tail must establish its type without an expected type.
        let result_fam = if self.rng.chance(40) || kind == FnKind::Proc && needs_cx(&tail) {
            Some(if self.rng.chance(50) {
                Fam::Fz
            } else {
                Fam::Error
            })
        } else {
            None
        };
        let (ret, annotate, prop, return_err) = match (kind, result_fam) {
            (FnKind::Pure, None) => (
                tail.clone(),
                needs_cx(&tail) || self.rng.chance(30),
                None,
                false,
            ),
            (FnKind::Proc | FnKind::Stream, None) => (
                Ty::res(tail.clone(), Fam::Error),
                false,
                Some(Fam::Error),
                false,
            ),
            (_, Some(fam)) => (Ty::res(tail.clone(), fam), true, Some(fam), true),
        };
        self.ctx = Ctx {
            kind: if kind == FnKind::Pure {
                Kind::Pure
            } else {
                Kind::Proc
            },
            prop,
            in_loop: false,
            return_err,
            observe: false,
        };
        self.scopes = vec![
            self.globals.clone(),
            params
                .iter()
                .map(|param| Local {
                    name: param.name.clone(),
                    ty: param.ty.clone(),
                    mutable: false,
                })
                .collect(),
        ];
        let mut stmts = Vec::new();
        let count = self.rng.below(self.config.max_block_stmts + 1);
        for _ in 0..count {
            self.stmt(&mut stmts);
        }
        let tail_expr = self.tail_expr(&tail, annotate, 0);
        self.scopes.clear();
        FnDecl {
            name: format!("fn{index}"),
            kind,
            params,
            ret,
            tail,
            annotate,
            body: Block {
                stmts,
                tail: Some(tail_expr),
            },
        }
    }

    /// `stream fnN(params) [] -> Stream[T] { ... yield ... }`
    fn stream_function(&mut self, index: usize, params: Vec<Param>, item: Ty) -> FnDecl {
        self.ctx = Ctx {
            kind: Kind::Stream,
            prop: None,
            in_loop: false,
            return_err: false,
            observe: false,
        };
        self.yield_ty = Some(item.clone());
        self.scopes = vec![
            self.globals.clone(),
            params
                .iter()
                .map(|param| Local {
                    name: param.name.clone(),
                    ty: param.ty.clone(),
                    mutable: false,
                })
                .collect(),
        ];
        let mut stmts = Vec::new();
        for _ in 0..1 + self.rng.below(self.config.max_block_stmts) {
            self.stmt(&mut stmts);
        }
        let value = self.expr_or_literal(&item, true, 1);
        stmts.push(Stmt::Yield(value));
        self.scopes.clear();
        self.yield_ty = None;
        FnDecl {
            name: format!("fn{index}"),
            kind: FnKind::Stream,
            params,
            ret: Ty::list(item.clone()),
            tail: item,
            annotate: true,
            body: Block { stmts, tail: None },
        }
    }

    /// A call of a stream producer yielding `item` (any item type when
    /// `None`), for an iterable position. Pure bodies cannot call producers.
    fn stream_call(&mut self, item: Option<&Ty>, depth: usize) -> Option<(Expr, Ty)> {
        if !matches!(self.ctx.kind, Kind::Main | Kind::Proc) {
            return None;
        }
        let candidates: Vec<usize> = self
            .functions
            .iter()
            .enumerate()
            .filter(|(_, function)| {
                function.kind == FnKind::Stream && item.is_none_or(|item| function.tail == *item)
            })
            .map(|(index, _)| index)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let index = *self.rng.pick(&candidates);
        let item = self.functions[index].tail.clone();
        Some((self.call(index, depth), item))
    }

    /// Visible locals: an inner entry (a narrowed Optional) hides an outer
    /// one with the same name.
    fn locals(&self) -> impl Iterator<Item = &Local> {
        let mut seen = rustc_hash::FxHashSet::default();
        let mut visible = Vec::new();
        for local in self
            .scopes
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
        {
            if seen.insert(local.name.as_str()) {
                visible.push(local);
            }
        }
        visible.into_iter()
    }

    fn bind(&mut self, name: String, ty: Ty, mutable: bool) {
        self.scopes
            .last_mut()
            .expect("scope")
            .push(Local { name, ty, mutable });
    }

    fn var_of(&mut self, ty: &Ty) -> Option<Expr> {
        let found: Vec<String> = self
            .locals()
            .filter(|local| local.ty == *ty)
            .map(|local| local.name.clone())
            .collect();
        if found.is_empty() {
            return None;
        }
        Some(Expr::Var(self.rng.pick(&found).clone()))
    }

    fn field_of(&mut self, ty: &Ty) -> Option<Expr> {
        let mut found = Vec::new();
        for local in self.locals() {
            if let Ty::Rec(index) = local.ty {
                for (field, field_ty) in &self.records[index].fields {
                    if field_ty == ty {
                        found.push((local.name.clone(), field.clone()));
                    }
                }
            }
        }
        if found.is_empty() {
            return None;
        }
        let (name, field) = self.rng.pick(&found).clone();
        Some(Expr::Field(Box::new(Expr::Var(name)), field))
    }

    fn callable(&self, index: usize) -> bool {
        // Pure functions and producers call only pure functions; producers are
        // called only in iterable positions.
        match self.functions[index].kind {
            FnKind::Stream => false,
            FnKind::Proc => !matches!(self.ctx.kind, Kind::Pure | Kind::Stream),
            FnKind::Pure => true,
        }
    }

    /// A call whose result has type `ty`, directly or through `?`/`??`.
    fn call_of(&mut self, ty: &Ty, depth: usize) -> Option<Expr> {
        let mut candidates = Vec::new();
        for (index, function) in self.functions.iter().enumerate() {
            if !self.callable(index) {
                continue;
            }
            if function.ret == *ty {
                candidates.push((index, 0));
            } else if let Ty::Res(inner, fam) = &function.ret
                && **inner == *ty
            {
                candidates.push((
                    index,
                    if fam_allows(self.ctx.prop, *fam) && self.rng.chance(50) {
                        1
                    } else {
                        2
                    },
                ));
            }
        }
        if candidates.is_empty() {
            return None;
        }
        let (index, wrap) = *self.rng.pick(&candidates);
        let call = match self.aliases.iter().find(|(_, func)| *func == index) {
            Some((alias, _)) if self.ctx.kind == Kind::Main && self.rng.chance(50) => {
                let alias = alias.clone();
                let Expr::Call { args, .. } = self.call(index, depth) else {
                    unreachable!()
                };
                Expr::AliasCall {
                    alias,
                    func: index,
                    args,
                }
            }
            _ => self.call(index, depth),
        };
        Some(match wrap {
            0 => call,
            1 => Expr::Propagate(Box::new(call)),
            _ => {
                let fallback = self.expr_or_literal(ty, true, depth + 1);
                Expr::Fallback(Box::new(call), Box::new(fallback))
            }
        })
    }

    fn call(&mut self, index: usize, depth: usize) -> Expr {
        let params = self.functions[index].params.clone();
        let required = params
            .iter()
            .filter(|param| param.default.is_none())
            .count();
        let supplied: Vec<&Param> = params
            .iter()
            .enumerate()
            .filter(|(position, _)| *position < required || self.rng.chance(50))
            .map(|(_, param)| param)
            .collect();
        let style = self.rng.below(4);
        let mut args = Vec::new();
        match style {
            0 => {
                // Positional prefix, named remainder in shuffled order.
                let positional = supplied
                    .iter()
                    .enumerate()
                    .take_while(|(position, param)| params[*position].name == param.name)
                    .count();
                let split = self.rng.below(positional + 1);
                let mut named = Vec::new();
                for (position, param) in supplied.iter().enumerate() {
                    let value = self.expr_or_literal(&param.ty, true, depth + 1);
                    if position < split {
                        args.push(Arg::Pos(value));
                    } else {
                        named.push(Arg::Named(param.name.clone(), value));
                    }
                }
                self.rng.shuffle(&mut named);
                args.extend(named);
            }
            1 => {
                // A record spread supplies the trailing parameters by name.
                let mut fields = Vec::new();
                // Spread fields have no expected type, so contextual values
                // stay named arguments.
                for param in &supplied {
                    if needs_cx(&param.ty) || self.rng.chance(35) {
                        let value = self.expr_or_literal(&param.ty, true, depth + 1);
                        args.push(Arg::Named(param.name.clone(), value));
                    } else {
                        let value = self.expr_or_literal(&param.ty, false, depth + 1);
                        fields.push((param.name.clone(), value));
                    }
                }
                if !fields.is_empty() {
                    args.push(Arg::Spread(fields));
                }
            }
            _ => {
                // Positional in declaration order; omitted defaults must be a suffix.
                let mut previous = true;
                for (position, param) in params.iter().enumerate() {
                    let wanted = supplied.iter().any(|supplied| supplied.name == param.name);
                    if wanted && previous {
                        let value = self.expr_or_literal(&param.ty, true, depth + 1);
                        args.push(Arg::Pos(value));
                    } else if wanted {
                        let value = self.expr_or_literal(&param.ty, true, depth + 1);
                        args.push(Arg::Named(param.name.clone(), value));
                    } else if position >= required {
                        previous = false;
                    }
                }
            }
        }
        Expr::Call { func: index, args }
    }

    fn string(&mut self) -> String {
        let len = self.rng.below(5);
        let mut text = String::new();
        for _ in 0..len {
            text.push_str(self.rng.pick(ALPHABET));
        }
        text
    }

    /// F-string text that also exercises brace escapes, `$`, and `#`.
    fn fmt_text(&mut self) -> String {
        let len = self.rng.below(5);
        let mut text = String::new();
        for _ in 0..len {
            if self.rng.chance(30) {
                text.push_str(self.rng.pick(&["{", "}", "$", "$5", "#", "\"", "${", "}{"]));
            } else {
                text.push_str(self.rng.pick(ALPHABET));
            }
        }
        text
    }

    fn path_text(&mut self) -> String {
        let segments = 1 + self.rng.below(3);
        let mut parts = Vec::new();
        for _ in 0..segments {
            parts.push(["a", "bin", "x.txt", "lib", "..", "."][self.rng.below(6)]);
        }
        let joined = parts.join("/");
        if self.rng.chance(30) {
            format!("/{joined}")
        } else {
            joined
        }
    }

    /// A literal of `ty`. `cx` says an expected type is available, which
    /// admits `null` and empty collections.
    fn literal(&mut self, ty: &Ty, cx: bool, depth: usize) -> Expr {
        match ty {
            Ty::Int => Expr::Int(if self.rng.chance(85) {
                self.rng.range(-9, 40)
            } else {
                self.rng.range(-100_000, 100_000)
            }),
            Ty::Float => Expr::Float(
                *self
                    .rng
                    .pick(&[0.5, 1.25, -2.0, 3.0, 0.1, 100.0, 0.001, -7.75, 2.5e3]),
            ),
            Ty::Str => Expr::Str(self.string()),
            Ty::Bool => Expr::Bool(self.rng.chance(50)),
            Ty::Path => Expr::Path(self.path_text()),
            Ty::Duration => Expr::Duration(*self.rng.pick(&[0, 1, 250, 1000, 1500, 60_000])),
            Ty::Bytes => {
                let len = self.rng.below(5);
                Expr::Bytes(
                    (0..len)
                        .map(|_| *self.rng.pick(&[b'a', b'Z', b'0', b' ', 0, 255, 10, 128]))
                        .collect(),
                )
            }
            Ty::List(element) => {
                let min = usize::from(!cx || depth > 2);
                let len = min + self.rng.below(3);
                let items = (0..len)
                    .map(|_| Elem::Item(self.literal(element, cx, depth + 1)))
                    .collect();
                Expr::List(items)
            }
            Ty::Map(key, value) => {
                let min = usize::from(!cx || depth > 2);
                let len = min + self.rng.below(3);
                let entries = (0..len)
                    .map(|_| {
                        (
                            self.literal(key, false, depth + 1),
                            self.literal(value, cx, depth + 1),
                        )
                    })
                    .collect();
                Expr::MapLit(entries)
            }
            Ty::Rec(index) => {
                let mut fields: Vec<(String, Expr)> = self.records[*index]
                    .fields
                    .clone()
                    .iter()
                    .map(|(name, field_ty)| (name.clone(), self.literal(field_ty, true, depth + 1)))
                    .collect();
                self.rng.shuffle(&mut fields);
                Expr::RecCtor {
                    rec: *index,
                    fields,
                }
            }
            Ty::Enum(index) => {
                let variant = self.rng.below(self.enums[*index].variants.len());
                let payload = self.enums[*index].variants[variant].1.clone();
                let args = payload
                    .iter()
                    .map(|ty| self.literal(ty, true, depth + 1))
                    .collect();
                Expr::Variant {
                    en: *index,
                    variant,
                    args,
                }
            }
            Ty::Opt(inner) => {
                if self.rng.chance(35) {
                    Expr::Null
                } else {
                    self.literal(inner, true, depth + 1)
                }
            }
            Ty::Res(inner, _) => {
                if self.rng.chance(30) {
                    Expr::Err(self.string())
                } else {
                    Expr::Ok(Box::new(self.literal(inner, true, depth + 1)))
                }
            }
        }
    }

    /// A block or function tail.
    fn tail_expr(&mut self, ty: &Ty, cx: bool, depth: usize) -> Expr {
        for _ in 0..6 {
            let expr = self.expr_or_literal(ty, cx, depth);
            if !starts_with_minus(&expr) && !brace_arm_match(&expr) && !field_not_in(&expr) {
                return expr;
            }
        }
        match self.literal(ty, true, depth) {
            Expr::Int(value) => Expr::Int(value.saturating_abs()),
            Expr::Float(value) => Expr::Float(value.abs()),
            other => other,
        }
    }

    fn expr_or_literal(&mut self, ty: &Ty, cx: bool, depth: usize) -> Expr {
        match self.expr(ty, cx, depth) {
            Some(expr) => expr,
            None => self.literal(ty, true, depth),
        }
    }

    /// An expression of exactly `ty`. Without `cx` (an expected type) the
    /// expression must establish `ty` on its own; `None` when no production
    /// can.
    fn expr(&mut self, ty: &Ty, cx: bool, depth: usize) -> Option<Expr> {
        if depth >= self.config.max_expr_depth || self.rng.chance(15 + 20 * depth as u32) {
            if let Some(var) = self.var_of(ty)
                && self.rng.chance(70)
            {
                return Some(var);
            }
            if cx || !needs_cx(ty) {
                return Some(self.literal(ty, cx, depth));
            }
            return self.var_of(ty).or_else(|| self.field_of(ty));
        }
        for _ in 0..8 {
            if let Some(expr) = self.production(ty, cx, depth) {
                return Some(expr);
            }
        }
        if cx || !needs_cx(ty) {
            Some(self.literal(ty, cx, depth))
        } else {
            self.var_of(ty).or_else(|| self.field_of(ty))
        }
    }

    fn production(&mut self, ty: &Ty, cx: bool, depth: usize) -> Option<Expr> {
        let next = depth + 1;
        match self.rng.below(20) {
            0 => self.var_of(ty),
            1 => self.field_of(ty),
            2 => self.call_of(ty, depth),
            3 if cx || !needs_cx(ty) => {
                let cond = self.expr_or_literal(&Ty::Bool, false, next);
                let then = self.value_block(ty, cx, next);
                let otherwise = self.value_block(ty, cx, next);
                Some(Expr::If(
                    Box::new(cond),
                    Box::new(then),
                    Box::new(otherwise),
                ))
            }
            4 if cx || !needs_cx(ty) => self.match_expr(ty, cx, next),
            5 => self.fallback(ty, cx, next),
            6 => {
                let fam = if self.ctx.prop == Some(Fam::Fz) {
                    Fam::Fz
                } else {
                    *self.rng.pick(&[Fam::Fz, Fam::Error])
                };
                if !fam_allows(self.ctx.prop, fam) {
                    return None;
                }
                let source = self.expr(&Ty::res(ty.clone(), fam), false, next)?;
                Some(Expr::Propagate(Box::new(source)))
            }
            7 => self.pipeline_of(ty, next),
            8 => self.method_of(ty, next),
            10 if (cx || !needs_cx(ty))
                && !matches!(ty, Ty::Res(..))
                && self.nesting < 2
                && self.rng.chance(40) =>
            {
                let block = self.value_block(ty, cx, next);
                Some(Expr::Ctx(self.string(), Box::new(block)))
            }
            9 => {
                // An element read: `list.get(i) ?? fallback`.
                if needs_cx(ty) && !cx {
                    return None;
                }
                let list = self.expr(&Ty::list(ty.clone()), false, next)?;
                let index = self.small_int(next);
                let fallback = self.expr_or_literal(ty, true, next);
                Some(Expr::Fallback(
                    Box::new(Expr::Method {
                        recv: Box::new(list),
                        name: "get",
                        args: vec![index],
                    }),
                    Box::new(fallback),
                ))
            }
            _ => self.typed_production(ty, cx, next),
        }
    }

    fn small_int(&mut self, depth: usize) -> Expr {
        if self.rng.chance(60) {
            Expr::Int(self.rng.range(-1, 3))
        } else {
            self.expr_or_literal(&Ty::Int, false, depth + 1)
        }
    }

    fn value_block(&mut self, ty: &Ty, cx: bool, depth: usize) -> Block {
        self.value_depth += 1;
        let block = self.value_block_inner(ty, cx, depth);
        self.value_depth -= 1;
        block
    }

    fn value_block_inner(&mut self, ty: &Ty, cx: bool, depth: usize) -> Block {
        self.scopes.push(Vec::new());
        let mut stmts = Vec::new();
        if self.nesting < 2 && depth <= 2 && self.rng.chance(15) {
            self.nesting += 1;
            let count = 1 + self.rng.below(2);
            for _ in 0..count {
                self.stmt(&mut stmts);
            }
            self.nesting -= 1;
        }
        let tail = self.tail_expr(ty, cx, depth);
        self.scopes.pop();
        Block {
            stmts,
            tail: Some(tail),
        }
    }

    /// A value block whose first statement is a `let`, so its braces never
    /// read as a record literal.
    fn value_block_with_let(&mut self, ty: &Ty, cx: bool, depth: usize) -> Block {
        self.value_depth += 1;
        self.scopes.push(Vec::new());
        self.nesting += 1;
        let mut stmts = Vec::new();
        self.let_stmt(&mut stmts, false);
        let count = self.rng.below(2);
        for _ in 0..count {
            self.stmt(&mut stmts);
        }
        self.nesting -= 1;
        let tail = self.tail_expr(ty, cx, depth);
        self.scopes.pop();
        self.value_depth -= 1;
        Block {
            stmts,
            tail: Some(tail),
        }
    }

    fn match_expr(&mut self, ty: &Ty, cx: bool, depth: usize) -> Option<Expr> {
        match self.rng.below(4) {
            0 if !self.enums.is_empty() => {
                let en = self.rng.below(self.enums.len());
                let subject = self.expr(&Ty::Enum(en), false, depth)?;
                let variants = self.enums[en].variants.clone();
                let wildcard = self.rng.chance(30);
                let mut arms = Vec::new();
                for (index, (name, payload)) in variants.iter().enumerate() {
                    if wildcard && index + 1 == variants.len() {
                        break;
                    }
                    self.scopes.push(Vec::new());
                    let binds: Vec<String> = payload
                        .iter()
                        .map(|payload_ty| {
                            let bind = self.fresh("m");
                            self.bind(bind.clone(), payload_ty.clone(), false);
                            bind
                        })
                        .collect();
                    let body = self.expr_or_literal(ty, cx, depth + 1);
                    self.scopes.pop();
                    arms.push((
                        Pat::Variant {
                            name: name.clone(),
                            binds,
                        },
                        body,
                    ));
                }
                if wildcard {
                    let body = self.expr_or_literal(ty, cx, depth + 1);
                    arms.push((Pat::Wild, body));
                }
                Some(Expr::Match(Box::new(subject), arms))
            }
            1 if self.rng.chance(40) => {
                let element = self.elem_ty(1);
                let subject = self.expr(&Ty::list(element.clone()), false, depth)?;
                let head = self.fresh("m");
                let rest = self.fresh("m");
                let empty = self.expr_or_literal(ty, cx, depth + 1);
                self.scopes.push(vec![
                    Local {
                        name: head.clone(),
                        ty: element.clone(),
                        mutable: false,
                    },
                    Local {
                        name: rest.clone(),
                        ty: Ty::list(element),
                        mutable: false,
                    },
                ]);
                let cons = self.expr_or_literal(ty, cx, depth + 1);
                self.scopes.pop();
                let arms = if self.rng.chance(50) {
                    vec![
                        (Pat::ListEmpty, empty),
                        (Pat::ListCons { head, rest }, cons),
                    ]
                } else {
                    vec![
                        (Pat::ListCons { head, rest }, cons),
                        (Pat::ListEmpty, empty),
                    ]
                };
                Some(Expr::Match(Box::new(subject), arms))
            }
            1 => {
                let subject = self.expr_or_literal(&Ty::Bool, false, depth);
                let first = self.rng.chance(50);
                let a = self.expr_or_literal(ty, cx, depth + 1);
                let b = self.expr_or_literal(ty, cx, depth + 1);
                Some(Expr::Match(
                    Box::new(subject),
                    vec![(Pat::Bool(first), a), (Pat::Bool(!first), b)],
                ))
            }
            2 => {
                let subject = self.expr_or_literal(&Ty::Int, false, depth);
                let mut arms = Vec::new();
                for _ in 0..1 + self.rng.below(2) {
                    let body = self.expr_or_literal(ty, cx, depth + 1);
                    arms.push((Pat::Int(self.rng.range(0, 4)), body));
                }
                let body = self.expr_or_literal(ty, cx, depth + 1);
                if self.rng.chance(50) {
                    arms.push((Pat::Wild, body));
                } else {
                    let bind = self.fresh("m");
                    self.scopes.push(vec![Local {
                        name: bind.clone(),
                        ty: Ty::Int,
                        mutable: false,
                    }]);
                    let body = self.expr_or_literal(ty, cx, depth + 1);
                    self.scopes.pop();
                    arms.push((Pat::Bind(bind), body));
                }
                Some(Expr::Match(Box::new(subject), arms))
            }
            _ => {
                let inner = self.elem_ty(1);
                let fam = *self.rng.pick(&[Fam::Fz, Fam::Error]);
                let subject = self.expr(&Ty::res(inner.clone(), fam), false, depth)?;
                let ok = self.fresh("m");
                let err = self.fresh("m");
                self.scopes.push(vec![Local {
                    name: ok.clone(),
                    ty: inner,
                    mutable: false,
                }]);
                let ok_body = self.expr_or_literal(ty, cx, depth + 1);
                self.scopes.pop();
                let err_body = if *ty == Ty::Str && self.rng.chance(50) {
                    Expr::Field(Box::new(Expr::Var(err.clone())), "message".into())
                } else {
                    self.expr_or_literal(ty, cx, depth + 1)
                };
                let arms = if self.rng.chance(50) {
                    vec![(Pat::Ok(ok), ok_body), (Pat::Err(err), err_body)]
                } else {
                    vec![(Pat::Err(err), err_body), (Pat::Ok(ok), ok_body)]
                };
                Some(Expr::Match(Box::new(subject), arms))
            }
        }
    }

    fn fallback(&mut self, ty: &Ty, cx: bool, depth: usize) -> Option<Expr> {
        let _ = cx;
        match self.rng.below(3) {
            0 if ty.is_scalar() => {
                let source = self.expr(&Ty::opt(ty.clone()), false, depth)?;
                let fallback = self.expr_or_literal(ty, true, depth);
                Some(Expr::Fallback(Box::new(source), Box::new(fallback)))
            }
            1 => {
                let fam = *self.rng.pick(&[Fam::Fz, Fam::Error]);
                let source = self.expr(&Ty::res(ty.clone(), fam), false, depth)?;
                let fallback = self.expr_or_literal(ty, true, depth);
                Some(Expr::Fallback(Box::new(source), Box::new(fallback)))
            }
            _ => {
                let fam = *self.rng.pick(&[Fam::Fz, Fam::Error]);
                let source = self.expr(&Ty::res(ty.clone(), fam), false, depth)?;
                let name = self.fresh("e");
                self.scopes.push(Vec::new());
                let tail = if *ty == Ty::Str && self.rng.chance(50) {
                    Expr::Field(Box::new(Expr::Var(name.clone())), "message".into())
                } else {
                    self.tail_expr(ty, true, depth)
                };
                self.scopes.pop();
                Some(Expr::FallbackBlock(
                    Box::new(source),
                    name,
                    Box::new(Block {
                        stmts: Vec::new(),
                        tail: Some(tail),
                    }),
                ))
            }
        }
    }

    fn shape_ty(shape: Shape, element: &Ty, key: &Ty, value: &Ty) -> Ty {
        match shape {
            Shape::Int => Ty::Int,
            Shape::Float => Ty::Float,
            Shape::Str => Ty::Str,
            Shape::Bool => Ty::Bool,
            Shape::Bytes => Ty::Bytes,
            Shape::T => element.clone(),
            Shape::K => key.clone(),
            Shape::V => value.clone(),
            Shape::ListT => Ty::list(element.clone()),
            Shape::ListK => Ty::list(key.clone()),
            Shape::ListV => Ty::list(value.clone()),
            Shape::ListStr => Ty::list(Ty::Str),
            Shape::MapKV => Ty::map(key.clone(), value.clone()),
            Shape::ResT => Ty::res(element.clone(), Fam::Error),
            Shape::ResV => Ty::res(value.clone(), Fam::Error),
        }
    }

    /// A registry method call producing `ty`.
    fn method_of(&mut self, ty: &Ty, depth: usize) -> Option<Expr> {
        let candidates: Vec<&OracleMethod> = ORACLE_METHODS.iter().collect();
        let entry = **self.rng.pick(&candidates);
        // Choose receiver parameters that make the result `ty`.
        let (element, key, value) = match ty {
            Ty::List(inner) => {
                let key = if matches!(**inner, Ty::Str | Ty::Int | Ty::Bool) {
                    (**inner).clone()
                } else {
                    Ty::Str
                };
                ((**inner).clone(), key, (**inner).clone())
            }
            Ty::Map(k, v) => (self.elem_ty(1), (**k).clone(), (**v).clone()),
            Ty::Res(inner, Fam::Error) => (
                (**inner).clone(),
                if self.rng.chance(70) {
                    Ty::Str
                } else {
                    Ty::Int
                },
                (**inner).clone(),
            ),
            other => {
                let element = if entry.name == "join" {
                    Ty::Str
                } else {
                    self.elem_ty(1)
                };
                (
                    element,
                    if self.rng.chance(70) {
                        Ty::Str
                    } else {
                        Ty::Int
                    },
                    other.clone(),
                )
            }
        };
        let element = if entry.name == "join" {
            Ty::Str
        } else {
            element
        };
        if Self::shape_ty(entry.ret, &element, &key, &value) != *ty {
            return None;
        }
        let recv_ty = match entry.receiver {
            MethodReceiver::Str => Ty::Str,
            MethodReceiver::Int => Ty::Int,
            MethodReceiver::Float => Ty::Float,
            MethodReceiver::Bytes => Ty::Bytes,
            MethodReceiver::Path => Ty::Path,
            MethodReceiver::List => Ty::list(element.clone()),
            MethodReceiver::Map => Ty::map(key.clone(), value.clone()),
            _ => return None,
        };
        // Arguments of element type need an expected type the receiver alone
        // may not provide; keep them context-free.
        let mut args = Vec::new();
        for shape in entry.params {
            let arg_ty = Self::shape_ty(*shape, &element, &key, &value);
            if needs_cx(&arg_ty) {
                return None;
            }
            let arg = if entry.name == "split" {
                Expr::Str((*self.rng.pick(&[",", "a", "-", " ", "xy", "é"])).to_string())
            } else {
                self.expr_or_literal(&arg_ty, false, depth + 1)
            };
            args.push(arg);
        }
        let recv = self.expr(&recv_ty, false, depth)?;
        Some(Expr::Method {
            recv: Box::new(recv),
            name: entry.name,
            args,
        })
    }

    fn typed_production(&mut self, ty: &Ty, cx: bool, depth: usize) -> Option<Expr> {
        let next = depth + 1;
        match ty {
            Ty::Int => Some(match self.rng.below(7) {
                0 | 1 => {
                    let op = *self.rng.pick(&[BinOp::Add, BinOp::Sub, BinOp::Mul]);
                    let left = self.expr_or_literal(&Ty::Int, false, next);
                    let right = self.expr_or_literal(&Ty::Int, false, next);
                    Expr::Binary(op, Box::new(left), Box::new(right))
                }
                2 => {
                    let op = *self.rng.pick(&[BinOp::Div, BinOp::Rem]);
                    let left = self.expr_or_literal(&Ty::Int, false, next);
                    let divisor = *self.rng.pick(&[1, 2, 3, 7, -2, -5, 10]);
                    Expr::Binary(op, Box::new(left), Box::new(Expr::Int(divisor)))
                }
                3 if self.rng.chance(70) => {
                    Expr::Neg(Box::new(self.expr_or_literal(&Ty::Int, false, next)))
                }
                3 => {
                    // An interval count of two Durations.
                    let left = self.expr_or_literal(&Ty::Duration, false, next);
                    let divisor = *self.rng.pick(&[1, 250, 1000]);
                    Expr::Binary(
                        BinOp::Div,
                        Box::new(left),
                        Box::new(Expr::Duration(divisor)),
                    )
                }
                4 => {
                    // Length of a comprehension.
                    let element = self.elem_ty(1);
                    let comp = self.comprehension(&element, next)?;
                    Expr::Method {
                        recv: Box::new(comp),
                        name: "len",
                        args: Vec::new(),
                    }
                }
                _ => return self.method_of(ty, next),
            }),
            Ty::Float => Some(match self.rng.below(4) {
                0 => {
                    let op = *self.rng.pick(&[BinOp::Add, BinOp::Sub, BinOp::Mul]);
                    let left = self.expr_or_literal(&Ty::Float, false, next);
                    let right = self.expr_or_literal(&Ty::Float, false, next);
                    Expr::Binary(op, Box::new(left), Box::new(right))
                }
                1 => {
                    let left = self.expr_or_literal(&Ty::Float, false, next);
                    Expr::Binary(
                        BinOp::Div,
                        Box::new(left),
                        Box::new(Expr::Float(*self.rng.pick(&[2.0, 0.5, -4.0, 10.0]))),
                    )
                }
                2 => Expr::Neg(Box::new(self.expr_or_literal(&Ty::Float, false, next))),
                _ => return self.method_of(ty, next),
            }),
            Ty::Str => Some(match self.rng.below(6) {
                0 => {
                    let left = self.expr_or_literal(&Ty::Str, false, next);
                    let right = self.expr_or_literal(&Ty::Str, false, next);
                    Expr::Binary(BinOp::Add, Box::new(left), Box::new(right))
                }
                1 | 2 => {
                    let mut parts = Vec::new();
                    for _ in 0..1 + self.rng.below(3) {
                        if self.rng.chance(40) {
                            parts.push(FmtPart::Lit(self.fmt_text()));
                        }
                        let display = self
                            .rng
                            .pick(&[Ty::Int, Ty::Str, Ty::Bool, Ty::Float, Ty::Path])
                            .clone();
                        let value = self.expr_or_literal(&display, false, next);
                        parts.push(if self.rng.chance(20) {
                            let align = *self.rng.pick(&['>', '<', '0']);
                            FmtPart::Width(value, align, 1 + self.rng.below(8))
                        } else {
                            FmtPart::Interp(value)
                        });
                    }
                    if self.rng.chance(30) {
                        parts.push(FmtPart::Lit(self.fmt_text()));
                    }
                    Expr::Fmt(parts)
                }
                3 => {
                    let base = self.expr_or_literal(&Ty::Str, false, next);
                    self.slice(base)
                }
                _ => return self.method_of(ty, next),
            }),
            Ty::Bool => Some(match self.rng.below(8) {
                0 | 1 => {
                    let operand = self.rng.pick(&[Ty::Int, Ty::Str, Ty::Float]).clone();
                    let op = *self.rng.pick(&[BinOp::Lt, BinOp::Le, BinOp::Gt, BinOp::Ge]);
                    let left = self.expr_or_literal(&operand, false, next);
                    let right = self.expr_or_literal(&operand, false, next);
                    Expr::Binary(op, Box::new(left), Box::new(right))
                }
                2 => {
                    let operand = self
                        .rng
                        .pick(&[
                            Ty::Int,
                            Ty::Str,
                            Ty::Bool,
                            Ty::Path,
                            Ty::Bytes,
                            Ty::list(Ty::Int),
                        ])
                        .clone();
                    let op = *self.rng.pick(&[BinOp::Eq, BinOp::Ne]);
                    let left = self.expr_or_literal(&operand, false, next);
                    let right = self.expr_or_literal(&operand, false, next);
                    Expr::Binary(op, Box::new(left), Box::new(right))
                }
                3 => {
                    let op = *self.rng.pick(&[BinOp::And, BinOp::Or]);
                    let left = self.expr_or_literal(&Ty::Bool, false, next);
                    let right = self.expr_or_literal(&Ty::Bool, false, next);
                    Expr::Binary(op, Box::new(left), Box::new(right))
                }
                4 => Expr::Not(Box::new(self.expr_or_literal(&Ty::Bool, false, next))),
                5 => {
                    let op = if self.rng.chance(70) {
                        BinOp::In
                    } else {
                        BinOp::NotIn
                    };
                    match self.rng.below(3) {
                        0 => {
                            let element = self.rng.pick(&[Ty::Int, Ty::Str]).clone();
                            let needle = self.expr_or_literal(&element, false, next);
                            let haystack = self.expr_or_literal(&Ty::list(element), false, next);
                            Expr::Binary(op, Box::new(needle), Box::new(haystack))
                        }
                        1 => {
                            let needle = self.expr_or_literal(&Ty::Str, false, next);
                            let haystack = self.expr_or_literal(&Ty::Str, false, next);
                            Expr::Binary(op, Box::new(needle), Box::new(haystack))
                        }
                        _ => {
                            let key = self.rng.pick(&[Ty::Int, Ty::Str]).clone();
                            let value = self.elem_ty(1);
                            let needle = self.expr_or_literal(&key, false, next);
                            let haystack = self.expr(&Ty::map(key, value), false, next)?;
                            Expr::Binary(op, Box::new(needle), Box::new(haystack))
                        }
                    }
                }
                6 if self.rng.chance(50) => {
                    // A non-binding pattern test.
                    if !self.enums.is_empty() && self.rng.chance(60) {
                        let en = self.rng.below(self.enums.len());
                        let subject = self.expr(&Ty::Enum(en), false, next)?;
                        let (name, payload) =
                            self.rng.pick(&self.enums[en].variants.clone()).clone();
                        Expr::Is(
                            Box::new(subject),
                            IsPat::Variant {
                                name,
                                arity: payload.len(),
                            },
                        )
                    } else {
                        let inner = self.elem_ty(1);
                        let fam = *self.rng.pick(&[Fam::Fz, Fam::Error]);
                        let subject = self.expr(&Ty::res(inner, fam), false, next)?;
                        Expr::Is(
                            Box::new(subject),
                            if self.rng.chance(50) {
                                IsPat::Ok
                            } else {
                                IsPat::Err
                            },
                        )
                    }
                }
                6 => {
                    let inner = self.scalar_ty();
                    let source = self.expr(&Ty::opt(inner), false, next)?;
                    // A presence test of a name narrows it in the checker; only the
                    // narrowing `if` statement tracks that, so test other sources here.
                    if matches!(source, Expr::Var(_) | Expr::Field(..)) {
                        return None;
                    }
                    let op = *self.rng.pick(&[BinOp::Eq, BinOp::Ne]);
                    Expr::Binary(op, Box::new(source), Box::new(Expr::Null))
                }
                _ => return self.method_of(ty, next),
            }),
            Ty::Bytes => {
                let base = self.expr_or_literal(&Ty::Bytes, false, next);
                Some(self.slice(base))
            }
            Ty::Duration => Some(match self.rng.below(4) {
                0 => {
                    let left = self.expr_or_literal(&Ty::Duration, false, next);
                    let right = self.expr_or_literal(&Ty::Duration, false, next);
                    Expr::Binary(BinOp::Add, Box::new(left), Box::new(right))
                }
                1 => {
                    let left = self.expr_or_literal(&Ty::Duration, false, next);
                    let right = Expr::Duration(*self.rng.pick(&[0, 1, 250]));
                    Expr::Binary(BinOp::Sub, Box::new(left), Box::new(right))
                }
                2 => {
                    let duration = self.expr_or_literal(&Ty::Duration, false, next);
                    let factor = Expr::Int(self.rng.range(0, 3));
                    if self.rng.chance(50) {
                        Expr::Binary(BinOp::Mul, Box::new(duration), Box::new(factor))
                    } else {
                        Expr::Binary(BinOp::Mul, Box::new(factor), Box::new(duration))
                    }
                }
                _ => {
                    let duration = self.expr_or_literal(&Ty::Duration, false, next);
                    Expr::Binary(
                        BinOp::Div,
                        Box::new(duration),
                        Box::new(Expr::Int(self.rng.range(1, 4))),
                    )
                }
            }),
            Ty::List(element) => match self.rng.below(6) {
                0 => {
                    let left = self.expr(ty, false, next)?;
                    let right = self.expr(ty, cx, next)?;
                    Some(Expr::Binary(BinOp::Add, Box::new(left), Box::new(right)))
                }
                1 | 2 => self.comprehension(element, next),
                3 if self.rng.chance(30) => {
                    let (call, _) = self.stream_call(Some(element), next)?;
                    Some(Expr::Method {
                        recv: Box::new(call),
                        name: "collect",
                        args: Vec::new(),
                    })
                }
                3 => {
                    let base = self.expr(ty, false, next)?;
                    Some(self.slice(base))
                }
                4 if !needs_cx(element) => {
                    let mut elems = Vec::new();
                    for _ in 0..1 + self.rng.below(3) {
                        if self.rng.chance(40) {
                            elems.push(Elem::Splice(self.expr(ty, false, next)?));
                        } else {
                            elems.push(Elem::Item(self.expr_or_literal(element, false, next)));
                        }
                    }
                    if elems.iter().all(|elem| matches!(elem, Elem::Splice(_))) {
                        elems.push(Elem::Item(self.literal(element, false, next)));
                    }
                    Some(Expr::List(elems))
                }
                _ => self.method_of(ty, next),
            },
            Ty::Map(key, value) => match self.rng.below(3) {
                0 if !needs_cx(value) => {
                    let element = self.elem_ty(1);
                    let iter = self.expr(&Ty::list(element.clone()), false, next)?;
                    let var = self.fresh("x");
                    self.scopes.push(vec![Local {
                        name: var.clone(),
                        ty: element,
                        mutable: false,
                    }]);
                    let key_expr = self.expr_or_literal(key, false, next);
                    let value_expr = self.expr_or_literal(value, false, next);
                    self.scopes.pop();
                    Some(Expr::MapComp {
                        key: Box::new(key_expr),
                        value: Box::new(value_expr),
                        var,
                        iter: Box::new(iter),
                    })
                }
                _ => self.method_of(ty, next),
            },
            Ty::Rec(index) => {
                let fields = self.records[*index].fields.clone();
                if self.rng.chance(50) {
                    let base = self.expr(ty, false, next)?;
                    let mut updates = Vec::new();
                    for (name, field_ty) in &fields {
                        if !needs_cx(field_ty) && (updates.is_empty() || self.rng.chance(30)) {
                            updates
                                .push((name.clone(), self.expr_or_literal(field_ty, false, next)));
                        }
                    }
                    if updates.is_empty() {
                        return None;
                    }
                    Some(Expr::RecUpdate {
                        base: Box::new(base),
                        updates,
                    })
                } else {
                    let mut values = Vec::new();
                    for (name, field_ty) in &fields {
                        values.push((name.clone(), self.expr_or_literal(field_ty, true, next)));
                    }
                    self.rng.shuffle(&mut values);
                    Some(Expr::RecCtor {
                        rec: *index,
                        fields: values,
                    })
                }
            }
            Ty::Enum(index) => {
                let variant = self.rng.below(self.enums[*index].variants.len());
                let payload = self.enums[*index].variants[variant].1.clone();
                let args = payload
                    .iter()
                    .map(|ty| self.expr_or_literal(ty, true, next))
                    .collect();
                Some(Expr::Variant {
                    en: *index,
                    variant,
                    args,
                })
            }
            Ty::Opt(inner) => {
                if cx {
                    return Some(if self.rng.chance(30) {
                        Expr::Null
                    } else {
                        self.expr_or_literal(inner, true, next)
                    });
                }
                // `opt?.method()` keeps the Optional layer.
                if **inner == Ty::Str {
                    let source = self.expr(&Ty::opt(Ty::Str), false, next)?;
                    let name = *self.rng.pick(&["upper", "lower", "trim", "reverse"]);
                    return Some(Expr::OptMethod {
                        recv: Box::new(source),
                        name,
                        args: Vec::new(),
                    });
                }
                if **inner == Ty::Int && self.rng.chance(50) {
                    let source = self.expr(&Ty::opt(Ty::Str), false, next)?;
                    let name = *self.rng.pick(&["count_chars", "byte_len"]);
                    return Some(Expr::OptMethod {
                        recv: Box::new(source),
                        name,
                        args: Vec::new(),
                    });
                }
                // `record?.field` on an Optional record.
                let candidates: Vec<(usize, String)> = self
                    .records
                    .iter()
                    .enumerate()
                    .flat_map(|(index, record)| {
                        record
                            .fields
                            .iter()
                            .filter(|(_, field_ty)| {
                                field_ty == &**inner || *field_ty == Ty::opt((**inner).clone())
                            })
                            .map(move |(name, _)| (index, name.clone()))
                    })
                    .collect();
                if candidates.is_empty() {
                    return None;
                }
                let (record, field) = self.rng.pick(&candidates).clone();
                let source = self.expr(&Ty::opt(Ty::Rec(record)), false, next)?;
                Some(Expr::OptField(Box::new(source), field))
            }
            Ty::Res(inner, fam) => {
                if !cx {
                    return None;
                }
                match self.rng.below(3) {
                    0 => Some(Expr::Ok(Box::new(self.expr_or_literal(inner, true, next)))),
                    1 => Some(Expr::Err(self.string())),
                    _ if *fam == Fam::Error && self.nesting < 2 => {
                        let saved = self.ctx;
                        self.ctx.prop = Some(Fam::Error);
                        self.ctx.in_loop = false;
                        self.ctx.return_err = false;
                        // An empty delay list makes one attempt and needs no time
                        // effect. `retry` passes no expected type into its block, so
                        // its values must establish their own type.
                        let retry = !needs_cx(inner) && self.rng.chance(30);
                        // A producer cannot yield from a retry attempt.
                        let yield_ty = if retry { self.yield_ty.take() } else { None };
                        let block = self.value_block(inner, !retry, next);
                        if retry {
                            self.yield_ty = yield_ty;
                        }
                        self.ctx = saved;
                        Some(if retry {
                            Expr::Retry(Box::new(block))
                        } else {
                            Expr::Try(Box::new(block))
                        })
                    }
                    _ => None,
                }
            }
            Ty::Path => None,
        }
    }

    fn slice(&mut self, base: Expr) -> Expr {
        let bound = |this: &mut Self| -> Option<Box<Expr>> {
            match this.rng.below(4) {
                0 => None,
                1 => Some(Box::new(Expr::Int(this.rng.range(-3, -1)))),
                _ => Some(Box::new(Expr::Int(this.rng.range(0, 4)))),
            }
        };
        let start = bound(self);
        let end = bound(self);
        Expr::Slice(Box::new(base), start, end)
    }

    fn comprehension(&mut self, element: &Ty, depth: usize) -> Option<Expr> {
        if needs_cx(element) {
            return None;
        }
        let (iter, var_ty) = match self.rng.below(6) {
            0 => (self.expr(&Ty::Str, false, depth)?, Ty::Str),
            1 => (self.expr(&Ty::Bytes, false, depth)?, Ty::Int),
            2 if self.rng.chance(50) => self.stream_call(None, depth)?,
            _ => {
                let item = self.elem_ty(1);
                (self.expr(&Ty::list(item.clone()), false, depth)?, item)
            }
        };
        let var = self.fresh("x");
        self.scopes.push(vec![Local {
            name: var.clone(),
            ty: var_ty,
            mutable: false,
        }]);
        let proj = self.expr_or_literal(element, false, depth + 1);
        let filter = if self.rng.chance(40) {
            Some(Box::new(self.expr_or_literal(&Ty::Bool, false, depth + 1)))
        } else {
            None
        };
        self.scopes.pop();
        Some(Expr::Comp {
            proj: Box::new(proj),
            var,
            iter: Box::new(iter),
            filter,
        })
    }

    /// A Str expression rendering the value of `expr` (a name or field path)
    /// canonically; the reference evaluator computes the same text.
    fn render(&mut self, expr: &Expr, ty: &Ty) -> Expr {
        let add =
            |left: Expr, right: Expr| Expr::Binary(BinOp::Add, Box::new(left), Box::new(right));
        let join = |comp: Expr, separator: &str| Expr::Method {
            recv: Box::new(comp),
            name: "join",
            args: vec![Expr::Str(separator.into())],
        };
        match ty {
            Ty::Int | Ty::Float | Ty::Bool | Ty::Path => {
                Expr::Fmt(vec![FmtPart::Interp(expr.clone())])
            }
            Ty::Duration => Expr::Fmt(vec![
                FmtPart::Interp(Expr::Binary(
                    BinOp::Div,
                    Box::new(expr.clone()),
                    Box::new(Expr::Duration(1)),
                )),
                FmtPart::Lit("ms".into()),
            ]),
            Ty::Str => add(
                add(Expr::Str("'".into()), expr.clone()),
                Expr::Str("'".into()),
            ),
            Ty::Bytes => {
                let var = self.fresh("r");
                let comp = Expr::Comp {
                    proj: Box::new(Expr::Fmt(vec![FmtPart::Interp(Expr::Var(var.clone()))])),
                    var,
                    iter: Box::new(expr.clone()),
                    filter: None,
                };
                add(
                    add(Expr::Str("b[".into()), join(comp, " ")),
                    Expr::Str("]".into()),
                )
            }
            Ty::List(element) => {
                let var = self.fresh("r");
                let proj = self.render(&Expr::Var(var.clone()), element);
                let comp = Expr::Comp {
                    proj: Box::new(proj),
                    var,
                    iter: Box::new(expr.clone()),
                    filter: None,
                };
                add(
                    add(Expr::Str("[".into()), join(comp, ",")),
                    Expr::Str("]".into()),
                )
            }
            Ty::Map(key, value) => {
                let var = self.fresh("r");
                let key_text = self.render(&Expr::Var(format!("{var}.key")), key);
                let value_text = self.render(&Expr::Var(format!("{var}.value")), value);
                let proj = add(add(key_text, Expr::Str(":".into())), value_text);
                let comp = Expr::Comp {
                    proj: Box::new(proj),
                    var,
                    iter: Box::new(expr.clone()),
                    filter: None,
                };
                add(
                    add(Expr::Str("{".into()), join(comp, ",")),
                    Expr::Str("}".into()),
                )
            }
            Ty::Rec(index) => {
                let fields = self.records[*index].fields.clone();
                let mut text = Expr::Str(format!("{}{{", self.records[*index].name));
                for (position, (name, field_ty)) in fields.iter().enumerate() {
                    if position > 0 {
                        text = add(text, Expr::Str(",".into()));
                    }
                    let field =
                        self.render(&Expr::Field(Box::new(expr.clone()), name.clone()), field_ty);
                    text = add(text, field);
                }
                add(text, Expr::Str("}".into()))
            }
            Ty::Enum(index) => {
                let variants = self.enums[*index].variants.clone();
                let mut arms = Vec::new();
                for (name, payload) in variants {
                    let binds: Vec<String> = payload.iter().map(|_| self.fresh("r")).collect();
                    let mut text = Expr::Str(name.clone());
                    if !payload.is_empty() {
                        text = add(text, Expr::Str("(".into()));
                        for (position, (bind, payload_ty)) in binds.iter().zip(&payload).enumerate()
                        {
                            if position > 0 {
                                text = add(text, Expr::Str(",".into()));
                            }
                            let rendered = self.render(&Expr::Var(bind.clone()), payload_ty);
                            text = add(text, rendered);
                        }
                        text = add(text, Expr::Str(")".into()));
                    }
                    arms.push((Pat::Variant { name, binds }, text));
                }
                Expr::Match(Box::new(expr.clone()), arms)
            }
            Ty::Opt(inner) => {
                let present = self.render(expr, inner);
                Expr::If(
                    Box::new(Expr::Binary(
                        BinOp::Eq,
                        Box::new(expr.clone()),
                        Box::new(Expr::Null),
                    )),
                    Box::new(Block {
                        stmts: Vec::new(),
                        tail: Some(Expr::Str("null".into())),
                    }),
                    Box::new(Block {
                        stmts: Vec::new(),
                        tail: Some(present),
                    }),
                )
            }
            Ty::Res(inner, _) => {
                let ok = self.fresh("r");
                let err = self.fresh("r");
                let ok_text = self.render(&Expr::Var(ok.clone()), inner);
                Expr::Match(
                    Box::new(expr.clone()),
                    vec![
                        (
                            Pat::Ok(ok),
                            add(add(Expr::Str("ok(".into()), ok_text), Expr::Str(")".into())),
                        ),
                        (
                            Pat::Err(err.clone()),
                            add(
                                add(
                                    Expr::Str("err(".into()),
                                    Expr::Field(Box::new(Expr::Var(err)), "message".into()),
                                ),
                                Expr::Str(")".into()),
                            ),
                        ),
                    ],
                )
            }
        }
    }

    fn let_stmt(&mut self, stmts: &mut Vec<Stmt>, allow_var: bool) {
        let ty = self.ty(0);
        let mutable = allow_var && self.rng.chance(40);
        let annotate = needs_cx(&ty) || self.rng.chance(35);
        let name = self.fresh("v");
        let value = if annotate && mutable && matches!(ty, Ty::List(_)) && self.rng.chance(30) {
            Expr::List(Vec::new())
        } else if self.ctx.kind == Kind::Main
            && self.nesting < 2
            && (annotate || !needs_cx(&ty))
            && self.rng.chance(8)
        {
            // A value block initializer; its first statement is a `let`.
            Expr::BlockValue(Box::new(self.value_block_with_let(&ty, annotate, 1)))
        } else {
            match self.expr(&ty, annotate, 0) {
                Some(value) => value,
                None => self.literal(&ty, annotate, 0),
            }
        };
        stmts.push(Stmt::Let {
            name: name.clone(),
            annot: annotate.then(|| ty.clone()),
            value,
            mutable,
        });
        self.bind(name, ty, mutable);
    }

    fn block(&mut self, locals: Vec<Local>) -> Block {
        self.scopes.push(locals);
        self.nesting += 1;
        let mut stmts = Vec::new();
        let count = 1 + self.rng.below(self.config.max_block_stmts);
        for _ in 0..count {
            self.stmt(&mut stmts);
        }
        self.nesting -= 1;
        self.scopes.pop();
        Block { stmts, tail: None }
    }

    fn stmt(&mut self, stmts: &mut Vec<Stmt>) {
        let nested = self.nesting < 3;
        match self.rng.below(22) {
            0..=4 => self.let_stmt(stmts, true),
            5..=7 => self.assign(stmts),
            8 | 9 if nested => {
                let narrowed = self
                    .locals()
                    .filter(|local| !local.mutable && matches!(local.ty, Ty::Opt(_)))
                    .cloned()
                    .collect::<Vec<_>>();
                if !narrowed.is_empty() && self.rng.chance(40) {
                    // A presence test narrows an immutable Optional.
                    let local = self.rng.pick(&narrowed).clone();
                    let Ty::Opt(inner) = &local.ty else {
                        unreachable!()
                    };
                    let cond = Expr::Binary(
                        BinOp::Ne,
                        Box::new(Expr::Var(local.name.clone())),
                        Box::new(Expr::Null),
                    );
                    let then = self.block(vec![Local {
                        name: local.name.clone(),
                        ty: (**inner).clone(),
                        mutable: false,
                    }]);
                    stmts.push(Stmt::If {
                        cond,
                        then,
                        otherwise: None,
                    });
                    return;
                }
                let cond = self.expr_or_literal(&Ty::Bool, false, 1);
                let then = self.block(Vec::new());
                let otherwise = if self.rng.chance(50) {
                    Some(self.block(Vec::new()))
                } else {
                    None
                };
                stmts.push(Stmt::If {
                    cond,
                    then,
                    otherwise,
                });
            }
            10 | 11 if nested => {
                if self.rng.chance(20)
                    && let Some((iter, item)) = self.stream_call(None, 1)
                {
                    let var = self.fresh("i");
                    let saved = self.ctx;
                    self.ctx.in_loop = true;
                    let body = self.block(vec![Local {
                        name: var.clone(),
                        ty: item,
                        mutable: false,
                    }]);
                    self.ctx = saved;
                    stmts.push(Stmt::For { var, iter, body });
                    return;
                }
                let (iter_ty, locals) = match self.rng.below(5) {
                    0 => (Ty::Str, vec![("", Ty::Str)]),
                    1 => {
                        let key = if self.rng.chance(60) {
                            Ty::Str
                        } else {
                            Ty::Int
                        };
                        let value = self.elem_ty(1);
                        (
                            Ty::map(key.clone(), value.clone()),
                            vec![(".key", key), (".value", value)],
                        )
                    }
                    _ => {
                        let item = self.elem_ty(1);
                        (Ty::list(item.clone()), vec![("", item)])
                    }
                };
                let Some(iter) = self.expr(&iter_ty, false, 1) else {
                    return;
                };
                let var = self.fresh("i");
                let locals = locals
                    .into_iter()
                    .map(|(suffix, ty)| Local {
                        name: format!("{var}{suffix}"),
                        ty,
                        mutable: false,
                    })
                    .collect();
                let saved = self.ctx;
                self.ctx.in_loop = true;
                let body = self.block(locals);
                self.ctx = saved;
                stmts.push(Stmt::For { var, iter, body });
            }
            12 if nested => {
                let counter = self.fresh("w");
                let limit = self.rng.range(0, 4);
                let saved = self.ctx;
                self.ctx.in_loop = true;
                let body = self.block(vec![Local {
                    name: counter.clone(),
                    ty: Ty::Int,
                    mutable: false,
                }]);
                self.ctx = saved;
                stmts.push(Stmt::While {
                    counter,
                    limit,
                    body,
                });
            }
            13 if nested && !self.enums.is_empty() => {
                let en = self.rng.below(self.enums.len());
                let Some(subject) = self.expr(&Ty::Enum(en), false, 1) else {
                    return;
                };
                let variants = self.enums[en].variants.clone();
                let mut arms = Vec::new();
                let wildcard = self.rng.chance(30);
                for (index, (name, payload)) in variants.iter().enumerate() {
                    if wildcard && index + 1 == variants.len() {
                        arms.push((Pat::Wild, self.block(Vec::new())));
                        break;
                    }
                    let binds: Vec<String> = payload.iter().map(|_| self.fresh("m")).collect();
                    let locals = binds
                        .iter()
                        .zip(payload)
                        .map(|(bind, ty)| Local {
                            name: bind.clone(),
                            ty: ty.clone(),
                            mutable: false,
                        })
                        .collect();
                    let body = self.block(locals);
                    arms.push((
                        Pat::Variant {
                            name: name.clone(),
                            binds,
                        },
                        body,
                    ));
                }
                stmts.push(Stmt::Match { subject, arms });
            }
            14 | 15 if self.ctx.observe => {
                let ty = self.ty(0);
                let name = self.fresh("o");
                let value = self.expr_or_literal(&ty, true, 0);
                stmts.push(Stmt::Let {
                    name: name.clone(),
                    annot: Some(ty.clone()),
                    value,
                    mutable: false,
                });
                stmts.push(Stmt::Out(self.render(&Expr::Var(name.clone()), &ty)));
                self.bind(name, ty, false);
            }
            16 if self.ctx.observe && self.value_depth == 0 => {
                let ty = self
                    .rng
                    .pick(&[
                        Ty::Int,
                        Ty::Str,
                        Ty::Bool,
                        Ty::Float,
                        Ty::Bytes,
                        Ty::Path,
                        Ty::Duration,
                    ])
                    .clone();
                let expr = self.expr_or_literal(&ty, false, 1);
                stmts.push(Stmt::AssertEq {
                    expr,
                    expected: None,
                });
            }
            17 if self.ctx.in_loop => {
                let cond = self.expr_or_literal(&Ty::Bool, false, 1);
                stmts.push(if self.rng.chance(50) {
                    Stmt::ContinueWhen(cond)
                } else {
                    Stmt::BreakWhen(cond)
                });
            }
            18 if self.ctx.return_err => {
                let cond = self.expr_or_literal(&Ty::Bool, false, 1);
                stmts.push(Stmt::ReturnErrWhen {
                    cond,
                    message: self.string(),
                });
            }
            19 if nested && self.ctx.kind == Kind::Main => {
                self.scopes.push(Vec::new());
                self.nesting += 1;
                let mut inner = Vec::new();
                self.let_stmt(&mut inner, true);
                for _ in 0..self.rng.below(3) {
                    self.stmt(&mut inner);
                }
                self.nesting -= 1;
                self.scopes.pop();
                stmts.push(Stmt::Block(Block {
                    stmts: inner,
                    tail: None,
                }));
            }
            20 => self.extra_stmt(stmts),
            _ => self.let_stmt(stmts, true),
        }
    }

    /// Defers, guards, `if let`, `yield`, and callable aliases.
    fn extra_stmt(&mut self, stmts: &mut Vec<Stmt>) {
        let nested = self.nesting < 3;
        match self.rng.below(6) {
            0 if self.yield_ty.is_some() => {
                let item = self.yield_ty.clone().expect("item type");
                let value = self.expr_or_literal(&item, true, 1);
                stmts.push(Stmt::Yield(value));
            }
            // Top-level defers would run after the entry's tail has taken
            // `out`, so they are generated only in nested blocks.
            1 if self.ctx.observe && self.nesting >= 1 && nested => {
                let locals: Vec<Local> = self
                    .locals()
                    .filter(|local| !local.name.contains('.'))
                    .cloned()
                    .collect();
                let text = if !locals.is_empty() && self.rng.chance(70) {
                    let local = self.rng.pick(&locals).clone();
                    self.render(&Expr::Var(local.name), &local.ty)
                } else {
                    Expr::Str(format!("defer {}", self.string()))
                };
                stmts.push(Stmt::Defer(Block {
                    stmts: vec![Stmt::Out(text)],
                    tail: None,
                }));
            }
            2 if self.ctx.in_loop || self.ctx.return_err => {
                let cond = self.expr_or_literal(&Ty::Bool, false, 1);
                let exit = if self.ctx.in_loop {
                    if self.rng.chance(60) {
                        Exit::Continue
                    } else {
                        Exit::Break
                    }
                } else {
                    Exit::ReturnErr(self.string())
                };
                stmts.push(Stmt::Guard { cond, exit });
            }
            3 if nested => {
                if !self.enums.is_empty() && self.rng.chance(40) {
                    let en = self.rng.below(self.enums.len());
                    let Some(subject) = self.expr(&Ty::Enum(en), false, 1) else {
                        return;
                    };
                    let (name, payload) = self.rng.pick(&self.enums[en].variants.clone()).clone();
                    let binds: Vec<String> = payload.iter().map(|_| self.fresh("m")).collect();
                    let locals = binds
                        .iter()
                        .zip(&payload)
                        .map(|(bind, ty)| Local {
                            name: bind.clone(),
                            ty: ty.clone(),
                            mutable: false,
                        })
                        .collect();
                    let then = self.block(locals);
                    let otherwise = if self.rng.chance(50) {
                        Some(self.block(Vec::new()))
                    } else {
                        None
                    };
                    stmts.push(Stmt::IfLet {
                        pat: Pat::Variant { name, binds },
                        subject,
                        then,
                        otherwise,
                    });
                } else {
                    let inner = self.elem_ty(1);
                    let fam = *self.rng.pick(&[Fam::Fz, Fam::Error]);
                    let Some(subject) = self.expr(&Ty::res(inner.clone(), fam), false, 1) else {
                        return;
                    };
                    let bind = self.fresh("m");
                    let (pat, local) = if self.rng.chance(60) {
                        (
                            Pat::Ok(bind.clone()),
                            Local {
                                name: bind,
                                ty: inner,
                                mutable: false,
                            },
                        )
                    } else {
                        // The error binding is read through `.message`.
                        (
                            Pat::Err(bind.clone()),
                            Local {
                                name: format!("{bind}.message"),
                                ty: Ty::Str,
                                mutable: false,
                            },
                        )
                    };
                    let then = self.block(vec![local]);
                    let otherwise = if self.rng.chance(50) {
                        Some(self.block(Vec::new()))
                    } else {
                        None
                    };
                    stmts.push(Stmt::IfLet {
                        pat,
                        subject,
                        then,
                        otherwise,
                    });
                }
            }
            4 if self.ctx.kind == Kind::Main && self.nesting == 0 => {
                let candidates: Vec<usize> = (0..self.functions.len())
                    .filter(|index| self.callable(*index))
                    .collect();
                if candidates.is_empty() {
                    return;
                }
                let func = *self.rng.pick(&candidates);
                let alias = self.fresh("a");
                stmts.push(Stmt::Let {
                    name: alias.clone(),
                    annot: None,
                    value: Expr::Var(self.functions[func].name.clone()),
                    mutable: false,
                });
                self.aliases.push((alias, func));
            }
            _ => self.let_stmt(stmts, true),
        }
    }

    /// A structured pipeline producing `ty`.
    fn pipeline_of(&mut self, ty: &Ty, depth: usize) -> Option<Expr> {
        if self.nesting >= 2 {
            return None;
        }
        let saved = self.ctx;
        self.ctx.prop = None;
        let result = self.pipeline_inner(ty, depth);
        self.ctx = saved;
        result
    }

    fn pipeline_inner(&mut self, ty: &Ty, depth: usize) -> Option<Expr> {
        let next = depth + 1;
        let orderable = |ty: &Ty| matches!(ty, Ty::Int | Ty::Str);
        // The terminal stage and the element type it consumes.
        let (terminal, mut element) = match ty {
            Ty::List(element) if !needs_cx(element) => (
                if self.rng.chance(30) {
                    Some(Stage::Collect)
                } else {
                    None
                },
                (**element).clone(),
            ),
            Ty::Int if self.rng.chance(40) => (Some(Stage::Sum), Ty::Int),
            Ty::Int if self.rng.chance(50) => (Some(Stage::Count), self.elem_ty(1)),
            Ty::Bool => {
                let element = self.elem_ty(1);
                let var = self.fresh("x");
                self.scopes.push(vec![Local {
                    name: var.clone(),
                    ty: element.clone(),
                    mutable: false,
                }]);
                let body = self.tail_expr(&Ty::Bool, false, next);
                self.scopes.pop();
                (
                    Some(if self.rng.chance(50) {
                        Stage::Any { var, body }
                    } else {
                        Stage::All { var, body }
                    }),
                    element,
                )
            }
            Ty::Res(inner, Fam::Error) if !needs_cx(inner) => {
                let stage = if orderable(inner) && self.rng.chance(50) {
                    if self.rng.chance(50) {
                        Stage::Min
                    } else {
                        Stage::Max
                    }
                } else if self.rng.chance(50) {
                    Stage::First
                } else {
                    Stage::Last
                };
                (Some(stage), (**inner).clone())
            }
            other if !needs_cx(other) && !matches!(other, Ty::Res(..)) => {
                let element = self.elem_ty(1);
                let init = self.expr_or_literal(other, false, next);
                let acc = self.fresh("x");
                let item = self.fresh("x");
                self.scopes.push(vec![
                    Local {
                        name: acc.clone(),
                        ty: other.clone(),
                        mutable: false,
                    },
                    Local {
                        name: item.clone(),
                        ty: element.clone(),
                        mutable: false,
                    },
                ]);
                let body = self.tail_expr(other, false, next);
                self.scopes.pop();
                (
                    Some(Stage::Fold {
                        init,
                        acc,
                        item,
                        body,
                    }),
                    element,
                )
            }
            _ => return None,
        };
        // Middle stages, built from the end back to the source.
        let mut stages = Vec::new();
        for _ in 0..self.rng.below(4) {
            let stage = match self.rng.below(7) {
                0 | 1 if !needs_cx(&element) => {
                    let mut from = self.elem_ty(1);
                    while needs_cx(&from) {
                        from = self.elem_ty(1);
                    }
                    let var = self.fresh("x");
                    self.scopes.push(vec![Local {
                        name: var.clone(),
                        ty: from.clone(),
                        mutable: false,
                    }]);
                    let body = self.tail_expr(&element, false, next);
                    self.scopes.pop();
                    element = from;
                    Stage::Map { var, body }
                }
                2 => {
                    let var = self.fresh("x");
                    self.scopes.push(vec![Local {
                        name: var.clone(),
                        ty: element.clone(),
                        mutable: false,
                    }]);
                    let body = self.tail_expr(&Ty::Bool, false, next);
                    self.scopes.pop();
                    Stage::Where { var, body }
                }
                3 => Stage::Take(self.rng.range(0, 4)),
                4 => Stage::Drop(self.rng.range(0, 3)),
                5 if orderable(&element) => Stage::Sort,
                _ => Stage::Repeat(self.rng.range(0, 2)),
            };
            stages.push(stage);
        }
        stages.reverse();
        let source = match self.stream_call(Some(&element), next) {
            Some((call, _)) if self.rng.chance(30) => call,
            _ => self.expr(&Ty::list(element), false, next)?,
        };
        if let Some(terminal) = terminal {
            stages.push(terminal);
        }
        if stages.is_empty() {
            stages.push(Stage::Collect);
        }
        Some(Expr::Pipeline {
            source: Box::new(source),
            stages,
        })
    }

    fn assign(&mut self, stmts: &mut Vec<Stmt>) {
        let mutable: Vec<Local> = self
            .locals()
            .filter(|local| local.mutable)
            .cloned()
            .collect();
        if mutable.is_empty() {
            self.let_stmt(stmts, true);
            return;
        }
        let local = self.rng.pick(&mutable).clone();
        let roll = self.rng.below(4);
        let stmt = match (&local.ty, roll) {
            (Ty::Int | Ty::Float, 0) => {
                let op = *self
                    .rng
                    .pick(&[AssignOp::Add, AssignOp::Sub, AssignOp::Mul]);
                let value = self.expr_or_literal(&local.ty, false, 1);
                Stmt::Assign {
                    target: Target::Var(local.name.clone()),
                    op,
                    value,
                }
            }
            (Ty::List(_), 0) => {
                let value = self.expr_or_literal(&local.ty, true, 1);
                Stmt::Assign {
                    target: Target::Var(local.name.clone()),
                    op: AssignOp::Add,
                    value,
                }
            }
            (Ty::Rec(index), 1) => {
                let fields = self.records[*index].fields.clone();
                let (field, field_ty) = self.rng.pick(&fields).clone();
                let value = self.expr_or_literal(&field_ty, true, 1);
                Stmt::Assign {
                    target: Target::Field(local.name.clone(), field),
                    op: AssignOp::Set,
                    value,
                }
            }
            (Ty::Map(key, value_ty), 1) if **key == Ty::Str => {
                let key = Expr::Str(self.string());
                let value = self.expr_or_literal(value_ty, true, 1);
                Stmt::Assign {
                    target: Target::Key(local.name.clone(), key),
                    op: AssignOp::Set,
                    value,
                }
            }
            (Ty::List(element), 2) => {
                let value = self.expr_or_literal(element, true, 1);
                Stmt::Assign {
                    target: Target::Key(local.name.clone(), Expr::Int(0)),
                    op: AssignOp::Set,
                    value,
                }
            }
            _ => {
                let value = self.expr_or_literal(&local.ty, true, 1);
                Stmt::Assign {
                    target: Target::Var(local.name.clone()),
                    op: AssignOp::Set,
                    value,
                }
            }
        };
        stmts.push(stmt);
    }
}
