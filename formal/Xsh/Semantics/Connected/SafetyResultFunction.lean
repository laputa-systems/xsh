import Xsh.Semantics.Connected.SafetyResultProgram
import Xsh.Semantics.Connected.TypedScope

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Syntax.Foundation
open Xsh.Typing.Connected
open Xsh.Conformance.Connected

def callOkBinding : Binding := ⟨⟨⟨8, by decide⟩⟩, resultOkBinder, .immutable, okScheme⟩
def callWrapBinding : Binding := ⟨⟨⟨5, by decide⟩⟩, wrapBinder, .immutable, resultFunctionScheme wrapSignature⟩
def callLiftBinding : Binding := ⟨⟨⟨9, by decide⟩⟩, liftBinder, .immutable, resultFunctionScheme liftSignature⟩
def callParameterBinding : Binding := ⟨⟨⟨6, by decide⟩⟩, resultFirstBinder, .immutable, ⟨[], .primitive .integer, []⟩⟩

def liftScope : Scope := [callOkBinding, callWrapBinding, callLiftBinding]
def wrapScope : Scope := callParameterBinding :: liftScope

structure BindingKey where
  name : NameId
  binder : BinderId
  kind : BindingKind
  quantified : List Variable
  body : MonoType
  requirements : List Constraint
  deriving DecidableEq

def bindingKey (binding : Binding) : BindingKey :=
  ⟨binding.name, binding.binder, binding.kind, binding.scheme.quantified, binding.scheme.body,
    binding.scheme.requirements⟩

def scopeKeys (scope : Option Scope) : Option (List BindingKey) :=
  scope.map (List.map bindingKey)

def certifiedScopesMatch : Bool :=
  match admitSource 0 resultCallSource with
  | .ok unit =>
      decide (scopeKeys (resultScope unit resultCallSignatures wrapSignature) = scopeKeys (some wrapScope)) &&
        decide (scopeKeys (resultScope unit resultCallSignatures liftSignature) = scopeKeys (some liftScope))
  | .error _ => false

set_option maxRecDepth 100000 in
theorem the_certified_scopes_are_the_typed_scopes : certifiedScopesMatch = true := by
  decide +kernel

theorem scope_values_agree_of_members (scope : Scope) (solution : Substitution) (cells : ActiveCells) (store : Store)
    (each : ∀ binding ∈ scope, ∃ value, Connected.read store cells binding.binder = some value ∧
      ((∃ declaration parameters result, value = .schemeFunction declaration binding.scheme ∧
          binding.scheme.body = .function parameters result) ∨
        OrdinaryBindingValueAgrees solution binding value)) :
    ScopeValuesAgree scope solution cells store :=
  fun _ binding found _ => each binding (List.mem_of_find?_eq_some found)

theorem mutable_cells_agree_of_immutable (scope : Scope) (solution : Substitution) (cells : ActiveCells)
    (store : Store) (immutable : ∀ binding ∈ scope, binding.kind = .immutable) :
    MutableCellsAgree scope solution cells store := by
  intro _ binding found mutable
  have := immutable binding (List.mem_of_find?_eq_some found)
  rw [this] at mutable
  cases mutable

theorem program_globals_cell (binder : BinderId) (cell : Environments.CellId) (found : programGlobals binder = some cell) :
    cell = ⟨0, binder⟩ ∧ (binder = resultOkBinder ∨ binder = wrapBinder ∨ binder = liftBinder) := by
  unfold programGlobals at found
  split at found
  · cases found
    exact ⟨rfl, ‹_›⟩
  · cases found

def topCells : ActiveCells := withGlobals emptyCells programGlobals

theorem top_cells_are_the_globals : topCells = programGlobals := by
  funext binder
  simp [topCells, withGlobals, emptyCells]

theorem program_globals_are_distinct : DistinctActiveCells programGlobals := by
  intro first second cell firstFound secondFound
  obtain ⟨rfl, _⟩ := program_globals_cell first cell firstFound
  obtain ⟨same, _⟩ := program_globals_cell second _ secondFound
  exact (Environments.CellId.mk.inj same).2

theorem program_globals_are_present : ActiveCellsPresent programGlobals programStore := by
  intro binder cell found
  obtain ⟨rfl, which⟩ := program_globals_cell binder cell found
  rcases which with rfl | rfl | rfl
  · simp [programStore]
  · simp [programStore, program_identities_are_distinct]
  · simp [programStore, program_identities_are_distinct]

theorem program_global_values (solution : Substitution) :
    ∀ binding ∈ liftScope, ∃ value, Connected.read programStore programGlobals binding.binder = some value ∧
      ((∃ declaration parameters result, value = .schemeFunction declaration binding.scheme ∧
          binding.scheme.body = .function parameters result) ∨
        OrdinaryBindingValueAgrees solution binding value) := by
  intro binding member
  simp only [liftScope, List.mem_cons, List.not_mem_nil, or_false] at member
  rcases member with rfl | rfl | rfl
  · exact ⟨.schemeFunction callOkDeclaration okScheme,
      by simp [Connected.read, programGlobals, programStore, callOkBinding],
      .inl ⟨callOkDeclaration, _, _, rfl, rfl⟩⟩
  · exact ⟨.schemeFunction programWrapDeclaration (resultFunctionScheme wrapSignature),
      by simp [Connected.read, programGlobals, programStore, program_identities_are_distinct, callWrapBinding],
      .inl ⟨programWrapDeclaration, _, _, rfl, rfl⟩⟩
  · exact ⟨.schemeFunction programLiftDeclaration (resultFunctionScheme liftSignature),
      by simp [Connected.read, programGlobals, programStore, program_identities_are_distinct, callLiftBinding],
      .inl ⟨programLiftDeclaration, _, _, rfl, rfl⟩⟩

theorem lift_scope_is_immutable : ∀ binding ∈ wrapScope, binding.kind = .immutable := by
  intro binding member
  simp only [wrapScope, liftScope, List.mem_cons, List.not_mem_nil, or_false] at member
  rcases member with rfl | rfl | rfl | rfl <;> rfl

theorem lift_scope_binders_are_distinct : (wrapScope.map Binding.binder).Nodup := by
  decide

theorem read_through_program_globals (store : Store) (binder : BinderId)
    (agrees : ∀ cell : Environments.CellId, cell.activation = 0 → store cell = programStore cell) :
    Connected.read store programGlobals binder = Connected.read programStore programGlobals binder := by
  unfold Connected.read
  cases found : programGlobals binder with
  | none => rfl
  | some cell =>
    obtain ⟨rfl, _⟩ := program_globals_cell binder cell found
    simp [agrees ⟨0, binder⟩ rfl]

theorem a_top_level_state_agrees_with_the_lift_scope (state : Machine)
    (store : ∀ cell : Environments.CellId, cell.activation = 0 → state.store cell = programStore cell)
    (cells : state.cells = programGlobals) : ScopeStateAgrees liftScope state := by
  rw [ScopeStateAgrees, cells]
  refine ⟨(List.nodup_cons.mp lift_scope_binders_are_distinct).2, ?_, ?_, program_globals_are_distinct, ?_⟩
  · refine scope_values_agree_of_members _ _ _ _ fun binding member => ?_
    rw [read_through_program_globals state.store binding.binder store]
    exact program_global_values _ binding member
  · exact mutable_cells_agree_of_immutable _ _ _ _ fun binding member =>
      lift_scope_is_immutable binding (List.mem_cons_of_mem _ member)
  · intro binder cell found
    obtain ⟨entry, present⟩ := program_globals_are_present binder cell found
    obtain ⟨rfl, _⟩ := program_globals_cell binder cell found
    exact ⟨entry, by rw [store ⟨0, binder⟩ rfl]; exact present⟩

def liftScheme : Scheme := resultFunctionScheme liftSignature
def liftFunction : Function := certifiedFunction liftSignature liftCandidate

def liftTarget (base : Machine) : Value :=
  .instantiatedFunction programLiftDeclaration .argumentsEmpty (.result .integer .string) base.solution []

def liftInvocation (base : Machine) : Machine :=
  { programMachine base with control := .invoke (liftTarget base) [] }

def liftEntered (base : Machine) : Machine :=
  { liftInvocation base with
    control := .statements liftFunction.body, cells := withGlobals emptyCells programGlobals,
    store := programStore, activation := 1, nextActivation := 2,
    solution := compose base.solution (compose (schemeSubstitution liftFunction.scheme (witnessSubstitution []))
      liftFunction.elaboration),
    stack := [.caller programGlobals 0 base.solution (.result .integer .string)] }

def liftReturning (base : Machine) : Machine :=
  { liftEntered base with stack := .returnValue :: (liftEntered base).stack }

theorem lift_scheme_instantiates (solution : Substitution) :
    instantiateFunction solution liftFunction.scheme [] = some (.argumentsEmpty, .result .integer .string, []) := by
  simp [liftFunction, certifiedFunction, resultFunctionScheme, resultFunctionType, liftSignature, instantiateFunction,
    instantiationKeysValid, instantiationSortsValid, substitute, checksConstraints, monoRuntime, integerStringResult]

theorem invoking_lift_enters_its_body (base : Machine) :
    Step (liftInvocation base) .internal (liftEntered base) := by
  have found : (liftInvocation base).functions programLiftDeclaration = some (.function liftFunction) := by
    simp [liftInvocation, programMachine, programFunctions, program_identities_are_distinct, liftFunction]
  exact Step.callFunction (programMachine base) programLiftDeclaration [] .argumentsEmpty (.result .integer .string)
    base.solution [] [] liftFunction emptyCells programStore found (lift_scheme_instantiates _) rfl

theorem program_store_is_below (limit : Nat) (positive : 0 < limit) : StoreBelowActivation programStore limit := by
  intro cell entry found
  simp only [programStore] at found
  split at found
  · rename_i same; subst same; exact positive
  · split at found
    · rename_i same; subst same; exact positive
    · split at found
      · rename_i same; subst same; exact positive
      · cases found

theorem lift_returning_satisfies_the_activation_invariant (base : Machine) :
    ActivationInvariant (liftReturning base) where
  active := by simp [liftReturning, liftEntered]
  saved := by
    intro frame member
    simp [liftReturning, liftEntered] at member
    rcases member with rfl | rfl <;> simp [FrameActivationBelow, liftReturning, liftEntered]
  store := program_store_is_below 2 (by decide)

theorem certified_program_of_the_same_tables {first second : Machine} {signatures : List ResultSignature}
    {cases : List (List StatementNode × List DerivationStep)}
    (store : first.store = second.store) (globals : first.globals = second.globals)
    (functions : first.functions = second.functions) (program : CertifiedResultProgram first signatures cases) :
    CertifiedResultProgram second signatures cases where
  functions := by
    rw [← store, ← globals, ← functions]
    exact program.functions
  ok := by
    rw [← store, ← globals, ← functions]
    exact program.ok

theorem lift_returning_holds_the_certified_program (base : Machine) :
    CertifiedResultProgram (liftReturning base) resultCallSignatures resultCallCandidates :=
  certified_program_of_the_same_tables (first := programMachine base) rfl rfl rfl
    (the_program_machine_loads_the_certified_program base)

def liftReturned (base : Machine) : Machine :=
  { callResumed (liftReturning base) with
    control := .value (.ok callOne .string), stack := [], cells := programGlobals, activation := 0,
    solution := base.solution }

theorem invoking_lift_runs_its_body_to_the_caller (base : Machine) :
    Execution (liftInvocation base) (List.replicate 26 .internal) (liftReturned base) := by
  obtain ⟨_, _, run⟩ := a_certified_call_program_runs_lift_through_wrap (liftReturning base)
    (lift_returning_holds_the_certified_program base) (lift_returning_satisfies_the_activation_invariant base)
    top_cells_are_the_globals
  refine .next (invoking_lift_enters_its_body base) ?_
  refine .next (Step.returnStart (liftEntered base) liftNode []) ?_
  refine execution_append (first := List.replicate 22 .internal) (second := List.replicate 2 .internal) run ?_
  let resumed := { callResumed (liftReturning base) with stack := (liftEntered base).stack }
  refine .next (Step.returnValue resumed (.ok callOne .string)) ?_
  exact .next (Step.returnedToCaller { resumed with stack := [] } (.ok callOne .string) programGlobals 0 base.solution
    (.result .integer .string) (by decide +kernel)) (.done _)

theorem lift_body_starts_in_the_typed_scope (base : Machine) : ScopeStateAgrees liftScope (liftEntered base) :=
  a_top_level_state_agrees_with_the_lift_scope _ (fun _ _ => rfl) top_cells_are_the_globals

theorem lift_returns_in_the_typed_scope (base : Machine) : ScopeStateAgrees liftScope (liftReturned base) := by
  refine a_top_level_state_agrees_with_the_lift_scope _ (fun cell zero => ?_) rfl
  have other : cell ≠ calleeCell (liftReturning base) := by
    intro same
    rw [same] at zero
    simp [calleeCell, liftReturning, liftEntered] at zero
  show calleeStore (liftReturning base) cell = programStore cell
  simp [calleeStore, other]
  rfl

theorem callee_cells_cell (state : Machine) (binder : BinderId) (cell : Environments.CellId)
    (globals : state.globals = programGlobals) (found : calleeCells state binder = some cell) :
    (binder = resultFirstBinder ∧ cell = calleeCell state) ∨ programGlobals binder = some cell := by
  by_cases parameter : binder = resultFirstBinder
  · subst parameter
    left
    simp [calleeCells, withGlobals, bindActive] at found
    exact ⟨rfl, found.symm⟩
  · right
    simpa [calleeCells, withGlobals, bindActive, emptyCells, parameter, globals] using found

theorem wrap_body_starts_in_the_typed_scope (base : Machine) (world : ResultCallWorld (liftReturning base) wrapCandidate)
    (stack : List Frame) : ScopeStateAgrees wrapScope (calleeEntered (liftReturning base) world stack) := by
  let state := liftReturning base
  have globals : state.globals = programGlobals := rfl
  have fresh : state.store (calleeCell state) = none := by
    simp [state, calleeCell, liftReturning, liftEntered, programStore]
  have visible : ∀ binder, state.globals binder = state.cells binder := by
    intro binder
    show programGlobals binder = topCells binder
    rw [top_cells_are_the_globals]
  have away : ∀ cell : Environments.CellId, cell.activation = 0 → cell ≠ calleeCell state := by
    intro cell zero same
    rw [same] at zero
    simp [state, calleeCell, liftReturning, liftEntered] at zero
  refine ⟨lift_scope_binders_are_distinct, ?_, ?_, ?_, ?_⟩
  · refine scope_values_agree_of_members _ _ _ _ fun binding member => ?_
    simp only [wrapScope, List.mem_cons] at member
    rcases member with rfl | member
    · exact ⟨callOne, callee_reads_its_parameter state, .inr (monomorphic_value_agrees _ _ _ _ _ _ .integer rfl
        (by simp [substitute, monoRuntime]) call_one_is_an_integer)⟩
    · obtain ⟨value, found, agrees⟩ := program_global_values (calleeEntered state world stack).solution binding member
      have other : binding.binder ≠ resultFirstBinder := by
        simp only [liftScope, List.mem_cons, List.not_mem_nil, or_false] at member
        rcases member with rfl | rfl | rfl <;> decide
      refine ⟨value, ?_, agrees⟩
      refine callee_reads_a_caller_global state binding.binder value other (visible _) fresh ?_
      show Connected.read programStore topCells binding.binder = some value
      rw [top_cells_are_the_globals]
      exact found
  · exact mutable_cells_agree_of_immutable _ _ _ _ lift_scope_is_immutable
  · intro first second cell firstFound secondFound
    rcases callee_cells_cell state first cell globals firstFound with ⟨rfl, rfl⟩ | firstGlobal <;>
      rcases callee_cells_cell state second _ globals secondFound with ⟨rfl, same⟩ | secondGlobal
    · rfl
    · obtain ⟨zero, _⟩ := program_globals_cell second _ secondGlobal
      exact absurd zero.symm (away _ rfl)
    · obtain ⟨zero, _⟩ := program_globals_cell first _ firstGlobal
      exact absurd (same.symm.trans zero) (fun h => away _ rfl h.symm)
    · exact program_globals_are_distinct first second cell firstGlobal secondGlobal
  · intro binder cell found
    rcases callee_cells_cell state binder cell globals found with ⟨rfl, rfl⟩ | global
    · show ∃ entry, calleeStore state (calleeCell state) = some entry
      simp [calleeStore]
    · obtain ⟨entry, present⟩ := program_globals_are_present binder cell global
      obtain ⟨rfl, _⟩ := program_globals_cell binder cell global
      show ∃ entry, calleeStore state ⟨0, binder⟩ = some entry
      have other := away ⟨0, binder⟩ rfl
      simp only [calleeStore, other]
      exact ⟨entry, present⟩

def wrapEntryStack (base : Machine) (world : ResultCallWorld (liftReturning base) wrapCandidate) : List Frame :=
  .propagate :: .callArguments (okCallTarget world.ok) [] [] :: (liftReturning base).stack

theorem invoking_lift_reaches_the_wrap_frame (base : Machine) (world : ResultCallWorld (liftReturning base) wrapCandidate) :
    Execution (liftInvocation base) (List.replicate 12 .internal)
      (calleeEntered (liftReturning base) world (wrapEntryStack base world)) := by
  refine .next (invoking_lift_enters_its_body base) ?_
  refine .next (Step.returnStart (liftEntered base) liftNode []) ?_
  exact lift_enters_the_wrap_call_frame (liftReturning base) world

theorem the_certified_lift_function_runs_in_its_typed_scopes (base : Machine) :
    resultCallCertified = true ∧ certifiedScopesMatch = true ∧
      Execution (liftInvocation base) (List.replicate 26 .internal) (liftReturned base) ∧
      ScopeStateAgrees liftScope (liftEntered base) ∧
      (∀ world : ResultCallWorld (liftReturning base) wrapCandidate,
        Execution (liftInvocation base) (List.replicate 12 .internal)
          (calleeEntered (liftReturning base) world (wrapEntryStack base world)) ∧
        ScopeStateAgrees wrapScope (calleeEntered (liftReturning base) world (wrapEntryStack base world))) ∧
      ScopeStateAgrees liftScope (liftReturned base) :=
  ⟨result_call_is_certified, the_certified_scopes_are_the_typed_scopes, invoking_lift_runs_its_body_to_the_caller base,
    lift_body_starts_in_the_typed_scope base,
    fun world => ⟨invoking_lift_reaches_the_wrap_frame base world, wrap_body_starts_in_the_typed_scope base world _⟩,
    lift_returns_in_the_typed_scope base⟩

theorem a_state_without_the_ok_cell_is_outside_the_typed_scope (base : Machine) :
    ¬ ScopeStateAgrees liftScope { liftEntered base with store := fun _ => none } := by
  intro agrees
  obtain ⟨_, values, _⟩ := agrees
  obtain ⟨value, found, _⟩ := values ⟨⟨8, by decide⟩⟩ callOkBinding rfl (by decide)
  simp [Connected.read] at found

end Xsh.Semantics.Connected
