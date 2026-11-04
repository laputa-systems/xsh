import Xsh.Typing.Connected.Certificate

namespace Xsh.Typing.Connected

inductive DerivationStep where
  | reflexive
  | bindLeft
  | bindRight
  | function
  | argument
  | result
  | record
  | row
  | nominal
  | field
  deriving DecidableEq

def bindingAllowed (id : Variable) (ty : MonoType) : Bool :=
  decide (id ∉ freeVariables ty) && checksWellFormed ty && decide (typeSort ty = id.kind.sort)

theorem binding_allowed_principal {id : Variable} {ty : MonoType}
    (allowed : bindingAllowed id ty = true) :
    KindedPrincipal [.equal (.variable id) ty] (bindVariable id ty) := by
  simp [bindingAllowed, Bool.and_eq_true, checks_well_formed_iff] at allowed
  exact kinded_variable_constraint_principal id ty allowed.1.1 allowed.1.2 allowed.2

theorem solves_cons_iff (constraint : Constraint) (rest : List Constraint) (solution : Substitution) :
    Solves (constraint :: rest) solution ↔ satisfies solution constraint ∧ Solves rest solution := by
  constructor
  · intro solved
    exact ⟨solved constraint (by simp), fun current member => solved current (by simp [member])⟩
  · rintro ⟨first, others⟩ current member
    simp only [List.mem_cons] at member
    rcases member with rfl | member
    · exact first
    · exact others current member

theorem principal_of_equivalent {first second : List Constraint} {solution : Substitution}
    (equivalent : ∀ other, Solves first other ↔ Solves second other)
    (principal : KindedPrincipal second solution) : KindedPrincipal first solution := by
  refine ⟨principal.1, (equivalent solution).mpr principal.2.1, ?_⟩
  intro other sorted solved
  exact principal.2.2 other sorted ((equivalent other).mp solved)

theorem known_field_equivalent (row value field : MonoType) (name : Xsh.Foundation.NameId)
    (found : fieldAt? name row = some field) (other : Substitution) :
    satisfies other (.field (.record row) name value) ↔ satisfies other (.equal field value) := by
  have known : FieldOccurs name (substitute other field) (substitute other row) :=
    field_occurs_substitute (field_occurs_iff.mpr found) other
  simp only [satisfies, substitute, MonoType.record.injEq]
  constructor
  · rintro ⟨_, rfl, actual⟩
    have same := (field_occurs_iff.mp known).symm.trans (field_occurs_iff.mp actual)
    exact Option.some.inj same
  · intro same
    rw [← same]
    exact ⟨_, rfl, known⟩

def checkDerivation : List DerivationStep → List Constraint → Except ValidationError Substitution
  | [], [] => .ok identity
  | [], _ :: _ => .error .exhausted
  | _ :: _, [] => .error .mismatch
  | step :: steps, constraint :: rest => do
    match step, constraint with
    | .reflexive, .equal left right => do
        requireCheck (decide (left = right))
        checkDerivation steps rest
    | .reflexive, .store storage replacement => do
        requireCheck (decide (storage = replacement))
        checkDerivation steps rest
    | .bindLeft, .equal (.variable id) ty => do
        requireCheck (bindingAllowed id ty)
        let later ← checkDerivation steps (rest.map (mapConstraint (bindVariable id ty)))
        pure (compose later (bindVariable id ty))
    | .bindRight, .equal ty (.variable id) => do
        requireCheck (bindingAllowed id ty)
        let later ← checkDerivation steps (rest.map (mapConstraint (bindVariable id ty)))
        pure (compose later (bindVariable id ty))
    | .function, .equal (.function arguments result) (.function otherArguments otherResult) =>
        checkDerivation steps (.equal arguments otherArguments :: .equal result otherResult :: rest)
    | .argument, .equal (.argument value tail) (.argument otherValue otherTail) =>
        checkDerivation steps (.equal value otherValue :: .equal tail otherTail :: rest)
    | .result, .equal (.result value error) (.result otherValue otherError) =>
        checkDerivation steps (.equal value otherValue :: .equal error otherError :: rest)
    | .record, .equal (.record row) (.record other) =>
        checkDerivation steps (.equal row other :: rest)
    | .row, .equal (.rowField name value tail) (.rowField otherName otherValue otherTail) => do
        requireCheck (decide (name = otherName))
        checkDerivation steps (.equal value otherValue :: .equal tail otherTail :: rest)
    | .nominal, .equal (.nominal identity base) (.nominal otherIdentity otherBase) => do
        requireCheck (decide (identity = otherIdentity))
        checkDerivation steps (.equal base otherBase :: rest)
    | .field, .field (.record row) name value => do
        let some field := fieldAt? name row | throw .unsupported
        checkDerivation steps (.equal field value :: rest)
    | _, _ => throw .mismatch

theorem check_derivation_principal (steps : List DerivationStep) :
    ∀ constraints solution, checkDerivation steps constraints = .ok solution →
      KindedPrincipal constraints solution := by
  induction steps with
  | nil =>
    intro constraints solution accepted
    cases constraints with
    | nil =>
      simp [checkDerivation] at accepted
      subst solution
      exact identity_principal_empty
    | cons _ _ => cases accepted
  | cons step steps ih =>
    intro constraints solution accepted
    cases constraints with
    | nil => cases accepted
    | cons constraint rest =>
      cases step with
      | reflexive =>
        cases constraint with
        | equal left right =>
          simp [checkDerivation, except_bind_ok, exists_require_ok] at accepted
          obtain ⟨rfl, restAccepted⟩ := accepted
          have principal := ih rest solution restAccepted
          refine ⟨principal.1, ?_, ?_⟩
          · intro current member
            simp only [List.mem_cons] at member
            rcases member with same | remaining
            · subst current
              rfl
            · exact principal.2.1 current remaining
          · intro other sorted solved
            apply principal.2.2 other sorted
            intro current member
            exact solved current (by simp [member])
        | field _ _ _ => cases accepted
        | store storage replacement =>
          simp [checkDerivation, except_bind_ok, exists_require_ok] at accepted
          obtain ⟨rfl, restAccepted⟩ := accepted
          have principal := ih rest solution restAccepted
          refine ⟨principal.1, ?_, ?_⟩
          · intro current member
            simp only [List.mem_cons] at member
            rcases member with same | remaining
            · subst current
              exact StoreCompatible.same _
            · exact principal.2.1 current remaining
          · intro other sorted solved
            apply principal.2.2 other sorted
            intro current member
            exact solved current (by simp [member])
      | bindLeft =>
        cases constraint with
        | equal left right =>
          cases left with
          | «variable» id =>
            simp [checkDerivation, except_bind_ok, except_map_ok, exists_require_ok] at accepted
            obtain ⟨allowed, later, restAccepted, rfl⟩ := accepted
            exact kinded_principal_cons (binding_allowed_principal allowed) (ih _ later restAccepted)
          | _ => cases accepted
        | field _ _ _ => cases accepted
        | store _ _ => cases accepted

      | bindRight =>
        cases constraint with
        | equal left right =>
          cases right with
          | «variable» id =>
            simp [checkDerivation, except_bind_ok, except_map_ok, exists_require_ok] at accepted
            obtain ⟨allowed, later, restAccepted, rfl⟩ := accepted
            have flipped : KindedPrincipal [.equal left (.variable id)] (bindVariable id left) := by
              apply principal_of_equivalent _ (binding_allowed_principal allowed)
              intro other
              simp [solves_cons_iff, satisfies, eq_comm]
            exact kinded_principal_cons flipped (ih _ later restAccepted)
          | _ => cases accepted
        | field _ _ _ => cases accepted
        | store _ _ => cases accepted
      | function =>
        cases constraint with
        | equal left right =>
          cases left <;> cases right <;> try cases accepted
          case function.function arguments result otherArguments otherResult =>
            apply principal_of_equivalent _ (ih _ solution accepted)
            intro other
            simp [solves_cons_iff, satisfies, substitute, and_assoc]
        | field _ _ _ => cases accepted
        | store _ _ => cases accepted
      | argument =>
        cases constraint with
        | equal left right =>
          cases left <;> cases right <;> try cases accepted
          case argument.argument value tail otherValue otherTail =>
            apply principal_of_equivalent _ (ih _ solution accepted)
            intro other
            simp [solves_cons_iff, satisfies, substitute, and_assoc]
        | field _ _ _ => cases accepted
        | store _ _ => cases accepted
      | result =>
        cases constraint with
        | equal left right =>
          cases left <;> cases right <;> try cases accepted
          case result.result value error otherValue otherError =>
            apply principal_of_equivalent _ (ih _ solution accepted)
            intro other
            simp [solves_cons_iff, satisfies, substitute, and_assoc]
        | field _ _ _ => cases accepted
        | store _ _ => cases accepted
      | record =>
        cases constraint with
        | equal left right =>
          cases left <;> cases right <;> try cases accepted
          case record.record row otherRow =>
            apply principal_of_equivalent _ (ih _ solution accepted)
            intro other
            simp [solves_cons_iff, satisfies, substitute]
        | field _ _ _ => cases accepted
        | store _ _ => cases accepted

      | row =>
        cases constraint with
        | equal left right =>
          cases left <;> cases right <;> try cases accepted
          case rowField.rowField name value tail otherName otherValue otherTail =>
            simp [checkDerivation, except_bind_ok, exists_require_ok] at accepted
            obtain ⟨rfl, accepted⟩ := accepted
            apply principal_of_equivalent _ (ih _ solution accepted)
            intro other
            simp [solves_cons_iff, satisfies, substitute, and_assoc]
        | field _ _ _ => cases accepted
        | store _ _ => cases accepted

      | nominal =>
        cases constraint with
        | equal left right =>
          cases left <;> cases right <;> try cases accepted
          case nominal.nominal identity base otherIdentity otherBase =>
            simp [checkDerivation, except_bind_ok, exists_require_ok] at accepted
            obtain ⟨rfl, accepted⟩ := accepted
            apply principal_of_equivalent _ (ih _ solution accepted)
            intro other
            simp [solves_cons_iff, satisfies, substitute]
        | field _ _ _ => cases accepted
        | store _ _ => cases accepted
      | field =>
        cases constraint with
        | equal _ _ => cases accepted
        | field record name value =>
          cases record with
          | record row =>
            cases found : fieldAt? name row with
            | none => simp [checkDerivation, found] at accepted
            | some field =>
              simp [checkDerivation, found] at accepted
              apply principal_of_equivalent _ (ih _ solution accepted)
              intro other
              simp only [solves_cons_iff]
              rw [known_field_equivalent row value field name found other]
          | _ => cases accepted
        | store _ _ => cases accepted


def validateDerivedExpression (fuel : Nat) (scope : Scope) (source : Xsh.Syntax.Expr)
    (candidate : Node) (steps : List DerivationStep) : Except ValidationError MonoType := do
  let (_, state) ← ((expression fuel scope source).run (initialState scope)).mapError generationFailure
  let solution ← checkDerivation steps state.constraints
  validateGeneratedExpression fuel scope solution source candidate

theorem derived_expression_certificate_sound (fuel : Nat) (scope : Scope)
    (source : Xsh.Syntax.Expr) (candidate : Node) (steps : List DerivationStep) (ty : MonoType)
    (accepted : validateDerivedExpression fuel scope source candidate steps = .ok ty) :
    ∃ generated state solution,
      (expression fuel scope source).run (initialState scope) = .ok (generated, state) ∧
      checkDerivation steps state.constraints = .ok solution ∧
      KindedPrincipal state.constraints solution ∧ ExpressionHasType scope solution source ty := by
  unfold validateDerivedExpression at accepted
  cases found : (expression fuel scope source).run (initialState scope) with
  | error error =>
    simp only [found, Except.mapError] at accepted
    cases accepted
  | ok output =>
    obtain ⟨generated, state⟩ := output
    simp [found, Except.mapError, except_bind_ok] at accepted
    obtain ⟨solution, derived, checked⟩ := accepted
    exact ⟨generated, state, solution, rfl, derived, check_derivation_principal steps _ _ derived,
      (generated_expression_certificate_sound fuel scope solution source candidate ty checked).1⟩


def identityDerivation : List DerivationStep :=
  [.function, .argument, .bindLeft, .reflexive, .bindRight]

def derivedIdentityAccepted : Bool :=
  match validateDerivedExpression 12 [identityBinding] identityCallSource
      generatedIdentityCandidate identityDerivation with
  | .ok (.primitive .integer) => true
  | _ => false

theorem derived_identity_certificate_is_inhabited : derivedIdentityAccepted = true := by
  decide +kernel


def derivationRefuses (steps : List DerivationStep) (constraints : List Constraint) : Bool :=
  match checkDerivation steps constraints with
  | .error .mismatch => true
  | _ => false

def derivationExhausted (steps : List DerivationStep) (constraints : List Constraint) : Bool :=
  match checkDerivation steps constraints with
  | .error .exhausted => true
  | _ => false

theorem derivation_rejects_cycles_wrong_steps_and_distinguishes_exhaustion :
    derivationRefuses [.bindLeft] [.equal (.variable identityVariable)
      (.function (.argument (.variable identityVariable) .argumentsEmpty) (.primitive .integer))] = true ∧
    derivationRefuses [.function] [.equal (.primitive .integer) (.primitive .boolean)] = true ∧
    derivationExhausted [] [.equal (.primitive .integer) (.primitive .integer)] = true := by
  constructor
  · decide +kernel
  constructor <;> decide +kernel


def knownFieldDerivationAccepted : Bool :=
  let fieldName := exampleName 1 (by decide)
  match checkDerivation [.field, .bindRight]
      [.field (.record (.rowField fieldName (.primitive .integer) .rowEmpty))
        fieldName (.variable identityVariable)] with
  | .ok solution => decide (solution identityVariable = .primitive .integer)
  | _ => false

theorem known_field_derivation_is_inhabited : knownFieldDerivationAccepted = true := by
  decide +kernel

end Xsh.Typing.Connected
