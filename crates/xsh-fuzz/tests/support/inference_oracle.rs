use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeVariable(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RowVariable(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeParameter(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RowParameter(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Atom {
    Int,
    Str,
    Bool,
    Unit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Type {
    Variable(TypeVariable),
    Atom(Atom),
    List(Box<Self>),
    Optional(Box<Self>),
    Result(Box<Self>, Box<Self>),
    Record(Row),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row {
    pub fields: BTreeMap<String, Type>,
    pub tail: RowTail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowTail {
    Closed,
    Variable(RowVariable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Equation {
    pub left: Type,
    pub right: Type,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Conflict {
    TypeMismatch { left: Type, right: Type },
    TypeOccurs(TypeVariable),
    RowMismatch { left: Row, right: Row },
    RowOccurs(RowVariable),
    RowLacks { variable: RowVariable, field: String },
    DuplicateField(String),
    UnknownTypeVariable(TypeVariable),
    UnknownRowVariable(RowVariable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RowCell {
    level: u32,
    lacks: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct State {
    type_levels: BTreeMap<TypeVariable, u32>,
    row_cells: BTreeMap<RowVariable, RowCell>,
    type_bindings: BTreeMap<TypeVariable, Type>,
    row_bindings: BTreeMap<RowVariable, Row>,
    next_type: u32,
    next_row: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Problem {
    type_levels: BTreeMap<TypeVariable, u32>,
    row_cells: BTreeMap<RowVariable, RowCell>,
    equations: Vec<Equation>,
    observations: Vec<Type>,
    next_type: u32,
    next_row: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Solver {
    state: State,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Solution {
    state: State,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemeType {
    Ground(Atom),
    Parameter(TypeParameter),
    Free(TypeVariable),
    List(Box<Self>),
    Optional(Box<Self>),
    Result(Box<Self>, Box<Self>),
    Record {
        fields: BTreeMap<String, Self>,
        tail: SchemeRowTail,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemeRowTail {
    Closed,
    Parameter(RowParameter),
    Free(RowVariable),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemeRowParameter {
    pub id: RowParameter,
    pub lacks: BTreeSet<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scheme {
    pub types: Vec<SchemeType>,
    pub type_parameters: Vec<TypeParameter>,
    pub row_parameters: Vec<SchemeRowParameter>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalScheme {
    pub types: Vec<SchemeType>,
    pub type_parameter_count: usize,
    pub row_parameter_lacks: Vec<BTreeSet<String>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalizationError {
    MissingTypeParameter(TypeParameter),
    MissingRowParameter(RowParameter),
}

#[derive(Default)]
struct Generalization {
    type_ids: BTreeMap<TypeVariable, TypeParameter>,
    row_ids: BTreeMap<RowVariable, RowParameter>,
    type_parameters: Vec<TypeParameter>,
    row_parameters: Vec<SchemeRowParameter>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeededProblem {
    pub seed: u64,
    pub problem: Problem,
    pub satisfiable: bool,
    pub expected_outer: Type,
}

impl Row {
    pub fn closed(fields: BTreeMap<String, Type>) -> Self {
        Self {
            fields,
            tail: RowTail::Closed,
        }
    }

    pub fn open(fields: BTreeMap<String, Type>, tail: RowVariable) -> Self {
        Self {
            fields,
            tail: RowTail::Variable(tail),
        }
    }
}

impl Problem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh_type(&mut self, level: u32) -> Type {
        let id = TypeVariable(self.next_type);
        self.next_type += 1;
        self.type_levels.insert(id, level);
        Type::Variable(id)
    }

    pub fn fresh_row(
        &mut self,
        level: u32,
        lacks: impl IntoIterator<Item = String>,
    ) -> RowVariable {
        let id = RowVariable(self.next_row);
        self.next_row += 1;
        self.row_cells.insert(
            id,
            RowCell {
                level,
                lacks: lacks.into_iter().collect(),
            },
        );
        id
    }

    pub fn equate(&mut self, left: Type, right: Type) {
        self.equations.push(Equation { left, right });
    }

    pub fn observe(&mut self, ty: Type) {
        self.observations.push(ty);
    }

    pub fn equations(&self) -> &[Equation] {
        &self.equations
    }

    pub fn observations(&self) -> &[Type] {
        &self.observations
    }

    pub fn type_variables(&self) -> impl Iterator<Item = (TypeVariable, u32)> + '_ {
        self.type_levels.iter().map(|(&id, &level)| (id, level))
    }

    pub fn row_variables(
        &self,
    ) -> impl Iterator<Item = (RowVariable, u32, &BTreeSet<String>)> + '_ {
        self.row_cells
            .iter()
            .map(|(&id, cell)| (id, cell.level, &cell.lacks))
    }

    pub fn solver(&self) -> Solver {
        Solver {
            state: State {
                type_levels: self.type_levels.clone(),
                row_cells: self.row_cells.clone(),
                type_bindings: BTreeMap::new(),
                row_bindings: BTreeMap::new(),
                next_type: self.next_type,
                next_row: self.next_row,
            },
        }
    }

    pub fn solve(&self) -> Result<Solution, Conflict> {
        let mut solver = self.solver();
        for equation in &self.equations {
            solver.unify(&equation.left, &equation.right)?;
        }
        Ok(Solution {
            state: solver.state,
        })
    }
}

impl Default for Problem {
    fn default() -> Self {
        Self {
            type_levels: BTreeMap::new(),
            row_cells: BTreeMap::new(),
            equations: Vec::new(),
            observations: Vec::new(),
            next_type: 0,
            next_row: 0,
        }
    }
}

impl Solver {
    pub fn unify(&mut self, left: &Type, right: &Type) -> Result<(), Conflict> {
        let before = self.clone();
        match self.state.unify_types(left, right) {
            Ok(()) => Ok(()),
            Err(conflict) => {
                *self = before;
                Err(conflict)
            }
        }
    }

    pub fn trial(&mut self, equations: &[Equation]) -> Result<(), Conflict> {
        let before = self.clone();
        let result = equations
            .iter()
            .try_for_each(|equation| self.unify(&equation.left, &equation.right));
        *self = before;
        result
    }

    pub fn resolve_type(&self, ty: &Type) -> Result<Type, Conflict> {
        self.state.resolve_type(ty)
    }

    pub fn finish(self) -> Solution {
        Solution { state: self.state }
    }
}

impl Solution {
    pub fn resolve_type(&self, ty: &Type) -> Result<Type, Conflict> {
        self.state.resolve_type(ty)
    }

    pub fn generalize(&self, types: &[Type], environment_level: u32) -> Result<Scheme, Conflict> {
        let mut row_lacks = BTreeMap::new();
        for ty in types {
            self.state.collect_row_lacks(ty, &mut row_lacks)?;
        }
        let mut generalization = Generalization::default();
        let types = types
            .iter()
            .map(|ty| {
                self.state.abstract_type(
                    ty,
                    environment_level,
                    &row_lacks,
                    &mut generalization,
                )
            })
            .collect::<Result<_, _>>()?;
        Ok(Scheme {
            types,
            type_parameters: generalization.type_parameters,
            row_parameters: generalization.row_parameters,
        })
    }
}

impl State {
    fn unify_types(&mut self, left: &Type, right: &Type) -> Result<(), Conflict> {
        let left = self.resolve_type(left)?;
        let right = self.resolve_type(right)?;
        match (&left, &right) {
            (Type::Variable(a), Type::Variable(b)) if a == b => Ok(()),
            (Type::Variable(variable), ty) | (ty, Type::Variable(variable)) => {
                self.bind_type(*variable, ty)
            }
            (Type::Atom(a), Type::Atom(b)) if a == b => Ok(()),
            (Type::List(a), Type::List(b)) | (Type::Optional(a), Type::Optional(b)) => {
                self.unify_types(a, b)
            }
            (Type::Result(a1, a2), Type::Result(b1, b2)) => {
                self.unify_types(a1, b1)?;
                self.unify_types(a2, b2)
            }
            (Type::Record(a), Type::Record(b)) => self.unify_rows(a, b),
            _ => Err(Conflict::TypeMismatch { left, right }),
        }
    }

    fn bind_type(&mut self, variable: TypeVariable, ty: &Type) -> Result<(), Conflict> {
        let level = *self
            .type_levels
            .get(&variable)
            .ok_or(Conflict::UnknownTypeVariable(variable))?;
        let ty = self.resolve_type(ty)?;
        if ty == Type::Variable(variable) {
            return Ok(());
        }
        if self.type_occurs(variable, &ty)? {
            return Err(Conflict::TypeOccurs(variable));
        }
        self.lower_type_levels(&ty, level)?;
        self.type_bindings.insert(variable, ty);
        Ok(())
    }

    fn type_occurs(&self, variable: TypeVariable, ty: &Type) -> Result<bool, Conflict> {
        match self.resolve_type(ty)? {
            Type::Variable(found) => Ok(found == variable),
            Type::Atom(_) => Ok(false),
            Type::List(inner) | Type::Optional(inner) => self.type_occurs(variable, &inner),
            Type::Result(ok, err) => {
                Ok(self.type_occurs(variable, &ok)? || self.type_occurs(variable, &err)?)
            }
            Type::Record(row) => {
                for field in row.fields.values() {
                    if self.type_occurs(variable, field)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    fn unify_rows(&mut self, left: &Row, right: &Row) -> Result<(), Conflict> {
        let left = self.normalize_row(left)?;
        let right = self.normalize_row(right)?;
        for (name, left_type) in &left.fields {
            if let Some(right_type) = right.fields.get(name) {
                self.unify_types(left_type, right_type)?;
            }
        }
        let left_only = left
            .fields
            .iter()
            .filter(|(name, _)| !right.fields.contains_key(*name))
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect::<BTreeMap<_, _>>();
        let right_only = right
            .fields
            .iter()
            .filter(|(name, _)| !left.fields.contains_key(*name))
            .map(|(name, ty)| (name.clone(), ty.clone()))
            .collect::<BTreeMap<_, _>>();
        match (left.tail, right.tail) {
            (RowTail::Closed, RowTail::Closed) if left_only.is_empty() && right_only.is_empty() => {
                Ok(())
            }
            (RowTail::Variable(variable), RowTail::Closed) if left_only.is_empty() => {
                self.bind_row(
                    variable,
                    Row::closed(right_only),
                )
            }
            (RowTail::Closed, RowTail::Variable(variable)) if right_only.is_empty() => {
                self.bind_row(
                    variable,
                    Row::closed(left_only),
                )
            }
            (RowTail::Variable(left_tail), RowTail::Variable(right_tail))
                if left_tail == right_tail && left_only.is_empty() && right_only.is_empty() =>
            {
                Ok(())
            }
            (RowTail::Variable(left_tail), RowTail::Variable(right_tail))
                if left_tail != right_tail && left_only.is_empty() =>
            {
                self.bind_row(
                    left_tail,
                    Row::open(right_only, right_tail),
                )
            }
            (RowTail::Variable(left_tail), RowTail::Variable(right_tail))
                if left_tail != right_tail && right_only.is_empty() =>
            {
                self.bind_row(
                    right_tail,
                    Row::open(left_only, left_tail),
                )
            }
            (RowTail::Variable(left_tail), RowTail::Variable(right_tail))
                if left_tail != right_tail =>
            {
                let left_cell = self
                    .row_cells
                    .get(&left_tail)
                    .cloned()
                    .ok_or(Conflict::UnknownRowVariable(left_tail))?;
                let right_cell = self
                    .row_cells
                    .get(&right_tail)
                    .cloned()
                    .ok_or(Conflict::UnknownRowVariable(right_tail))?;
                let mut lacks = left_cell.lacks;
                lacks.extend(right_cell.lacks);
                lacks.extend(left.fields.keys().cloned());
                lacks.extend(right.fields.keys().cloned());
                let shared = self.fresh_row(left_cell.level.min(right_cell.level), lacks);
                self.bind_row(left_tail, Row::open(right_only, shared))?;
                self.bind_row(right_tail, Row::open(left_only, shared))
            }
            _ => Err(Conflict::RowMismatch { left, right }),
        }
    }

    fn normalize_row(&mut self, row: &Row) -> Result<Row, Conflict> {
        let row = self.resolve_row(row)?;
        if let RowTail::Variable(variable) = row.tail {
            self.extend_lacks(variable, row.fields.keys().cloned())?;
        }
        Ok(row)
    }

    fn bind_row(&mut self, variable: RowVariable, row: Row) -> Result<(), Conflict> {
        if let Some(existing) = self.row_bindings.get(&variable).cloned() {
            return self.unify_rows(&existing, &row);
        }
        let cell = self
            .row_cells
            .get(&variable)
            .cloned()
            .ok_or(Conflict::UnknownRowVariable(variable))?;
        let row = self.normalize_row(&row)?;
        for field in row.fields.keys() {
            if cell.lacks.contains(field) {
                return Err(Conflict::RowLacks {
                    variable,
                    field: field.clone(),
                });
            }
        }
        if self.row_occurs(variable, &row)? {
            return Err(Conflict::RowOccurs(variable));
        }
        if let RowTail::Variable(tail) = row.tail {
            self.extend_lacks(tail, cell.lacks.iter().cloned())?;
        }
        self.lower_row_levels(&row, cell.level)?;
        self.row_bindings.insert(variable, row);
        Ok(())
    }

    fn row_occurs(&self, variable: RowVariable, row: &Row) -> Result<bool, Conflict> {
        let row = self.resolve_row(row)?;
        if row.tail == RowTail::Variable(variable) {
            return Ok(true);
        }
        for ty in row.fields.values() {
            if self.row_occurs_in_type(variable, ty)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn row_occurs_in_type(&self, variable: RowVariable, ty: &Type) -> Result<bool, Conflict> {
        match self.resolve_type(ty)? {
            Type::Variable(_) | Type::Atom(_) => Ok(false),
            Type::List(inner) | Type::Optional(inner) => {
                self.row_occurs_in_type(variable, &inner)
            }
            Type::Result(ok, err) => Ok(
                self.row_occurs_in_type(variable, &ok)?
                    || self.row_occurs_in_type(variable, &err)?,
            ),
            Type::Record(row) => self.row_occurs(variable, &row),
        }
    }

    fn extend_lacks(
        &mut self,
        variable: RowVariable,
        fields: impl IntoIterator<Item = String>,
    ) -> Result<(), Conflict> {
        let fields = fields.into_iter().collect::<BTreeSet<_>>();
        if let Some(binding) = self.row_bindings.get(&variable).cloned() {
            let resolved = self.resolve_row(&binding)?;
            for field in &fields {
                if resolved.fields.contains_key(field) {
                    return Err(Conflict::RowLacks {
                        variable,
                        field: field.clone(),
                    });
                }
            }
            if let RowTail::Variable(tail) = resolved.tail {
                self.extend_lacks(tail, fields)?;
            }
            return Ok(());
        }
        let cell = self
            .row_cells
            .get_mut(&variable)
            .ok_or(Conflict::UnknownRowVariable(variable))?;
        cell.lacks.extend(fields);
        Ok(())
    }

    fn fresh_row(
        &mut self,
        level: u32,
        lacks: impl IntoIterator<Item = String>,
    ) -> RowVariable {
        let variable = RowVariable(self.next_row);
        self.next_row += 1;
        self.row_cells.insert(
            variable,
            RowCell {
                level,
                lacks: lacks.into_iter().collect(),
            },
        );
        variable
    }

    fn lower_type_levels(&mut self, ty: &Type, ceiling: u32) -> Result<(), Conflict> {
        match self.resolve_type(ty)? {
            Type::Variable(variable) => {
                let level = self
                    .type_levels
                    .get_mut(&variable)
                    .ok_or(Conflict::UnknownTypeVariable(variable))?;
                *level = (*level).min(ceiling);
            }
            Type::Atom(_) => {}
            Type::List(inner) | Type::Optional(inner) => {
                self.lower_type_levels(&inner, ceiling)?;
            }
            Type::Result(ok, err) => {
                self.lower_type_levels(&ok, ceiling)?;
                self.lower_type_levels(&err, ceiling)?;
            }
            Type::Record(row) => self.lower_row_levels(&row, ceiling)?,
        }
        Ok(())
    }

    fn lower_row_levels(&mut self, row: &Row, ceiling: u32) -> Result<(), Conflict> {
        let row = self.resolve_row(row)?;
        for ty in row.fields.values() {
            self.lower_type_levels(ty, ceiling)?;
        }
        if let RowTail::Variable(variable) = row.tail {
            let cell = self
                .row_cells
                .get_mut(&variable)
                .ok_or(Conflict::UnknownRowVariable(variable))?;
            cell.level = cell.level.min(ceiling);
        }
        Ok(())
    }

    fn resolve_type(&self, ty: &Type) -> Result<Type, Conflict> {
        match ty {
            Type::Variable(variable) => {
                if !self.type_levels.contains_key(variable) {
                    return Err(Conflict::UnknownTypeVariable(*variable));
                }
                match self.type_bindings.get(variable) {
                    Some(bound) => self.resolve_type(bound),
                    None => Ok(ty.clone()),
                }
            }
            Type::Atom(_) => Ok(ty.clone()),
            Type::List(inner) => Ok(Type::List(Box::new(self.resolve_type(inner)?))),
            Type::Optional(inner) => Ok(Type::Optional(Box::new(self.resolve_type(inner)?))),
            Type::Result(ok, err) => Ok(Type::Result(
                Box::new(self.resolve_type(ok)?),
                Box::new(self.resolve_type(err)?),
            )),
            Type::Record(row) => Ok(Type::Record(self.resolve_row(row)?)),
        }
    }

    fn resolve_row(&self, row: &Row) -> Result<Row, Conflict> {
        let mut fields = BTreeMap::new();
        for (name, ty) in &row.fields {
            fields.insert(name.clone(), self.resolve_type(ty)?);
        }
        let mut tail = row.tail;
        let mut seen = BTreeSet::new();
        while let RowTail::Variable(variable) = tail {
            if !self.row_cells.contains_key(&variable) {
                return Err(Conflict::UnknownRowVariable(variable));
            }
            if !seen.insert(variable) {
                return Err(Conflict::RowOccurs(variable));
            }
            let Some(bound) = self.row_bindings.get(&variable) else {
                break;
            };
            for (name, ty) in &bound.fields {
                if fields.contains_key(name) {
                    return Err(Conflict::DuplicateField(name.clone()));
                }
                fields.insert(name.clone(), self.resolve_type(ty)?);
            }
            tail = bound.tail;
        }
        Ok(Row { fields, tail })
    }

    fn collect_row_lacks(
        &self,
        ty: &Type,
        row_lacks: &mut BTreeMap<RowVariable, BTreeSet<String>>,
    ) -> Result<(), Conflict> {
        match self.resolve_type(ty)? {
            Type::Variable(_) | Type::Atom(_) => {}
            Type::List(inner) | Type::Optional(inner) => {
                self.collect_row_lacks(&inner, row_lacks)?;
            }
            Type::Result(ok, err) => {
                self.collect_row_lacks(&ok, row_lacks)?;
                self.collect_row_lacks(&err, row_lacks)?;
            }
            Type::Record(row) => {
                if let RowTail::Variable(variable) = row.tail {
                    row_lacks
                        .entry(variable)
                        .or_default()
                        .extend(row.fields.keys().cloned());
                }
                for field in row.fields.values() {
                    self.collect_row_lacks(field, row_lacks)?;
                }
            }
        }
        Ok(())
    }

    fn abstract_type(
        &self,
        ty: &Type,
        environment_level: u32,
        row_lacks: &BTreeMap<RowVariable, BTreeSet<String>>,
        generalization: &mut Generalization,
    ) -> Result<SchemeType, Conflict> {
        Ok(match self.resolve_type(ty)? {
            Type::Variable(variable) => {
                let level = *self
                    .type_levels
                    .get(&variable)
                    .ok_or(Conflict::UnknownTypeVariable(variable))?;
                if level <= environment_level {
                    SchemeType::Free(variable)
                } else {
                    let parameter = if let Some(&parameter) =
                        generalization.type_ids.get(&variable)
                    {
                        parameter
                    } else {
                        let parameter =
                            TypeParameter(generalization.type_parameters.len() as u32);
                        generalization.type_ids.insert(variable, parameter);
                        generalization.type_parameters.push(parameter);
                        parameter
                    };
                    SchemeType::Parameter(parameter)
                }
            }
            Type::Atom(atom) => SchemeType::Ground(atom),
            Type::List(inner) => SchemeType::List(Box::new(self.abstract_type(
                &inner,
                environment_level,
                row_lacks,
                generalization,
            )?)),
            Type::Optional(inner) => SchemeType::Optional(Box::new(self.abstract_type(
                &inner,
                environment_level,
                row_lacks,
                generalization,
            )?)),
            Type::Result(ok, err) => SchemeType::Result(
                Box::new(self.abstract_type(
                    &ok,
                    environment_level,
                    row_lacks,
                    generalization,
                )?),
                Box::new(self.abstract_type(
                    &err,
                    environment_level,
                    row_lacks,
                    generalization,
                )?),
            ),
            Type::Record(row) => {
                let fields = row
                    .fields
                    .iter()
                    .map(|(name, ty)| {
                        Ok((
                            name.clone(),
                            self.abstract_type(
                                ty,
                                environment_level,
                                row_lacks,
                                generalization,
                            )?,
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, Conflict>>()?;
                let tail = match row.tail {
                    RowTail::Closed => SchemeRowTail::Closed,
                    RowTail::Variable(variable) => {
                        let level = self
                            .row_cells
                            .get(&variable)
                            .ok_or(Conflict::UnknownRowVariable(variable))?
                            .level;
                        if level <= environment_level {
                            SchemeRowTail::Free(variable)
                        } else {
                            let parameter = if let Some(&parameter) =
                                generalization.row_ids.get(&variable)
                            {
                                parameter
                            } else {
                                let parameter =
                                    RowParameter(generalization.row_parameters.len() as u32);
                                let mut lacks = self
                                    .row_cells
                                    .get(&variable)
                                    .expect("checked row cell")
                                    .lacks
                                    .clone();
                                lacks.extend(row_lacks.get(&variable).into_iter().flatten().cloned());
                                generalization.row_ids.insert(variable, parameter);
                                generalization.row_parameters.push(SchemeRowParameter {
                                    id: parameter,
                                    lacks,
                                });
                                parameter
                            };
                            SchemeRowTail::Parameter(parameter)
                        }
                    }
                };
                SchemeType::Record { fields, tail }
            }
        })
    }
}

impl Scheme {
    pub fn canonical_form(&self) -> Result<CanonicalScheme, CanonicalizationError> {
        let mut canonicalizer = Canonicalizer::new(self);
        let types = self
            .types
            .iter()
            .map(|ty| canonicalizer.scheme_type(ty))
            .collect::<Result<Vec<_>, _>>()?;
        for &parameter in &self.type_parameters {
            canonicalizer.type_parameter(parameter)?;
        }
        for parameter in &self.row_parameters {
            canonicalizer.row_parameter(parameter.id)?;
        }
        Ok(CanonicalScheme {
            types,
            type_parameter_count: canonicalizer.type_parameters.len(),
            row_parameter_lacks: canonicalizer.row_parameter_lacks,
        })
    }
}

pub fn schemes_equivalent(left: &Scheme, right: &Scheme) -> bool {
    matches!((left.canonical_form(), right.canonical_form()), (Ok(a), Ok(b)) if a == b)
}

pub fn types_equivalent(left: &[Type], right: &[Type]) -> bool {
    let to_scheme = |types: &[Type]| Scheme {
        types: types.iter().map(to_free_scheme_type).collect(),
        type_parameters: Vec::new(),
        row_parameters: Vec::new(),
    };
    schemes_equivalent(&to_scheme(left), &to_scheme(right))
}

fn to_free_scheme_type(ty: &Type) -> SchemeType {
    match ty {
        Type::Variable(variable) => SchemeType::Free(*variable),
        Type::Atom(atom) => SchemeType::Ground(*atom),
        Type::List(inner) => SchemeType::List(Box::new(to_free_scheme_type(inner))),
        Type::Optional(inner) => SchemeType::Optional(Box::new(to_free_scheme_type(inner))),
        Type::Result(ok, err) => SchemeType::Result(
            Box::new(to_free_scheme_type(ok)),
            Box::new(to_free_scheme_type(err)),
        ),
        Type::Record(row) => SchemeType::Record {
            fields: row
                .fields
                .iter()
                .map(|(name, ty)| (name.clone(), to_free_scheme_type(ty)))
                .collect(),
            tail: match row.tail {
                RowTail::Closed => SchemeRowTail::Closed,
                RowTail::Variable(variable) => SchemeRowTail::Free(variable),
            },
        },
    }
}

struct Canonicalizer<'a> {
    scheme: &'a Scheme,
    type_parameters: BTreeMap<TypeParameter, TypeParameter>,
    free_types: BTreeMap<TypeVariable, TypeVariable>,
    row_parameters: BTreeMap<RowParameter, RowParameter>,
    free_rows: BTreeMap<RowVariable, RowVariable>,
    row_parameter_lacks: Vec<BTreeSet<String>>,
}

impl<'a> Canonicalizer<'a> {
    fn new(scheme: &'a Scheme) -> Self {
        Self {
            scheme,
            type_parameters: BTreeMap::new(),
            free_types: BTreeMap::new(),
            row_parameters: BTreeMap::new(),
            free_rows: BTreeMap::new(),
            row_parameter_lacks: Vec::new(),
        }
    }

    fn type_parameter(
        &mut self,
        parameter: TypeParameter,
    ) -> Result<TypeParameter, CanonicalizationError> {
        if !self.scheme.type_parameters.contains(&parameter) {
            return Err(CanonicalizationError::MissingTypeParameter(parameter));
        }
        if let Some(&canonical) = self.type_parameters.get(&parameter) {
            return Ok(canonical);
        }
        let canonical = TypeParameter(self.type_parameters.len() as u32);
        self.type_parameters.insert(parameter, canonical);
        Ok(canonical)
    }

    fn free_type(&mut self, variable: TypeVariable) -> TypeVariable {
        if let Some(&canonical) = self.free_types.get(&variable) {
            return canonical;
        }
        let canonical = TypeVariable(self.free_types.len() as u32);
        self.free_types.insert(variable, canonical);
        canonical
    }

    fn free_row(&mut self, variable: RowVariable) -> RowVariable {
        if let Some(&canonical) = self.free_rows.get(&variable) {
            return canonical;
        }
        let canonical = RowVariable(self.free_rows.len() as u32);
        self.free_rows.insert(variable, canonical);
        canonical
    }

    fn row_parameter(
        &mut self,
        parameter: RowParameter,
    ) -> Result<RowParameter, CanonicalizationError> {
        if let Some(&canonical) = self.row_parameters.get(&parameter) {
            return Ok(canonical);
        }
        let source = self
            .scheme
            .row_parameters
            .iter()
            .find(|entry| entry.id == parameter)
            .ok_or(CanonicalizationError::MissingRowParameter(parameter))?;
        let canonical = RowParameter(self.row_parameters.len() as u32);
        self.row_parameters.insert(parameter, canonical);
        self.row_parameter_lacks.push(source.lacks.clone());
        Ok(canonical)
    }

    fn scheme_type(&mut self, ty: &SchemeType) -> Result<SchemeType, CanonicalizationError> {
        Ok(match ty {
            SchemeType::Ground(atom) => SchemeType::Ground(*atom),
            SchemeType::Parameter(parameter) => {
                SchemeType::Parameter(self.type_parameter(*parameter)?)
            }
            SchemeType::Free(variable) => SchemeType::Free(self.free_type(*variable)),
            SchemeType::List(inner) => {
                SchemeType::List(Box::new(self.scheme_type(inner)?))
            }
            SchemeType::Optional(inner) => {
                SchemeType::Optional(Box::new(self.scheme_type(inner)?))
            }
            SchemeType::Result(ok, err) => SchemeType::Result(
                Box::new(self.scheme_type(ok)?),
                Box::new(self.scheme_type(err)?),
            ),
            SchemeType::Record { fields, tail } => SchemeType::Record {
                fields: fields
                    .iter()
                    .map(|(name, ty)| Ok((name.clone(), self.scheme_type(ty)?)))
                    .collect::<Result<_, CanonicalizationError>>()?,
                tail: match tail {
                    SchemeRowTail::Closed => SchemeRowTail::Closed,
                    SchemeRowTail::Parameter(parameter) => {
                        SchemeRowTail::Parameter(self.row_parameter(*parameter)?)
                    }
                    SchemeRowTail::Free(variable) => {
                        SchemeRowTail::Free(self.free_row(*variable))
                    }
                },
            },
        })
    }
}

pub fn generate_problem(seed: u64) -> SeededProblem {
    let mut rng = SeedRng::new(seed);
    let atoms = [Atom::Int, Atom::Str, Atom::Bool, Atom::Unit];
    let atom = atoms[rng.below(atoms.len())];
    let conflict_atom = atoms[(atoms.iter().position(|item| *item == atom).unwrap() + 1) % atoms.len()];
    let wrap = match rng.below(4) {
        0 => Wrapper::List,
        1 => Wrapper::Optional,
        2 => Wrapper::ResultLeft,
        _ => Wrapper::ResultRight,
    };
    let mut problem = Problem::new();
    let outer = problem.fresh_type(2);
    let inner = problem.fresh_type(3);
    let left_tail = problem.fresh_row(2, std::iter::empty());
    let right_tail = problem.fresh_row(2, std::iter::empty());
    let outer_type = wrap.apply(inner.clone());
    let left_name = format!("left_{seed}");
    let right_name = format!("right_{seed}");
    let left_row = Type::Record(Row::open(
        BTreeMap::from([
            ("common".to_owned(), outer.clone()),
            (left_name, Type::Atom(atom)),
        ]),
        left_tail,
    ));
    let right_row = Type::Record(Row::open(
        BTreeMap::from([
            ("common".to_owned(), outer_type.clone()),
            (right_name, Type::Atom(Atom::Bool)),
        ]),
        right_tail,
    ));
    problem.equate(outer.clone(), outer_type.clone());
    problem.equate(inner.clone(), Type::Atom(atom));
    problem.equate(left_row.clone(), right_row.clone());
    let satisfiable = seed % 7 != 0;
    if !satisfiable {
        problem.equate(outer.clone(), Type::Atom(conflict_atom));
    }
    problem.observe(outer);
    problem.observe(inner);
    problem.observe(left_row);
    problem.observe(right_row);
    SeededProblem {
        seed,
        problem,
        satisfiable,
        expected_outer: wrap.apply(Type::Atom(atom)),
    }
}

#[derive(Clone, Copy)]
enum Wrapper {
    List,
    Optional,
    ResultLeft,
    ResultRight,
}

impl Wrapper {
    fn apply(self, value: Type) -> Type {
        match self {
            Self::List => Type::List(Box::new(value)),
            Self::Optional => Type::Optional(Box::new(value)),
            Self::ResultLeft => Type::Result(Box::new(value), Box::new(Type::Atom(Atom::Unit))),
            Self::ResultRight => Type::Result(Box::new(Type::Atom(Atom::Unit)), Box::new(value)),
        }
    }
}

struct SeedRng(u64);

impl SeedRng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn below(&mut self, upper: usize) -> usize {
        (self.next() % upper as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(entries: impl IntoIterator<Item = (String, Type)>) -> BTreeMap<String, Type> {
        entries.into_iter().collect()
    }

    #[test]
    fn seeded_problems_compare_satisfiability_and_resolved_type_and_row_outputs() {
        for seed in 0..128 {
            let generated = generate_problem(seed);
            let result = generated.problem.solve();
            assert_eq!(
                result.is_ok(),
                generated.satisfiable,
                "seed {seed}: {result:?}"
            );
            if let Ok(solution) = result {
                let observations = generated
                    .problem
                    .observations()
                    .iter()
                    .map(|ty| solution.resolve_type(ty).unwrap())
                    .collect::<Vec<_>>();
                assert!(types_equivalent(
                    &[observations[0].clone()],
                    &[generated.expected_outer.clone()]
                ));
                assert!(types_equivalent(
                    &[observations[2].clone()],
                    &[observations[3].clone()]
                ));
            }
        }
    }

    #[test]
    fn output_comparators_accept_binder_renaming_and_reject_mutated_terms() {
        let generated = generate_problem(1);
        let solved = generated.problem.solve().unwrap();
        let outputs = generated
            .problem
            .observations()
            .iter()
            .map(|ty| solved.resolve_type(ty).unwrap())
            .collect::<Vec<_>>();
        let mut mutated_outputs = outputs.clone();
        mutated_outputs[0] = Type::Atom(Atom::Str);
        assert!(!types_equivalent(&outputs, &mutated_outputs));

        let mut problem = Problem::new();
        let generic = problem.fresh_type(3);
        let monomorphic = problem.fresh_type(1);
        let row = problem.fresh_row(2, ["reserved".to_owned()]);
        let generic_list = Type::List(Box::new(generic.clone()));
        let open_record = Type::Record(Row::open(
            fields([("value".to_owned(), monomorphic.clone())]),
            row,
        ));
        let solution = problem.solve().unwrap();
        let scheme = solution
            .generalize(
                &[generic_list.clone(), generic_list, open_record],
                1,
            )
            .unwrap();
        assert_eq!(scheme.type_parameters, [TypeParameter(0)]);
        assert_eq!(scheme.row_parameters.len(), 1);
        assert!(scheme.row_parameters[0]
            .lacks
            .contains("value"));
        assert!(scheme.row_parameters[0]
            .lacks
            .contains("reserved"));

        let renamed = rename_scheme(&scheme, TypeParameter(41), RowParameter(73));
        assert!(schemes_equivalent(&scheme, &renamed));

        let mut mutated_scheme = renamed;
        mutated_scheme.types[0] = SchemeType::Ground(Atom::Str);
        assert!(!schemes_equivalent(&scheme, &mutated_scheme));
    }

    fn rename_scheme(
        scheme: &Scheme,
        type_parameter: TypeParameter,
        row_parameter: RowParameter,
    ) -> Scheme {
        let mut renamed = scheme.clone();
        for ty in &mut renamed.types {
            rename_scheme_type(ty, type_parameter, row_parameter);
        }
        for parameter in &mut renamed.type_parameters {
            *parameter = type_parameter;
        }
        for parameter in &mut renamed.row_parameters {
            parameter.id = row_parameter;
        }
        renamed
    }

    fn rename_scheme_type(
        ty: &mut SchemeType,
        type_parameter: TypeParameter,
        row_parameter: RowParameter,
    ) {
        match ty {
            SchemeType::Parameter(parameter) => *parameter = type_parameter,
            SchemeType::List(inner) | SchemeType::Optional(inner) => {
                rename_scheme_type(inner, type_parameter, row_parameter)
            }
            SchemeType::Result(ok, err) => {
                rename_scheme_type(ok, type_parameter, row_parameter);
                rename_scheme_type(err, type_parameter, row_parameter);
            }
            SchemeType::Record { fields, tail } => {
                for value in fields.values_mut() {
                    rename_scheme_type(value, type_parameter, row_parameter);
                }
                if let SchemeRowTail::Parameter(parameter) = tail {
                    *parameter = row_parameter;
                }
            }
            SchemeType::Ground(_) | SchemeType::Free(_) => {}
        }
    }

    #[test]
    fn generalization_keeps_environment_variables_free_and_rows_lacking_fields() {
        let mut problem = Problem::new();
        let generic = problem.fresh_type(4);
        let environment = problem.fresh_type(1);
        let linked = problem.fresh_type(3);
        let row = problem.fresh_row(3, ["reserved".to_owned()]);
        let linked_row = problem.fresh_row(3, std::iter::empty());
        let environment_row = problem.fresh_row(1, std::iter::empty());
        problem.equate(linked.clone(), environment.clone());
        problem.equate(
            Type::Record(Row::open(BTreeMap::new(), linked_row)),
            Type::Record(Row::open(BTreeMap::new(), environment_row)),
        );
        let root = Type::Record(Row::open(
            fields([
                ("generic".to_owned(), generic.clone()),
                ("environment".to_owned(), environment.clone()),
                ("linked".to_owned(), linked.clone()),
            ]),
            row,
        ));
        let monomorphic_row = Type::Record(Row::open(
            fields([("fixed".to_owned(), Type::Atom(Atom::Unit))]),
            linked_row,
        ));
        let solution = problem.solve().unwrap();
        let scheme = solution.generalize(&[root, monomorphic_row], 1).unwrap();
        assert_eq!(scheme.type_parameters, [TypeParameter(0)]);
        assert_eq!(scheme.row_parameters.len(), 1);
        assert!(scheme.row_parameters[0].lacks.contains("generic"));
        assert!(scheme.row_parameters[0].lacks.contains("environment"));
        assert!(scheme.row_parameters[0].lacks.contains("reserved"));
        let SchemeType::Record { fields, .. } = &scheme.types[0] else {
            panic!("record scheme expected")
        };
        assert_eq!(fields["generic"], SchemeType::Parameter(TypeParameter(0)));
        assert_eq!(
            fields["environment"],
            SchemeType::Free(match environment {
                Type::Variable(id) => id,
                _ => unreachable!(),
            })
        );
        assert_eq!(fields["linked"], fields["environment"]);
        let SchemeType::Record { tail, .. } = &scheme.types[1] else {
            panic!("monomorphic row expected")
        };
        assert_eq!(*tail, SchemeRowTail::Free(environment_row));
    }

    #[test]
    fn failed_trial_restores_type_bindings_row_lacks_and_fresh_tails() {
        let mut problem = Problem::new();
        let shared_type = problem.fresh_type(2);
        let left_tail = problem.fresh_row(2, std::iter::empty());
        let right_tail = problem.fresh_row(2, std::iter::empty());
        let first = Equation {
            left: Type::Record(Row::open(
                fields([
                    ("common".to_owned(), shared_type.clone()),
                    ("left".to_owned(), Type::Atom(Atom::Bool)),
                ]),
                left_tail,
            )),
            right: Type::Record(Row::open(
                fields([
                    ("common".to_owned(), Type::Atom(Atom::Bool)),
                    ("right".to_owned(), Type::Atom(Atom::Int)),
                ]),
                right_tail,
            )),
        };
        let bind_type = Equation {
            left: shared_type.clone(),
            right: Type::Atom(Atom::Bool),
        };
        let conflict = Equation {
            left: shared_type,
            right: Type::Atom(Atom::Str),
        };
        let mut solver = problem.solver();
        let before = solver.clone();
        let mut partial = solver.clone();
        partial.unify(&first.left, &first.right).unwrap();
        partial.unify(&bind_type.left, &bind_type.right).unwrap();
        assert_ne!(partial, before);
        assert!(solver.trial(&[first, bind_type, conflict]).is_err());
        assert_eq!(solver, before);
    }

    #[test]
    fn occurs_and_lacks_conflicts_leave_the_solver_unchanged() {
        let mut problem = Problem::new();
        let ty = problem.fresh_type(1);
        let partially_bound = problem.fresh_type(1);
        let row = problem.fresh_row(1, ["reserved".to_owned()]);
        let mut solver = problem.solver();
        let before = solver.clone();
        assert!(matches!(
            solver.unify(
                &ty,
                &Type::List(Box::new(ty.clone()))
            ),
            Err(Conflict::TypeOccurs(_))
        ));
        assert_eq!(solver, before);

        let left = Type::Record(Row::closed(fields([
            ("first".to_owned(), partially_bound.clone()),
            ("second".to_owned(), Type::Atom(Atom::Int)),
        ])));
        let right = Type::Record(Row::closed(fields([
            ("first".to_owned(), Type::Atom(Atom::Int)),
            ("second".to_owned(), Type::Atom(Atom::Str)),
        ])));
        assert!(matches!(
            solver.unify(&left, &right),
            Err(Conflict::TypeMismatch { .. })
        ));
        assert_eq!(solver, before);

        let closed = Type::Record(Row::closed(fields([(
            "reserved".to_owned(),
            Type::Atom(Atom::Int),
        )])));
        let open = Type::Record(Row::open(BTreeMap::new(), row));
        assert!(matches!(
            solver.unify(&open, &closed),
            Err(Conflict::RowLacks { .. })
        ));
        assert_eq!(solver, before);
    }

    #[test]
    fn row_occurs_check_rejects_nested_self_reference() {
        let mut problem = Problem::new();
        let row = problem.fresh_row(1, std::iter::empty());
        let rest = problem.fresh_row(1, std::iter::empty());
        let recursive = Type::Record(Row::open(
            fields([(
                "nested".to_owned(),
                Type::Record(Row::open(BTreeMap::new(), row)),
            )]),
            rest,
        ));
        let mut solver = problem.solver();
        let before = solver.clone();
        assert!(matches!(
            solver.unify(&Type::Record(Row::open(BTreeMap::new(), row)), &recursive),
            Err(Conflict::RowOccurs(_))
        ));
        assert_eq!(solver, before);
    }
}
