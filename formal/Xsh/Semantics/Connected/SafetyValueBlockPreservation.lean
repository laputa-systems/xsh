import Xsh.Semantics.Connected.SafetyValueBlockPendingProof
import Xsh.Semantics.Connected.SafetyValueBlockGround

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem typed_machine_propagation_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Node) (error : MonoType)
    (typed : TypedMachine globals storage types { base with control := .expression (.propagate value error) }) :
    TypedMachine globals storage types
      { base with control := .expression value, stack := .propagate :: base.stack } := by
  have step := Step.propagateStart base value error
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | expression ground closed fresh =>
    rename_i context ty runtimeType
    obtain ⟨failure, inner, innerClosed, frame⟩ :=
      ground_propagation_opens_closed_frame (state := { base with control := .expression value, stack := .propagate :: base.stack }) ground
    have newPending : PendingBindingsSafe { base with control := .expression value, stack := .propagate :: base.stack } := by
      constructor
      · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders,
          pendingFrameBinders, frameCallerActivation, nodeDeclaredBinders] using pending.unique
      · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders,
          pendingFrameBinders, frameCallerActivation, nodeDeclaredBinders] using pending.fresh
    have oldStack : StackTyped { base with control := .expression value, stack := .propagate :: base.stack }
        (.expression context ty) output base.stack := by
      apply stack_typing_survives_pending step stack
      intro binding member
      apply newPending.fresh binding
      simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
        frameCallerActivation, List.mem_append]
      right
      simpa [activation] using member
    have innerFresh : FutureNodeBindingsFresh base.store context.activation value := by
      simpa [FutureNodeBindingsFresh, nodeDeclaredBinders] using fresh
    obtain ⟨resultType, resultClosed⟩ := ground_node_is_closed inner
    refine ⟨.expression context (.result ty failure), output, runtimeSolution, activation, storageEqual,
      proofSorted, runtimeSorted, ?_, innerClosed, outputClosed, ?_, .cons frame inputClosed oldStack, newPending⟩
    · simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active
    · exact .expression inner resultClosed innerFresh

end Xsh.Semantics.Connected
