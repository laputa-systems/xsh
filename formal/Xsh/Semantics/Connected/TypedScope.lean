import Xsh.Semantics.Connected.Invariant
import Xsh.Typing.Connected.NodeJudgment

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected
open Xsh.Semantics.Environments

def ScopeSchemesAgree (scope : Scope) (binder : BinderId) (scheme : Scheme) : Prop :=
  ∀ name binding, scope.find? (fun item => decide (item.name = name)) = some binding →
    binding.binder = binder → binding.scheme = scheme


theorem scope_binding_unique (scope : Scope) (first second : Binding)
    (unique : (scope.map Binding.binder).Nodup)
    (firstIn : first ∈ scope) (secondIn : second ∈ scope)
    (same : first.binder = second.binder) : first = second := by
  induction scope with
  | nil => cases firstIn
  | cons head rest ih =>
    simp only [List.map_cons, List.nodup_cons] at unique
    simp only [List.mem_cons] at firstIn secondIn
    rcases firstIn with rfl | firstIn
    · rcases secondIn with rfl | secondIn
      · rfl
      · exfalso
        apply unique.1
        exact List.mem_map.mpr ⟨second, secondIn, same.symm⟩
    · rcases secondIn with rfl | secondIn
      · exfalso
        apply unique.1
        exact List.mem_map.mpr ⟨first, firstIn, same⟩
      · exact ih unique.2 firstIn secondIn

theorem scope_schemes_agree_of_unique (scope : Scope) (binding : Binding)
    (unique : (scope.map Binding.binder).Nodup) (present : binding ∈ scope) :
    ScopeSchemesAgree scope binding.binder binding.scheme := by
  intro name other found same
  have otherIn := List.mem_of_find?_eq_some found
  have equal := scope_binding_unique scope other binding unique otherIn present same
  cases equal
  rfl

theorem typed_function_read_instantiates (scope : Scope) (solution : Substitution)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (scheme : Scheme)
    (parameters result ty : MonoType) (runtimeParameters runtimeResult : RuntimeType)
    (typed : NodeHasType scope solution (.read binder arguments) ty)
    (functionType : ty = .function parameters result)
    (sameScheme : ScopeSchemesAgree scope binder scheme)
    (parametersClosed : monoRuntime parameters = some runtimeParameters)
    (resultClosed : monoRuntime result = some runtimeResult) :
    ∃ normalized, instantiateFunction solution scheme arguments = some (runtimeParameters, runtimeResult, normalized) := by
  cases typed with
  | read found notSchema keys sorts instantiated solved wellFormed sort =>
    rename_i name binding raw requirements
    have equalScheme := sameScheme name binding found rfl
    subst scheme
    obtain ⟨rfl, rfl⟩ := instantiated
    have constraints : checksConstraints solution
        (binding.scheme.requirements.map (mapConstraint (schemeSubstitution binding.scheme (witnessSubstitution arguments)))) = true :=
      (checks_constraints_iff _ _).mpr solved
    refine ⟨arguments.map fun (id, ty) => (id, substitute solution ty), ?_⟩
    simp [instantiateFunction, keys, sorts, constraints, functionType, parametersClosed, resultClosed]

theorem typed_scheme_function_read_steps (state : Machine) (scope : Scope)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (scheme : Scheme)
    (declaration : Xsh.Syntax.Foundation.DeclarationId) (parameters result ty : MonoType)
    (runtimeParameters runtimeResult : RuntimeType)
    (typed : NodeHasType scope state.solution (.read binder arguments) ty)
    (functionType : ty = .function parameters result)
    (sameScheme : ScopeSchemesAgree scope binder scheme)
    (found : Connected.read state.store state.cells binder = some (.schemeFunction declaration scheme))
    (parametersClosed : monoRuntime parameters = some runtimeParameters)
    (resultClosed : monoRuntime result = some runtimeResult) :
    ∃ value, Step { state with control := .expression (.read binder arguments) } .internal
        { state with control := .value value } ∧
      valueType value = some (.function runtimeParameters runtimeResult) := by
  obtain ⟨normalized, instantiated⟩ := typed_function_read_instantiates scope state.solution binder arguments scheme
    parameters result ty runtimeParameters runtimeResult typed functionType sameScheme parametersClosed resultClosed
  exact ⟨.instantiatedFunction declaration runtimeParameters runtimeResult state.solution arguments,
    Step.readFunction state binder arguments declaration scheme runtimeParameters runtimeResult normalized found instantiated, rfl⟩

def OrdinaryBindingValueAgrees (solution : Substitution) (binding : Binding) (value : Value) : Prop :=
  ordinaryValue value = true ∧
    ∀ arguments raw requirements runtimeType,
      Instantiates binding.scheme (witnessSubstitution arguments) raw requirements →
      Solves requirements solution → monoRuntime (substitute solution raw) = some runtimeType →
      valueType value = some runtimeType

def ScopeValuesAgree (scope : Scope) (solution : Substitution) (cells : ActiveCells) (store : Store) : Prop :=
  ∀ name binding, scope.find? (fun item => decide (item.name = name)) = some binding →
    binding.kind ≠ .schema →
    ∃ value, Connected.read store cells binding.binder = some value ∧
      ((∃ declaration parameters result, value = .schemeFunction declaration binding.scheme ∧
          binding.scheme.body = .function parameters result) ∨
        OrdinaryBindingValueAgrees solution binding value)

theorem typed_function_read_progress (state : Machine) (scope : Scope)
    (binder : BinderId) (arguments : List (Variable × MonoType)) (parameters result ty : MonoType)
    (runtimeParameters runtimeResult : RuntimeType)
    (typed : NodeHasType scope state.solution (.read binder arguments) ty)
    (functionType : ty = .function parameters result)
    (unique : (scope.map Binding.binder).Nodup)
    (values : ScopeValuesAgree scope state.solution state.cells state.store)
    (parametersClosed : monoRuntime parameters = some runtimeParameters)
    (resultClosed : monoRuntime result = some runtimeResult) :
    ∃ value, Step { state with control := .expression (.read binder arguments) } .internal
        { state with control := .value value } ∧
      valueType value = some (.function runtimeParameters runtimeResult) := by
  cases typed with
  | read found notSchema keys sorts instantiated solved wellFormed sort =>
    rename_i name binding raw requirements
    obtain ⟨value, readValue, agrees⟩ := values name binding found notSchema
    rcases agrees with ⟨declaration, _, _, rfl, _⟩ | ⟨ordinary, typedValue⟩
    · have present := List.mem_of_find?_eq_some found
      have sameScheme := scope_schemes_agree_of_unique scope binding unique present
      exact typed_scheme_function_read_steps state scope binding.binder arguments binding.scheme declaration
        parameters result (substitute state.solution raw) runtimeParameters runtimeResult
        (.read found notSchema keys sorts instantiated solved wellFormed sort) functionType sameScheme
        readValue parametersClosed resultClosed
    · have closed : monoRuntime (substitute state.solution raw) = some (.function runtimeParameters runtimeResult) := by
        rw [functionType]
        simp [monoRuntime, parametersClosed, resultClosed]
      exact ⟨value, Step.read state binding.binder arguments value readValue ordinary,
        typedValue arguments raw requirements (.function runtimeParameters runtimeResult) instantiated solved closed⟩

theorem monomorphic_value_agrees (solution : Substitution) (name : NameId) (binder : BinderId)
    (kind : BindingKind) (raw : MonoType) (value : Value) (runtimeType : RuntimeType)
    (ordinary : ordinaryValue value = true)
    (closed : monoRuntime (substitute solution raw) = some runtimeType)
    (typed : valueType value = some runtimeType) :
    OrdinaryBindingValueAgrees solution ⟨name, binder, kind, ⟨[], raw, []⟩⟩ value := by
  refine ⟨ordinary, ?_⟩
  intro arguments actual requirements actualRuntime instantiated solved converted
  obtain ⟨same, _⟩ := instantiated
  have identityScheme : schemeSubstitution ⟨[], raw, []⟩ (witnessSubstitution arguments) = identity := by
    funext id
    simp [schemeSubstitution, identity]
  rw [identityScheme, substitute_identity] at same
  subst actual
  rw [closed] at converted
  cases converted
  exact typed

theorem binding_commit_preserves_scope_values (state : Machine) (scope : Scope)
    (name : NameId) (binder : BinderId) (kind : BindingKind) (raw : MonoType)
    (value : Value) (runtimeType : RuntimeType) (rest : List StatementNode)
    (values : ScopeValuesAgree scope state.solution state.cells state.store)
    (binderFresh : ∀ binding ∈ scope, binding.binder ≠ binder)
    (cellFresh : state.store ⟨state.activation, binder⟩ = none)
    (ordinary : ordinaryValue value = true)
    (closed : monoRuntime (substitute state.solution raw) = some runtimeType)
    (typed : valueType value = some runtimeType) :
    ∃ next, allocate state.store ⟨state.activation, binder⟩
        (if kind = .mutable then .mutable else .immutable) value = some next ∧
      Step { state with control := .value value, stack := .bind binder kind rest :: state.stack } .internal
        { state with control := .statements rest, store := next, cells := bindActive state.cells binder ⟨state.activation, binder⟩ } ∧
      ScopeValuesAgree (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: scope) state.solution
        (bindActive state.cells binder ⟨state.activation, binder⟩) next := by
  obtain ⟨next, current, allocated, present, mutable, cellType, cellValue⟩ :=
    allocate_fresh_typed_cell state.store ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value runtimeType cellFresh typed
  refine ⟨next, allocated, Step.bindingCommit state binder kind value rest next allocated, ?_⟩
  intro sought found foundAt notSchema
  by_cases same : name = sought
  · subst sought
    simp at foundAt
    cases foundAt
    refine ⟨value, ?_, Or.inr (monomorphic_value_agrees state.solution name binder kind raw value runtimeType ordinary closed typed)⟩
    simp [Connected.read, bindActive, present, cellValue]
  · have original : scope.find? (fun item => decide (item.name = sought)) = some found := by
      simpa [same] using foundAt
    obtain ⟨oldValue, oldRead, oldAgrees⟩ := values sought found original notSchema
    have different : found.binder ≠ binder := binderFresh found (List.mem_of_find?_eq_some original)
    have retained : Connected.read next (bindActive state.cells binder ⟨state.activation, binder⟩) found.binder = some oldValue := by
      unfold Connected.read at oldRead ⊢
      simp only [bindActive, different, ↓reduceIte]
      cases active : state.cells found.binder with
      | none => simp [active] at oldRead
      | some cell =>
        cases entry : state.store cell with
        | none => simp [active, entry] at oldRead
        | some old =>
          have unchanged := allocate_preserves_existing state.store next ⟨state.activation, binder⟩
            (if kind = .mutable then .mutable else .immutable) value allocated cell old entry
          simpa [active, unchanged, entry] using oldRead
    exact ⟨oldValue, retained, oldAgrees⟩

def DistinctActiveCells (cells : ActiveCells) : Prop :=
  ∀ first second cell, cells first = some cell → cells second = some cell → first = second

theorem assignment_commit_preserves_scope_values (state : Machine) (scope : Scope)
    (focusName : NameId) (focus : Binding) (raw : MonoType) (value : Value) (runtimeType : RuntimeType)
    (rest : List StatementNode) (cell : CellId) (next : Store)
    (values : ScopeValuesAgree scope state.solution state.cells state.store)
    (unique : (scope.map Binding.binder).Nodup) (distinct : DistinctActiveCells state.cells)
    (found : scope.find? (fun item => decide (item.name = focusName)) = some focus)
    (monomorphic : focus.scheme = ⟨[], raw, []⟩)
    (active : state.cells focus.binder = some cell)
    (written : write state.store cell value = .accepted next)
    (ordinary : ordinaryValue value = true)
    (closed : monoRuntime (substitute state.solution raw) = some runtimeType)
    (typed : valueType value = some runtimeType) :
    Step { state with control := .value value, stack := .assign focus.binder rest :: state.stack } .internal
        { state with control := .statements rest, store := next } ∧
      ScopeValuesAgree scope state.solution state.cells next := by
  refine ⟨Step.assignmentCommit state focus.binder value rest cell next active written, ?_⟩
  obtain ⟨current, present, ⟨replaced, replacedAt, sameType, newValue, sameMutability⟩, unchanged⟩ :=
    write_accepted_replaces_only_its_cell (fun _ _ => inferInstance) state.store cell value next written
  intro name binding lookup notSchema
  by_cases same : binding.binder = focus.binder
  · have equal := scope_binding_unique scope binding focus unique
      (List.mem_of_find?_eq_some lookup) (List.mem_of_find?_eq_some found) same
    subst binding
    refine ⟨value, ?_, Or.inr ?_⟩
    · simp [Connected.read, active, replacedAt, newValue]
    · cases focus with
      | mk bindingName bindingBinder bindingKind bindingScheme =>
        simp only at monomorphic
        subst bindingScheme
        exact monomorphic_value_agrees state.solution bindingName bindingBinder bindingKind raw value runtimeType ordinary closed typed
  · obtain ⟨oldValue, oldRead, oldAgrees⟩ := values name binding lookup notSchema
    have retained : Connected.read next state.cells binding.binder = some oldValue := by
      unfold Connected.read at oldRead ⊢
      cases otherActive : state.cells binding.binder with
      | none => simp [otherActive] at oldRead
      | some otherCell =>
        have different : otherCell ≠ cell := by
          intro equal
          subst otherCell
          exact same (distinct binding.binder focus.binder cell otherActive active)
        simpa [otherActive, unchanged otherCell different] using oldRead
    exact ⟨oldValue, retained, oldAgrees⟩

def MutableCellsAgree (scope : Scope) (solution : Substitution) (cells : ActiveCells) (store : Store) : Prop :=
  ∀ name binding, scope.find? (fun item => decide (item.name = name)) = some binding →
    binding.kind = .mutable →
    binding.scheme.quantified = [] ∧ binding.scheme.requirements = [] ∧
      ∃ cell current runtimeType, cells binding.binder = some cell ∧ store cell = some current ∧
        current.mutability = .mutable ∧ current.contents.typeTag = runtimeType ∧
        monoRuntime (substitute solution binding.scheme.body) = some runtimeType

theorem mutable_cells_survive_store_evolution (scope : Scope) (solution : Substitution) (cells : ActiveCells)
    (before after : Store) (evolves : StoreEvolves before after)
    (agrees : MutableCellsAgree scope solution cells before) : MutableCellsAgree scope solution cells after := by
  intro name binding found mutable
  obtain ⟨quantified, requirements, cell, old, runtimeType, active, present, oldMutable, oldType, closed⟩ :=
    agrees name binding found mutable
  obtain ⟨current, retained, sameType, sameMutability⟩ := evolves cell old present
  exact ⟨quantified, requirements, cell, current, runtimeType, active, retained,
    sameMutability.trans oldMutable, sameType.trans oldType, closed⟩

theorem typed_assignment_commit (state : Machine) (scope : Scope) (name : NameId) (binding : Binding)
    (value : Value) (runtimeType : RuntimeType) (rest : List StatementNode)
    (found : scope.find? (fun item => decide (item.name = name)) = some binding)
    (mutable : binding.kind = .mutable)
    (cells : MutableCellsAgree scope state.solution state.cells state.store)
    (closed : monoRuntime (substitute state.solution binding.scheme.body) = some runtimeType)
    (typed : valueType value = some runtimeType) :
    ∃ next, Step { state with control := .value value, stack := .assign binding.binder rest :: state.stack } .internal
      { state with control := .statements rest, store := next } ∧ StoreEvolves state.store next := by
  obtain ⟨_, _, cell, current, actualType, active, present, writable, sameType, sameClosed⟩ :=
    cells name binding found mutable
  rw [closed] at sameClosed
  cases sameClosed
  have member : HasType current.contents.typeTag value := by
    unfold HasType
    rw [sameType]
    exact typed
  obtain ⟨next, written⟩ := (write_accepted_iff (fun _ _ => inferInstance) state.store cell value).mpr
    ⟨current, present, writable, member⟩
  exact ⟨next, Step.assignmentCommit state binding.binder value rest cell next active written,
    write_preserves_layout state.store next cell value written⟩

theorem typed_assignment_preserves_scope (state : Machine) (scope : Scope) (name : NameId) (binding : Binding)
    (value : Value) (runtimeType : RuntimeType) (rest : List StatementNode)
    (found : scope.find? (fun item => decide (item.name = name)) = some binding)
    (mutable : binding.kind = .mutable)
    (values : ScopeValuesAgree scope state.solution state.cells state.store)
    (cells : MutableCellsAgree scope state.solution state.cells state.store)
    (unique : (scope.map Binding.binder).Nodup) (distinct : DistinctActiveCells state.cells)
    (closed : monoRuntime (substitute state.solution binding.scheme.body) = some runtimeType)
    (ordinary : ordinaryValue value = true) (typed : valueType value = some runtimeType) :
    ∃ next, Step { state with control := .value value, stack := .assign binding.binder rest :: state.stack } .internal
        { state with control := .statements rest, store := next } ∧
      ScopeValuesAgree scope state.solution state.cells next ∧
      MutableCellsAgree scope state.solution state.cells next := by
  obtain ⟨quantified, requirements, cell, current, actualType, active, present, writable, sameType, sameClosed⟩ :=
    cells name binding found mutable
  have monomorphic : binding.scheme = ⟨[], binding.scheme.body, []⟩ := by
    cases binding with
    | mk bindingName bindingBinder bindingKind bindingScheme =>
      cases bindingScheme with
      | mk variables body predicates =>
        simp only at quantified requirements ⊢
        subst variables
        subst predicates
        rfl
  rw [closed] at sameClosed
  cases sameClosed
  have member : HasType current.contents.typeTag value := by
    unfold HasType
    rw [sameType]
    exact typed
  obtain ⟨next, written⟩ := (write_accepted_iff (fun _ _ => inferInstance) state.store cell value).mpr
    ⟨current, present, writable, member⟩
  obtain ⟨step, retained⟩ := assignment_commit_preserves_scope_values state scope name binding binding.scheme.body
    value runtimeType rest cell next values unique distinct found monomorphic active written ordinary closed typed
  exact ⟨next, step, retained,
    mutable_cells_survive_store_evolution scope state.solution state.cells state.store next
      (write_preserves_layout state.store next cell value written) cells⟩

theorem binding_commit_preserves_mutable_cells (state : Machine) (scope : Scope)
    (name : NameId) (binder : BinderId) (kind : BindingKind) (raw : MonoType)
    (value : Value) (runtimeType : RuntimeType) (next : Store)
    (cells : MutableCellsAgree scope state.solution state.cells state.store)
    (binderFresh : ∀ binding ∈ scope, binding.binder ≠ binder)
    (cellFresh : state.store ⟨state.activation, binder⟩ = none)
    (closed : monoRuntime (substitute state.solution raw) = some runtimeType)
    (typed : valueType value = some runtimeType)
    (allocated : allocate state.store ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value = some next) :
    MutableCellsAgree (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: scope) state.solution
      (bindActive state.cells binder ⟨state.activation, binder⟩) next := by
  obtain ⟨constructed, current, built, present, mutable, cellType, cellValue⟩ :=
    allocate_fresh_typed_cell state.store ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value runtimeType cellFresh typed
  rw [allocated] at built
  cases built
  intro sought found foundAt isMutable
  by_cases same : name = sought
  · subst sought
    simp at foundAt
    cases foundAt
    refine ⟨rfl, rfl, ⟨state.activation, binder⟩, current, runtimeType, ?_, present, ?_, cellType, closed⟩
    · simp [bindActive]
    · change kind = .mutable at isMutable
      simpa [isMutable] using mutable
  · have original : scope.find? (fun item => decide (item.name = sought)) = some found := by
      simpa [same] using foundAt
    obtain ⟨quantified, requirements, oldCell, old, oldType, active, oldPresent, oldMutable, oldTag, oldClosed⟩ :=
      cells sought found original isMutable
    have different : found.binder ≠ binder := binderFresh found (List.mem_of_find?_eq_some original)
    have retained := allocate_preserves_existing state.store next ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value allocated oldCell old oldPresent
    exact ⟨quantified, requirements, oldCell, old, oldType,
      by simpa [bindActive, different] using active, retained, oldMutable, oldTag, oldClosed⟩

def ActiveCellsPresent (cells : ActiveCells) (store : Store) : Prop :=
  ∀ binder cell, cells binder = some cell → ∃ entry, store cell = some entry

theorem active_cells_survive_store_evolution (cells : ActiveCells) (before after : Store)
    (present : ActiveCellsPresent cells before) (evolves : StoreEvolves before after) :
    ActiveCellsPresent cells after := by
  intro binder cell active
  obtain ⟨old, oldAt⟩ := present binder cell active
  obtain ⟨current, currentAt, _, _⟩ := evolves cell old oldAt
  exact ⟨current, currentAt⟩

theorem fresh_binding_preserves_distinct_cells (cells : ActiveCells) (store : Store)
    (binder : BinderId) (freshCell : CellId) (distinct : DistinctActiveCells cells)
    (present : ActiveCellsPresent cells store) (fresh : store freshCell = none) :
    DistinctActiveCells (bindActive cells binder freshCell) := by
  intro first second cell firstAt secondAt
  by_cases firstNew : first = binder
  · subst first
    simp [bindActive] at firstAt
    subst cell
    by_cases secondNew : second = binder
    · exact secondNew.symm
    · have secondOld : cells second = some freshCell := by simpa [bindActive, secondNew] using secondAt
      obtain ⟨entry, entryAt⟩ := present second freshCell secondOld
      rw [fresh] at entryAt
      cases entryAt
  · by_cases secondNew : second = binder
    · subst second
      simp [bindActive] at secondAt
      subst cell
      have firstOld : cells first = some freshCell := by simpa [bindActive, firstNew] using firstAt
      obtain ⟨entry, entryAt⟩ := present first freshCell firstOld
      rw [fresh] at entryAt
      cases entryAt
    · exact distinct first second cell (by simpa [bindActive, firstNew] using firstAt)
        (by simpa [bindActive, secondNew] using secondAt)

theorem fresh_binding_preserves_active_cells (cells : ActiveCells) (before after : Store)
    (binder : BinderId) (freshCell : CellId) (present : ActiveCellsPresent cells before)
    (evolves : StoreEvolves before after) (newPresent : ∃ entry, after freshCell = some entry) :
    ActiveCellsPresent (bindActive cells binder freshCell) after := by
  intro other cell active
  by_cases same : other = binder
  · subst other
    simp [bindActive] at active
    subst cell
    exact newPresent
  · have oldActive : cells other = some cell := by simpa [bindActive, same] using active
    exact active_cells_survive_store_evolution cells before after present evolves other cell oldActive

def ScopeStateAgrees (scope : Scope) (state : Machine) : Prop :=
  (scope.map Binding.binder).Nodup ∧
    ScopeValuesAgree scope state.solution state.cells state.store ∧
    MutableCellsAgree scope state.solution state.cells state.store ∧
    DistinctActiveCells state.cells ∧ ActiveCellsPresent state.cells state.store

theorem typed_binding_preserves_scope_state (state : Machine) (scope : Scope)
    (name : NameId) (binder : BinderId) (kind : BindingKind) (raw : MonoType)
    (value : Value) (runtimeType : RuntimeType) (rest : List StatementNode)
    (agrees : ScopeStateAgrees scope state)
    (binderFresh : ∀ binding ∈ scope, binding.binder ≠ binder)
    (cellFresh : state.store ⟨state.activation, binder⟩ = none)
    (ordinary : ordinaryValue value = true)
    (closed : monoRuntime (substitute state.solution raw) = some runtimeType)
    (typed : valueType value = some runtimeType) :
    ∃ next, Step { state with control := .value value, stack := .bind binder kind rest :: state.stack } .internal
        { state with control := .statements rest, store := next, cells := bindActive state.cells binder ⟨state.activation, binder⟩ } ∧
      ScopeStateAgrees (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: scope)
        { state with control := .statements rest, store := next, cells := bindActive state.cells binder ⟨state.activation, binder⟩ } := by
  obtain ⟨unique, values, mutableCells, distinct, activePresent⟩ := agrees
  obtain ⟨next, allocated, step, valuesAfter⟩ := binding_commit_preserves_scope_values state scope name binder kind raw
    value runtimeType rest values binderFresh cellFresh ordinary closed typed
  have evolution := allocate_preserves_layout state.store next ⟨state.activation, binder⟩
    (if kind = .mutable then .mutable else .immutable) value allocated
  obtain ⟨constructed, current, built, present, _, _, _⟩ :=
    allocate_fresh_typed_cell state.store ⟨state.activation, binder⟩
      (if kind = .mutable then .mutable else .immutable) value runtimeType cellFresh typed
  rw [allocated] at built
  cases built
  have newUnique : ((⟨name, binder, kind, ⟨[], raw, []⟩⟩ : Binding) :: scope).map Binding.binder |>.Nodup := by
    simp only [List.map_cons, List.nodup_cons]
    refine ⟨?_, unique⟩
    intro member
    obtain ⟨old, oldIn, same⟩ := List.mem_map.mp member
    exact binderFresh old oldIn same
  refine ⟨next, step, newUnique, valuesAfter, ?_, ?_, ?_⟩
  · exact binding_commit_preserves_mutable_cells state scope name binder kind raw value runtimeType next
      mutableCells binderFresh cellFresh closed typed allocated
  · exact fresh_binding_preserves_distinct_cells state.cells state.store binder ⟨state.activation, binder⟩ distinct activePresent cellFresh
  · exact fresh_binding_preserves_active_cells state.cells state.store next binder ⟨state.activation, binder⟩
      activePresent evolution ⟨current, present⟩

theorem typed_assignment_preserves_scope_state (state : Machine) (scope : Scope)
    (name : NameId) (binding : Binding) (value : Value) (runtimeType : RuntimeType) (rest : List StatementNode)
    (agrees : ScopeStateAgrees scope state)
    (found : scope.find? (fun item => decide (item.name = name)) = some binding)
    (mutable : binding.kind = .mutable)
    (closed : monoRuntime (substitute state.solution binding.scheme.body) = some runtimeType)
    (ordinary : ordinaryValue value = true) (typed : valueType value = some runtimeType) :
    ∃ next, Step { state with control := .value value, stack := .assign binding.binder rest :: state.stack } .internal
        { state with control := .statements rest, store := next } ∧
      ScopeStateAgrees scope { state with control := .statements rest, store := next } := by
  obtain ⟨unique, values, cells, distinct, present⟩ := agrees
  obtain ⟨next, step, retainedValues, retainedCells⟩ := typed_assignment_preserves_scope state scope name binding
    value runtimeType rest found mutable values cells unique distinct closed ordinary typed
  have evolves := step_preserves_store_layout step
  exact ⟨next, step, unique, retainedValues, retainedCells, distinct,
    active_cells_survive_store_evolution state.cells state.store next present evolves⟩

theorem typed_scope_read_progress (state : Machine) (scope : Scope) (binder : BinderId)
    (arguments : List (Variable × MonoType)) (ty : MonoType) (runtimeType : RuntimeType)
    (agrees : ScopeStateAgrees scope state)
    (typed : NodeHasType scope state.solution (.read binder arguments) ty)
    (closed : monoRuntime ty = some runtimeType) :
    ∃ value, Step { state with control := .expression (.read binder arguments) } .internal
        { state with control := .value value } ∧ valueType value = some runtimeType := by
  obtain ⟨unique, values, _, _, _⟩ := agrees
  cases typed with
  | read found notSchema keys sorts instantiated solved wellFormed sort =>
    rename_i name binding raw requirements
    obtain ⟨value, readValue, valueAgrees⟩ := values name binding found notSchema
    rcases valueAgrees with ⟨declaration, parameters, result, rfl, schemeShape⟩ | ⟨ordinary, typedValue⟩
    · have present := List.mem_of_find?_eq_some found
      have sameScheme := scope_schemes_agree_of_unique scope binding unique present
      have rawEqual := instantiated.1
      have shape : substitute state.solution raw =
          .function (substitute state.solution (substitute (schemeSubstitution binding.scheme (witnessSubstitution arguments)) parameters))
            (substitute state.solution (substitute (schemeSubstitution binding.scheme (witnessSubstitution arguments)) result)) := by
        rw [rawEqual, schemeShape]
        rfl
      rw [shape] at closed
      cases paramClosed : monoRuntime (substitute state.solution (substitute (schemeSubstitution binding.scheme (witnessSubstitution arguments)) parameters)) with
      | none => simp [monoRuntime, paramClosed] at closed
      | some runtimeParameters =>
        cases resultClosed : monoRuntime (substitute state.solution (substitute (schemeSubstitution binding.scheme (witnessSubstitution arguments)) result)) with
        | none => simp [monoRuntime, paramClosed, resultClosed] at closed
        | some runtimeResult =>
          simp [monoRuntime, paramClosed, resultClosed] at closed
          subst runtimeType
          exact typed_scheme_function_read_steps state scope binding.binder arguments binding.scheme declaration
            _ _ (substitute state.solution raw) runtimeParameters runtimeResult
            (.read found notSchema keys sorts instantiated solved wellFormed sort) shape sameScheme
            readValue paramClosed resultClosed
    · exact ⟨value, Step.read state binding.binder arguments value readValue ordinary,
        typedValue arguments raw requirements runtimeType instantiated solved closed⟩

end Xsh.Semantics.Connected
