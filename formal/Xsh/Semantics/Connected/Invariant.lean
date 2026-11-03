import Xsh.Semantics.Connected.Machine
import Xsh.Typing.Connected.Validate

namespace Xsh.Semantics.Connected

open Xsh.Semantics.Environments

def StoreEvolves (before after : Store) : Prop :=
  ∀ cell old, before cell = some old →
    ∃ current, after cell = some current ∧
      current.contents.typeTag = old.contents.typeTag ∧ current.mutability = old.mutability

theorem store_evolves_refl (store : Store) : StoreEvolves store store := by
  intro cell old present
  exact ⟨old, present, rfl, rfl⟩

theorem store_evolves_trans {first middle last : Store}
    (left : StoreEvolves first middle) (right : StoreEvolves middle last) :
    StoreEvolves first last := by
  intro cell old present
  obtain ⟨between, atMiddle, typeLeft, mutableLeft⟩ := left cell old present
  obtain ⟨current, atLast, typeRight, mutableRight⟩ := right cell between atMiddle
  exact ⟨current, atLast, typeRight.trans typeLeft, mutableRight.trans mutableLeft⟩

theorem allocate_preserves_layout (store next : Store) (cell : CellId)
    (mutability : Mutability) (value : Value) (allocated : allocate store cell mutability value = some next) :
    StoreEvolves store next := by
  intro other old present
  by_cases same : other = cell
  · subst other
    simp [allocate, present] at allocated
  · unfold allocate at allocated
    cases occupied : store cell with
    | some entry => simp [occupied] at allocated
    | none =>
      simp only [occupied, Option.isSome_none, Bool.false_eq_true, ↓reduceIte] at allocated
      split at allocated
      · cases allocated
      · rename_i ty found
        cases allocated
        exact ⟨old, by simp [same, present], rfl, rfl⟩


def StoreExtends (before after : Store) : Prop :=
  ∀ cell old, before cell = some old → after cell = some old

theorem allocate_preserves_existing (store next : Store) (cell : CellId)
    (mutability : Mutability) (value : Value) (allocated : allocate store cell mutability value = some next) :
    StoreExtends store next := by
  intro other old present
  by_cases same : other = cell
  · subst other
    simp [allocate, present] at allocated
  · unfold allocate at allocated
    cases occupied : store cell with
    | some entry => simp [occupied] at allocated
    | none =>
      simp only [occupied, Option.isSome_none, Bool.false_eq_true, ↓reduceIte] at allocated
      split at allocated
      · cases allocated
      · cases allocated
        simp [same, present]

theorem store_extends_trans {first middle last : Store}
    (left : StoreExtends first middle) (right : StoreExtends middle last) : StoreExtends first last := by
  intro cell old present
  exact right cell old (left cell old present)

theorem write_preserves_layout (store next : Store) (cell : CellId) (value : Value)
    (written : write store cell value = .accepted next) : StoreEvolves store next := by
  obtain ⟨current, present, ⟨replaced, atCell, sameType, _, sameMutability⟩, unchanged⟩ :=
    write_accepted_replaces_only_its_cell (fun _ _ => inferInstance) store cell value next written
  intro other old oldAt
  by_cases same : other = cell
  · subst other
    rw [present] at oldAt
    cases oldAt
    exact ⟨replaced, atCell, sameType, sameMutability⟩
  · exact ⟨old, by rw [unchanged other same]; exact oldAt, rfl, rfl⟩

theorem parameters_preserve_layout (activation : Nat) (binders : List Xsh.Foundation.BinderId)
    (values : List Value) (types : RuntimeType) (store next : Store) (cells : ActiveCells)
    (bound : bindParameters activation binders values types store = .ok (cells, next)) :
    StoreEvolves store next := by
  induction binders generalizing values types store next cells with
  | nil =>
    cases values with
    | nil =>
      cases types with
      | argumentsEmpty =>
        cases bound
        exact store_evolves_refl _
      | _ => cases bound
    | cons _ _ => cases types <;> cases bound
  | cons binder rest ih =>
    cases values with
    | nil => cases types <;> cases bound
    | cons value values =>
      cases types with
      | argument ty types =>
        by_cases typed : valueType value = some ty
        · cases allocated : allocate store ⟨activation, binder⟩ .immutable value with
          | none => simp [bindParameters, typed, allocated] at bound
          | some intermediate =>
            simp [bindParameters, typed, allocated, Xsh.Typing.Connected.except_bind_ok] at bound
            obtain ⟨priorCells, finalStore, recursive, finished⟩ := bound
            split at finished
            · cases finished
            · cases finished
              exact store_evolves_trans (allocate_preserves_layout store intermediate _ _ _ allocated)
                (ih values types intermediate _ priorCells recursive)
        · simp only [bindParameters, typed, ↓reduceIte] at bound
          cases bound
      | _ => cases bound



theorem parameters_preserve_existing (activation : Nat) (binders : List Xsh.Foundation.BinderId)
    (values : List Value) (types : RuntimeType) (store next : Store) (cells : ActiveCells)
    (bound : bindParameters activation binders values types store = .ok (cells, next)) :
    StoreExtends store next := by
  induction binders generalizing values types store next cells with
  | nil =>
    cases values with
    | nil =>
      cases types with
      | argumentsEmpty => cases bound; intro _ _ present; exact present
      | _ => cases bound
    | cons _ _ => cases types <;> cases bound
  | cons binder rest ih =>
    cases values with
    | nil => cases types <;> cases bound
    | cons value values =>
      cases types with
      | argument ty types =>
        by_cases typed : valueType value = some ty
        · cases allocated : allocate store ⟨activation, binder⟩ .immutable value with
          | none => simp [bindParameters, typed, allocated] at bound
          | some intermediate =>
            simp [bindParameters, typed, allocated, Xsh.Typing.Connected.except_bind_ok] at bound
            obtain ⟨priorCells, finalStore, recursive, finished⟩ := bound
            split at finished
            · cases finished
            · cases finished
              exact store_extends_trans (allocate_preserves_existing store intermediate _ _ _ allocated)
                (ih values types intermediate _ priorCells recursive)
        · simp only [bindParameters, typed, ↓reduceIte] at bound
          cases bound
      | _ => cases bound

def ImmutableStable (before after : Store) : Prop :=
  ∀ cell old, before cell = some old → old.mutability = .immutable → after cell = some old

theorem store_extension_preserves_immutable {before after : Store}
    (extension : StoreExtends before after) : ImmutableStable before after := by
  intro cell old present _
  exact extension cell old present

theorem write_preserves_immutable (store next : Store) (cell : CellId) (value : Value)
    (written : write store cell value = .accepted next) : ImmutableStable store next := by
  obtain ⟨current, present, ⟨replaced, _, _, _, _⟩, unchanged⟩ :=
    write_accepted_replaces_only_its_cell (fun _ _ => inferInstance) store cell value next written
  obtain ⟨current2, present2, mutable, _⟩ := assignment_commit_requires_mutable_matching_cell store next cell value written
  have sameCurrent : current2 = current := Option.some.inj (present2.symm.trans present)
  subst current2
  intro other old oldAt immutable
  by_cases same : other = cell
  · subst other
    rw [present] at oldAt
    cases oldAt
    rw [immutable] at mutable
    cases mutable
  · rw [unchanged other same]
    exact oldAt

theorem step_preserves_store_layout {before after : Machine} {event : Label}
    (step : Step before event after) : StoreEvolves before.store after.store := by
  cases step <;> try exact store_evolves_refl _
  case bindingCommit state binder kind value rest next allocated =>
    exact allocate_preserves_layout state.store next _ _ value allocated
  case assignmentCommit state binder value rest cell next active written =>
    exact write_preserves_layout state.store next cell value written
  case assignmentRejected state binder value rest cell next active rejected =>
    have same := rejected_assignment_does_not_change_store state.store next cell value rejected
    subst next
    exact store_evolves_refl _
  case callFunction state declaration arguments parameterTypes resultType captured typeArguments normalized function cells store found signature bound =>
    exact parameters_preserve_layout state.nextActivation function.parameters arguments parameterTypes state.store store cells bound



theorem step_preserves_immutable_cells {before after : Machine} {event : Label}
    (step : Step before event after) : ImmutableStable before.store after.store := by
  cases step <;> try (intro _ _ present _; exact present)
  case bindingCommit state binder kind value rest next allocated =>
    exact store_extension_preserves_immutable (allocate_preserves_existing state.store next _ _ value allocated)
  case assignmentCommit state binder value rest cell next active written =>
    exact write_preserves_immutable state.store next cell value written
  case assignmentRejected state binder value rest cell next active rejected =>
    have same := rejected_assignment_does_not_change_store state.store next cell value rejected
    subst next
    intro _ _ present _
    exact present
  case callFunction state declaration arguments parameterTypes resultType captured typeArguments normalized function cells store found signature bound =>
    exact store_extension_preserves_immutable
      (parameters_preserve_existing state.nextActivation function.parameters arguments parameterTypes state.store store cells bound)

theorem execution_preserves_immutable_cells {before after : Machine} {events : List Label}
    (execution : Execution before events after) : ImmutableStable before.store after.store := by
  induction execution with
  | done state => intro _ _ present _; exact present
  | next step _ ih =>
    intro cell old present immutable
    exact ih cell old (step_preserves_immutable_cells step cell old present immutable) immutable

theorem execution_preserves_store_layout {before after : Machine} {events : List Label}
    (execution : Execution before events after) : StoreEvolves before.store after.store := by
  induction execution with
  | done state => exact store_evolves_refl state.store
  | next step _ ih => exact store_evolves_trans (step_preserves_store_layout step) ih

def CellsLive (cells : ActiveCells) (store : Store) : Prop :=
  ∀ binder cell, cells binder = some cell → ∃ current, store cell = some current

theorem store_evolution_preserves_live_cells {before after : Store} {cells : ActiveCells}
    (evolves : StoreEvolves before after) (live : CellsLive cells before) : CellsLive cells after := by
  intro binder cell active
  obtain ⟨old, present⟩ := live binder cell active
  obtain ⟨current, stillPresent, _, _⟩ := evolves cell old present
  exact ⟨current, stillPresent⟩

theorem execution_preserves_existing_live_cells {before after : Machine} {events : List Label}
    (execution : Execution before events after) (live : CellsLive before.cells before.store) :
    CellsLive before.cells after.store :=
  store_evolution_preserves_live_cells (execution_preserves_store_layout execution) live


theorem guard_plain_success_has_matching_type {guard : Guard} {value : Value}
    (accepted : guardPlain guard value = some true) :
    ∃ actual, valueType value = some actual ∧ typeFits (guardBase guard) actual = true := by
  cases guard with
  | integerRange minimum maximum =>
    cases value with
    | scalar scalar =>
      cases scalar <;> simp [guardPlain] at accepted
      exact ⟨.integer, rfl, rfl⟩
    | _ => simp [guardPlain] at accepted
  | recordShape row =>
    cases value with
    | record fields =>
      cases actual : valueType (.record fields) with
      | none => simp [guardPlain, actual] at accepted
      | some ty =>
        simp [guardPlain, actual] at accepted
        exact ⟨ty, rfl, accepted⟩
    | _ => simp [guardPlain] at accepted

theorem guard_success_has_matching_type {guard : Guard} {value : Value}
    (accepted : guardDecision guard value = some true) :
    ∃ actual, valueType (eraseNominal value) = some actual ∧ typeFits (guardBase guard) actual = true :=
  guard_plain_success_has_matching_type accepted

theorem require_success_has_nominal_result_type {guard : Guard} {value : Value}
    (identity : Xsh.Validation.Nominal.NominalId) (accepted : guardDecision guard value = some true) :
    valueType (.ok (.nominal identity (guardBase guard) (eraseNominal value)) .error) =
      some (.result (.nominal identity (guardBase guard)) .error) := by
  obtain ⟨actual, typed, fits⟩ := guard_success_has_matching_type accepted
  simp [valueType, typed, fits]

theorem require_failure_has_nominal_result_type (identity : Xsh.Validation.Nominal.NominalId)
    (guard : Guard) :
    valueType (.error .validationError (.nominal identity (guardBase guard))) =
      some (.result (.nominal identity (guardBase guard)) .error) := by
  rfl


abbrev CellTypes := Xsh.Foundation.BinderId → Option RuntimeType

def CellsTyped (cells : ActiveCells) (types : CellTypes) (store : Store) : Prop :=
  ∀ binder ty, types binder = some ty →
    ∃ cell current, cells binder = some cell ∧ store cell = some current ∧ current.contents.typeTag = ty

theorem store_evolution_preserves_typed_cells {before after : Store} {cells : ActiveCells}
    {types : CellTypes} (evolves : StoreEvolves before after) (typed : CellsTyped cells types before) :
    CellsTyped cells types after := by
  intro binder ty declared
  obtain ⟨cell, old, active, present, same⟩ := typed binder ty declared
  obtain ⟨current, stillPresent, sameType, _⟩ := evolves cell old present
  exact ⟨cell, current, active, stillPresent, sameType.trans same⟩

theorem execution_preserves_saved_cell_types {before after : Machine} {events : List Label}
    {cells : ActiveCells} {types : CellTypes} (execution : Execution before events after)
    (typed : CellsTyped cells types before.store) : CellsTyped cells types after.store :=
  store_evolution_preserves_typed_cells (execution_preserves_store_layout execution) typed


def SchemaMatches (scope : Xsh.Typing.Connected.Scope) (solution : Xsh.Typing.Connected.Substitution)
    (schemas : SchemaTable) : Prop :=
  ∀ name binding identity row,
    scope.find? (fun item => decide (item.name = name)) = some binding →
    binding.kind = .schema → binding.scheme.body = .nominal identity (.record row) →
    ∃ guard, schemas binding.binder = some (identity, guard) ∧
      runtimeMonotype (guardBase guard) = some (.record (Xsh.Typing.Connected.substitute solution row))

theorem successful_guard_projects_to_checked_type
    (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard) (value : Value)
    (solution : Xsh.Typing.Connected.Substitution) (row : Xsh.Typing.Connected.MonoType)
    (matching : runtimeMonotype (guardBase guard) = some (.record (Xsh.Typing.Connected.substitute solution row)))
    (accepted : guardDecision guard value = some true) :
    ∃ runtimeType, valueType (.ok (.nominal identity (guardBase guard) (eraseNominal value)) .error) = some runtimeType ∧
      runtimeMonotype runtimeType = some (.result (.nominal identity (.record (Xsh.Typing.Connected.substitute solution row))) (.primitive .error)) := by
  refine ⟨.result (.nominal identity (guardBase guard)) .error,
    require_success_has_nominal_result_type identity accepted, ?_⟩
  simp [runtimeMonotype, matching]

theorem failed_guard_projects_to_checked_type
    (identity : Xsh.Validation.Nominal.NominalId) (guard : Guard)
    (solution : Xsh.Typing.Connected.Substitution) (row : Xsh.Typing.Connected.MonoType)
    (matching : runtimeMonotype (guardBase guard) = some (.record (Xsh.Typing.Connected.substitute solution row))) :
    ∃ runtimeType, valueType (.error .validationError (.nominal identity (guardBase guard))) = some runtimeType ∧
      runtimeMonotype runtimeType = some (.result (.nominal identity (.record (Xsh.Typing.Connected.substitute solution row))) (.primitive .error)) := by
  refine ⟨.result (.nominal identity (guardBase guard)) .error,
    require_failure_has_nominal_result_type identity guard, ?_⟩
  simp [runtimeMonotype, matching]


theorem validated_require_guard_matches (fuel : Nat) (scope : Xsh.Typing.Connected.Scope)
    (solution : Xsh.Typing.Connected.Substitution) (schemas : SchemaTable)
    (value : Xsh.Syntax.Expr) (name : Xsh.Foundation.NameId) (binder : Xsh.Foundation.BinderId)
    (identity : Xsh.Validation.Nominal.NominalId) (candidate : Xsh.Typing.Connected.Node)
    (ty : Xsh.Typing.Connected.MonoType)
    (accepted : Xsh.Typing.Connected.validateExpression (fuel + 1) scope solution
      (.require value (some (.named name))) (.requireValue binder identity candidate) = .ok ty)
    (matching : SchemaMatches scope solution schemas) :
    ∃ row guard, schemas binder = some (identity, guard) ∧
      runtimeMonotype (guardBase guard) = some (.record (Xsh.Typing.Connected.substitute solution row)) ∧
      ty = .result (.nominal identity (.record (Xsh.Typing.Connected.substitute solution row))) (.primitive .error) := by
  cases found : scope.find? (fun item => decide (item.name = name)) with
  | none => simp [Xsh.Typing.Connected.validateExpression, found] at accepted
  | some binding =>
    cases body : binding.scheme.body with
    | nominal expected base =>
      cases base with
      | record row =>
        simp [Xsh.Typing.Connected.validateExpression, found, body,
          Xsh.Typing.Connected.except_bind_ok, Xsh.Typing.Connected.except_map_ok,
          Xsh.Typing.Connected.exists_require_ok, Bool.and_eq_true,
          Xsh.Typing.Connected.checks_well_formed_iff] at accepted
        obtain ⟨⟨binderEq, kind⟩, _, ⟨identityEq, _⟩, _, rfl⟩ := accepted
        obtain ⟨guard, guardFound, baseMatches⟩ := matching name binding expected row found kind body
        subst binder
        subst identity
        exact ⟨row, guard, guardFound, baseMatches, rfl⟩
      | _ => simp [Xsh.Typing.Connected.validateExpression, found, body,
          Xsh.Typing.Connected.except_bind_ok] at accepted
    | _ => simp [Xsh.Typing.Connected.validateExpression, found, body,
        Xsh.Typing.Connected.except_bind_ok] at accepted


theorem validated_require_decision_preserves_type (state : Machine) (fuel : Nat)
    (scope : Xsh.Typing.Connected.Scope) (solution : Xsh.Typing.Connected.Substitution)
    (source : Xsh.Syntax.Expr) (name : Xsh.Foundation.NameId) (binder : Xsh.Foundation.BinderId)
    (identity : Xsh.Validation.Nominal.NominalId) (candidate : Xsh.Typing.Connected.Node)
    (ty : Xsh.Typing.Connected.MonoType) (guard : Guard) (value : Value) (decision : Bool)
    (accepted : Xsh.Typing.Connected.validateExpression (fuel + 1) scope solution
      (.require source (some (.named name))) (.requireValue binder identity candidate) = .ok ty)
    (matching : SchemaMatches scope solution state.schemas)
    (selected : state.schemas binder = some (identity, guard))
    (checked : guardDecision guard value = some decision) :
    ∃ output runtimeType,
      Step { state with control := .value value, stack := .requireGuard identity guard :: state.stack }
        .internal { state with control := .value output } ∧
      valueType output = some runtimeType ∧ runtimeMonotype runtimeType = some ty := by
  obtain ⟨row, expectedGuard, expected, sameBase, rfl⟩ :=
    validated_require_guard_matches fuel scope solution state.schemas source name binder identity candidate ty accepted matching
  have sameGuard : expectedGuard = guard := by
    rw [selected] at expected
    exact (Prod.mk.inj (Option.some.inj expected)).2.symm
  subst expectedGuard
  cases decision with
  | false =>
    obtain ⟨runtimeType, typed, projected⟩ := failed_guard_projects_to_checked_type identity guard solution row sameBase
    exact ⟨.error .validationError (.nominal identity (guardBase guard)), runtimeType,
      Step.requireFailure state identity guard value checked, typed, projected⟩
  | true =>
    obtain ⟨runtimeType, typed, projected⟩ := successful_guard_projects_to_checked_type identity guard value solution row sameBase checked
    exact ⟨.ok (.nominal identity (guardBase guard) (eraseNominal value)) .error, runtimeType,
      Step.requireSuccess state identity guard value checked, typed, projected⟩


theorem require_rechecks_nominal_input (state : Machine)
    (target source : Xsh.Validation.Nominal.NominalId) (base : RuntimeType)
    (guard : Guard) (value : Value) (accepted : guardDecision guard value = some true) :
    Step { state with control := .value (.nominal source base value), stack := .requireGuard target guard :: state.stack } .internal
      { state with control := .value (.ok (.nominal target (guardBase guard) (eraseNominal value)) .error) } := by
  exact Step.requireSuccess state target guard (.nominal source base value) accepted


theorem parameter_binding_preserves_unshadowed_global (state : Machine) (locals : ActiveCells)
    (store : Store) (binders : List Xsh.Foundation.BinderId) (arguments : List Value)
    (types : RuntimeType) (globalBinder : Xsh.Foundation.BinderId) (globalValue : Value)
    (bound : bindParameters state.nextActivation binders arguments types state.store = .ok (locals, store))
    (unshadowed : locals globalBinder = none)
    (globalRead : Connected.read state.store state.globals globalBinder = some globalValue) :
    Connected.read store (withGlobals locals state.globals) globalBinder = some globalValue := by
  have unchanged := parameters_preserve_existing state.nextActivation binders arguments types state.store store locals bound
  unfold Connected.read at globalRead ⊢
  cases found : state.globals globalBinder with
  | none => simp [found] at globalRead
  | some cell =>
    cases present : state.store cell with
    | none => simp [found, present] at globalRead
    | some current =>
      have retained := unchanged cell current present
      simp [withGlobals, unshadowed, found, retained] at ⊢
      simpa [found, present] using globalRead


theorem checked_function_read_instantiates (fuel : Nat) (scope : Xsh.Typing.Connected.Scope)
    (solution : Xsh.Typing.Connected.Substitution) (name : Xsh.Foundation.NameId)
    (binder : Xsh.Foundation.BinderId) (arguments : List (Xsh.Typing.Connected.Variable × Xsh.Typing.Connected.MonoType))
    (binding : Xsh.Typing.Connected.Binding) (parameters result : Xsh.Typing.Connected.MonoType)
    (runtimeParameters runtimeResult : RuntimeType)
    (found : scope.find? (fun item => decide (item.name = name)) = some binding)
    (accepted : Xsh.Typing.Connected.validateExpression (fuel + 1) scope solution (.ident name)
      (.read binder arguments) = .ok (.function parameters result))
    (parametersClosed : monoRuntime parameters = some runtimeParameters)
    (resultClosed : monoRuntime result = some runtimeResult) :
    ∃ normalized, instantiateFunction solution binding.scheme arguments = some (runtimeParameters, runtimeResult, normalized) := by
  simp [Xsh.Typing.Connected.validateExpression, found, Xsh.Typing.Connected.except_bind_ok,
    Xsh.Typing.Connected.except_map_ok, Xsh.Typing.Connected.exists_require_ok,
    Bool.and_eq_true] at accepted
  obtain ⟨_, _, keys, sorts, constraints, _, same⟩ := accepted
  refine ⟨arguments.map fun (id, ty) => (id, Xsh.Typing.Connected.substitute solution ty), ?_⟩
  simp [instantiateFunction, keys, sorts, constraints, same, parametersClosed, resultClosed]

end Xsh.Semantics.Connected
