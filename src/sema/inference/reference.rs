//! Independent tree-and-substitution model for a bounded equality fragment.
//!
//! Atoms, invariant lists, single-parameter arrows, rank-one schemes and unique
//! record rows are modeled here. Recursive tree traversal and copied substitution
//! maps deliberately avoid the production graph's representative and scheduling
//! algorithms. This model supplies no language checking or executable evidence.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Atom {
    Unit,
    Bool,
    Int,
    Str,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TypeVariable(u32);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct RowVariable(u32);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Label(u16);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Bound {
    scheme: u32,
    index: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Term {
    Meta(TypeVariable),
    Bound(Bound),
    Atom(Atom),
    List(Box<Term>),
    Arrow(Box<Term>, Box<Term>),
    Record(Row),
}

impl Term {
    fn list(item: Term) -> Self {
        Self::List(Box::new(item))
    }

    fn arrow(parameter: Term, result: Term) -> Self {
        Self::Arrow(Box::new(parameter), Box::new(result))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tail {
    Closed,
    Meta(RowVariable),
    Bound(Bound),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Row {
    fields: Vec<(Label, Term)>,
    tail: Tail,
}

#[derive(Clone, Debug, Default)]
struct RowSubstitution {
    binding: Option<Row>,
    lacks: BTreeSet<Label>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Error {
    ForeignVariable,
    AtomConflict,
    ConstructorConflict,
    DuplicateLabel,
    MissingLabel,
    LacksConflict,
    InfiniteType,
    InfiniteRow,
    RigidConflict,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct FreeVariables {
    types: BTreeSet<TypeVariable>,
    rows: BTreeSet<RowVariable>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValueClass {
    NamedFunction,
    ImmutableValue,
    MutableStorage,
    ExpansiveInitializer,
}

impl ValueClass {
    fn permits_generalization(self) -> bool {
        matches!(self, Self::NamedFunction | Self::ImmutableValue)
    }
}

#[derive(Clone, Debug)]
struct Scheme {
    identity: u32,
    body: Term,
    type_variables: usize,
    row_lacks: Vec<BTreeSet<Label>>,
}

#[derive(Clone, Debug, Default)]
struct ReferenceSolver {
    types: BTreeMap<TypeVariable, Option<Term>>,
    rows: BTreeMap<RowVariable, RowSubstitution>,
    next_type: u32,
    next_row: u32,
    next_scheme: u32,
}

impl ReferenceSolver {
    fn fresh_type(&mut self) -> Term {
        let variable = TypeVariable(self.next_type);
        self.next_type += 1;
        self.types.insert(variable, None);
        Term::Meta(variable)
    }

    fn fresh_row(&mut self) -> Tail {
        let variable = RowVariable(self.next_row);
        self.next_row += 1;
        self.rows.insert(variable, RowSubstitution::default());
        Tail::Meta(variable)
    }

    fn record(&mut self, fields: Vec<(Label, Term)>, tail: Tail) -> Result<Term, Error> {
        let row = Row { fields, tail };
        self.probe(|solver| solver.validate_row(&row))?;
        Ok(Term::Record(row))
    }

    fn probe<T>(&mut self, action: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        let checkpoint = self.clone();
        match action(self) {
            Ok(value) => Ok(value),
            Err(error) => {
                // Retired trial variables never acquire the identity of a later
                // allocation, even though all trial substitutions are discarded.
                let next_type = self.next_type;
                let next_row = self.next_row;
                let next_scheme = self.next_scheme;
                *self = checkpoint;
                self.next_type = next_type;
                self.next_row = next_row;
                self.next_scheme = next_scheme;
                Err(error)
            }
        }
    }

    fn unify(&mut self, left: &Term, right: &Term) -> Result<(), Error> {
        self.probe(|solver| {
            solver.validate_type(left)?;
            solver.validate_type(right)?;
            solver.unify_type(left, right)
        })
    }

    fn validate_type(&mut self, ty: &Term) -> Result<(), Error> {
        match ty {
            Term::Meta(variable) => self.types.contains_key(variable).then_some(()).ok_or(Error::ForeignVariable),
            Term::Atom(_) | Term::Bound(_) => Ok(()),
            Term::List(item) => self.validate_type(item),
            Term::Arrow(parameter, result) => {
                self.validate_type(parameter)?;
                self.validate_type(result)
            }
            Term::Record(row) => self.validate_row(row),
        }
    }

    fn validate_row(&mut self, row: &Row) -> Result<(), Error> {
        let mut labels = BTreeSet::new();
        for (label, _) in &row.fields {
            if !labels.insert(*label) {
                return Err(Error::DuplicateLabel);
            }
        }
        for (_, ty) in &row.fields {
            self.validate_type(ty)?;
        }
        self.require_lacks(row.tail, &labels)
    }

    fn require_lacks(&mut self, tail: Tail, labels: &BTreeSet<Label>) -> Result<(), Error> {
        let Tail::Meta(variable) = tail else { return Ok(()); };
        let binding = self.rows.get(&variable).ok_or(Error::ForeignVariable)?.binding.clone();
        if let Some(row) = binding {
            if row.fields.iter().any(|(label, _)| labels.contains(label)) {
                return Err(Error::LacksConflict);
            }
            self.require_lacks(row.tail, labels)?;
        }
        self.rows.get_mut(&variable).unwrap().lacks.extend(labels);
        Ok(())
    }

    fn apply_type(&self, ty: &Term) -> Result<Term, Error> {
        match ty {
            Term::Meta(variable) => match self.types.get(variable).ok_or(Error::ForeignVariable)? {
                Some(binding) => self.apply_type(binding),
                None => Ok(ty.clone()),
            },
            Term::Atom(_) | Term::Bound(_) => Ok(ty.clone()),
            Term::List(item) => Ok(Term::list(self.apply_type(item)?)),
            Term::Arrow(parameter, result) => Ok(Term::arrow(self.apply_type(parameter)?, self.apply_type(result)?)),
            Term::Record(row) => Ok(Term::Record(self.apply_row(row)?)),
        }
    }

    fn apply_row(&self, row: &Row) -> Result<Row, Error> {
        let mut fields = row.fields.iter().map(|(label, ty)| Ok((*label, self.apply_type(ty)?))).collect::<Result<Vec<_>, Error>>()?;
        let mut tail = row.tail;
        while let Tail::Meta(variable) = tail {
            let substitution = self.rows.get(&variable).ok_or(Error::ForeignVariable)?;
            let Some(binding) = &substitution.binding else { break; };
            fields.extend(binding.fields.iter().map(|(label, ty)| Ok((*label, self.apply_type(ty)?))).collect::<Result<Vec<_>, Error>>()?);
            tail = binding.tail;
        }
        fields.sort_by_key(|(label, _)| *label);
        if fields.windows(2).any(|fields| fields[0].0 == fields[1].0) {
            return Err(Error::DuplicateLabel);
        }
        Ok(Row { fields, tail })
    }

    fn unify_type(&mut self, left: &Term, right: &Term) -> Result<(), Error> {
        let left = self.apply_type(left)?;
        let right = self.apply_type(right)?;
        match (&left, &right) {
            (Term::Meta(a), Term::Meta(b)) if a == b => Ok(()),
            (Term::Meta(variable), ty) | (ty, Term::Meta(variable)) => self.bind_type(*variable, ty),
            (Term::Bound(a), Term::Bound(b)) if a == b => Ok(()),
            (Term::Bound(_), _) | (_, Term::Bound(_)) => Err(Error::RigidConflict),
            (Term::Atom(a), Term::Atom(b)) => if a == b { Ok(()) } else { Err(Error::AtomConflict) },
            (Term::List(a), Term::List(b)) => self.unify_type(a, b),
            (Term::Arrow(a_parameter, a_result), Term::Arrow(b_parameter, b_result)) => {
                self.unify_type(a_parameter, b_parameter)?;
                self.unify_type(a_result, b_result)
            }
            (Term::Record(a), Term::Record(b)) => self.unify_rows(a, b),
            _ => Err(Error::ConstructorConflict),
        }
    }

    fn bind_type(&mut self, variable: TypeVariable, ty: &Term) -> Result<(), Error> {
        let ty = self.apply_type(ty)?;
        if free_variables(&ty).types.contains(&variable) {
            return Err(Error::InfiniteType);
        }
        *self.types.get_mut(&variable).ok_or(Error::ForeignVariable)? = Some(ty);
        Ok(())
    }

    fn bind_row(&mut self, variable: RowVariable, row: Row) -> Result<(), Error> {
        let row = self.apply_row(&row)?;
        if free_variables(&Term::Record(row.clone())).rows.contains(&variable) {
            return Err(Error::InfiniteRow);
        }
        let lacks = self.rows.get(&variable).ok_or(Error::ForeignVariable)?.lacks.clone();
        if row.fields.iter().any(|(label, _)| lacks.contains(label)) {
            return Err(Error::LacksConflict);
        }
        self.validate_row(&row)?;
        self.require_lacks(row.tail, &lacks)?;
        self.rows.get_mut(&variable).unwrap().binding = Some(row);
        Ok(())
    }

    fn unify_rows(&mut self, left: &Row, right: &Row) -> Result<(), Error> {
        let left = self.apply_row(left)?;
        let right = self.apply_row(right)?;
        let left_fields: BTreeMap<_, _> = left.fields.into_iter().collect();
        let right_fields: BTreeMap<_, _> = right.fields.into_iter().collect();
        for (label, left_type) in &left_fields {
            if let Some(right_type) = right_fields.get(label) {
                self.unify_type(left_type, right_type)?;
            }
        }
        let left_only = left_fields.iter().filter(|(label, _)| !right_fields.contains_key(label)).map(|(label, ty)| (*label, ty.clone())).collect::<Vec<_>>();
        let right_only = right_fields.iter().filter(|(label, _)| !left_fields.contains_key(label)).map(|(label, ty)| (*label, ty.clone())).collect::<Vec<_>>();
        match (left.tail, right.tail) {
            (Tail::Closed, Tail::Closed) => if left_only.is_empty() && right_only.is_empty() { Ok(()) } else { Err(Error::MissingLabel) },
            (Tail::Meta(variable), Tail::Closed) => {
                if !left_only.is_empty() { return Err(Error::MissingLabel); }
                self.bind_row(variable, Row { fields: right_only, tail: Tail::Closed })
            }
            (Tail::Closed, Tail::Meta(variable)) => {
                if !right_only.is_empty() { return Err(Error::MissingLabel); }
                self.bind_row(variable, Row { fields: left_only, tail: Tail::Closed })
            }
            (Tail::Meta(a), Tail::Meta(b)) if a == b => {
                if left_only.is_empty() && right_only.is_empty() { Ok(()) } else { Err(Error::InfiniteRow) }
            }
            (Tail::Meta(a), Tail::Meta(b)) => {
                if left_only.is_empty() && right_only.is_empty() {
                    return self.bind_row(a, Row { fields: Vec::new(), tail: Tail::Meta(b) });
                }
                let tail = self.fresh_row();
                self.bind_row(a, Row { fields: right_only, tail })?;
                self.bind_row(b, Row { fields: left_only, tail })
            }
            (Tail::Bound(a), Tail::Bound(b)) if a == b && left_only.is_empty() && right_only.is_empty() => Ok(()),
            _ => Err(Error::RigidConflict),
        }
    }

    fn generalize(&mut self, root: &Term, environment: &[Term], value: ValueClass) -> Result<Scheme, Error> {
        let root = self.apply_type(root)?;
        let mut free = free_variables(&root);
        for captured in environment {
            let captured = free_variables(&self.apply_type(captured)?);
            free.types.retain(|variable| !captured.types.contains(variable));
            free.rows.retain(|variable| !captured.rows.contains(variable));
        }
        if !value.permits_generalization() {
            free = FreeVariables::default();
        }
        let identity = self.next_scheme;
        self.next_scheme += 1;
        let types: BTreeMap<_, _> = free.types.into_iter().enumerate().map(|(index, variable)| (variable, Bound { scheme: identity, index: index as u32 })).collect();
        let rows: BTreeMap<_, _> = free.rows.into_iter().enumerate().map(|(index, variable)| (variable, Bound { scheme: identity, index: index as u32 })).collect();
        let row_lacks = rows.keys().map(|variable| self.rows.get(variable).ok_or(Error::ForeignVariable).map(|row| row.lacks.clone())).collect::<Result<Vec<_>, _>>()?;
        Ok(Scheme { identity, body: freeze_type(&root, &types, &rows), type_variables: types.len(), row_lacks })
    }

    fn instantiate(&mut self, scheme: &Scheme) -> Result<Term, Error> {
        let types = (0..scheme.type_variables).map(|_| self.fresh_type()).collect::<Vec<_>>();
        let mut rows = Vec::new();
        for lacks in &scheme.row_lacks {
            let tail = self.fresh_row();
            self.require_lacks(tail, lacks)?;
            rows.push(tail);
        }
        instantiate_type(&scheme.body, scheme.identity, &types, &rows)
    }
}

fn free_variables(ty: &Term) -> FreeVariables {
    fn visit(ty: &Term, variables: &mut FreeVariables) {
        match ty {
            Term::Meta(variable) => { variables.types.insert(*variable); }
            Term::Atom(_) | Term::Bound(_) => {}
            Term::List(item) => visit(item, variables),
            Term::Arrow(parameter, result) => { visit(parameter, variables); visit(result, variables); }
            Term::Record(row) => {
                for (_, ty) in &row.fields { visit(ty, variables); }
                if let Tail::Meta(variable) = row.tail { variables.rows.insert(variable); }
            }
        }
    }
    let mut variables = FreeVariables::default();
    visit(ty, &mut variables);
    variables
}

fn freeze_type(ty: &Term, types: &BTreeMap<TypeVariable, Bound>, rows: &BTreeMap<RowVariable, Bound>) -> Term {
    match ty {
        Term::Meta(variable) => types.get(variable).copied().map_or_else(|| ty.clone(), Term::Bound),
        Term::Atom(_) | Term::Bound(_) => ty.clone(),
        Term::List(item) => Term::list(freeze_type(item, types, rows)),
        Term::Arrow(parameter, result) => Term::arrow(freeze_type(parameter, types, rows), freeze_type(result, types, rows)),
        Term::Record(row) => Term::Record(Row {
            fields: row.fields.iter().map(|(label, ty)| (*label, freeze_type(ty, types, rows))).collect(),
            tail: match row.tail {
                Tail::Meta(variable) => rows.get(&variable).copied().map_or(row.tail, Tail::Bound),
                tail => tail,
            },
        }),
    }
}

fn instantiate_type(ty: &Term, scheme: u32, types: &[Term], rows: &[Tail]) -> Result<Term, Error> {
    match ty {
        Term::Bound(bound) if bound.scheme == scheme => types.get(bound.index as usize).cloned().ok_or(Error::RigidConflict),
        Term::Meta(_) | Term::Atom(_) | Term::Bound(_) => Ok(ty.clone()),
        Term::List(item) => Ok(Term::list(instantiate_type(item, scheme, types, rows)?)),
        Term::Arrow(parameter, result) => Ok(Term::arrow(instantiate_type(parameter, scheme, types, rows)?, instantiate_type(result, scheme, types, rows)?)),
        Term::Record(row) => Ok(Term::Record(Row {
            fields: row.fields.iter().map(|(label, ty)| Ok((*label, instantiate_type(ty, scheme, types, rows)?))).collect::<Result<Vec<_>, Error>>()?,
            tail: match row.tail {
                Tail::Bound(bound) if bound.scheme == scheme => *rows.get(bound.index as usize).ok_or(Error::RigidConflict)?,
                tail => tail,
            },
        })),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NormalType {
    Free(u32),
    Bound(u32),
    Atom(Atom),
    List(Box<NormalType>),
    Arrow(Box<NormalType>, Box<NormalType>),
    Record(Vec<(Label, NormalType)>, NormalTail),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum NormalTail {
    Closed,
    Free(u32, Vec<Label>),
    Bound(u32, Vec<Label>),
}

#[derive(Default)]
struct NormalNames {
    types: BTreeMap<TypeVariable, u32>,
    rows: BTreeMap<RowVariable, u32>,
    bound_types: BTreeMap<Bound, u32>,
    bound_rows: BTreeMap<Bound, u32>,
}

fn normal_index<K: Copy + Ord>(names: &mut BTreeMap<K, u32>, key: K) -> u32 {
    let next = names.len() as u32;
    *names.entry(key).or_insert(next)
}

impl NormalNames {
    fn ty(&mut self, solver: &ReferenceSolver, ty: &Term, scheme: Option<&Scheme>) -> Result<NormalType, Error> {
        let ty = solver.apply_type(ty)?;
        Ok(match ty {
            Term::Meta(variable) => NormalType::Free(normal_index(&mut self.types, variable)),
            Term::Bound(bound) => NormalType::Bound(normal_index(&mut self.bound_types, bound)),
            Term::Atom(atom) => NormalType::Atom(atom),
            Term::List(item) => NormalType::List(Box::new(self.ty(solver, &item, scheme)?)),
            Term::Arrow(parameter, result) => NormalType::Arrow(Box::new(self.ty(solver, &parameter, scheme)?), Box::new(self.ty(solver, &result, scheme)?)),
            Term::Record(row) => {
                let fields = row.fields.iter().map(|(label, ty)| Ok((*label, self.ty(solver, ty, scheme)?))).collect::<Result<Vec<_>, Error>>()?;
                let tail = match row.tail {
                    Tail::Closed => NormalTail::Closed,
                    Tail::Meta(variable) => NormalTail::Free(normal_index(&mut self.rows, variable), solver.rows.get(&variable).ok_or(Error::ForeignVariable)?.lacks.iter().copied().collect()),
                    Tail::Bound(bound) => {
                        let owner = scheme.filter(|scheme| scheme.identity == bound.scheme).ok_or(Error::RigidConflict)?;
                        let lacks = owner.row_lacks.get(bound.index as usize).ok_or(Error::RigidConflict)?;
                        NormalTail::Bound(normal_index(&mut self.bound_rows, bound), lacks.iter().copied().collect())
                    }
                };
                NormalType::Record(fields, tail)
            }
        })
    }
}

fn normalized(solver: &ReferenceSolver, roots: &[Term]) -> Result<Vec<NormalType>, Error> {
    let mut names = NormalNames::default();
    roots.iter().map(|root| names.ty(solver, root, None)).collect()
}

#[derive(Clone, Debug)]
enum Expression {
    Variable(usize),
    Atom(Atom),
    List(Box<Expression>),
    Arrow(Box<Expression>, Box<Expression>),
    Record(Vec<(Label, Expression)>, Option<usize>),
}

impl Expression {
    fn list(item: Self) -> Self {
        Self::List(Box::new(item))
    }

    fn arrow(parameter: Self, result: Self) -> Self {
        Self::Arrow(Box::new(parameter), Box::new(result))
    }

    fn variable_mask(&self) -> usize {
        match self {
            Self::Variable(index) => 1 << index,
            Self::Atom(_) => 0,
            Self::List(item) => item.variable_mask(),
            Self::Arrow(parameter, result) => parameter.variable_mask() | result.variable_mask(),
            Self::Record(fields, _) => fields.iter().fold(0, |mask, (_, ty)| mask | ty.variable_mask()),
        }
    }

    fn concrete(&self, atoms: &[Atom; 4]) -> Self {
        match self {
            Self::Variable(index) => Self::Atom(atoms[*index]),
            Self::Atom(_) => self.clone(),
            Self::List(item) => Self::list(item.concrete(atoms)),
            Self::Arrow(parameter, result) => Self::arrow(parameter.concrete(atoms), result.concrete(atoms)),
            Self::Record(fields, tail) => {
                assert!(tail.is_none(), "concrete substitution requires a closed row");
                Self::Record(fields.iter().map(|(label, ty)| (*label, ty.concrete(atoms))).collect(), None)
            }
        }
    }
}

#[derive(Clone, Debug)]
struct FragmentCase {
    name: &'static str,
    equations: Vec<(Expression, Expression)>,
    accepted: bool,
}

const TYPE_VARIABLES: usize = 4;
const ROW_VARIABLES: usize = 2;

fn reference_expression(solver: &mut ReferenceSolver, expression: &Expression, types: &[Term], rows: &[Tail]) -> Result<Term, Error> {
    match expression {
        Expression::Variable(index) => Ok(types[*index].clone()),
        Expression::Atom(atom) => Ok(Term::Atom(*atom)),
        Expression::List(item) => Ok(Term::list(reference_expression(solver, item, types, rows)?)),
        Expression::Arrow(parameter, result) => Ok(Term::arrow(reference_expression(solver, parameter, types, rows)?, reference_expression(solver, result, types, rows)?)),
        Expression::Record(fields, tail) => {
            let fields = fields.iter().map(|(label, ty)| Ok((*label, reference_expression(solver, ty, types, rows)?))).collect::<Result<Vec<_>, Error>>()?;
            solver.record(fields, tail.map_or(Tail::Closed, |index| rows[index]))
        }
    }
}

fn reference_case(case: &FragmentCase, order: &[usize]) -> Result<Vec<NormalType>, Error> {
    let mut solver = ReferenceSolver::default();
    let types = (0..TYPE_VARIABLES).map(|_| solver.fresh_type()).collect::<Vec<_>>();
    let rows = (0..ROW_VARIABLES).map(|_| solver.fresh_row()).collect::<Vec<_>>();
    solver.probe(|solver| {
        for index in order {
            let (left, right) = &case.equations[*index];
            let left = reference_expression(solver, left, &types, &rows)?;
            let right = reference_expression(solver, right, &types, &rows)?;
            solver.unify(&left, &right)?;
        }
        Ok(())
    })?;
    let mut roots = types;
    roots.extend(rows.into_iter().map(|tail| Term::Record(Row { fields: Vec::new(), tail })));
    normalized(&solver, &roots)
}

struct Generator(u64);

impl Generator {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }

    fn atom(&mut self) -> Atom {
        match self.next() % 4 {
            0 => Atom::Unit,
            1 => Atom::Bool,
            2 => Atom::Int,
            _ => Atom::Str,
        }
    }

    fn tree(&mut self, depth: usize) -> Expression {
        if depth == 0 {
            return Expression::Variable(1 + self.next() as usize % 3);
        }
        match self.next() % 4 {
            0 => Expression::Atom(self.atom()),
            1 => Expression::list(self.tree(depth - 1)),
            2 => Expression::arrow(self.tree(depth - 1), self.tree(depth - 1)),
            _ => {
                let first = self.tree(depth - 1);
                let second = self.tree(depth - 1);
                let fields = if self.next() & 1 == 0 { vec![(Label(0), first), (Label(1), second)] } else { vec![(Label(1), second), (Label(0), first)] };
                Expression::Record(fields, None)
            }
        }
    }

    fn permutation(&mut self, size: usize) -> Vec<usize> {
        let mut order = (0..size).collect::<Vec<_>>();
        for index in (1..size).rev() {
            order.swap(index, self.next() as usize % (index + 1));
        }
        order
    }

    fn case(&mut self, family: usize) -> FragmentCase {
        use Expression::{Atom as A, Record as R, Variable as V};
        let atom = self.atom();
        let different = match atom { Atom::Unit => Atom::Bool, Atom::Bool => Atom::Int, Atom::Int => Atom::Str, Atom::Str => Atom::Unit };
        match family % 12 {
            0 => {
                let tree = self.tree(3);
                let atoms = [self.atom(), self.atom(), self.atom(), self.atom()];
                FragmentCase { name: "nested-ground-anchor", accepted: true, equations: vec![(V(0), tree.clone()), (V(0), tree.concrete(&atoms)), (V(1), A(atoms[1])), (V(2), A(atoms[2])), (V(3), A(atoms[3]))] }
            }
            1 => FragmentCase { name: "infinite-container", accepted: false, equations: vec![(V(1), A(atom)), (V(0), Expression::list(Expression::list(V(0))))] },
            2 => FragmentCase { name: "arrow-sharing", accepted: true, equations: vec![(Expression::arrow(V(0), V(1)), Expression::arrow(A(atom), V(2))), (Expression::list(V(1)), Expression::list(A(different))), (V(2), A(different))] },
            3 => FragmentCase { name: "open-closed-preserves-extra-field", accepted: true, equations: vec![(R(vec![(Label(0), V(0))], Some(0)), R(vec![(Label(1), A(atom)), (Label(0), A(different))], None)), (V(1), Expression::list(V(0)))] },
            4 => FragmentCase { name: "row-field-conflict", accepted: false, equations: vec![(R(vec![(Label(0), V(0)), (Label(1), A(atom))], Some(0)), R(vec![(Label(1), A(different)), (Label(0), A(atom))], None))] },
            5 => FragmentCase { name: "row-occurs-through-field", accepted: false, equations: vec![(R(Vec::new(), Some(0)), R(vec![(Label(0), Expression::list(R(Vec::new(), Some(0))))], None))] },
            6 => FragmentCase { name: "disjoint-open-rows-share-remainder", accepted: true, equations: vec![(R(vec![(Label(0), V(0))], Some(0)), R(vec![(Label(1), V(1))], Some(1))), (R(Vec::new(), Some(0)), R(vec![(Label(2), V(2)), (Label(1), A(atom))], None)), (V(0), A(different))] },
            7 => FragmentCase { name: "duplicate-label", accepted: false, equations: vec![(R(vec![(Label(0), V(0)), (Label(0), A(atom))], None), R(vec![(Label(0), A(atom))], None))] },
            8 => FragmentCase { name: "same-row-tail-incompatible-prefix", accepted: false, equations: vec![(R(vec![(Label(0), V(0))], Some(0)), R(vec![(Label(1), V(1))], Some(0)))] },
            9 => FragmentCase { name: "infinite-arrow", accepted: false, equations: vec![(V(0), Expression::arrow(A(atom), V(1))), (V(1), Expression::list(V(0)))] },
            10 => FragmentCase { name: "closed-row-width-is-not-equality", accepted: false, equations: vec![(R(vec![(Label(0), V(0))], None), R(vec![(Label(0), A(atom)), (Label(1), A(different))], None))] },
            _ => FragmentCase { name: "nested-open-residual-relationships", accepted: true, equations: vec![(V(0), R(vec![(Label(0), R(vec![(Label(1), V(1))], Some(0))), (Label(2), V(2))], Some(1))), (V(1), A(atom))] },
        }
    }
}

#[test]
fn reference_generated_equations_preserve_principal_relationships_under_permutation() {
    let mut generator = Generator(0x75ac_942b_58e1_043d);
    for case_index in 0..144 {
        let case = generator.case(case_index);
        let order = (0..case.equations.len()).collect::<Vec<_>>();
        let original = reference_case(&case, &order);
        assert_eq!(original.is_ok(), case.accepted, "case {case_index} {}: {original:?}", case.name);
        for _ in 0..6 {
            let order = generator.permutation(case.equations.len());
            let permuted = reference_case(&case, &order);
            assert_eq!(permuted.is_ok(), original.is_ok(), "case {case_index} {} order {order:?}", case.name);
            if let (Ok(original), Ok(permuted)) = (&original, &permuted) {
                assert_eq!(permuted, original, "case {case_index} {} order {order:?}", case.name);
            }
        }
    }
}

#[test]
fn reference_value_restriction_keeps_mutable_and_expansive_aliases_monomorphic() {
    for value in [ValueClass::MutableStorage, ValueClass::ExpansiveInitializer] {
        let mut solver = ReferenceSolver::default();
        let variable = solver.fresh_type();
        let identity = Term::arrow(variable.clone(), variable);
        let scheme = solver.generalize(&identity, &[], value).unwrap();
        assert_eq!(scheme.type_variables, 0);
        let first = solver.instantiate(&scheme).unwrap();
        let alias = solver.instantiate(&scheme).unwrap();
        solver.unify(&first, &Term::arrow(Term::Atom(Atom::Int), Term::Atom(Atom::Int))).unwrap();
        assert_eq!(solver.unify(&alias, &Term::arrow(Term::Atom(Atom::Str), Term::Atom(Atom::Str))), Err(Error::AtomConflict));
    }
    let mut solver = ReferenceSolver::default();
    let variable = solver.fresh_type();
    let collection = Term::list(variable);
    let scheme = solver.generalize(&collection, &[], ValueClass::ImmutableValue).unwrap();
    assert_eq!(scheme.type_variables, 1);
    let int_list = solver.instantiate(&scheme).unwrap();
    let bool_list = solver.instantiate(&scheme).unwrap();
    solver.unify(&int_list, &Term::list(Term::Atom(Atom::Int))).unwrap();
    solver.unify(&bool_list, &Term::list(Term::Atom(Atom::Bool))).unwrap();
}

#[test]
fn reference_scheme_instantiation_freshens_rows_and_preserves_lacks() {
    let mut solver = ReferenceSolver::default();
    let value = solver.fresh_type();
    let tail = solver.fresh_row();
    let row = solver.record(vec![(Label(0), value.clone())], tail).unwrap();
    let signature = Term::arrow(row, value);
    let scheme = solver.generalize(&signature, &[], ValueClass::NamedFunction).unwrap();
    assert_eq!(scheme.type_variables, 1);
    assert_eq!(scheme.row_lacks, vec![BTreeSet::from([Label(0)])]);
    let first = solver.instantiate(&scheme).unwrap();
    let second = solver.instantiate(&scheme).unwrap();
    let narrow = solver.record(vec![(Label(0), Term::Atom(Atom::Int))], Tail::Closed).unwrap();
    let wide = solver.record(vec![(Label(1), Term::Atom(Atom::Unit)), (Label(0), Term::Atom(Atom::Str))], Tail::Closed).unwrap();
    solver.unify(&first, &Term::arrow(narrow, Term::Atom(Atom::Int))).unwrap();
    solver.unify(&second, &Term::arrow(wide, Term::Atom(Atom::Str))).unwrap();
    let mut names = NormalNames::default();
    let normalized = names.ty(&solver, &scheme.body, Some(&scheme)).unwrap();
    assert_eq!(normalized, NormalType::Arrow(Box::new(NormalType::Record(vec![(Label(0), NormalType::Bound(0))], NormalTail::Bound(0, vec![Label(0)]))), Box::new(NormalType::Bound(0))));
}

#[test]
fn reference_environment_escape_tracks_metas_reachable_through_substitution() {
    let mut solver = ReferenceSolver::default();
    let environment = solver.fresh_type();
    let captured = solver.fresh_type();
    let independent = solver.fresh_type();
    solver.unify(&environment, &Term::list(captured.clone())).unwrap();
    let signature = Term::arrow(captured.clone(), independent);
    let scheme = solver.generalize(&signature, &[environment], ValueClass::NamedFunction).unwrap();
    assert_eq!(scheme.type_variables, 1);
    let first = solver.instantiate(&scheme).unwrap();
    let second = solver.instantiate(&scheme).unwrap();
    solver.unify(&first, &Term::arrow(Term::Atom(Atom::Int), Term::Atom(Atom::Bool))).unwrap();
    assert_eq!(solver.unify(&second, &Term::arrow(Term::Atom(Atom::Str), Term::Atom(Atom::Unit))), Err(Error::AtomConflict));
    assert_eq!(solver.apply_type(&captured).unwrap(), Term::Atom(Atom::Int));
}

#[test]
fn reference_failed_row_probe_restores_lacks_and_common_field_bindings() {
    let mut solver = ReferenceSolver::default();
    let field = solver.fresh_type();
    let tail = solver.fresh_row();
    let open = solver.record(Vec::new(), tail).unwrap();
    let before = normalized(&solver, &[field.clone(), open.clone()]).unwrap();
    assert_eq!(solver.probe(|trial| {
        let with_label = trial.record(vec![(Label(0), field.clone())], tail)?;
        let concrete = trial.record(vec![(Label(0), Term::Atom(Atom::Int)), (Label(1), Term::Atom(Atom::Str))], Tail::Closed)?;
        trial.unify(&with_label, &concrete)?;
        trial.unify(&field, &Term::Atom(Atom::Bool))
    }), Err(Error::AtomConflict));
    assert_eq!(normalized(&solver, &[field, open.clone()]).unwrap(), before);
    let previously_forbidden = solver.record(vec![(Label(0), Term::Atom(Atom::Bool))], Tail::Closed).unwrap();
    solver.unify(&open, &previously_forbidden).unwrap();
}

#[test]
fn reference_substitution_rejects_cross_kind_recursive_equations() {
    let mut solver = ReferenceSolver::default();
    let ty = solver.fresh_type();
    let tail = solver.fresh_row();
    let record = solver.record(vec![(Label(0), ty.clone())], tail).unwrap();
    let list = Term::list(record.clone());
    solver.unify(&ty, &Term::Atom(Atom::Int)).unwrap();
    let right = solver.record(vec![(Label(0), Term::Atom(Atom::Int)), (Label(1), list)], Tail::Closed).unwrap();
    assert_eq!(solver.unify(&record, &right), Err(Error::InfiniteRow));
    let still_open = normalized(&solver, &[record]).unwrap();
    assert!(matches!(still_open[0], NormalType::Record(_, NormalTail::Free(_, _))));
}

#[test]
fn reference_substitution_preserves_captured_metas_and_row_lacks() {
    let mut solver = ReferenceSolver::default();
    let captured = solver.fresh_type();
    let result = solver.fresh_type();
    let tail = solver.fresh_row();
    let record = solver.record(vec![(Label(0), captured.clone())], tail).unwrap();
    let callable = Term::arrow(record.clone(), result);
    let scheme = solver.generalize(&callable, &[record], ValueClass::NamedFunction).unwrap();
    assert_eq!(scheme.type_variables, 1);
    assert!(scheme.row_lacks.is_empty());
    let first = solver.instantiate(&scheme).unwrap();
    let second = solver.instantiate(&scheme).unwrap();
    let Term::Arrow(first_record, first_result) = first else { panic!("arrow expected"); };
    let Term::Arrow(second_record, second_result) = second else { panic!("arrow expected"); };
    solver.unify(&first_record, &second_record).unwrap();
    solver.unify(&first_result, &Term::Atom(Atom::Bool)).unwrap();
    solver.unify(&second_result, &Term::Atom(Atom::Str)).unwrap();
    assert_eq!(solver.apply_type(&captured).unwrap(), captured);
}

#[test]
fn reference_failed_probe_retires_variables_and_restores_substitutions() {
    let mut solver = ReferenceSolver::default();
    let original = solver.fresh_type();
    let before = normalized(&solver, &[original.clone()]).unwrap();
    let mut escaped = None;
    assert_eq!(solver.probe(|trial| {
        escaped = Some(trial.fresh_type());
        trial.unify(&original, &Term::Atom(Atom::Int))?;
        trial.unify(&original, &Term::Atom(Atom::Str))
    }), Err(Error::AtomConflict));
    assert_eq!(normalized(&solver, &[original.clone()]).unwrap(), before);
    let escaped = escaped.unwrap();
    let later = solver.fresh_type();
    assert_ne!(escaped, later);
    assert_eq!(solver.unify(&escaped, &later), Err(Error::ForeignVariable));
    solver.unify(&original, &Term::Atom(Atom::Bool)).unwrap();
}

fn graph_span() -> crate::source::Span {
    crate::source::Span::new(crate::source::SourceId::new(0), 0, 1)
}

fn graph_label(label: Label) -> crate::symbol::Name {
    crate::symbol::Name::intern(match label.0 {
        0 => "a", 1 => "b", 2 => "c", 3 => "d", _ => panic!("label outside reference fragment"),
    })
}

fn reference_label(label: crate::symbol::Name) -> Label {
    (0..4).map(Label).find(|candidate| graph_label(*candidate) == label).expect("label outside reference fragment")
}

fn graph_atom(atom: Atom) -> super::Atom {
    match atom { Atom::Unit => super::Atom::Unit, Atom::Bool => super::Atom::Bool, Atom::Int => super::Atom::Int, Atom::Str => super::Atom::Str }
}

fn graph_arrow(graph: &mut super::InferenceContext, parameter: super::TypeId, result: super::TypeId) -> Result<super::TypeId, super::InferenceError> {
    graph.arrow(super::Arrow {
        kind: super::CallableKind::Pure,
        params: vec![super::Parameter { label: crate::symbol::Name::intern("value"), ty: parameter, defaulted: false, rest: false }],
        result,
        effects: super::EffectSummary::Closed(super::EffectSet::EMPTY),
    })
}

fn graph_record(graph: &mut super::InferenceContext, fields: Vec<(Label, super::TypeId)>, tail: Option<super::TypeId>) -> Result<super::TypeId, super::InferenceError> {
    let fields = fields.into_iter().map(|(label, ty)| super::RowField { label: graph_label(label), ty }).collect();
    let row = graph.row(fields, tail)?;
    graph.record(row)
}

fn graph_expression(graph: &mut super::InferenceContext, expression: &Expression, types: &[super::TypeId], rows: &[super::TypeId]) -> Result<super::TypeId, super::InferenceError> {
    match expression {
        Expression::Variable(index) => Ok(types[*index]),
        Expression::Atom(atom) => graph.atom(graph_atom(*atom)),
        Expression::List(item) => { let item = graph_expression(graph, item, types, rows)?; graph.list(item) }
        Expression::Arrow(parameter, result) => {
            let parameter = graph_expression(graph, parameter, types, rows)?;
            let result = graph_expression(graph, result, types, rows)?;
            graph_arrow(graph, parameter, result)
        }
        Expression::Record(fields, tail) => {
            let fields = fields.iter().map(|(label, ty)| Ok((*label, graph_expression(graph, ty, types, rows)?))).collect::<Result<Vec<_>, super::InferenceError>>()?;
            graph_record(graph, fields, tail.map(|index| rows[index]))
        }
    }
}

/// Read-only structural normalization preserves relationships across all roots.
/// Representative numbers and allocation order are deliberately not compared.
#[derive(Default)]
struct GraphNames {
    active_types: BTreeSet<super::TypeId>,
    types: BTreeMap<super::MetaId, u32>,
    rows: BTreeMap<super::MetaId, u32>,
    bound_types: BTreeMap<(super::SchemeId, u32), u32>,
    bound_rows: BTreeMap<(super::SchemeId, u32), u32>,
}

impl GraphNames {
    fn ty(&mut self, graph: &super::InferenceContext, ty: super::TypeId) -> NormalType {
        let ty = graph.resolved(ty).unwrap();
        assert!(self.active_types.insert(ty), "cyclic type escaped occurs checking");
        let normalized = match graph.node(ty).unwrap() {
            super::TypeNode::Atom(atom) => NormalType::Atom(match atom {
                super::Atom::Unit => Atom::Unit, super::Atom::Bool => Atom::Bool,
                super::Atom::Int => Atom::Int, super::Atom::Str => Atom::Str,
                _ => panic!("atom outside reference fragment"),
            }),
            super::TypeNode::Meta(id) => {
                assert_eq!(graph.variable(ty).unwrap().unwrap().kind, super::VariableKind::Type);
                NormalType::Free(normal_index(&mut self.types, *id))
            }
            super::TypeNode::Rigid { scope, index, kind } => {
                assert_eq!(*kind, super::VariableKind::Type);
                NormalType::Bound(normal_index(&mut self.bound_types, (*scope, *index)))
            }
            super::TypeNode::List(item) => NormalType::List(Box::new(self.ty(graph, *item))),
            super::TypeNode::Arrow(arrow) => {
                assert_eq!(arrow.kind, super::CallableKind::Pure);
                assert_eq!(arrow.params.len(), 1);
                assert_eq!(arrow.params[0].label, crate::symbol::Name::intern("value"));
                assert!(!arrow.params[0].defaulted && !arrow.params[0].rest);
                assert_eq!(arrow.effects, super::EffectSummary::Closed(super::EffectSet::EMPTY));
                NormalType::Arrow(Box::new(self.ty(graph, arrow.params[0].ty)), Box::new(self.ty(graph, arrow.result)))
            }
            super::TypeNode::Record(row) => self.row(graph, *row),
            _ => panic!("constructor outside reference fragment"),
        };
        self.active_types.remove(&ty);
        normalized
    }

    fn row(&mut self, graph: &super::InferenceContext, mut row: super::RowId) -> NormalType {
        let mut fields = Vec::new();
        let mut visited = BTreeSet::new();
        let terminal = loop {
            assert!(visited.insert(row), "cyclic row escaped occurs checking");
            let descriptor = graph.row_data(row).unwrap();
            fields.extend(descriptor.fields.iter().map(|field| (reference_label(field.label), field.ty)));
            let Some(tail) = descriptor.tail else { break None; };
            let tail = graph.resolved(tail).unwrap();
            match graph.node(tail).unwrap() {
                super::TypeNode::Row(next) => row = *next,
                _ => break Some(tail),
            }
        };
        fields.sort_by_key(|(label, _)| *label);
        assert!(fields.windows(2).all(|pair| pair[0].0 != pair[1].0));
        let fields = fields.into_iter().map(|(label, ty)| (label, self.ty(graph, ty))).collect();
        let tail = match terminal {
            None => NormalTail::Closed,
            Some(tail) => match graph.node(tail).unwrap() {
                super::TypeNode::Meta(id) => {
                    let variable = graph.variable(tail).unwrap().unwrap();
                    assert_eq!(variable.kind, super::VariableKind::Row);
                    let mut lacks = variable.lacks.iter().copied().map(reference_label).collect::<Vec<_>>();
                    lacks.sort();
                    NormalTail::Free(normal_index(&mut self.rows, *id), lacks)
                }
                super::TypeNode::Rigid { scope, index, kind } => {
                    assert_eq!(*kind, super::VariableKind::Row);
                    let quantifier = &graph.scheme(*scope).unwrap().quantifiers[*index as usize];
                    assert_eq!(quantifier.kind, super::VariableKind::Row);
                    let mut lacks = quantifier.lacks.iter().copied().map(reference_label).collect::<Vec<_>>();
                    lacks.sort();
                    NormalTail::Bound(normal_index(&mut self.bound_rows, (*scope, *index)), lacks)
                }
                _ => panic!("non-row tail"),
            },
        };
        NormalType::Record(fields, tail)
    }

}

fn graph_normalized(graph: &super::InferenceContext, roots: &[super::TypeId]) -> Vec<NormalType> {
    let mut names = GraphNames::default();
    roots.iter().map(|root| names.ty(graph, *root)).collect()
}

fn graph_case(case: &FragmentCase, order: &[usize]) -> Result<Vec<NormalType>, super::InferenceError> {
    let mut graph = super::InferenceContext::default();
    let reason = graph.reason(graph_span(), None)?;
    let types = (0..TYPE_VARIABLES).map(|_| graph.fresh(1, graph_span())).collect::<Result<Vec<_>, _>>()?;
    let rows = (0..ROW_VARIABLES).map(|_| graph.fresh_row(1, graph_span())).collect::<Result<Vec<_>, _>>()?;
    graph.probe(|graph| {
        for index in order {
            let (left, right) = &case.equations[*index];
            let left = graph_expression(graph, left, &types, &rows)?;
            let right = graph_expression(graph, right, &types, &rows)?;
            graph.unify(left, right, reason)?;
        }
        Ok(())
    })?;
    let mut roots = types;
    for tail in rows { roots.push(graph_record(&mut graph, Vec::new(), Some(tail))?); }
    Ok(graph_normalized(&graph, &roots))
}

#[test]
fn production_matches_independent_generated_equations_and_permutations() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        let mut generator = Generator(0x75ac_942b_58e1_043d);
        for case_index in 0..144 {
            let case = generator.case(case_index);
            for permutation in 0..7 {
                let order = if permutation == 0 { (0..case.equations.len()).collect() } else { generator.permutation(case.equations.len()) };
                let reference = reference_case(&case, &order);
                let production = graph_case(&case, &order);
                assert_eq!(reference.is_ok(), case.accepted, "reference case {case_index} {} order {order:?}: {reference:?}", case.name);
                assert_eq!(production.is_ok(), reference.is_ok(), "production case {case_index} {} order {order:?}: {production:?}", case.name);
                if let (Ok(reference), Ok(production)) = (reference, production) {
                    assert_eq!(production, reference, "case {case_index} {} order {order:?}", case.name);
                }
            }
        }
    });
}

fn scheme_normalized(reference: &ReferenceSolver, scheme: &Scheme) -> NormalType {
    NormalNames::default().ty(reference, &scheme.body, Some(scheme)).unwrap()
}

fn assert_scheme_matches(reference: &ReferenceSolver, reference_scheme: &Scheme, graph: &super::InferenceContext, graph_scheme: super::SchemeId) {
    let actual = graph.scheme(graph_scheme).unwrap();
    assert_eq!(actual.quantifiers.iter().filter(|item| item.kind == super::VariableKind::Type).count(), reference_scheme.type_variables);
    assert_eq!(actual.quantifiers.iter().filter(|item| item.kind == super::VariableKind::Row).count(), reference_scheme.row_lacks.len());
    assert!(actual.requirements.is_empty());
    assert_eq!(GraphNames::default().ty(graph, actual.body), scheme_normalized(reference, reference_scheme));
}

#[test]
fn production_matches_rank_one_freshening_and_monomorphic_aliases() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        // The fragment receives the value classification as an input. It checks
        // the resulting generalization policy, not the language's classifier.
        for value in [ValueClass::NamedFunction, ValueClass::ImmutableValue, ValueClass::MutableStorage, ValueClass::ExpansiveInitializer] {
            for collection in [false, true] {
                let mut reference = ReferenceSolver::default();
                let variable = reference.fresh_type();
                let signature = if collection { Term::list(variable) } else { Term::arrow(variable.clone(), variable) };
                let reference_scheme = reference.generalize(&signature, &[], value).unwrap();
                let mut graph = super::InferenceContext::default();
                let reason = graph.reason(graph_span(), None).unwrap();
                let variable = graph.fresh(2, graph_span()).unwrap();
                let signature = if collection { graph.list(variable).unwrap() } else { graph_arrow(&mut graph, variable, variable).unwrap() };
                let policy = if value.permits_generalization() { super::Generalization::Allowed } else { super::Generalization::Monomorphic };
                let graph_scheme = graph.generalize(signature, 0, policy, &[]).unwrap();
                assert_scheme_matches(&reference, &reference_scheme, &graph, graph_scheme);
                let mut reference_instances = Vec::new();
                let mut graph_instances = Vec::new();
                for _ in 0..3 {
                    reference_instances.push(reference.instantiate(&reference_scheme).unwrap());
                    graph_instances.push(graph.instantiate(graph_scheme, 2, reason).unwrap().ty);
                }
                assert_eq!(graph_normalized(&graph, &graph_instances), normalized(&reference, &reference_instances).unwrap());
                for (index, atom) in [Atom::Int, Atom::Str, Atom::Bool].into_iter().enumerate() {
                    let reference_actual = if collection { Term::list(Term::Atom(atom)) } else { Term::arrow(Term::Atom(atom), Term::Atom(atom)) };
                    let atom = graph.atom(graph_atom(atom)).unwrap();
                    let graph_actual = if collection { graph.list(atom).unwrap() } else { graph_arrow(&mut graph, atom, atom).unwrap() };
                    let reference_result = reference.unify(&reference_instances[index], &reference_actual);
                    let graph_result = graph.unify(graph_instances[index], graph_actual, reason);
                    assert_eq!(reference_result.is_ok(), index == 0 || value.permits_generalization());
                    assert_eq!(graph_result.is_ok(), reference_result.is_ok(), "{value:?}, collection {collection}, instance {index}");
                    assert_eq!(graph_normalized(&graph, &graph_instances), normalized(&reference, &reference_instances).unwrap());
                }
            }
        }
    });
}

#[test]
fn production_matches_quantified_open_rows_and_residual_lacks() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        let mut reference = ReferenceSolver::default();
        let item = reference.fresh_type();
        let tail = reference.fresh_row();
        let row = reference.record(vec![(Label(0), item.clone())], tail).unwrap();
        let signature = Term::arrow(row, item);
        let reference_scheme = reference.generalize(&signature, &[], ValueClass::NamedFunction).unwrap();
        let mut graph = super::InferenceContext::default();
        let reason = graph.reason(graph_span(), None).unwrap();
        let item = graph.fresh(2, graph_span()).unwrap();
        let tail = graph.fresh_row(2, graph_span()).unwrap();
        let row = graph_record(&mut graph, vec![(Label(0), item)], Some(tail)).unwrap();
        let signature = graph_arrow(&mut graph, row, item).unwrap();
        let graph_scheme = graph.generalize(signature, 0, super::Generalization::Allowed, &[]).unwrap();
        assert_scheme_matches(&reference, &reference_scheme, &graph, graph_scheme);
        let mut reference_instances = Vec::new();
        let mut graph_instances = Vec::new();
        for _ in 0..3 {
            reference_instances.push(reference.instantiate(&reference_scheme).unwrap());
            graph_instances.push(graph.instantiate(graph_scheme, 2, reason).unwrap().ty);
        }
        assert_eq!(graph_normalized(&graph, &graph_instances), normalized(&reference, &reference_instances).unwrap());
        for (index, atom) in [Atom::Int, Atom::Str, Atom::Bool].into_iter().enumerate() {
            let mut reference_fields = vec![(Label(0), Term::Atom(atom))];
            let atom_id = graph.atom(graph_atom(atom)).unwrap();
            let mut graph_fields = vec![(Label(0), atom_id)];
            for extra in 0..index {
                reference_fields.push((Label(extra as u16 + 1), Term::Atom(Atom::Unit)));
                let unit = graph.atom(super::Atom::Unit).unwrap();
                graph_fields.push((Label(extra as u16 + 1), unit));
            }
            let reference_row = reference.record(reference_fields, Tail::Closed).unwrap();
            let graph_row = graph_record(&mut graph, graph_fields, None).unwrap();
            let graph_actual = graph_arrow(&mut graph, graph_row, atom_id).unwrap();
            reference.unify(&reference_instances[index], &Term::arrow(reference_row, Term::Atom(atom))).unwrap();
            graph.unify(graph_instances[index], graph_actual, reason).unwrap();
        }
        assert_eq!(graph_normalized(&graph, &graph_instances), normalized(&reference, &reference_instances).unwrap());
    });
}

#[test]
fn production_matches_environment_escape_through_substitutions_and_capture() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        for explicit_capture in [false, true] {
            let mut reference = ReferenceSolver::default();
            let environment = reference.fresh_type();
            let captured = reference.fresh_type();
            let independent = reference.fresh_type();
            reference.unify(&environment, &Term::list(captured.clone())).unwrap();
            let reference_captured = captured.clone();
            let signature = Term::arrow(captured.clone(), independent);
            let reference_scheme = reference.generalize(&signature, &[environment], ValueClass::NamedFunction).unwrap();
            let mut graph = super::InferenceContext::default();
            let reason = graph.reason(graph_span(), None).unwrap();
            let environment = graph.fresh(if explicit_capture { 2 } else { 0 }, graph_span()).unwrap();
            let captured = graph.fresh(2, graph_span()).unwrap();
            let independent = graph.fresh(2, graph_span()).unwrap();
            let list = graph.list(captured).unwrap();
            graph.unify(environment, list, reason).unwrap();
            if explicit_capture { graph.capture(environment, 0, reason).unwrap(); }
            let signature = graph_arrow(&mut graph, captured, independent).unwrap();
            let graph_scheme = graph.generalize(signature, 0, super::Generalization::Allowed, &[]).unwrap();
            assert_scheme_matches(&reference, &reference_scheme, &graph, graph_scheme);
            let reference_first = reference.instantiate(&reference_scheme).unwrap();
            let reference_second = reference.instantiate(&reference_scheme).unwrap();
            let graph_first = graph.instantiate(graph_scheme, 2, reason).unwrap().ty;
            let graph_second = graph.instantiate(graph_scheme, 2, reason).unwrap().ty;
            for (reference_instance, graph_instance, input, output, accepted) in [
                (&reference_first, graph_first, Atom::Int, Atom::Bool, true),
                (&reference_second, graph_second, Atom::Str, Atom::Unit, false),
                (&reference_second, graph_second, Atom::Int, Atom::Str, true),
            ] {
                let reference_actual = Term::arrow(Term::Atom(input), Term::Atom(output));
                let input = graph.atom(graph_atom(input)).unwrap();
                let output = graph.atom(graph_atom(output)).unwrap();
                let actual = graph_arrow(&mut graph, input, output).unwrap();
                assert_eq!(reference.unify(reference_instance, &reference_actual).is_ok(), accepted);
                assert_eq!(graph.unify(graph_instance, actual, reason).is_ok(), accepted);
                assert_eq!(graph_normalized(&graph, &[graph_first, graph_second, captured]), normalized(&reference, &[reference_first.clone(), reference_second.clone(), reference_captured.clone()]).unwrap());
            }
        }
    });
}

#[test]
fn production_matches_failed_probe_isolation_and_retired_handles() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        let mut reference = ReferenceSolver::default();
        let item = reference.fresh_type();
        let tail = reference.fresh_row();
        let open = reference.record(Vec::new(), tail).unwrap();
        let before = normalized(&reference, &[item.clone(), open.clone()]).unwrap();
        let mut graph = super::InferenceContext::default();
        let reason = graph.reason(graph_span(), None).unwrap();
        let graph_item = graph.fresh(2, graph_span()).unwrap();
        let graph_tail = graph.fresh_row(2, graph_span()).unwrap();
        let graph_open = graph_record(&mut graph, Vec::new(), Some(graph_tail)).unwrap();
        assert_eq!(graph_normalized(&graph, &[graph_item, graph_open]), before);
        let mut reference_retired = None;
        let mut graph_retired = None;
        let reference_result = reference.probe(|trial| {
            reference_retired = Some(trial.fresh_type());
            let row = trial.record(vec![(Label(0), item.clone())], tail)?;
            let actual = trial.record(vec![(Label(0), Term::Atom(Atom::Int)), (Label(1), Term::Atom(Atom::Str))], Tail::Closed)?;
            trial.unify(&row, &actual)?;
            trial.unify(&item, &Term::Atom(Atom::Bool))
        });
        let graph_result = graph.probe(|trial| {
            graph_retired = Some(trial.fresh(2, graph_span())?);
            trial.capture(graph_item, 0, reason)?;
            let row = graph_record(trial, vec![(Label(0), graph_item)], Some(graph_tail))?;
            let int = trial.atom(super::Atom::Int)?;
            let string = trial.atom(super::Atom::Str)?;
            let actual = graph_record(trial, vec![(Label(0), int), (Label(1), string)], None)?;
            trial.unify(row, actual, reason)?;
            let boolean = trial.atom(super::Atom::Bool)?;
            trial.unify(graph_item, boolean, reason)
        });
        assert!(reference_result.is_err() && graph_result.is_err());
        assert_eq!(normalized(&reference, &[item.clone(), open.clone()]).unwrap(), before);
        assert_eq!(graph_normalized(&graph, &[graph_item, graph_open]), before);
        assert_eq!(graph.variable(graph_item).unwrap().unwrap().level, 2);
        let later = reference.fresh_type();
        assert_eq!(reference.unify(&reference_retired.unwrap(), &later), Err(Error::ForeignVariable));
        let later = graph.fresh(2, graph_span()).unwrap();
        assert_ne!(graph_retired.unwrap(), later);
        assert!(matches!(graph.unify(graph_retired.unwrap(), later, reason), Err(super::InferenceError::ForeignHandle)));
        let reference_row = reference.record(vec![(Label(0), Term::Atom(Atom::Bool))], Tail::Closed).unwrap();
        let boolean = graph.atom(super::Atom::Bool).unwrap();
        let graph_row = graph_record(&mut graph, vec![(Label(0), boolean)], None).unwrap();
        reference.unify(&open, &reference_row).unwrap();
        graph.unify(graph_open, graph_row, reason).unwrap();
        assert_eq!(graph_normalized(&graph, &[graph_item, graph_open]), normalized(&reference, &[item, open]).unwrap());
    });
}

#[test]
fn production_matches_captured_open_row_aliases() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        let mut reference = ReferenceSolver::default();
        let item = reference.fresh_type();
        let result = reference.fresh_type();
        let tail = reference.fresh_row();
        let row = reference.record(vec![(Label(0), item)], tail).unwrap();
        let signature = Term::arrow(row.clone(), result);
        let reference_scheme = reference.generalize(&signature, &[row], ValueClass::NamedFunction).unwrap();
        let mut graph = super::InferenceContext::default();
        let reason = graph.reason(graph_span(), None).unwrap();
        let item = graph.fresh(2, graph_span()).unwrap();
        let result = graph.fresh(2, graph_span()).unwrap();
        let tail = graph.fresh_row(2, graph_span()).unwrap();
        let row = graph_record(&mut graph, vec![(Label(0), item)], Some(tail)).unwrap();
        graph.capture(row, 0, reason).unwrap();
        let signature = graph_arrow(&mut graph, row, result).unwrap();
        let graph_scheme = graph.generalize(signature, 0, super::Generalization::Allowed, &[]).unwrap();
        assert_scheme_matches(&reference, &reference_scheme, &graph, graph_scheme);
        let first = reference.instantiate(&reference_scheme).unwrap();
        let second = reference.instantiate(&reference_scheme).unwrap();
        let graph_first = graph.instantiate(graph_scheme, 2, reason).unwrap().ty;
        let graph_second = graph.instantiate(graph_scheme, 2, reason).unwrap().ty;
        for (reference_instance, graph_instance, extra_field, output, accepted) in [
            (&first, graph_first, false, Atom::Bool, true),
            (&second, graph_second, true, Atom::Str, false),
            (&second, graph_second, false, Atom::Str, true),
        ] {
            let mut reference_fields = vec![(Label(0), Term::Atom(Atom::Int))];
            let int = graph.atom(super::Atom::Int).unwrap();
            let mut graph_fields = vec![(Label(0), int)];
            if extra_field {
                reference_fields.push((Label(1), Term::Atom(Atom::Unit)));
                let unit = graph.atom(super::Atom::Unit).unwrap();
                graph_fields.push((Label(1), unit));
            }
            let row = reference.record(reference_fields, Tail::Closed).unwrap();
            let graph_row = graph_record(&mut graph, graph_fields, None).unwrap();
            let graph_output = graph.atom(graph_atom(output)).unwrap();
            let actual = graph_arrow(&mut graph, graph_row, graph_output).unwrap();
            assert_eq!(reference.unify(reference_instance, &Term::arrow(row, Term::Atom(output))).is_ok(), accepted);
            assert_eq!(graph.unify(graph_instance, actual, reason).is_ok(), accepted);
            assert_eq!(graph_normalized(&graph, &[graph_first, graph_second]), normalized(&reference, &[first.clone(), second.clone()]).unwrap());
        }
    });
}

#[test]
fn production_matches_generated_rank_one_schemes_with_captured_variables() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        let mut generator = Generator(0x901b_ef39_740a_1c65);
        for case_index in 0..96 {
            let expression = generator.tree(3);
            let environment_mask = generator.next() as usize & 15;
            let value = match case_index % 4 {
                0 => ValueClass::NamedFunction,
                1 => ValueClass::ImmutableValue,
                2 => ValueClass::MutableStorage,
                _ => ValueClass::ExpansiveInitializer,
            };
            let mut reference = ReferenceSolver::default();
            let reference_variables = (0..TYPE_VARIABLES).map(|_| reference.fresh_type()).collect::<Vec<_>>();
            let environment = reference_variables.iter().enumerate().filter(|(index, _)| environment_mask & (1 << index) != 0).map(|(_, ty)| ty.clone()).collect::<Vec<_>>();
            let signature = reference_expression(&mut reference, &expression, &reference_variables, &[]).unwrap();
            let reference_scheme = reference.generalize(&signature, &environment, value).unwrap();
            let mut graph = super::InferenceContext::default();
            let reason = graph.reason(graph_span(), None).unwrap();
            let graph_variables = (0..TYPE_VARIABLES).map(|index| graph.fresh(if environment_mask & (1 << index) != 0 { 0 } else { 2 }, graph_span()).unwrap()).collect::<Vec<_>>();
            let signature = graph_expression(&mut graph, &expression, &graph_variables, &[]).unwrap();
            let policy = if value.permits_generalization() { super::Generalization::Allowed } else { super::Generalization::Monomorphic };
            let graph_scheme = graph.generalize(signature, 0, policy, &[]).unwrap();
            assert_scheme_matches(&reference, &reference_scheme, &graph, graph_scheme);
            let present = expression.variable_mask();
            let pinned = if value.permits_generalization() { present & environment_mask } else { present };
            assert_eq!(reference_scheme.type_variables, (present & !pinned).count_ones() as usize);
            let mut pinned_atoms = [None; TYPE_VARIABLES];
            let mut reference_roots = Vec::new();
            let mut graph_roots = Vec::new();
            for _ in 0..3 {
                reference_roots.push(reference.instantiate(&reference_scheme).unwrap());
                graph_roots.push(graph.instantiate(graph_scheme, 2, reason).unwrap().ty);
            }
            // Captured variables stay live in the environment. Generalized
            // declaration variables are represented by the scheme body instead.
            reference_roots.extend(reference_variables.iter().enumerate().filter(|(index, _)| pinned & (1 << index) != 0).map(|(_, ty)| ty.clone()));
            graph_roots.extend(graph_variables.iter().enumerate().filter(|(index, _)| pinned & (1 << index) != 0).map(|(_, ty)| *ty));
            assert_eq!(graph_normalized(&graph, &graph_roots), normalized(&reference, &reference_roots).unwrap(), "scheme case {case_index}");
            for instance in 0..3 {
                let atoms = [generator.atom(), generator.atom(), generator.atom(), generator.atom()];
                let accepted = (0..TYPE_VARIABLES).all(|index| pinned & (1 << index) == 0 || pinned_atoms[index].is_none_or(|previous| previous == atoms[index]));
                let concrete = expression.concrete(&atoms);
                let expected = reference_expression(&mut reference, &concrete, &[], &[]).unwrap();
                let actual = graph_expression(&mut graph, &concrete, &[], &[]).unwrap();
                let expected = reference.unify(&reference_roots[instance], &expected);
                let actual = graph.unify(graph_roots[instance], actual, reason);
                assert_eq!(expected.is_ok(), accepted, "reference scheme case {case_index}, instance {instance}, {value:?}, environment {environment_mask}");
                if accepted {
                    for index in 0..TYPE_VARIABLES { if pinned & (1 << index) != 0 { pinned_atoms[index] = Some(atoms[index]); } }
                }
                assert_eq!(actual.is_ok(), expected.is_ok(), "scheme case {case_index}, instance {instance}, {value:?}, environment {environment_mask}: {actual:?}, {expected:?}");
                assert_eq!(graph_normalized(&graph, &graph_roots), normalized(&reference, &reference_roots).unwrap(), "scheme case {case_index}, instance {instance}");
            }
        }
    });
}

#[test]
fn production_matches_immutable_alias_of_a_monomorphic_binding() {
    crate::symbol::SymbolOwner::new().with_current(|| {
        for value in [ValueClass::MutableStorage, ValueClass::ExpansiveInitializer] {
            let mut reference = ReferenceSolver::default();
            let variable = reference.fresh_type();
            let original = Term::arrow(variable.clone(), variable);
            let monomorphic = reference.generalize(&original, &[], value).unwrap();
            let alias = reference.instantiate(&monomorphic).unwrap();
            let alias_scheme = reference.generalize(&alias, &[original], ValueClass::ImmutableValue).unwrap();
            let mut graph = super::InferenceContext::default();
            let reason = graph.reason(graph_span(), None).unwrap();
            let variable = graph.fresh(2, graph_span()).unwrap();
            let original = graph_arrow(&mut graph, variable, variable).unwrap();
            let monomorphic = graph.generalize(original, 0, super::Generalization::Monomorphic, &[]).unwrap();
            let alias = graph.instantiate(monomorphic, 2, reason).unwrap().ty;
            let graph_alias_scheme = graph.generalize(alias, 0, super::Generalization::Allowed, &[]).unwrap();
            assert_scheme_matches(&reference, &alias_scheme, &graph, graph_alias_scheme);
            let reference_first = reference.instantiate(&alias_scheme).unwrap();
            let reference_second = reference.instantiate(&alias_scheme).unwrap();
            let graph_first = graph.instantiate(graph_alias_scheme, 2, reason).unwrap().ty;
            let graph_second = graph.instantiate(graph_alias_scheme, 2, reason).unwrap().ty;
            for (reference_instance, graph_instance, atom, accepted) in [
                (&reference_first, graph_first, Atom::Int, true),
                (&reference_second, graph_second, Atom::Str, false),
            ] {
                let actual = Term::arrow(Term::Atom(atom), Term::Atom(atom));
                let atom = graph.atom(graph_atom(atom)).unwrap();
                let graph_actual = graph_arrow(&mut graph, atom, atom).unwrap();
                assert_eq!(reference.unify(reference_instance, &actual).is_ok(), accepted);
                assert_eq!(graph.unify(graph_instance, graph_actual, reason).is_ok(), accepted);
                assert_eq!(graph_normalized(&graph, &[graph_first, graph_second]), normalized(&reference, &[reference_first.clone(), reference_second.clone()]).unwrap());
            }
        }
    });
}
