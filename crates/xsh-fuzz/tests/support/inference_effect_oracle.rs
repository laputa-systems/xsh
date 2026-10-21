#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Atom {
    Fs,
    Net,
    Process,
    Env,
    Time,
    Error,
    Io,
}

impl Atom {
    pub const ALL: [Self; 7] = [
        Self::Fs, Self::Net, Self::Process, Self::Env, Self::Time, Self::Error, Self::Io,
    ];
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Set(u8);

impl Set {
    pub const EMPTY: Self = Self(0);

    pub fn new(atoms: impl IntoIterator<Item = Atom>) -> Self {
        Self(atoms.into_iter().fold(0, |bits, atom| bits | (1 << atom as u8)))
    }

    pub fn atoms(self) -> impl Iterator<Item = Atom> {
        Atom::ALL.into_iter().filter(move |&atom| self.0 & (1 << atom as u8) != 0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    Known(Set),
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Term {
    Finite(Set),
    Variable(usize),
    Union(Vec<Self>),
    Mask(Box<Self>, Set),
    CaptureError(Box<Self>),
    Unknown,
}

impl Term {
    pub fn evaluate(&self, assignment: &[Set]) -> Value {
        match self {
            Self::Finite(set) => Value::Known(*set),
            Self::Variable(index) => Value::Known(assignment[*index]),
            Self::Unknown => Value::Unknown,
            Self::Union(terms) => {
                let mut bits = 0;
                for term in terms {
                    match term.evaluate(assignment) {
                        Value::Known(set) => bits |= set.0,
                        Value::Unknown => return Value::Unknown,
                    }
                }
                Value::Known(Set(bits))
            }
            Self::Mask(term, mask) => match term.evaluate(assignment) {
                Value::Known(set) => Value::Known(Set(set.0 & mask.0)),
                Value::Unknown => Value::Unknown,
            },
            Self::CaptureError(term) => match term.evaluate(assignment) {
                Value::Known(set) => Value::Known(Set(set.0 & !(1 << Atom::Error as u8))),
                Value::Unknown => Value::Unknown,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Predicate {
    Equal(Term, Term),
    Included(Term, Term),
    Covered(Term, Term),
    Excluded(Term, Set),
}

fn covers(permission: Atom, requirement: Atom) -> bool {
    permission == requirement
        || matches!((permission, requirement), (Atom::Io, Atom::Fs | Atom::Net | Atom::Process | Atom::Env))
}

impl Predicate {
    pub fn accepts(&self, assignment: &[Set]) -> bool {
        match self {
            Self::Equal(left, right) => left.evaluate(assignment) == right.evaluate(assignment),
            Self::Included(left, right) | Self::Covered(left, right) => {
                let right = right.evaluate(assignment);
                if right == Value::Unknown {
                    return true;
                }
                match (left.evaluate(assignment), right) {
                    (Value::Known(left), Value::Known(right)) => match self {
                        Self::Included(_, _) => left.0 & !right.0 == 0,
                        Self::Covered(_, _) => left.atoms().all(|required| {
                            right.atoms().any(|permission| covers(permission, required))
                        }),
                        _ => unreachable!(),
                    },
                    _ => false,
                }
            }
            Self::Excluded(term, excluded) => match term.evaluate(assignment) {
                Value::Unknown => false,
                Value::Known(set) => !set.atoms().any(|required| {
                    excluded.atoms().any(|denied| covers(required, denied))
                }),
            },
        }
    }
}

pub type Relation = Vec<Option<Vec<Value>>>;

#[derive(Clone, Debug)]
pub struct Problem {
    pub variables: usize,
    pub predicates: Vec<Predicate>,
    pub observations: Vec<Term>,
}

impl Problem {
    pub fn bounded(variables: usize, predicates: Vec<Predicate>, observations: Vec<Term>) -> Self {
        assert!(variables <= 2);
        let domain = Set::new([Atom::Fs, Atom::Io, Atom::Time, Atom::Error]);
        let mut bounded = (0..variables).map(|index| {
            Predicate::Included(Term::Variable(index), Term::Finite(domain))
        }).collect::<Vec<_>>();
        bounded.extend(predicates);
        Self { variables, predicates: bounded, observations }
    }

    pub fn assignments(&self) -> Vec<Vec<Set>> {
        let atoms = [Atom::Fs, Atom::Io, Atom::Time, Atom::Error];
        let domain = (0..16).map(|bits| {
            Set::new(atoms.into_iter().enumerate().filter_map(|(index, atom)| {
                (bits & (1 << index) != 0).then_some(atom)
            }))
        }).collect::<Vec<_>>();
        (0..16_usize.pow(self.variables as u32)).map(|index| {
            (0..self.variables).map(|variable| domain[(index / 16_usize.pow(variable as u32)) % 16]).collect()
        }).collect()
    }

    pub fn relation(&self) -> Relation {
        self.assignments().iter().map(|assignment| {
            self.predicates.iter().all(|predicate| predicate.accepts(assignment)).then(|| {
                self.observations.iter().map(|term| term.evaluate(assignment)).collect()
            })
        }).collect()
    }
}

pub fn generate_problem(seed: u64) -> Problem {
    let mut rng = super::SeedRng::new(seed);
    let first = Term::Variable(0);
    let second = Term::Variable(1);
    let set = Set::new([Atom::Fs, Atom::Io, Atom::Time, Atom::Error].into_iter()
        .filter(|_| rng.below(2) == 0));
    let captured = Term::CaptureError(Box::new(first.clone()));
    let repeated = Term::Union(vec![
        Term::Mask(Box::new(first.clone()), Set::new([Atom::Fs, Atom::Error])),
        captured.clone(),
    ]);
    let predicate = match seed % 8 {
        0 => Predicate::Equal(first.clone(), second.clone()),
        1 => Predicate::Equal(first.clone(), Term::Union(vec![second.clone(), Term::Finite(set)])),
        2 => Predicate::Included(first.clone(), second.clone()),
        3 => Predicate::Covered(first.clone(), second.clone()),
        4 => Predicate::Excluded(first.clone(), set),
        5 => Predicate::Equal(captured.clone(), Term::Mask(Box::new(second.clone()), set)),
        6 => Predicate::Equal(repeated.clone(), first.clone()),
        _ => Predicate::Equal(first.clone(), Term::Unknown),
    };
    let mut predicates = vec![predicate];
    if seed.is_multiple_of(7) {
        predicates.push(Predicate::Equal(
            Term::Finite(Set::new([Atom::Fs])), Term::Finite(Set::new([Atom::Io])),
        ));
    }
    Problem::bounded(2, predicates, vec![
        first.clone(), second.clone(), captured.clone(),
        Term::Union(vec![captured, second]), repeated,
        Term::Mask(Box::new(Term::CaptureError(Box::new(first.clone()))), Set::new([Atom::Io, Atom::Error])),
        Term::Mask(Box::new(first), Set::EMPTY),
        Term::Mask(Box::new(Term::Unknown), Set::EMPTY),
        Term::CaptureError(Box::new(Term::Unknown)),
        Term::Union(vec![Term::Unknown, Term::Finite(set)]),
        Term::Finite(Set::EMPTY),
    ])
}
