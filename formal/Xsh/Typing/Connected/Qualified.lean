import Xsh.Typing.Connected.BlockCertificate

namespace Xsh.Typing.Connected

open Xsh.Syntax

def QualifiedPrincipal (constraints : List Constraint) (solution : Substitution)
    (residual : List Constraint) : Prop :=
  WellSorted solution ∧
    (∀ later, WellSorted later →
      (Solves constraints (compose later solution) ↔ Solves residual later)) ∧
    ∀ other, WellSorted other → Solves constraints other →
      ∃ later, WellSorted later ∧ (∀ id, other id = compose later solution id) ∧ Solves residual later

theorem solves_append_iff (first rest : List Constraint) (solution : Substitution) :
    Solves (first ++ rest) solution ↔ Solves first solution ∧ Solves rest solution := by
  constructor
  · intro solved
    constructor
    · intro constraint member
      exact solved constraint (List.mem_append_left rest member)
    · intro constraint member
      exact solved constraint (List.mem_append_right first member)
  · rintro ⟨left, right⟩ constraint member
    rcases List.mem_append.mp member with member | member
    · exact left constraint member
    · exact right constraint member

theorem principal_prefix_qualifies_remaining {first rest : List Constraint} {solution : Substitution}
    (principal : KindedPrincipal first solution) :
    QualifiedPrincipal (first ++ rest) solution (rest.map (mapConstraint solution)) := by
  refine ⟨principal.1, ?_, ?_⟩
  · intro later _
    rw [solves_append_iff]
    constructor
    · intro both
      exact (solves_mapped rest solution later).mpr both.2
    · intro remaining
      exact ⟨solutions_are_preserved_by_instantiation principal.2.1 later,
        (solves_mapped rest solution later).mp remaining⟩
  · intro other sorted solved
    obtain ⟨firstSolved, restSolved⟩ := (solves_append_iff first rest other).mp solved
    obtain ⟨later, laterSorted, factor⟩ := principal.2.2 other sorted firstSolved
    have same : other = compose later solution := funext factor
    refine ⟨later, laterSorted, factor, ?_⟩
    apply (solves_mapped rest solution later).mpr
    rw [← same]
    exact restSolved

theorem checked_prefix_qualifies_remaining (steps : List DerivationStep)
    (first rest : List Constraint) (solution : Substitution)
    (checked : checkDerivation steps first = .ok solution) :
    QualifiedPrincipal (first ++ rest) solution (rest.map (mapConstraint solution)) :=
  principal_prefix_qualifies_remaining (check_derivation_principal steps first solution checked)

def QualifiedScheme (policy : GeneralizationPolicy) (environment : TypeEnvironment)
    (ty : MonoType) (constraints : List Constraint) (solution : Substitution) (scheme : Scheme) : Prop :=
  QualifiedPrincipal constraints solution scheme.requirements ∧
    Generalizes policy environment (substitute solution ty) scheme.requirements scheme


def generalizableVariables (policy : GeneralizationPolicy) (environment : TypeEnvironment)
    (ty : MonoType) (requirements : List Constraint) : List Variable :=
  match policy with
  | .fixed => []
  | .generalize =>
    (freeVariables ty ++ requirements.flatMap constraintVariables).filter
      fun id => !decide (id ∈ environmentVariables environment)

theorem mem_generalizable_variables (policy : GeneralizationPolicy) (environment : TypeEnvironment)
    (ty : MonoType) (requirements : List Constraint) (id : Variable) :
    id ∈ generalizableVariables policy environment ty requirements ↔
      policy = .generalize ∧ id ∈ (freeVariables ty ++ requirements.flatMap constraintVariables) ∧
        id ∉ environmentVariables environment := by
  cases policy with
  | fixed => simp [generalizableVariables]
  | generalize =>
    simp [generalizableVariables]
    constructor
    · rintro (⟨left, absent⟩ | ⟨right, absent⟩)
      · exact ⟨Or.inl left, absent⟩
      · exact ⟨Or.inr right, absent⟩
    · rintro ⟨left | right, absent⟩
      · exact Or.inl ⟨left, absent⟩
      · exact Or.inr ⟨right, absent⟩

def checksGeneralization (policy : GeneralizationPolicy) (environment : TypeEnvironment)
    (ty : MonoType) (requirements : List Constraint) (scheme : Scheme) : Bool :=
  let eligible := generalizableVariables policy environment ty requirements
  decide (scheme.body = ty) && decide (scheme.requirements = requirements) &&
    decide scheme.quantified.Nodup &&
    scheme.quantified.all (fun id => decide (id ∈ eligible)) &&
    eligible.all (fun id => decide (id ∈ scheme.quantified))

theorem checks_generalization_iff (policy : GeneralizationPolicy) (environment : TypeEnvironment)
    (ty : MonoType) (requirements : List Constraint) (scheme : Scheme) :
    checksGeneralization policy environment ty requirements scheme = true ↔
      Generalizes policy environment ty requirements scheme := by
  simp [checksGeneralization, Generalizes, Bool.and_eq_true, List.all_eq_true,
    mem_generalizable_variables]
  constructor
  · rintro ⟨⟨⟨⟨body, requirementsEqual⟩, unique⟩, forward⟩, backward⟩
    exact ⟨body, requirementsEqual, unique, fun id => ⟨forward id, fun ⟨p, member, absent⟩ => backward id p member absent⟩⟩
  · rintro ⟨body, requirementsEqual, unique, both⟩
    exact ⟨⟨⟨⟨body, requirementsEqual⟩, unique⟩, fun id => (both id).mp⟩,
      fun id p member absent => (both id).mpr ⟨p, member, absent⟩⟩


def validateQualifiedScheme (steps : List DerivationStep) (first rest : List Constraint)
    (policy : GeneralizationPolicy) (environment : TypeEnvironment) (ty : MonoType)
    (scheme : Scheme) : Except ValidationError Substitution := do
  let solution ← checkDerivation steps first
  let residual := rest.map (mapConstraint solution)
  requireCheck (checksGeneralization policy environment (substitute solution ty) residual scheme)
  pure solution

theorem validated_qualified_scheme_is_principal (steps : List DerivationStep)
    (first rest : List Constraint) (policy : GeneralizationPolicy) (environment : TypeEnvironment)
    (ty : MonoType) (scheme : Scheme) (solution : Substitution)
    (accepted : validateQualifiedScheme steps first rest policy environment ty scheme = .ok solution) :
    QualifiedScheme policy environment ty (first ++ rest) solution scheme := by
  simp [validateQualifiedScheme, except_bind_ok, except_map_ok, exists_require_ok,
    checks_generalization_iff] at accepted
  obtain ⟨checked, generalizes⟩ := accepted
  have principal := checked_prefix_qualifies_remaining steps first rest solution checked
  unfold QualifiedScheme
  rw [generalizes.2.1]
  exact ⟨principal, generalizes⟩

theorem identity_qualifies_all_constraints (constraints : List Constraint) :
    QualifiedPrincipal constraints identity constraints := by
  refine ⟨identity_well_sorted, ?_, ?_⟩
  · intro later _
    have same : compose later identity = later := by funext id; rfl
    rw [same]
  · intro other sorted solved
    refine ⟨other, sorted, ?_, solved⟩
    intro id
    rfl


def closedRowFields : MonoType → Option (List (Xsh.Foundation.NameId × MonoType))
  | .rowEmpty => some []
  | .rowField name value tail => (closedRowFields tail).map ((name, value) :: ·)
  | _ => none

theorem closed_field_occurs_is_member {name : Xsh.Foundation.NameId} {value row : MonoType}
    {fields : List (Xsh.Foundation.NameId × MonoType)} (closed : closedRowFields row = some fields)
    (found : FieldOccurs name value row) : (name, value) ∈ fields := by
  induction found generalizing fields with
  | here =>
    simp [closedRowFields] at closed
    obtain ⟨tail, _, rfl⟩ := closed
    simp
  | later different tailFound ih =>
    simp [closedRowFields] at closed
    obtain ⟨tail, tailClosed, rfl⟩ := closed
    exact List.mem_cons_of_mem _ (ih tailClosed)

theorem unique_closed_member_is_field {name : Xsh.Foundation.NameId} {value row : MonoType}
    {fields : List (Xsh.Foundation.NameId × MonoType)} (closed : closedRowFields row = some fields)
    (unique : (fields.map Prod.fst).Nodup) (member : (name, value) ∈ fields) :
    FieldOccurs name value row := by
  induction row generalizing fields with
  | rowEmpty => simp [closedRowFields] at closed; subst fields; simp at member
  | rowField other head tail ihHead ihTail =>
    simp [closedRowFields] at closed
    obtain ⟨tailFields, tailClosed, rfl⟩ := closed
    simp only [List.map_cons, List.nodup_cons] at unique
    simp only [List.mem_cons] at member
    rcases member with same | later
    · cases same
      exact .here
    · have different : name ≠ other := by
        intro same
        subst name
        apply unique.1
        exact List.mem_map.mpr ⟨(other, value), later, rfl⟩
      exact .later different (ihTail tailClosed unique.2 later)
  | _ => simp [closedRowFields] at closed

theorem closed_field_constraint_iff_membership (name : Xsh.Foundation.NameId) (value row : MonoType)
    (fields : List (Xsh.Foundation.NameId × MonoType)) (closed : closedRowFields row = some fields)
    (unique : (fields.map Prod.fst).Nodup) :
    FieldOccurs name value row ↔ (name, value) ∈ fields := by
  constructor
  · exact closed_field_occurs_is_member closed
  · exact unique_closed_member_is_field closed unique



theorem split_member {α : Type} (item : α) : ∀ items : List α, item ∈ items →
    ∃ before after, items = before ++ item :: after := by
  intro items member
  induction items with
  | nil => cases member
  | cons head tail ih =>
    simp only [List.mem_cons] at member
    rcases member with same | later
    · subst head
      exact ⟨[], tail, rfl⟩
    · obtain ⟨before, after, split⟩ := ih later
      exact ⟨head :: before, after, by simp [split]⟩

def RowLacksRepresentation (name : Xsh.Foundation.NameId) (value : MonoType)
    (fields : List (Xsh.Foundation.NameId × MonoType)) : Prop :=
  ∃ rest, fields.Perm ((name, value) :: rest) ∧ name ∉ rest.map Prod.fst

theorem unique_member_iff_row_lacks (name : Xsh.Foundation.NameId) (value : MonoType)
    (fields : List (Xsh.Foundation.NameId × MonoType)) (unique : (fields.map Prod.fst).Nodup) :
    (name, value) ∈ fields ↔ RowLacksRepresentation name value fields := by
  constructor
  · intro member
    obtain ⟨before, after, rfl⟩ := split_member (name, value) fields member
    have rearranged : (before ++ (name, value) :: after).Perm ((name, value) :: (after ++ before)) :=
      List.perm_append_comm
    have distinct : (name :: (after ++ before).map Prod.fst).Nodup := by
      apply (rearranged.map Prod.fst).nodup
      exact unique
    exact ⟨after ++ before, rearranged, (List.nodup_cons.mp distinct).1⟩
  · rintro ⟨rest, permuted, _⟩
    exact permuted.mem_iff.mpr (by simp)

theorem closed_projection_iff_row_lacks (name : Xsh.Foundation.NameId) (value row : MonoType)
    (fields : List (Xsh.Foundation.NameId × MonoType)) (closed : closedRowFields row = some fields)
    (unique : (fields.map Prod.fst).Nodup) :
    FieldOccurs name value row ↔ RowLacksRepresentation name value fields :=
  (closed_field_constraint_iff_membership name value row fields closed unique).trans
    (unique_member_iff_row_lacks name value fields unique)


theorem projection_scheme_is_qualified (name : Xsh.Foundation.NameId) :
    QualifiedScheme .generalize []
      (.function (.argument (.record (.variable projectionRow)) .argumentsEmpty) (.variable identityVariable))
      [.field (.record (.variable projectionRow)) name (.variable identityVariable)] identity
      (projectionScheme name) := by
  constructor
  · exact identity_qualifies_all_constraints _
  · simp [Generalizes, projectionScheme, freeVariables, environmentVariables, constraintVariables,
      substitute, identity, projectionRow, identityVariable]
    intro id
    constructor
    · rintro (left | right)
      · exact Or.inl left
      · exact Or.inr (Or.inl right)
    · rintro (left | right | left | right)
      · exact Or.inl left
      · exact Or.inr right
      · exact Or.inl left
      · exact Or.inr right


theorem instantiated_projection_iff_row_lacks (solution : Substitution)
    (name : Xsh.Foundation.NameId) (value row : MonoType)
    (fields : List (Xsh.Foundation.NameId × MonoType))
    (closed : closedRowFields (substitute solution row) = some fields)
    (unique : (fields.map Prod.fst).Nodup) :
    satisfies solution (.field (.record row) name value) ↔
      RowLacksRepresentation name (substitute solution value) fields := by
  constructor
  · rintro ⟨actual, same, found⟩
    simp only [substitute, MonoType.record.injEq] at same
    subst actual
    exact (closed_projection_iff_row_lacks name (substitute solution value)
      (substitute solution row) fields closed unique).mp found
  · intro represented
    refine ⟨substitute solution row, rfl, ?_⟩
    exact (closed_projection_iff_row_lacks name (substitute solution value)
      (substitute solution row) fields closed unique).mpr represented


def rowLacksMutation (name : Xsh.Foundation.NameId) : Prop :=
  ∃ rest, [(name, MonoType.primitive .integer), (name, MonoType.primitive .boolean)].Perm
    ((name, MonoType.primitive .integer) :: rest)

theorem omitting_lacks_allows_duplicate_field (name : Xsh.Foundation.NameId) :
    rowLacksMutation name ∧
      ¬ RowLacksRepresentation name (MonoType.primitive .integer)
        [(name, MonoType.primitive .integer), (name, MonoType.primitive .boolean)] := by
  constructor
  · exact ⟨[(name, MonoType.primitive .boolean)], List.Perm.refl _⟩
  · rintro ⟨rest, permuted, missing⟩
    have member : name ∈ rest.map Prod.fst := by
      have allFields := permuted.mem_iff.mp (show (name, MonoType.primitive .boolean) ∈
        [(name, MonoType.primitive .integer), (name, MonoType.primitive .boolean)] by simp)
      simp at allFields
      exact List.mem_map.mpr ⟨(name, MonoType.primitive .boolean), allFields, rfl⟩
    exact missing member


def projectionType : MonoType :=
  .function (.argument (.record (.variable projectionRow)) .argumentsEmpty) (.variable identityVariable)

def projectionRequirement (name : Xsh.Foundation.NameId) : Constraint :=
  .field (.record (.variable projectionRow)) name (.variable identityVariable)

def qualifiedProjectionAccepted (name : Xsh.Foundation.NameId) : Bool :=
  match validateQualifiedScheme [] [] [projectionRequirement name] .generalize [] projectionType (projectionScheme name) with
  | .ok _ => true
  | _ => false

def monomorphicProjectionRejected (name : Xsh.Foundation.NameId) : Bool :=
  match validateQualifiedScheme [] [] [projectionRequirement name] .generalize [] projectionType
      ⟨[], projectionType, [projectionRequirement name]⟩ with
  | .error .mismatch => true
  | _ => false

theorem qualified_projection_checker_accepts_and_rejects_monomorphic :
    qualifiedProjectionAccepted (exampleName 1 (by decide)) = true ∧
      monomorphicProjectionRejected (exampleName 1 (by decide)) = true := by
  constructor <;> decide +kernel


theorem solved_projection_iff_row_lacks (solution : Substitution)
    (name : Xsh.Foundation.NameId) (record value row : MonoType)
    (fields : List (Xsh.Foundation.NameId × MonoType))
    (isRecord : substitute solution record = .record row)
    (closed : closedRowFields row = some fields) (unique : (fields.map Prod.fst).Nodup) :
    satisfies solution (.field record name value) ↔
      RowLacksRepresentation name (substitute solution value) fields := by
  constructor
  · rintro ⟨actual, same, found⟩
    rw [isRecord] at same
    cases same
    exact (closed_projection_iff_row_lacks name (substitute solution value) row fields closed unique).mp found
  · intro represented
    exact ⟨row, isRecord, (closed_projection_iff_row_lacks name (substitute solution value)
      row fields closed unique).mpr represented⟩



theorem solves_permutation {first second : List Constraint} (permutation : first.Perm second)
    (solution : Substitution) : Solves first solution ↔ Solves second solution := by
  constructor <;> intro solved constraint member
  · exact solved constraint (permutation.mem_iff.mpr member)
  · exact solved constraint (permutation.mem_iff.mp member)

theorem qualified_principal_permutation {first second : List Constraint} {solution : Substitution}
    {residual : List Constraint} (permutation : first.Perm second)
    (principal : QualifiedPrincipal second solution residual) : QualifiedPrincipal first solution residual := by
  refine ⟨principal.1, ?_, ?_⟩
  · intro later sorted
    exact (solves_permutation permutation (compose later solution)).trans (principal.2.1 later sorted)
  · intro other sorted solved
    exact principal.2.2 other sorted ((solves_permutation permutation other).mp solved)

theorem qualified_scheme_permutation {first second : List Constraint} {solution : Substitution}
    {policy : GeneralizationPolicy} {environment : TypeEnvironment} {ty : MonoType} {scheme : Scheme}
    (permutation : first.Perm second) (qualified : QualifiedScheme policy environment ty second solution scheme) :
    QualifiedScheme policy environment ty first solution scheme :=
  ⟨qualified_principal_permutation permutation qualified.1, qualified.2⟩

def parameterTypes (parameters : List Binding) : MonoType :=
  parameters.foldr (fun binding rest => .argument binding.scheme.body rest) .argumentsEmpty

def parameterDeclarationsValid (parameters globals : List Binding) : Bool :=
  decide ((parameters ++ globals).map Binding.binder).Nodup &&
    decide (parameters.map Binding.name).Nodup &&
    parameters.all fun binding =>
      decide (binding.kind = .immutable) && binding.scheme.quantified.isEmpty && binding.scheme.requirements.isEmpty

def globalEnvironment (globals : List Binding) : TypeEnvironment :=
  globals.map fun binding => (binding.name, binding.scheme)

def validateGeneratedFunctionScheme (fuel : Nat) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (body : List Statement) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (scheme : Scheme) : Except ValidationError Substitution := do
  if parameterDeclarationsValid parameters globals then pure () else throw .unsupported
  let (_, state) ← ((block fuel names (parameters ++ globals) result body).run
    (initialBlockState (parameters ++ globals) result)).mapError generationFailure
  requireCheck (decide (state.constraints.Perm ordered))
  if solvedCount ≤ ordered.length then pure () else throw .mismatch
  validateQualifiedScheme steps (ordered.take solvedCount) (ordered.drop solvedCount)
    .generalize (globalEnvironment globals) (.function (parameterTypes parameters) result) scheme

theorem generated_function_scheme_is_principal (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat) (scheme : Scheme) (solution : Substitution)
    (accepted : validateGeneratedFunctionScheme fuel names parameters globals result body ordered steps solvedCount scheme = .ok solution) :
    ∃ generated state,
      (block fuel names (parameters ++ globals) result body).run (initialBlockState (parameters ++ globals) result) =
        .ok (generated, state) ∧
      QualifiedScheme .generalize (globalEnvironment globals) (.function (parameterTypes parameters) result)
        state.constraints solution scheme := by
  unfold validateGeneratedFunctionScheme at accepted
  cases valid : parameterDeclarationsValid parameters globals with
  | false =>
    simp only [valid, Bool.false_eq_true, ↓reduceIte] at accepted
    cases accepted
  | true =>
    simp only [valid, ↓reduceIte] at accepted
    cases found : (block fuel names (parameters ++ globals) result body).run
        (initialBlockState (parameters ++ globals) result) with
    | error error =>
      simp only [found, Except.mapError] at accepted
      cases accepted
    | ok output =>
      obtain ⟨generated, state⟩ := output
      by_cases within : solvedCount ≤ ordered.length
      · simp [found, Except.mapError, except_bind_ok, exists_require_ok, within] at accepted
        obtain ⟨permutation, checked⟩ := accepted
        have principal := validated_qualified_scheme_is_principal steps (ordered.take solvedCount)
          (ordered.drop solvedCount) .generalize (globalEnvironment globals)
          (.function (parameterTypes parameters) result) scheme solution checked
        rw [List.take_append_drop] at principal
        exact ⟨generated, state, rfl, qualified_scheme_permutation permutation principal⟩
      · simp [found, Except.mapError, except_bind_ok, exists_require_ok, within] at accepted


def generatedProjectionName : Xsh.Foundation.NameId := exampleName 40 (by decide)
def generatedProjectionField : Xsh.Foundation.NameId := exampleName 41 (by decide)
def generatedParameterVariable : Variable := ⟨0, .value⟩
def generatedReturnVariable : Variable := ⟨1, .value⟩
def generatedFieldVariable : Variable := ⟨2, .value⟩
def generatedProjectionParameter : Binding :=
  ⟨generatedProjectionName, ⟨⟨40, by decide⟩⟩, .immutable, ⟨[], .variable generatedParameterVariable, []⟩⟩
def generatedProjectionBody : List Statement :=
  [.returnValue (some (.field (.ident generatedProjectionName) generatedProjectionField))]
def generatedProjectionConstraints : List Constraint :=
  [.equal (.variable generatedFieldVariable) (.variable generatedReturnVariable),
   .field (.variable generatedParameterVariable) generatedProjectionField (.variable generatedFieldVariable)]
def generatedProjectionScheme : Scheme :=
  ⟨[generatedParameterVariable, generatedReturnVariable],
    .function (.argument (.variable generatedParameterVariable) .argumentsEmpty) (.variable generatedReturnVariable),
    [.field (.variable generatedParameterVariable) generatedProjectionField (.variable generatedReturnVariable)]⟩

def generatedProjectionAccepted : Bool :=
  match validateGeneratedFunctionScheme 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedReturnVariable) generatedProjectionBody generatedProjectionConstraints [.bindLeft] 1
      generatedProjectionScheme with
  | .ok _ => true
  | _ => false

def generatedProjectionMonomorphicRejected : Bool :=
  let forged := { generatedProjectionScheme with quantified := [] }
  match validateGeneratedFunctionScheme 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedReturnVariable) generatedProjectionBody generatedProjectionConstraints [.bindLeft] 1 forged with
  | .error .mismatch => true
  | _ => false

theorem generated_projection_scheme_accepts_and_rejects_monomorphic :
    generatedProjectionAccepted = true ∧ generatedProjectionMonomorphicRejected = true := by
  constructor <;> decide +kernel


theorem qualified_call_discharges_original_constraints
    (policy : GeneralizationPolicy) (environment : TypeEnvironment) (ty : MonoType)
    (constraints : List Constraint) (principal : Substitution) (scheme : Scheme)
    (arguments solution : Substitution) (instanceType : MonoType) (requirements : List Constraint)
    (qualified : QualifiedScheme policy environment ty constraints principal scheme)
    (instantiated : Instantiates scheme arguments instanceType requirements)
    (checked : checksConstraints solution requirements = true)
    (sorted : WellSorted (compose solution (schemeSubstitution scheme arguments))) :
    Solves constraints (compose (compose solution (schemeSubstitution scheme arguments)) principal) := by
  have residual : Solves scheme.requirements (compose solution (schemeSubstitution scheme arguments)) := by
    apply (solves_mapped scheme.requirements (schemeSubstitution scheme arguments) solution).mp
    rw [← instantiated.2]
    exact (checks_constraints_iff solution requirements).mp checked
  exact (qualified.1.2.1 _ sorted).mpr residual


theorem qualified_call_preserves_required_field
    (policy : GeneralizationPolicy) (environment : TypeEnvironment) (ty : MonoType)
    (constraints : List Constraint) (principal : Substitution) (scheme : Scheme)
    (arguments solution : Substitution) (instanceType : MonoType) (requirements : List Constraint)
    (record value : MonoType) (name : Xsh.Foundation.NameId)
    (qualified : QualifiedScheme policy environment ty constraints principal scheme)
    (instantiated : Instantiates scheme arguments instanceType requirements)
    (checked : checksConstraints solution requirements = true)
    (sorted : WellSorted (compose solution (schemeSubstitution scheme arguments)))
    (required : Constraint.field record name value ∈ constraints) :
    satisfies (compose (compose solution (schemeSubstitution scheme arguments)) principal)
      (.field record name value) :=
  qualified_call_discharges_original_constraints policy environment ty constraints principal scheme
    arguments solution instanceType requirements qualified instantiated checked sorted _ required

def validateQualifiedFunctionInstance (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (scheme : Scheme) (instantiation : Substitution) (candidate : List StatementNode)
    (binderTypes : BinderTypes) : Except ValidationError Substitution := do
  let principal ← validateGeneratedFunctionScheme fuel names parameters globals result body
    ordered steps solvedCount scheme
  requireCheck (checksConstraints instantiation scheme.requirements)
  let solution := compose instantiation principal
  requireCheck (decide (declaredBinders candidate).Nodup)
  if supportedStatementTails true candidate then pure () else throw .unsupported
  requireCheck (checksPropagationErrors (propagationContext (substitute solution result)) solution
    (blockPropagationErrors candidate))
  validateBlock fuel names (parameters ++ globals) solution binderTypes
    (substitute solution result) body candidate
  pure principal

theorem qualified_function_instance_sound (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (scheme : Scheme) (instantiation : Substitution) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (principal : Substitution)
    (sorted : WellSorted instantiation)
    (accepted : validateQualifiedFunctionInstance fuel names parameters globals result body ordered
      steps solvedCount scheme instantiation candidate binderTypes = .ok principal) :
    ∃ generated state,
      (block fuel names (parameters ++ globals) result body).run
        (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
      QualifiedScheme .generalize (globalEnvironment globals)
        (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
      Solves state.constraints (compose instantiation principal) ∧
      (declaredBinders candidate).Nodup ∧
      PropagationMatches (propagationContext (substitute (compose instantiation principal) result))
        (compose instantiation principal) (blockPropagationErrors candidate) ∧
      BlockHasType names (compose instantiation principal) (parameters ++ globals)
        (substitute (compose instantiation principal) result) body := by
  cases supported : supportedStatementTails true candidate with
  | false =>
    simp [validateQualifiedFunctionInstance, except_bind_ok, except_map_ok,
      exists_require_ok, supported] at accepted
  | true =>
    simp [validateQualifiedFunctionInstance, except_bind_ok, except_map_ok,
      exists_require_ok, supported, checks_constraints_iff,
      checks_propagation_errors_iff] at accepted
    obtain ⟨chosen, found, requirements, unique, propagation, checked, same⟩ := accepted
    subst chosen
    obtain ⟨unit, checked⟩ := checked
    cases unit
    obtain ⟨generated, state, generatedBody, qualified⟩ :=
      generated_function_scheme_is_principal fuel names parameters globals result body ordered
        steps solvedCount scheme principal found
    exact ⟨generated, state, generatedBody, qualified,
      (qualified.1.2.1 instantiation sorted).mpr requirements, unique, propagation,
      accepted_block_has_type fuel names (parameters ++ globals) (compose instantiation principal)
        binderTypes (substitute (compose instantiation principal) result) body candidate checked⟩

def projectionInstance (hasField : Bool) : Substitution := fun id =>
  if id = generatedParameterVariable then
    .record (if hasField then .rowField generatedProjectionField (.primitive .integer) .rowEmpty else .rowEmpty)
  else if id = generatedReturnVariable then .primitive .integer else .variable id

def generatedProjectionCandidate : List StatementNode :=
  [.returned (.field (.read generatedProjectionParameter.binder []) generatedProjectionField)]

def projectionInstanceAccepted (hasField : Bool) : Bool :=
  match validateQualifiedFunctionInstance 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedReturnVariable) generatedProjectionBody generatedProjectionConstraints [.bindLeft] 1
      generatedProjectionScheme (projectionInstance hasField) generatedProjectionCandidate (fun _ => none) with
  | .ok _ => true
  | _ => false

theorem projection_instance_checks_body_and_rejects_missing_field :
    projectionInstanceAccepted true = true ∧ projectionInstanceAccepted false = false := by
  constructor <;> decide +kernel

end Xsh.Typing.Connected
