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

end Xsh.Semantics.Connected
