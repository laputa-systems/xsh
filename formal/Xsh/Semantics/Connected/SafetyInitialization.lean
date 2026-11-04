import Xsh.Semantics.Connected.SafetyPreservation
import Xsh.Semantics.Connected.SafetyValueBlockWitness

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

def checkedInitialMachine (base : Machine) (node : Node) : Machine :=
  { base with cells := base.globals, control := .expression node, stack := [] }

theorem checked_initial_machine_typed (base : Machine) (globals : Scope) (storage : BindingStorageMap)
    (types : CellTypes) (solution : Substitution) (node : Node) (result ty : MonoType) (resultType : RuntimeType)
    (activeBelow : base.activation < base.nextActivation)
    (storeBelow : StoreBelowActivation base.store base.nextActivation)
    (globalTyped : CellsTyped base.globals types base.store)
    (globalLive : CellsLive base.globals base.store)
    (globalScope : ScopeStateAgrees globals { base with cells := base.globals, solution := solution })
    (globalWritable : ScopeWritableContract globals solution base.globals base.store)
    (globalSchemas : SchemaMatches globals solution base.schemas)
    (globalSchemes : SchemeCellsImmutable base.globals base.store)
    (globalStorage : StorageAgreement base.globals base.store storage)
    (storedValues : StoreCallables base.functions base.store)
    (functions : FunctionTableCertified globals storage base)
    (solutionSorted : WellSorted solution) (runtimeSorted : WellSorted base.solution)
    (resultClosed : monoRuntime result = some resultType)
    (ground : GroundNode solution base.solution storage globals result node ty)
    (bindersUnique : (nodeDeclaredBinders node).Nodup)
    (bindersFresh : FutureNodeBindingsFresh base.store base.activation node) :
    TypedMachine globals storage types (checkedInitialMachine base node) := by
  let context : TypingContext := ⟨globals, storage, solution, base.solution, result, some result,
    base.activation, ⟨resultType, resultClosed⟩⟩
  obtain ⟨runtimeType, closed⟩ := ground_node_is_closed ground
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · exact ⟨⟨activeBelow, by intro frame member; simp [checkedInitialMachine] at member, storeBelow⟩,
      ⟨globalTyped, globalLive⟩⟩
  · exact functions
  · exact globalScope.2.2.2.1
  · exact globalSchemes
  · exact globalStorage
  · exact storedValues
  · refine ⟨.expression context ty, .expression context ty, rfl, rfl, rfl, solutionSorted, runtimeSorted,
      ?_, ⟨runtimeType, closed⟩, ⟨runtimeType, closed⟩, .expression ground closed bindersFresh, .nil, ?_⟩
    · exact ⟨globalScope, globalWritable, globalSchemas, globalSchemes, globalStorage⟩
    · constructor
      · simpa [checkedInitialMachine, pendingMachineBinders, pendingControlBinders, pendingStackBinders]
          using activation_binders_nodup base.activation (nodeDeclaredBinders node) bindersUnique
      · intro binding member
        simp only [checkedInitialMachine, pendingMachineBinders, pendingControlBinders, pendingStackBinders,
          List.append_nil] at member
        obtain ⟨binder, binderMember, same⟩ := List.mem_map.mp member
        subst binding
        exact bindersFresh binder binderMember

theorem typed_machine_execution_preservation (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (response : Xsh.Values.IntValue) {initial final : Machine} {events : List Label}
    (execution : Execution initial events final) (typed : TypedMachine globals storage types initial) :
    TypedMachine globals storage types final := by
  induction execution with
  | done => exact typed
  | next step rest ih => exact ih (typed_machine_step_preservation globals storage types response typed step)

theorem typed_execution_fault_is_integer (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (response : Xsh.Values.IntValue) {initial final : Machine} {events : List Label}
    (execution : Execution initial events final) (typed : TypedMachine globals storage types initial)
    (fault : Fault) (failed : final.control = .fault fault) : ∃ issue, fault = .integer issue := by
  have finalTyped := typed_machine_execution_preservation globals storage types response execution typed
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := finalTyped.focus
  rw [failed] at control
  exact typed_fault_control_shape final input fault control

theorem value_block_initial_machine_is_typed (base : Machine) :
    TypedMachine [] (fun _ => .ordinary) (fun _ => none)
      (checkedInitialMachine (booleanInitialMachine base)
        (.valueBlock (.primitive .boolean) [.evaluated (.boolean true)])) := by
  have existing := boolean_initial_machine_is_typed base
  apply checked_initial_machine_typed (booleanInitialMachine base) [] (fun _ => .ordinary) (fun _ => none)
    identity (.valueBlock (.primitive .boolean) [.evaluated (.boolean true)]) (.primitive .boolean)
    (.primitive .boolean) .boolean
  · exact Nat.zero_lt_succ 0
  · simpa [booleanInitialMachine] using existing.memory.activation.store
  · simpa [booleanInitialMachine] using existing.memory.globals.typed
  · simpa [booleanInitialMachine] using existing.memory.globals.live
  · refine ⟨by simp, ?_, ?_, ?_, ?_⟩
    · intro name binding found
      simp at found
    · intro name binding found
      simp at found
    · intro first second cell found
      simp [booleanInitialMachine, emptyCells] at found
    · intro binder cell found
      simp [booleanInitialMachine, emptyCells] at found
  · intro name binding cell entry found
    simp at found
  · intro name binding identity row found
    simp at found
  · exact existing.globalSchemes
  · exact existing.globalStorageAgrees
  · exact existing.storedValues
  · exact existing.functions
  · exact identity_well_sorted
  · exact identity_well_sorted
  · rfl
  · exact .valueBlock (.evaluatedTail .boolean (Or.inr rfl)) rfl
  · simp [nodeDeclaredBinders, declaredBinders]
  · intro binder member
    simp [nodeDeclaredBinders, declaredBinders] at member

end Xsh.Semantics.Connected
