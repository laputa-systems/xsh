import Xsh.Semantics.Connected.SafetyValueBlockInvariant

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected
open Xsh.Syntax.Foundation

def booleanInitialMachine (base : Machine) : Machine :=
  { base with functions := fun _ => none, schemas := fun _ => none, globals := emptyCells, cells := emptyCells, store := fun _ => none, activation := 0, nextActivation := 1, solution := identity, control := .expression (.boolean true), stack := [] }

def booleanInitialContext : TypingContext :=
  ⟨[], fun _ => .ordinary, identity, identity, .primitive .boolean, some (.primitive .boolean), 0, ⟨.boolean, rfl⟩⟩

theorem boolean_initial_machine_is_typed (base : Machine) :
    TypedMachine [] (fun _ => .ordinary) (fun _ => none) (booleanInitialMachine base) := by
  constructor
  · constructor
    · constructor
      · exact Nat.zero_lt_succ 0
      · intro frame member
        simp [booleanInitialMachine] at member
      · intro cell entry present
        simp [booleanInitialMachine] at present
    · constructor
      · intro binder ty found
        cases found
      · intro binder cell found
        simp [booleanInitialMachine, emptyCells] at found
  · intro declaration function found
    simp [booleanInitialMachine] at found
  · intro first second cell found
    simp [booleanInitialMachine, emptyCells] at found
  · intro binder declaration scheme found
    simp [Connected.read, booleanInitialMachine, emptyCells] at found
  · constructor
    · intro binder declaration scheme found
      simp [Connected.read, booleanInitialMachine, emptyCells] at found
    · intro binder found
      cases found
  · intro cell entry present
    simp [booleanInitialMachine] at present
  · refine ⟨.expression booleanInitialContext (.primitive .boolean),
      .expression booleanInitialContext (.primitive .boolean), rfl, rfl, rfl, identity_well_sorted, identity_well_sorted, ?_, ⟨.boolean, rfl⟩,
      ⟨.boolean, rfl⟩, ?_, .nil, ?_⟩
    · refine ⟨?_, ?_, ?_, ?_, ?_⟩
      · refine ⟨by simp [booleanInitialContext, TypedDemand.context], ?_, ?_, ?_, ?_⟩
        · intro name binding found
          simp [booleanInitialContext, TypedDemand.context] at found
        · intro name binding found
          simp [booleanInitialContext, TypedDemand.context] at found
        · intro first second cell found
          simp [booleanInitialMachine, emptyCells] at found
        · intro binder cell found
          simp [booleanInitialMachine, emptyCells] at found
      · intro name binding cell entry found
        simp [booleanInitialContext, TypedDemand.context] at found
      · intro name binding identity row found
        simp [booleanInitialContext, TypedDemand.context] at found
      · intro binder declaration scheme found
        simp [Connected.read, booleanInitialMachine, emptyCells] at found
      · constructor
        · intro binder declaration scheme found
          simp [Connected.read, booleanInitialMachine, emptyCells] at found
        · intro binder found
          cases found
    · exact .expression .boolean rfl (by intro binder member; simp [nodeDeclaredBinders] at member)
    · constructor
      · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, booleanInitialMachine, nodeDeclaredBinders]
      · intro binding member
        simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, booleanInitialMachine, nodeDeclaredBinders] at member

theorem missing_callable_rejected (functions : FunctionTable) (declaration : DeclarationId)
    (missing : functions declaration = none) :
    ¬ ValueCallables functions (.function declaration .argumentsEmpty .integer) := by
  intro callable
  cases callable <;> simp_all

theorem missing_callable_store_rejected (state : Machine) (globals : Scope)
    (storage : BindingStorageMap) (types : CellTypes) (declaration : DeclarationId)
    (cell : Xsh.Semantics.Environments.CellId)
    (entry : Xsh.Semantics.Environments.StoreCell RuntimeType Value HasType)
    (missing : state.functions declaration = none) (present : state.store cell = some entry)
    (value : entry.contents.value = .function declaration .argumentsEmpty .integer) :
    ¬ TypedMachine globals storage types state := by
  intro invariant
  have callable := invariant.storedValues cell entry present
  rw [value] at callable
  exact missing_callable_rejected state.functions declaration missing callable

end Xsh.Semantics.Connected
