//! An independent reference evaluator for generated programs.
//!
//! It implements only the fragment the generator emits, directly from the
//! language specification, and shares no code with the XSH runtime. Its
//! output is the expected stdout of the printed program. A domain failure
//! (overflow, division by zero, a missing index) means the generator should
//! draw another program rather than that XSH went wrong.

use crate::ast::{Arg, AssignOp, BinOp, Block, Elem, Exit, Expr, FmtPart, FnKind, IsPat, Pat, Program, Stage, Stmt, Target};
use std::cmp::Ordering;
use rustc_hash::FxHashMap as HashMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Path(String),
    Bytes(Vec<u8>),
    Duration(u64),
    List(Vec<Val>),
    /// Entries in canonical key order.
    Map(Vec<(Val, Val)>),
    Rec(Vec<(String, Val)>),
    /// Enum index, variant index, payload.
    Variant(usize, usize, Vec<Val>),
    Null,
    Ok(Box<Val>),
    Err(String),
}

/// Why evaluation stopped early.
#[derive(Debug)]
pub enum Flow {
    /// An `Err` propagating to the nearest try or callable boundary.
    Propagate(String),
    Return(Val),
    Break,
    Continue,
    /// The program would fail at runtime for a legitimate domain reason.
    Domain(String),
}

type Res<T> = Result<T, Flow>;

fn domain<T>(message: impl Into<String>) -> Res<T> {
    Err(Flow::Domain(message.into()))
}

pub fn key_cmp(left: &Val, right: &Val) -> Ordering {
    match (left, right) {
        (Val::Int(a), Val::Int(b)) => a.cmp(b),
        (Val::Str(a), Val::Str(b)) | (Val::Path(a), Val::Path(b)) => a.as_bytes().cmp(b.as_bytes()),
        (Val::Bool(a), Val::Bool(b)) => a.cmp(b),
        (Val::Duration(a), Val::Duration(b)) => a.cmp(b),
        (Val::Bytes(a), Val::Bytes(b)) => a.cmp(b),
        _ => Ordering::Equal,
    }
}

pub fn map_insert(entries: &mut Vec<(Val, Val)>, key: Val, value: Val) {
    match entries.binary_search_by(|(existing, _)| key_cmp(existing, &key)) {
        Ok(index) => entries[index].1 = value,
        Err(index) => entries.insert(index, (key, value)),
    }
}

/// XSH display text for a displayable scalar.
pub fn display(value: &Val) -> String {
    match value {
        Val::Int(value) => value.to_string(),
        Val::Float(value) => display_float(*value),
        Val::Str(text) | Val::Path(text) => text.clone(),
        Val::Bool(value) => value.to_string(),
        other => format!("{other:?}"),
    }
}

pub fn display_float(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity".into() } else { "-Infinity".into() }
    } else {
        format!("{value}")
    }
}

pub struct Evaluator<'a> {
    program: &'a Program,
    scopes: Vec<HashMap<String, Val>>,
    pub out: Vec<String>,
    /// Values observed by `assert expr == ?` holes, keyed by statement
    /// address; `None` once two executions observed different values.
    pub assert_values: HashMap<usize, Option<Val>>,
    steps: usize,
    /// Items yielded by the stream producers being run, innermost last.
    yields: Vec<Vec<Val>>,
}

const STEP_LIMIT: usize = 200_000;

impl<'a> Evaluator<'a> {
    pub fn new(program: &'a Program) -> Self {
        Self {
            program,
            scopes: Vec::new(),
            out: Vec::new(),
            assert_values: HashMap::default(),
            steps: 0,
            yields: Vec::new(),
        }
    }

    /// Runs the entry body and returns the printed lines.
    pub fn run(&mut self) -> Result<Vec<String>, String> {
        let globals = match self.globals() {
            Ok(globals) => globals,
            Err(Flow::Domain(message)) => return Err(message),
            Err(other) => return Err(format!("constant failed: {other:?}")),
        };
        self.scopes.push(globals);
        let result = self.block(&self.program.body);
        self.scopes.pop();
        match result {
            Ok(_) => Ok(std::mem::take(&mut self.out)),
            // The entry's try converts a propagated Err into the shim's
            // top-level failure; generated programs never do that.
            Err(Flow::Propagate(message)) => Err(format!("entry propagated Err({message})")),
            Err(Flow::Domain(message)) => Err(message),
            Err(other) => Err(format!("unexpected control flow {other:?}")),
        }
    }

    /// Top-level constants, evaluated in declaration order.
    fn globals(&mut self) -> Res<HashMap<String, Val>> {
        self.scopes.push(HashMap::default());
        for (name, _, value) in &self.program.consts {
            let value = self.expr(value)?;
            self.bind(name, value);
        }
        Ok(self.scopes.pop().expect("constant scope"))
    }

    fn lookup(&self, name: &str) -> Val {
        // Map entry bindings are referenced as `entry.key` / `entry.value`.
        if let Some((base, field)) = name.split_once('.') {
            let Val::Rec(fields) = self.lookup(base) else { panic!("field path on non-record {name}") };
            return fields.into_iter().find(|(label, _)| label == field).map(|(_, value)| value).expect("entry field");
        }
        for scope in self.scopes.iter().rev() {
            if let Some(value) = scope.get(name) {
                return value.clone();
            }
        }
        // A callable alias binds a function name, which has no value here.
        if self.program.functions.iter().any(|function| function.name == name) {
            return Val::Null;
        }
        panic!("generator referenced unbound name {name}")
    }

    fn lookup_mut(&mut self, name: &str) -> &mut Val {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(value) = scope.get_mut(name) {
                return value;
            }
        }
        panic!("generator assigned unbound name {name}")
    }

    fn bind(&mut self, name: &str, value: Val) {
        self.scopes
            .last_mut()
            .expect("scope")
            .insert(name.to_string(), value);
    }

    fn tick(&mut self) -> Res<()> {
        self.steps += 1;
        if self.steps > STEP_LIMIT {
            return domain("step limit");
        }
        Ok(())
    }

    /// Evaluates a block in a fresh scope, returning its tail value.
    fn block(&mut self, block: &Block) -> Res<Val> {
        self.scopes.push(HashMap::default());
        let result = self.block_in_scope(block);
        self.scopes.pop();
        result
    }

    /// Runs a block's statements and tail in the current scope; its defers
    /// run last-in-first-out when it is left, after the tail is evaluated.
    fn block_in_scope(&mut self, block: &Block) -> Res<Val> {
        let mut defers: Vec<&Block> = Vec::new();
        let mut result = Ok(Val::Null);
        for stmt in &block.stmts {
            if let Stmt::Defer(body) = stmt {
                defers.push(body);
                continue;
            }
            if let Err(flow) = self.stmt(stmt) {
                result = Err(flow);
                break;
            }
        }
        if result.is_ok()
            && let Some(tail) = &block.tail
        {
            result = self.expr(tail);
        }
        for body in defers.into_iter().rev() {
            match self.block(body) {
                Ok(_) => {}
                Err(Flow::Domain(message)) => return Err(Flow::Domain(message)),
                Err(other) => return Err(Flow::Domain(format!("defer left with {other:?}"))),
            }
        }
        result
    }

    fn stmt(&mut self, stmt: &Stmt) -> Res<()> {
        self.tick()?;
        match stmt {
            Stmt::Let { name, value, .. } => {
                let value = self.expr(value)?;
                self.bind(name, value);
            }
            Stmt::Assign { target, op, value } => {
                let rhs = self.expr(value)?;
                match target {
                    Target::Var(name) => {
                        let old = self.lookup(name);
                        let new = assign_value(*op, old, rhs)?;
                        *self.lookup_mut(name) = new;
                    }
                    Target::Field(name, field) => {
                        let Val::Rec(mut fields) = self.lookup(name) else { panic!("field target") };
                        let slot = fields.iter_mut().find(|(label, _)| label == field).expect("field");
                        slot.1 = assign_value(*op, slot.1.clone(), rhs)?;
                        *self.lookup_mut(name) = Val::Rec(fields);
                    }
                    Target::Key(name, key) => {
                        let key = self.expr(key)?;
                        match self.lookup(name) {
                            Val::Map(mut entries) => {
                                let old = entries
                                    .iter()
                                    .find(|(existing, _)| *existing == key)
                                    .map(|(_, value)| value.clone());
                                let new = match (op, old) {
                                    (AssignOp::Set, _) => rhs,
                                    (_, Some(old)) => assign_value(*op, old, rhs)?,
                                    (_, None) => return domain("compound assignment to a missing key"),
                                };
                                map_insert(&mut entries, key, new);
                                *self.lookup_mut(name) = Val::Map(entries);
                            }
                            Val::List(mut items) => {
                                let Val::Int(index) = key else { panic!("list index") };
                                if index < 0 || index as usize >= items.len() {
                                    return domain("list index out of range");
                                }
                                let slot = &mut items[index as usize];
                                *slot = assign_value(*op, slot.clone(), rhs)?;
                                *self.lookup_mut(name) = Val::List(items);
                            }
                            other => panic!("key target on {other:?}"),
                        }
                    }
                }
            }
            Stmt::If { cond, then, otherwise } => {
                if self.truth(cond)? {
                    self.block(then)?;
                } else if let Some(otherwise) = otherwise {
                    self.block(otherwise)?;
                }
            }
            Stmt::For { var, iter, body } => {
                let items = iterate(self.expr(iter)?);
                for item in items {
                    self.tick()?;
                    self.scopes.push(HashMap::default());
                    self.bind(var, item);
                    let result = self.block_in_scope(body);
                    self.scopes.pop();
                    match result {
                        Ok(_) | Err(Flow::Continue) => {}
                        Err(Flow::Break) => break,
                        Err(other) => return Err(other),
                    }
                }
            }
            Stmt::While { counter, limit, body } => {
                self.bind(counter, Val::Int(0));
                loop {
                    let Val::Int(current) = self.lookup(counter) else { panic!("counter") };
                    if current >= *limit {
                        break;
                    }
                    self.tick()?;
                    *self.lookup_mut(counter) = Val::Int(current + 1);
                    let result = self.block(body);
                    match result {
                        Ok(_) | Err(Flow::Continue) => {}
                        Err(Flow::Break) => break,
                        Err(other) => return Err(other),
                    }
                }
            }
            Stmt::Match { subject, arms } => {
                let subject = self.expr(subject)?;
                for (pat, body) in arms {
                    if let Some(binds) = self.matches(pat, &subject) {
                        self.scopes.push(binds);
                        let result = self.block(body);
                        self.scopes.pop();
                        result?;
                        return Ok(());
                    }
                }
                return domain("match-no-arm");
            }
            Stmt::AssertEq { expr, .. } => {
                let value = self.expr(expr)?;
                let key = std::ptr::from_ref(stmt) as usize;
                let entry = self.assert_values.entry(key).or_insert_with(|| Some(value.clone()));
                if entry.as_ref().is_some_and(|previous| !values_equal(previous, &value)) {
                    *entry = None;
                }
            }
            Stmt::Assert(cond) => {
                if !self.truth(cond)? {
                    return domain("assertion is false");
                }
            }
            Stmt::Out(expr) => {
                let Val::Str(text) = self.expr(expr)? else { panic!("out expects Str") };
                self.out.push(text);
            }
            Stmt::ContinueWhen(cond) => {
                if self.truth(cond)? {
                    return Err(Flow::Continue);
                }
            }
            Stmt::BreakWhen(cond) => {
                if self.truth(cond)? {
                    return Err(Flow::Break);
                }
            }
            Stmt::ReturnErrWhen { cond, message } => {
                if self.truth(cond)? {
                    return Err(Flow::Return(Val::Err(message.clone())));
                }
            }
            Stmt::Block(block) => {
                self.block(block)?;
            }
            Stmt::Defer(_) => unreachable!("defers register in block_in_scope"),
            Stmt::Guard { cond, exit } => {
                if !self.truth(cond)? {
                    return Err(match exit {
                        Exit::Continue => Flow::Continue,
                        Exit::Break => Flow::Break,
                        Exit::ReturnErr(message) => Flow::Return(Val::Err(message.clone())),
                    });
                }
            }
            Stmt::IfLet { pat, subject, then, otherwise } => {
                let subject = self.expr(subject)?;
                if let Some(binds) = self.matches(pat, &subject) {
                    self.scopes.push(binds);
                    let result = self.block(then);
                    self.scopes.pop();
                    result?;
                } else if let Some(otherwise) = otherwise {
                    self.block(otherwise)?;
                }
            }
            Stmt::Yield(value) => {
                let value = self.expr(value)?;
                self.yields.last_mut().expect("yield inside a stream producer").push(value);
            }
        }
        Ok(())
    }

    fn truth(&mut self, expr: &Expr) -> Res<bool> {
        match self.expr(expr)? {
            Val::Bool(value) => Ok(value),
            other => panic!("condition is not Bool: {other:?}"),
        }
    }

    fn matches(&self, pat: &Pat, value: &Val) -> Option<HashMap<String, Val>> {
        let mut binds = HashMap::default();
        let ok = match (pat, value) {
            (Pat::Wild, _) => true,
            (Pat::Bind(name), value) => {
                binds.insert(name.clone(), value.clone());
                true
            }
            (Pat::Int(expected), Val::Int(actual)) => expected == actual,
            (Pat::Str(expected), Val::Str(actual)) => expected == actual,
            (Pat::Bool(expected), Val::Bool(actual)) => expected == actual,
            (Pat::Variant { name, binds: names }, Val::Variant(en, variant, payload)) => {
                let matched = self.program.enums[*en].variants[*variant].0 == *name;
                if matched {
                    for (bind, value) in names.iter().zip(payload) {
                        binds.insert(bind.clone(), value.clone());
                    }
                }
                matched
            }
            (Pat::Ok(name), Val::Ok(inner)) => {
                binds.insert(name.clone(), (**inner).clone());
                true
            }
            (Pat::Err(name), Val::Err(message)) => {
                binds.insert(name.clone(), Val::Rec(vec![("message".into(), Val::Str(message.clone()))]));
                true
            }
            (Pat::ListEmpty, Val::List(items)) => items.is_empty(),
            (Pat::ListCons { head, rest }, Val::List(items)) if !items.is_empty() => {
                binds.insert(head.clone(), items[0].clone());
                binds.insert(rest.clone(), Val::List(items[1..].to_vec()));
                true
            }
            _ => false,
        };
        ok.then_some(binds)
    }

    pub fn expr(&mut self, expr: &Expr) -> Res<Val> {
        self.tick()?;
        Ok(match expr {
            Expr::Int(value) => Val::Int(*value),
            Expr::Float(value) => Val::Float(*value),
            Expr::Str(text) => Val::Str(text.clone()),
            Expr::Bool(value) => Val::Bool(*value),
            Expr::Path(text) => Val::Path(text.clone()),
            Expr::Bytes(bytes) => Val::Bytes(bytes.clone()),
            Expr::Null => Val::Null,
            Expr::Var(name) => self.lookup(name),
            Expr::Not(inner) => match self.expr(inner)? {
                Val::Bool(value) => Val::Bool(!value),
                other => panic!("! on {other:?}"),
            },
            Expr::Neg(inner) => match self.expr(inner)? {
                Val::Int(value) => Val::Int(value.checked_neg().ok_or_else(|| Flow::Domain("negation overflow".into()))?),
                Val::Float(value) => Val::Float(-value),
                other => panic!("- on {other:?}"),
            },
            Expr::Binary(op, left, right) => self.binary(*op, left, right)?,
            Expr::If(cond, then, otherwise) => {
                if self.truth(cond)? {
                    self.block(then)?
                } else {
                    self.block(otherwise)?
                }
            }
            Expr::Match(subject, arms) => {
                let subject = self.expr(subject)?;
                for (pat, body) in arms {
                    if let Some(binds) = self.matches(pat, &subject) {
                        self.scopes.push(binds);
                        let result = self.expr(body);
                        self.scopes.pop();
                        return result;
                    }
                }
                return domain("match-no-arm");
            }
            Expr::Call { func, args } => self.call(*func, args)?,
            Expr::Method { recv, name, args } => {
                let recv = self.expr(recv)?;
                let args = args.iter().map(|arg| self.expr(arg)).collect::<Res<Vec<_>>>()?;
                method(&recv, name, &args)?
            }
            Expr::OptMethod { recv, name, args } => {
                let recv = self.expr(recv)?;
                if recv == Val::Null {
                    Val::Null
                } else {
                    let args = args.iter().map(|arg| self.expr(arg)).collect::<Res<Vec<_>>>()?;
                    method(&recv, name, &args)?
                }
            }
            Expr::List(elems) => {
                let mut items = Vec::new();
                for elem in elems {
                    match elem {
                        Elem::Item(value) => items.push(self.expr(value)?),
                        Elem::Splice(value) => match self.expr(value)? {
                            Val::List(values) => items.extend(values),
                            other => panic!("splice of {other:?}"),
                        },
                    }
                }
                Val::List(items)
            }
            Expr::Comp { proj, var, iter, filter } => {
                let source = iterate(self.expr(iter)?);
                let mut items = Vec::new();
                for item in source {
                    self.scopes.push(HashMap::default());
                    self.bind(var, item);
                    let keep = match filter {
                        Some(filter) => self.truth(filter),
                        None => Ok(true),
                    };
                    let result = match keep {
                        Ok(true) => self.expr(proj).map(Some),
                        Ok(false) => Ok(None),
                        Err(flow) => Err(flow),
                    };
                    self.scopes.pop();
                    if let Some(value) = result? {
                        items.push(value);
                    }
                }
                Val::List(items)
            }
            Expr::MapLit(entries) => {
                let mut map = Vec::new();
                for (key, value) in entries {
                    let key = self.expr(key)?;
                    let value = self.expr(value)?;
                    map_insert(&mut map, key, value);
                }
                Val::Map(map)
            }
            Expr::MapComp { key, value, var, iter } => {
                let source = iterate(self.expr(iter)?);
                let mut map = Vec::new();
                for item in source {
                    self.scopes.push(HashMap::default());
                    self.bind(var, item);
                    let entry = self.expr(key).and_then(|key| Ok((key, self.expr(value)?)));
                    self.scopes.pop();
                    let (key, value) = entry?;
                    map_insert(&mut map, key, value);
                }
                Val::Map(map)
            }
            Expr::RecCtor { rec, fields } => {
                let mut values = HashMap::default();
                for (name, value) in fields {
                    values.insert(name.clone(), self.expr(value)?);
                }
                let decl = &self.program.records[*rec];
                Val::Rec(
                    decl.fields
                        .iter()
                        .map(|(name, _)| (name.clone(), values.remove(name).expect("field value")))
                        .collect(),
                )
            }
            Expr::RecUpdate { base, updates } => {
                let Val::Rec(mut fields) = self.expr(base)? else { panic!("update base") };
                for (name, value) in updates {
                    let value = self.expr(value)?;
                    fields.iter_mut().find(|(label, _)| label == name).expect("field").1 = value;
                }
                Val::Rec(fields)
            }
            Expr::Field(base, field) => match self.expr(base)? {
                Val::Rec(fields) => fields
                    .into_iter()
                    .find(|(label, _)| label == field)
                    .map(|(_, value)| value)
                    .expect("field"),
                other => panic!("field of {other:?}"),
            },
            Expr::Index(base, index) => {
                let base = self.expr(base)?;
                let index = self.expr(index)?;
                match (base, index) {
                    (Val::List(items), Val::Int(index)) => {
                        if index < 0 || index as usize >= items.len() {
                            return domain("list index out of range");
                        }
                        items[index as usize].clone()
                    }
                    (Val::Map(entries), key) => match entries.into_iter().find(|(existing, _)| *existing == key) {
                        Some((_, value)) => value,
                        None => return domain("missing map key"),
                    },
                    (other, _) => panic!("index of {other:?}"),
                }
            }
            Expr::Slice(base, start, end) => {
                let base = self.expr(base)?;
                let start = match start {
                    Some(start) => Some(self.int(start)?),
                    None => None,
                };
                let end = match end {
                    Some(end) => Some(self.int(end)?),
                    None => None,
                };
                slice(base, start, end)
            }
            Expr::Variant { en, variant, args } => {
                let args = args.iter().map(|arg| self.expr(arg)).collect::<Res<Vec<_>>>()?;
                Val::Variant(*en, *variant, args)
            }
            Expr::Ok(inner) => Val::Ok(Box::new(self.expr(inner)?)),
            Expr::Err(message) => Val::Err(message.clone()),
            Expr::Try(block) => match self.block(block) {
                Ok(value) => Val::Ok(Box::new(value)),
                Err(Flow::Propagate(message)) => Val::Err(message),
                Err(other) => return Err(other),
            },
            Expr::Fallback(left, right) => match self.expr(left)? {
                Val::Ok(inner) => *inner,
                Val::Err(_) | Val::Null => self.expr(right)?,
                present => present,
            },
            Expr::FallbackBlock(left, name, block) => match self.expr(left)? {
                Val::Ok(inner) => *inner,
                Val::Err(message) => {
                    self.scopes.push(HashMap::default());
                    self.bind(name, Val::Rec(vec![("message".into(), Val::Str(message))]));
                    let result = self.block(block);
                    self.scopes.pop();
                    result?
                }
                other => panic!("fallback block on {other:?}"),
            },
            Expr::Propagate(inner) => match self.expr(inner)? {
                Val::Ok(inner) => *inner,
                Val::Err(message) => return Err(Flow::Propagate(message)),
                other => panic!("? on {other:?}"),
            },
            Expr::Fmt(parts) => {
                let mut text = String::new();
                for part in parts {
                    match part {
                        FmtPart::Lit(literal) => text.push_str(literal),
                        FmtPart::Interp(value) => text.push_str(&display(&self.expr(value)?)),
                    }
                }
                Val::Str(text)
            }
            Expr::BlockValue(block) => self.block(block)?,
            Expr::Duration(millis) => Val::Duration(*millis),
            Expr::Pipeline { source, stages } => {
                let mut value = self.expr(source)?;
                for stage in stages {
                    value = self.stage(stage, value)?;
                }
                value
            }
            Expr::Is(subject, pat) => {
                let subject = self.expr(subject)?;
                Val::Bool(match (pat, &subject) {
                    (IsPat::Ok, Val::Ok(_)) | (IsPat::Err, Val::Err(_)) => true,
                    (IsPat::Variant { name, .. }, Val::Variant(en, variant, _)) => self.program.enums[*en].variants[*variant].0 == *name,
                    _ => false,
                })
            }
            Expr::Retry(block) => match self.block(block) {
                Ok(value) => Val::Ok(Box::new(value)),
                Err(Flow::Propagate(message)) => Val::Err(message),
                Err(other) => return Err(other),
            },
            Expr::Ctx(_, block) => self.block(block)?,
            Expr::AliasCall { func, args, .. } => self.call(*func, args)?,
            Expr::OptField(recv, field) => match self.expr(recv)? {
                Val::Null => Val::Null,
                Val::Rec(fields) => fields.into_iter().find(|(label, _)| label == field).map(|(_, value)| value).expect("field"),
                other => panic!("?. field of {other:?}"),
            },
        })
    }

    fn stage(&mut self, stage: &Stage, input: Val) -> Res<Val> {
        let Val::List(items) = input else { panic!("pipeline input {input:?}") };
        let empty = || Val::Err("stream was empty".into());
        Ok(match stage {
            Stage::Map { var, body } => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.with_binding(var, item, body)?);
                }
                Val::List(out)
            }
            Stage::Where { var, body } => {
                let mut out = Vec::new();
                for item in items {
                    if self.with_binding(var, item.clone(), body)? == Val::Bool(true) {
                        out.push(item);
                    }
                }
                Val::List(out)
            }
            Stage::Take(count) => Val::List(items.into_iter().take(*count as usize).collect()),
            Stage::Drop(count) => Val::List(items.into_iter().skip(*count as usize).collect()),
            Stage::Sort => {
                let mut items = items;
                items.sort_by(key_cmp);
                Val::List(items)
            }
            Stage::Repeat(count) => {
                let mut out = Vec::new();
                for _ in 0..*count {
                    out.extend(items.iter().cloned());
                }
                Val::List(out)
            }
            Stage::Collect => Val::List(items),
            Stage::Sum => {
                let mut total: i64 = 0;
                for item in items {
                    let Val::Int(value) = item else { panic!("sum of {item:?}") };
                    total = total.checked_add(value).ok_or_else(|| Flow::Domain("integer-overflow".into()))?;
                }
                Val::Int(total)
            }
            Stage::Count => Val::Int(items.len() as i64),
            Stage::Min => items.into_iter().min_by(key_cmp).map_or_else(empty, |item| Val::Ok(Box::new(item))),
            Stage::Max => items.into_iter().reduce(|best, item| if key_cmp(&item, &best) == Ordering::Greater { item } else { best }).map_or_else(empty, |item| Val::Ok(Box::new(item))),
            Stage::First => items.into_iter().next().map_or_else(empty, |item| Val::Ok(Box::new(item))),
            Stage::Last => items.into_iter().last().map_or_else(empty, |item| Val::Ok(Box::new(item))),
            Stage::Any { var, body } => {
                for item in items {
                    if self.with_binding(var, item, body)? == Val::Bool(true) {
                        return Ok(Val::Bool(true));
                    }
                }
                Val::Bool(false)
            }
            Stage::All { var, body } => {
                for item in items {
                    if self.with_binding(var, item, body)? != Val::Bool(true) {
                        return Ok(Val::Bool(false));
                    }
                }
                Val::Bool(true)
            }
            Stage::Fold { init, acc, item, body } => {
                let mut state = self.expr(init)?;
                for value in items {
                    self.scopes.push(HashMap::default());
                    self.bind(acc, state);
                    self.bind(item, value);
                    let result = self.expr(body);
                    self.scopes.pop();
                    state = result?;
                }
                state
            }
        })
    }

    fn with_binding(&mut self, name: &str, value: Val, body: &Expr) -> Res<Val> {
        self.scopes.push(HashMap::default());
        self.bind(name, value);
        let result = self.expr(body);
        self.scopes.pop();
        result
    }

    fn int(&mut self, expr: &Expr) -> Res<i64> {
        match self.expr(expr)? {
            Val::Int(value) => Ok(value),
            other => panic!("expected Int, found {other:?}"),
        }
    }

    fn call(&mut self, func: usize, args: &[Arg]) -> Res<Val> {
        let decl = &self.program.functions[func];
        let mut supplied: HashMap<String, Val> = HashMap::default();
        let mut position = 0;
        for arg in args {
            match arg {
                Arg::Pos(value) => {
                    let value = self.expr(value)?;
                    supplied.insert(decl.params[position].name.clone(), value);
                    position += 1;
                }
                Arg::Named(name, value) => {
                    let value = self.expr(value)?;
                    supplied.insert(name.clone(), value);
                }
                Arg::Spread(fields) => {
                    for (name, value) in fields {
                        let value = self.expr(value)?;
                        supplied.insert(name.clone(), value);
                    }
                }
            }
        }
        let mut frame = HashMap::default();
        for param in &decl.params {
            let value = match supplied.remove(&param.name) {
                Some(value) => value,
                None => {
                    let default = param.default.as_ref().expect("omitted parameter has a default");
                    self.expr(default)?
                }
            };
            frame.insert(param.name.clone(), value);
        }
        // Callables see the constants and their parameters.
        let globals = self.scopes.first().cloned().unwrap_or_default();
        let saved = std::mem::replace(&mut self.scopes, vec![globals, frame]);
        if decl.kind == FnKind::Stream {
            self.yields.push(Vec::new());
            let result = self.block_in_scope(&decl.body);
            self.scopes = saved;
            let items = self.yields.pop().expect("yield buffer");
            return match result {
                Ok(_) => Ok(Val::List(items)),
                Err(other) => Err(other),
            };
        }
        let result = self.block_in_scope(&decl.body);
        self.scopes = saved;
        let wraps = decl.kind == FnKind::Proc || matches!(decl.ret, crate::ast::Ty::Res(..));
        match result {
            Ok(value) if wraps && !matches!(decl.tail, crate::ast::Ty::Res(..)) => Ok(Val::Ok(Box::new(value))),
            Ok(value) => Ok(value),
            Err(Flow::Return(value)) => Ok(value),
            Err(Flow::Propagate(message)) if wraps => Ok(Val::Err(message)),
            Err(other) => Err(other),
        }
    }

    fn binary(&mut self, op: BinOp, left: &Expr, right: &Expr) -> Res<Val> {
        if op == BinOp::And || op == BinOp::Or {
            let left = self.truth(left)?;
            if (op == BinOp::And && !left) || (op == BinOp::Or && left) {
                return Ok(Val::Bool(left));
            }
            return Ok(Val::Bool(self.truth(right)?));
        }
        let left = self.expr(left)?;
        let right = self.expr(right)?;
        let overflow = || Flow::Domain("integer-overflow".into());
        Ok(match (op, left, right) {
            (BinOp::Add, Val::Int(a), Val::Int(b)) => Val::Int(a.checked_add(b).ok_or_else(overflow)?),
            (BinOp::Sub, Val::Int(a), Val::Int(b)) => Val::Int(a.checked_sub(b).ok_or_else(overflow)?),
            (BinOp::Mul, Val::Int(a), Val::Int(b)) => Val::Int(a.checked_mul(b).ok_or_else(overflow)?),
            (BinOp::Div, Val::Int(a), Val::Int(b)) => {
                if b == 0 {
                    return domain("division-by-zero");
                }
                Val::Int(a.checked_div(b).ok_or_else(overflow)?)
            }
            (BinOp::Rem, Val::Int(a), Val::Int(b)) => {
                if b == 0 {
                    return domain("division-by-zero");
                }
                Val::Int(a.checked_rem(b).ok_or_else(overflow)?)
            }
            (BinOp::Add, Val::Float(a), Val::Float(b)) => finite(a + b)?,
            (BinOp::Sub, Val::Float(a), Val::Float(b)) => finite(a - b)?,
            (BinOp::Mul, Val::Float(a), Val::Float(b)) => finite(a * b)?,
            (BinOp::Div, Val::Float(a), Val::Float(b)) => finite(a / b)?,
            (BinOp::Add, Val::Str(a), Val::Str(b)) => Val::Str(a + &b),
            (BinOp::Add, Val::Duration(a), Val::Duration(b)) => Val::Duration(a.checked_add(b).ok_or_else(|| Flow::Domain("duration-overflow".into()))?),
            (BinOp::Sub, Val::Duration(a), Val::Duration(b)) => Val::Duration(a.checked_sub(b).ok_or_else(|| Flow::Domain("duration-underflow".into()))?),
            (BinOp::Mul, Val::Duration(a), Val::Int(b)) | (BinOp::Mul, Val::Int(b), Val::Duration(a)) => {
                let factor = u64::try_from(b).map_err(|_| Flow::Domain("duration-negative-factor".into()))?;
                Val::Duration(a.checked_mul(factor).ok_or_else(|| Flow::Domain("duration-overflow".into()))?)
            }
            (BinOp::Div, Val::Duration(a), Val::Int(b)) => {
                let divisor = u64::try_from(b).ok().filter(|divisor| *divisor > 0).ok_or_else(|| Flow::Domain("division-by-zero".into()))?;
                Val::Duration(a / divisor)
            }
            (BinOp::Div, Val::Duration(a), Val::Duration(b)) => {
                if b == 0 {
                    return domain("division-by-zero");
                }
                Val::Int(i64::try_from(a / b).map_err(|_| Flow::Domain("integer-overflow".into()))?)
            }
            (BinOp::Add, Val::List(mut a), Val::List(b)) => {
                a.extend(b);
                Val::List(a)
            }
            (BinOp::Eq, a, b) => Val::Bool(values_equal(&a, &b)),
            (BinOp::Ne, a, b) => Val::Bool(!values_equal(&a, &b)),
            (BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge, a, b) => {
                let ordering = match (&a, &b) {
                    (Val::Int(a), Val::Int(b)) => a.cmp(b),
                    (Val::Str(a), Val::Str(b)) => a.cmp(b),
                    (Val::Float(a), Val::Float(b)) => a.partial_cmp(b).ok_or_else(|| Flow::Domain("NaN comparison".into()))?,
                    (Val::Duration(a), Val::Duration(b)) => a.cmp(b),
                    other => panic!("ordering on {other:?}"),
                };
                Val::Bool(match op {
                    BinOp::Lt => ordering == Ordering::Less,
                    BinOp::Le => ordering != Ordering::Greater,
                    BinOp::Gt => ordering == Ordering::Greater,
                    _ => ordering != Ordering::Less,
                })
            }
            (BinOp::In | BinOp::NotIn, needle, haystack) => {
                let found = match (&needle, &haystack) {
                    (needle, Val::List(items)) => items.iter().any(|item| values_equal(item, needle)),
                    (Val::Str(needle), Val::Str(text)) => text.contains(needle.as_str()),
                    (key, Val::Map(entries)) => entries.iter().any(|(existing, _)| existing == key),
                    other => panic!("membership on {other:?}"),
                };
                Val::Bool(if op == BinOp::In { found } else { !found })
            }
            (op, a, b) => panic!("unsupported {op:?} on {a:?} and {b:?}"),
        })
    }
}

fn finite(value: f64) -> Res<Val> {
    if value.is_finite() { Ok(Val::Float(value)) } else { domain("non-finite float") }
}

pub fn values_equal(left: &Val, right: &Val) -> bool {
    match (left, right) {
        (Val::Float(a), Val::Float(b)) => a.to_bits() == b.to_bits() || a == b,
        (Val::List(a), Val::List(b)) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| values_equal(a, b)),
        _ => left == right,
    }
}

fn assign_value(op: AssignOp, old: Val, rhs: Val) -> Res<Val> {
    let overflow = || Flow::Domain("integer-overflow".into());
    Ok(match (op, old, rhs) {
        (AssignOp::Set, _, rhs) => rhs,
        (AssignOp::Add, Val::Int(a), Val::Int(b)) => Val::Int(a.checked_add(b).ok_or_else(overflow)?),
        (AssignOp::Sub, Val::Int(a), Val::Int(b)) => Val::Int(a.checked_sub(b).ok_or_else(overflow)?),
        (AssignOp::Mul, Val::Int(a), Val::Int(b)) => Val::Int(a.checked_mul(b).ok_or_else(overflow)?),
        (AssignOp::Add, Val::Float(a), Val::Float(b)) => finite(a + b)?,
        (AssignOp::Sub, Val::Float(a), Val::Float(b)) => finite(a - b)?,
        (AssignOp::Mul, Val::Float(a), Val::Float(b)) => finite(a * b)?,
        (AssignOp::Add, Val::Str(a), Val::Str(b)) => Val::Str(a + &b),
        (AssignOp::Add, Val::Duration(a), Val::Duration(b)) => Val::Duration(a.checked_add(b).ok_or_else(|| Flow::Domain("duration-overflow".into()))?),
        (AssignOp::Add, Val::List(mut a), Val::List(b)) => {
            a.extend(b);
            Val::List(a)
        }
        (op, old, rhs) => panic!("unsupported assignment {op:?} {old:?} {rhs:?}"),
    })
}

fn iterate(value: Val) -> Vec<Val> {
    match value {
        Val::List(items) => items,
        Val::Map(entries) => entries
            .into_iter()
            .map(|(key, value)| Val::Rec(vec![("key".into(), key), ("value".into(), value)]))
            .collect(),
        Val::Str(text) => text.chars().map(|ch| Val::Str(ch.to_string())).collect(),
        Val::Bytes(bytes) => bytes.into_iter().map(|byte| Val::Int(i64::from(byte))).collect(),
        other => panic!("iterate {other:?}"),
    }
}

fn clamp_bounds(len: usize, start: Option<i64>, end: Option<i64>) -> (usize, usize) {
    let len = len as i64;
    let normalize = |bound: i64| -> i64 {
        let bound = if bound < 0 { len + bound } else { bound };
        bound.clamp(0, len)
    };
    let start = normalize(start.unwrap_or(0));
    let end = normalize(end.unwrap_or(len));
    if end < start { (start as usize, start as usize) } else { (start as usize, end as usize) }
}

fn slice(base: Val, start: Option<i64>, end: Option<i64>) -> Val {
    match base {
        Val::List(items) => {
            let (start, end) = clamp_bounds(items.len(), start, end);
            Val::List(items[start..end].to_vec())
        }
        Val::Str(text) => {
            let chars: Vec<char> = text.chars().collect();
            let (start, end) = clamp_bounds(chars.len(), start, end);
            Val::Str(chars[start..end].iter().collect())
        }
        Val::Bytes(bytes) => {
            let (start, end) = clamp_bounds(bytes.len(), start, end);
            Val::Bytes(bytes[start..end].to_vec())
        }
        other => panic!("slice of {other:?}"),
    }
}

fn str_arg(args: &[Val], index: usize) -> &str {
    match &args[index] {
        Val::Str(text) => text,
        other => panic!("expected Str argument, found {other:?}"),
    }
}

/// Reference semantics for the methods in [`crate::methods::ORACLE_METHODS`].
/// Programs print and iterate error messages, so the runtime texts of a
/// missing `get` (and of an empty `first`/`last`/`min`/`max` stage) are part
/// of the reference.
pub fn method(recv: &Val, name: &str, args: &[Val]) -> Res<Val> {
    Ok(match (recv, name) {
        (Val::Str(text), "upper") => Val::Str(text.to_uppercase()),
        (Val::Str(text), "lower") => Val::Str(text.to_lowercase()),
        (Val::Str(text), "trim") => Val::Str(text.trim().to_string()),
        (Val::Str(text), "reverse") => Val::Str(text.chars().rev().collect()),
        (Val::Str(text), "count_chars") => Val::Int(text.chars().count() as i64),
        (Val::Str(text), "byte_len") => Val::Int(text.len() as i64),
        (Val::Str(text), "starts_with") => Val::Bool(text.starts_with(str_arg(args, 0))),
        (Val::Str(text), "ends_with") => Val::Bool(text.ends_with(str_arg(args, 0))),
        (Val::Str(text), "replace") => Val::Str(text.replace(str_arg(args, 0), str_arg(args, 1))),
        (Val::Str(text), "split") => Val::List(
            text.split(str_arg(args, 0)).map(|part| Val::Str(part.to_string())).collect(),
        ),
        (Val::Int(value), "float") => Val::Float(*value as f64),
        (Val::Float(value), "abs") => Val::Float(value.abs()),
        (Val::List(items), "len") => Val::Int(items.len() as i64),
        (Val::List(items), "collect") => Val::List(items.clone()),
        (Val::List(items), "get") => match &args[0] {
            Val::Int(index) if *index >= 0 && (*index as usize) < items.len() => {
                Val::Ok(Box::new(items[*index as usize].clone()))
            }
            Val::Int(index) => Val::Err(format!("list index {index} is out of bounds")),
            other => panic!("list index {other:?}"),
        },
        (Val::List(items), "join") => {
            let parts: Vec<&str> = items
                .iter()
                .map(|item| match item {
                    Val::Str(text) => text.as_str(),
                    other => panic!("join of {other:?}"),
                })
                .collect();
            Val::Str(parts.join(str_arg(args, 0)))
        }
        (Val::List(items), "push") => {
            let mut items = items.clone();
            items.push(args[0].clone());
            Val::List(items)
        }
        (Val::List(items), "extend") => {
            let mut items = items.clone();
            let Val::List(more) = &args[0] else { panic!("extend argument") };
            items.extend(more.iter().cloned());
            Val::List(items)
        }
        (Val::Map(entries), "len") => Val::Int(entries.len() as i64),
        (Val::Map(entries), "get") => match entries.iter().find(|(key, _)| *key == args[0]) {
            Some((_, value)) => Val::Ok(Box::new(value.clone())),
            None => Val::Err(format!("map has no key {}", match &args[0] {
                Val::Str(key) => format!("Str({key:?})"),
                Val::Int(key) => format!("Int({key})"),
                Val::Bool(key) => format!("Bool({key})"),
                other => panic!("map key {other:?}"),
            })),
        },
        (Val::Map(entries), "keys") => Val::List(entries.iter().map(|(key, _)| key.clone()).collect()),
        (Val::Map(entries), "values") => Val::List(entries.iter().map(|(_, value)| value.clone()).collect()),
        (Val::Map(entries), "set") => {
            let mut entries = entries.clone();
            map_insert(&mut entries, args[0].clone(), args[1].clone());
            Val::Map(entries)
        }
        (Val::Map(entries), "remove") => {
            Val::Map(entries.iter().filter(|(key, _)| *key != args[0]).cloned().collect())
        }
        (Val::Bytes(bytes), "len") => Val::Int(bytes.len() as i64),
        (Val::Bytes(bytes), "starts_with") => match &args[0] {
            Val::Bytes(prefix) => Val::Bool(bytes.starts_with(prefix)),
            other => panic!("starts_with argument {other:?}"),
        },
        (Val::Path(text), "display") => Val::Str(text.clone()),
        (recv, name) => panic!("no reference semantics for {name} on {recv:?}"),
    })
}
