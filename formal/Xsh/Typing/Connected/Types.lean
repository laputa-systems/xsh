import Xsh.Syntax.Core

namespace Xsh.Typing.Connected

open Xsh.Foundation

inductive Primitive where
  | integer
  | boolean
  | string
  | unit
  deriving DecidableEq, Repr

inductive VariableKind where
  | value
  | row
  deriving DecidableEq, BEq, ReflBEq, LawfulBEq, Repr

structure Variable where
  id : Nat
  kind : VariableKind
  deriving DecidableEq, BEq, ReflBEq, LawfulBEq, Repr

inductive MonoType where
  | variable (id : Variable)
  | primitive (value : Primitive)
  | function (arguments result : MonoType)
  | argumentsEmpty
  | argument (value rest : MonoType)
  | result (value error : MonoType)
  | record (row : MonoType)
  | rowEmpty
  | rowField (name : NameId) (value tail : MonoType)
  deriving DecidableEq

inductive TypeSort where
  | value
  | row
  | arguments
  deriving DecidableEq

def VariableKind.sort : VariableKind → TypeSort
  | .value => .value
  | .row => .row

def typeSort : MonoType → TypeSort
  | .variable id => id.kind.sort
  | .primitive _ | .function _ _ | .result _ _ | .record _ => .value
  | .rowEmpty | .rowField _ _ _ => .row
  | .argumentsEmpty | .argument _ _ => .arguments

def WellFormed : MonoType → Prop
  | .variable _ | .primitive _ | .rowEmpty | .argumentsEmpty => True
  | .function arguments result =>
      WellFormed arguments ∧ typeSort arguments = .arguments ∧
        WellFormed result ∧ typeSort result = .value
  | .argument value rest =>
      WellFormed value ∧ typeSort value = .value ∧
        WellFormed rest ∧ typeSort rest = .arguments
  | .result value error =>
      WellFormed value ∧ typeSort value = .value ∧
        WellFormed error ∧ typeSort error = .value
  | .record row => WellFormed row ∧ typeSort row = .row
  | .rowField _ value tail =>
      WellFormed value ∧ typeSort value = .value ∧
        WellFormed tail ∧ typeSort tail = .row

def checksWellFormed : MonoType → Bool
  | .variable _ | .primitive _ | .rowEmpty | .argumentsEmpty => true
  | .function arguments result =>
      checksWellFormed arguments && decide (typeSort arguments = .arguments) &&
        checksWellFormed result && decide (typeSort result = .value)
  | .argument value rest =>
      checksWellFormed value && decide (typeSort value = .value) &&
        checksWellFormed rest && decide (typeSort rest = .arguments)
  | .result value error =>
      checksWellFormed value && decide (typeSort value = .value) &&
        checksWellFormed error && decide (typeSort error = .value)
  | .record row => checksWellFormed row && decide (typeSort row = .row)
  | .rowField _ value tail =>
      checksWellFormed value && decide (typeSort value = .value) &&
        checksWellFormed tail && decide (typeSort tail = .row)

theorem checks_well_formed_iff (ty : MonoType) : checksWellFormed ty = true ↔ WellFormed ty := by
  induction ty <;> simp_all [checksWellFormed, WellFormed, and_assoc]

abbrev Substitution := Variable → MonoType

def substitute (substitution : Substitution) : MonoType → MonoType
  | .variable id => substitution id
  | .primitive value => .primitive value
  | .function argument result => .function (substitute substitution argument) (substitute substitution result)
  | .argumentsEmpty => .argumentsEmpty
  | .argument value rest => .argument (substitute substitution value) (substitute substitution rest)
  | .result value error => .result (substitute substitution value) (substitute substitution error)
  | .record row => .record (substitute substitution row)
  | .rowEmpty => .rowEmpty
  | .rowField name value tail => .rowField name (substitute substitution value) (substitute substitution tail)

def identity : Substitution := MonoType.variable

def compose (outer inner : Substitution) : Substitution := fun id => substitute outer (inner id)

def WellSorted (substitution : Substitution) : Prop :=
  ∀ id, WellFormed (substitution id) ∧ typeSort (substitution id) = id.kind.sort

theorem substitute_sort (substitution : Substitution) (sorted : WellSorted substitution)
    (ty : MonoType) : typeSort (substitute substitution ty) = typeSort ty := by
  cases ty with
  | «variable» id => exact (sorted id).2
  | _ => rfl

theorem substitute_well_formed (substitution : Substitution) (sorted : WellSorted substitution)
    (ty : MonoType) (wellFormed : WellFormed ty) : WellFormed (substitute substitution ty) := by
  induction ty with
  | «variable» id => exact (sorted id).1
  | primitive _ | rowEmpty | argumentsEmpty => trivial
  | function _ _ left right | argument _ _ left right | result _ _ left right | rowField _ _ _ left right =>
    simp only [WellFormed] at wellFormed
    simp only [substitute, WellFormed, substitute_sort substitution sorted]
    exact ⟨left wellFormed.1, wellFormed.2.1, right wellFormed.2.2.1, wellFormed.2.2.2⟩
  | record _ ih =>
    simp only [WellFormed] at wellFormed
    simp only [substitute, WellFormed, substitute_sort substitution sorted]
    exact ⟨ih wellFormed.1, wellFormed.2⟩

theorem substitute_identity (ty : MonoType) : substitute identity ty = ty := by
  induction ty <;> simp_all [substitute, identity]

theorem substitute_compose (outer inner : Substitution) (ty : MonoType) :
    substitute (compose outer inner) ty = substitute outer (substitute inner ty) := by
  induction ty <;> simp_all [substitute, compose]

def freeVariables : MonoType → List Variable
  | .variable id => [id]
  | .primitive _ | .rowEmpty | .argumentsEmpty => []
  | .function argument result | .argument argument result | .result argument result => freeVariables argument ++ freeVariables result
  | .record row => freeVariables row
  | .rowField _ value tail => freeVariables value ++ freeVariables tail

def occurs (id : Variable) (ty : MonoType) : Bool := decide (id ∈ freeVariables ty)

def bindVariable (id : Variable) (ty : MonoType) : Substitution := fun other =>
  if other = id then ty else .variable other

theorem substitute_absent (id : Variable) (replacement ty : MonoType)
    (absent : id ∉ freeVariables ty) : substitute (bindVariable id replacement) ty = ty := by
  induction ty with
  | «variable» other =>
    have different : other ≠ id := Ne.symm (by simpa [freeVariables] using absent)
    simp [substitute, bindVariable, different]
  | primitive _ | rowEmpty | argumentsEmpty => rfl
  | function _ _ left right | argument _ _ left right | result _ _ left right | rowField _ _ _ left right =>
    simp only [freeVariables, List.mem_append, not_or] at absent
    simp [substitute, left absent.1, right absent.2]
  | record _ ih => simp [freeVariables] at absent; simp [substitute, ih absent]

inductive Constraint where
  | equal (left right : MonoType)
  | field (record : MonoType) (name : NameId) (value : MonoType)
  deriving DecidableEq

inductive FieldOccurs : NameId → MonoType → MonoType → Prop where
  | here {name : NameId} {value tail : MonoType} :
      FieldOccurs name value (.rowField name value tail)
  | later {name other : NameId} {value head tail : MonoType} :
      name ≠ other → FieldOccurs name value tail →
        FieldOccurs name value (.rowField other head tail)

theorem field_occurs_substitute {name : NameId} {value row : MonoType}
    (found : FieldOccurs name value row) (substitution : Substitution) :
    FieldOccurs name (substitute substitution value) (substitute substitution row) := by
  induction found with
  | here => exact .here
  | later different _ ih => exact .later different ih

theorem field_absent_from_singleton {name other : NameId} {value field : MonoType}
    (different : name ≠ other) : ¬ FieldOccurs name value (.rowField other field .rowEmpty) := by
  intro found
  cases found with
  | here => exact different rfl
  | later _ tail => cases tail

def fieldAt? (name : NameId) : MonoType → Option MonoType
  | .rowField other value tail => if name = other then some value else fieldAt? name tail
  | _ => none

theorem field_occurs_iff {name : NameId} {value row : MonoType} :
    FieldOccurs name value row ↔ fieldAt? name row = some value := by
  induction row with
  | rowField other head tail _ ih =>
    by_cases same : name = other
    · subst other
      simp only [fieldAt?, ↓reduceIte, Option.some.injEq]
      constructor
      · intro found
        cases found with
        | here => rfl
        | later different _ => exact (different rfl).elim
      · intro equal
        subst head
        exact .here
    · simp only [fieldAt?, if_neg same]
      constructor
      · intro found
        cases found with
        | here => exact (same rfl).elim
        | later _ tailFound => exact ih.mp tailFound
      · intro found
        exact .later same (ih.mpr found)
  | «variable» _ | primitive _ | function _ _ _ _ | argumentsEmpty | argument _ _ _ _ | result _ _ _ _ | record _ _ | rowEmpty =>
    simp only [fieldAt?]
    constructor
    · intro found; cases found
    · intro impossible; cases impossible

def satisfies (substitution : Substitution) : Constraint → Prop
  | .equal left right => substitute substitution left = substitute substitution right
  | .field record name value =>
      ∃ row, substitute substitution record = .record row ∧
        FieldOccurs name (substitute substitution value) row

def Solves (constraints : List Constraint) (substitution : Substitution) : Prop :=
  ∀ constraint ∈ constraints, satisfies substitution constraint

def checksConstraint (substitution : Substitution) : Constraint → Bool
  | .equal left right => decide (substitute substitution left = substitute substitution right)
  | .field record name value =>
      match substitute substitution record with
      | .record row => decide (fieldAt? name row = some (substitute substitution value))
      | _ => false

theorem checks_constraint_iff (substitution : Substitution) (constraint : Constraint) :
    checksConstraint substitution constraint = true ↔ satisfies substitution constraint := by
  cases constraint with
  | equal left right => simp [checksConstraint, satisfies]
  | field record name value =>
    cases found : substitute substitution record <;>
      simp [checksConstraint, satisfies, found, field_occurs_iff]

def checksConstraints (substitution : Substitution) (constraints : List Constraint) : Bool :=
  constraints.all (checksConstraint substitution)

theorem checks_constraints_iff (substitution : Substitution) (constraints : List Constraint) :
    checksConstraints substitution constraints = true ↔ Solves constraints substitution := by
  simp [checksConstraints, Solves, checks_constraint_iff]

theorem absent_field_constraint_is_unsatisfiable {name other : NameId} {field value : MonoType}
    (different : name ≠ other) (substitution : Substitution) :
    ¬ satisfies substitution (.field (.record (.rowField other field .rowEmpty)) name value) := by
  intro solved
  obtain ⟨row, same, found⟩ := solved
  simp only [substitute, MonoType.record.injEq] at same
  subst row
  exact field_absent_from_singleton different found

def mapConstraint (substitution : Substitution) : Constraint → Constraint
  | .equal left right => .equal (substitute substitution left) (substitute substitution right)
  | .field record name value => .field (substitute substitution record) name (substitute substitution value)

theorem satisfies_mapped (first second : Substitution) (constraint : Constraint) :
    satisfies second (mapConstraint first constraint) ↔ satisfies (compose second first) constraint := by
  cases constraint <;> simp [mapConstraint, satisfies, substitute_compose]

theorem solves_mapped (constraints : List Constraint) (first second : Substitution) :
    Solves (constraints.map (mapConstraint first)) second ↔ Solves constraints (compose second first) := by
  constructor
  · intro solved constraint member
    apply (satisfies_mapped first second constraint).mp
    apply solved
    exact List.mem_map.mpr ⟨constraint, member, rfl⟩
  · intro solved mapped member
    obtain ⟨constraint, original, rfl⟩ := List.mem_map.mp member
    exact (satisfies_mapped first second constraint).mpr (solved constraint original)

def MoreGeneral (first second : Substitution) : Prop :=
  ∃ remainder, ∀ id, second id = compose remainder first id

def Principal (constraints : List Constraint) (substitution : Substitution) : Prop :=
  Solves constraints substitution ∧
    ∀ other, Solves constraints other → MoreGeneral substitution other

theorem substitution_extensional (first second : Substitution)
    (same : ∀ id, first id = second id) (ty : MonoType) :
    substitute first ty = substitute second ty := by
  induction ty <;> simp_all [substitute]

theorem compose_associative (first second third : Substitution) :
    compose first (compose second third) = compose (compose first second) third := by
  funext id
  simp [compose, substitute_compose]

theorem more_general_preserves_substitution {first second : Substitution}
    (general : MoreGeneral first second) :
    ∃ remainder, ∀ ty, substitute second ty = substitute remainder (substitute first ty) := by
  obtain ⟨remainder, same⟩ := general
  refine ⟨remainder, ?_⟩
  intro ty
  rw [← substitute_compose]
  exact substitution_extensional second (compose remainder first) same ty

theorem solutions_are_preserved_by_instantiation {constraints : List Constraint}
    {substitution : Substitution} (solves : Solves constraints substitution) (later : Substitution) :
    Solves constraints (compose later substitution) := by
  intro constraint member
  cases constraint with
  | equal left right =>
    have solved := solves (.equal left right) member
    simp only [satisfies, substitute_compose] at solved ⊢
    exact congrArg (substitute later) solved
  | field record name value =>
    obtain ⟨row, same, found⟩ := solves (.field record name value) member
    refine ⟨substitute later row, ?_, ?_⟩
    · rw [substitute_compose, same]
      rfl
    · rw [substitute_compose]
      exact field_occurs_substitute found later

theorem principal_cons {constraint : Constraint} {rest : List Constraint}
    {first second : Substitution} (firstPrincipal : Principal [constraint] first)
    (secondPrincipal : Principal (rest.map (mapConstraint first)) second) :
    Principal (constraint :: rest) (compose second first) := by
  constructor
  · intro current member
    simp only [List.mem_cons] at member
    rcases member with same | remaining
    · subst current
      exact solutions_are_preserved_by_instantiation firstPrincipal.1 second constraint (by simp)
    · exact (solves_mapped rest first second).mp secondPrincipal.1 current remaining
  · intro other solved
    have firstSolved : Solves [constraint] other := by
      intro current member
      simp only [List.mem_singleton] at member
      subst current
      exact solved constraint (by simp)
    obtain ⟨middle, factor⟩ := firstPrincipal.2 other firstSolved
    have sameOther : other = compose middle first := funext factor
    have restSolved : Solves rest other := by
      intro current member
      exact solved current (by simp [member])
    have middleSolved : Solves (rest.map (mapConstraint first)) middle := by
      apply (solves_mapped rest first middle).mpr
      rw [← sameOther]
      exact restSolved
    obtain ⟨last, finalFactor⟩ := secondPrincipal.2 middle middleSolved
    have sameMiddle : middle = compose last second := funext finalFactor
    refine ⟨last, ?_⟩
    intro id
    calc
      other id = compose middle first id := factor id
      _ = compose (compose last second) first id := by rw [sameMiddle]
      _ = compose last (compose second first) id := by rw [compose_associative]

theorem variable_constraint_principal (id : Variable) (ty : MonoType)
    (absent : id ∉ freeVariables ty) :
    Principal [.equal (.variable id) ty] (bindVariable id ty) := by
  constructor
  · intro constraint member
    simp only [List.mem_singleton] at member
    subst constraint
    simp [satisfies, substitute, bindVariable, substitute_absent id ty ty absent]
  · intro other solved
    have equation := solved (.equal (.variable id) ty) (by simp)
    simp only [satisfies, substitute] at equation
    refine ⟨other, ?_⟩
    intro otherId
    by_cases same : otherId = id
    · subst otherId
      simp [compose, bindVariable, equation]
    · simp [compose, bindVariable, substitute, same]

theorem bind_variable_well_sorted (id : Variable) (ty : MonoType)
    (wellFormed : WellFormed ty) (sameSort : typeSort ty = id.kind.sort) :
    WellSorted (bindVariable id ty) := by
  intro other
  by_cases same : other = id
  · subst other
    simpa [bindVariable] using And.intro wellFormed sameSort
  · simp [bindVariable, same, WellFormed, typeSort]

theorem compose_well_sorted {outer inner : Substitution}
    (outerSorted : WellSorted outer) (innerSorted : WellSorted inner) :
    WellSorted (compose outer inner) := by
  intro id
  constructor
  · exact substitute_well_formed outer outerSorted (inner id) (innerSorted id).1
  · change typeSort (substitute outer (inner id)) = id.kind.sort
    rw [substitute_sort outer outerSorted]
    exact (innerSorted id).2

def KindedMoreGeneral (first second : Substitution) : Prop :=
  ∃ remainder, WellSorted remainder ∧ ∀ id, second id = compose remainder first id

def KindedPrincipal (constraints : List Constraint) (substitution : Substitution) : Prop :=
  WellSorted substitution ∧ Solves constraints substitution ∧
    ∀ other, WellSorted other → Solves constraints other → KindedMoreGeneral substitution other

theorem kinded_variable_constraint_principal (id : Variable) (ty : MonoType)
    (absent : id ∉ freeVariables ty) (wellFormed : WellFormed ty)
    (sameSort : typeSort ty = id.kind.sort) :
    KindedPrincipal [.equal (.variable id) ty] (bindVariable id ty) := by
  have solves := (variable_constraint_principal id ty absent).1
  refine ⟨bind_variable_well_sorted id ty wellFormed sameSort, solves, ?_⟩
  intro other sorted solved
  have equation := solved (.equal (.variable id) ty) (by simp)
  simp only [satisfies, substitute] at equation
  refine ⟨other, sorted, ?_⟩
  intro otherId
  by_cases same : otherId = id
  · subst otherId
    simp [compose, bindVariable, equation]
  · simp [compose, bindVariable, substitute, same]

theorem kinded_principal_cons {constraint : Constraint} {rest : List Constraint}
    {first second : Substitution} (firstPrincipal : KindedPrincipal [constraint] first)
    (secondPrincipal : KindedPrincipal (rest.map (mapConstraint first)) second) :
    KindedPrincipal (constraint :: rest) (compose second first) := by
  constructor
  · exact compose_well_sorted secondPrincipal.1 firstPrincipal.1
  constructor
  · intro current member
    simp only [List.mem_cons] at member
    rcases member with same | remaining
    · subst current
      exact solutions_are_preserved_by_instantiation firstPrincipal.2.1 second constraint (by simp)
    · exact (solves_mapped rest first second).mp secondPrincipal.2.1 current remaining
  · intro other sorted solved
    have firstSolved : Solves [constraint] other := by
      intro current member
      simp only [List.mem_singleton] at member
      subst current
      exact solved constraint (by simp)
    obtain ⟨middle, middleSorted, factor⟩ := firstPrincipal.2.2 other sorted firstSolved
    have sameOther : other = compose middle first := funext factor
    have restSolved : Solves rest other := by
      intro current member
      exact solved current (by simp [member])
    have middleSolved : Solves (rest.map (mapConstraint first)) middle := by
      apply (solves_mapped rest first middle).mpr
      rw [← sameOther]
      exact restSolved
    obtain ⟨last, lastSorted, finalFactor⟩ := secondPrincipal.2.2 middle middleSorted middleSolved
    have sameMiddle : middle = compose last second := funext finalFactor
    refine ⟨last, lastSorted, ?_⟩
    intro id
    calc
      other id = compose middle first id := factor id
      _ = compose (compose last second) first id := by rw [sameMiddle]
      _ = compose last (compose second first) id := by rw [compose_associative]

theorem identity_well_sorted : WellSorted identity := by
  intro id
  exact ⟨trivial, rfl⟩

theorem identity_principal_empty : KindedPrincipal [] identity := by
  refine ⟨identity_well_sorted, ?_, ?_⟩
  · intro constraint member
    cases member
  · intro other sorted _
    refine ⟨other, sorted, ?_⟩
    intro id
    rfl

theorem function_constraints_decompose (substitution : Substitution)
    (arguments result otherArguments otherResult : MonoType) :
    Solves [.equal (.function arguments result) (.function otherArguments otherResult)] substitution ↔
      Solves [.equal arguments otherArguments, .equal result otherResult] substitution := by
  simp [Solves, satisfies, substitute]

theorem argument_constraints_decompose (substitution : Substitution)
    (value rest otherValue otherRest : MonoType) :
    Solves [.equal (.argument value rest) (.argument otherValue otherRest)] substitution ↔
      Solves [.equal value otherValue, .equal rest otherRest] substitution := by
  simp [Solves, satisfies, substitute]

theorem result_constraints_decompose (substitution : Substitution)
    (value error otherValue otherError : MonoType) :
    Solves [.equal (.result value error) (.result otherValue otherError)] substitution ↔
      Solves [.equal value otherValue, .equal error otherError] substitution := by
  simp [Solves, satisfies, substitute]

theorem record_constraints_decompose (substitution : Substitution) (row other : MonoType) :
    Solves [.equal (.record row) (.record other)] substitution ↔
      Solves [.equal row other] substitution := by
  simp [Solves, satisfies, substitute]

theorem row_constraints_decompose (substitution : Substitution) (name : NameId)
    (value tail otherValue otherTail : MonoType) :
    Solves [.equal (.rowField name value tail) (.rowField name otherValue otherTail)] substitution ↔
      Solves [.equal value otherValue, .equal tail otherTail] substitution := by
  simp [Solves, satisfies, substitute]

theorem unequal_primitive_constraint_is_unsatisfiable (left right : Primitive)
    (different : left ≠ right) (substitution : Substitution) :
    ¬ Solves [.equal (.primitive left) (.primitive right)] substitution := by
  simp [Solves, satisfies, substitute, different]

structure Scheme where
  quantified : List Variable
  body : MonoType
  requirements : List Constraint

abbrev TypeEnvironment := List (NameId × Scheme)

def constraintVariables : Constraint → List Variable
  | .equal left right => freeVariables left ++ freeVariables right
  | .field record _ value => freeVariables record ++ freeVariables value

def schemeVariables (scheme : Scheme) : List Variable :=
  freeVariables scheme.body ++ scheme.requirements.flatMap constraintVariables

def environmentVariables (environment : TypeEnvironment) : List Variable :=
  environment.flatMap fun (_, scheme) =>
    (schemeVariables scheme).filter fun id => !decide (id ∈ scheme.quantified)

inductive GeneralizationPolicy where
  | generalize
  | fixed
  deriving DecidableEq

def Generalizes (policy : GeneralizationPolicy) (environment : TypeEnvironment)
    (ty : MonoType) (requirements : List Constraint) (scheme : Scheme) : Prop :=
  scheme.body = ty ∧ scheme.requirements = requirements ∧ scheme.quantified.Nodup ∧
    ∀ id, id ∈ scheme.quantified ↔
      policy = .generalize ∧ id ∈ (freeVariables ty ++ requirements.flatMap constraintVariables) ∧
        id ∉ environmentVariables environment

def schemeSubstitution (scheme : Scheme) (arguments : Substitution) : Substitution :=
  fun id => if id ∈ scheme.quantified then arguments id else .variable id

def Instantiates (scheme : Scheme) (arguments : Substitution)
    (instantiated : MonoType) (requirements : List Constraint) : Prop :=
  instantiated = substitute (schemeSubstitution scheme arguments) scheme.body ∧
    requirements = scheme.requirements.map (mapConstraint (schemeSubstitution scheme arguments))

def identityVariable : Variable := ⟨0, .value⟩

def identityScheme : Scheme :=
  ⟨[identityVariable], .function (.argument (.variable identityVariable) .argumentsEmpty)
    (.variable identityVariable), []⟩

theorem identity_scheme_generalizes :
    Generalizes .generalize [] (.function (.argument (.variable identityVariable) .argumentsEmpty)
      (.variable identityVariable)) [] identityScheme := by
  simp [Generalizes, identityScheme, freeVariables, environmentVariables]

theorem identity_scheme_has_independent_instances :
    Instantiates identityScheme (fun _ => .primitive .integer)
      (.function (.argument (.primitive .integer) .argumentsEmpty) (.primitive .integer)) [] ∧
    Instantiates identityScheme (fun _ => .primitive .boolean)
      (.function (.argument (.primitive .boolean) .argumentsEmpty) (.primitive .boolean)) [] := by
  simp [Instantiates, schemeSubstitution, identityScheme, substitute]

theorem monomorphic_identity_is_not_a_generalization :
    ¬ Generalizes .generalize [] (.function (.argument (.variable identityVariable) .argumentsEmpty)
      (.variable identityVariable)) []
      ⟨[], .function (.argument (.variable identityVariable) .argumentsEmpty)
        (.variable identityVariable), []⟩ := by
  simp [Generalizes, freeVariables, environmentVariables]

theorem fixed_binding_does_not_generalize (environment : TypeEnvironment) (ty : MonoType)
    (requirements : List Constraint) (scheme : Scheme)
    (valid : Generalizes .fixed environment ty requirements scheme) :
    scheme.quantified = [] := by
  cases found : scheme.quantified with
  | nil => rfl
  | cons id rest =>
    have member : id ∈ scheme.quantified := by simp [found]
    have impossible := (valid.2.2.2 id).mp member
    cases impossible.1

def projectionRow : Variable := ⟨1, .row⟩

def projectionScheme (name : NameId) : Scheme :=
  ⟨[projectionRow, identityVariable],
    .function (.argument (.record (.variable projectionRow)) .argumentsEmpty)
      (.variable identityVariable),
    [.field (.record (.variable projectionRow)) name (.variable identityVariable)]⟩

def projectionArguments (name : NameId) : Substitution := fun id =>
  if id = projectionRow then .rowField name (.primitive .integer) .rowEmpty
  else if id = identityVariable then .primitive .integer
  else .variable id

theorem projection_scheme_accepts_an_integer_field (name : NameId) :
    Instantiates (projectionScheme name) (projectionArguments name)
      (.function (.argument (.record (.rowField name (.primitive .integer) .rowEmpty))
        .argumentsEmpty) (.primitive .integer))
      [.field (.record (.rowField name (.primitive .integer) .rowEmpty)) name (.primitive .integer)] ∧
    checksConstraints identity
      [.field (.record (.rowField name (.primitive .integer) .rowEmpty)) name (.primitive .integer)] = true := by
  simp [Instantiates, schemeSubstitution, projectionScheme, projectionArguments, projectionRow,
    identityVariable, substitute, mapConstraint, checksConstraints, checksConstraint, identity, fieldAt?]

theorem projection_scheme_rejects_an_absent_field (name other : NameId) (different : name ≠ other) :
    checksConstraints identity
      [.field (.record (.rowField other (.primitive .integer) .rowEmpty)) name (.primitive .integer)] = false := by
  simp [checksConstraints, checksConstraint, identity, substitute, fieldAt?, different]

end Xsh.Typing.Connected
