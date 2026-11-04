import Xsh.Semantics.Connected.SafetyValueBlockPendingProof
import Xsh.Semantics.Connected.SafetyValueBlockGround

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem typed_machine_expression_push (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (outer inner : Node) (frame : Frame) (event : Label)
    (step : Step { base with control := .expression outer } event
      { base with control := .expression inner, stack := frame :: base.stack })
    (decompose : ∀ context ty,
      GroundNode context.solution context.runtimeSolution context.storage context.scope context.result outer ty →
      SchemaMatches context.scope context.solution base.schemas →
      ∃ innerType, GroundNode context.solution context.runtimeSolution context.storage context.scope context.result inner innerType ∧
        FrameTyped { base with control := .expression inner, stack := frame :: base.stack }
          (.expression context innerType) (.expression context ty) frame)
    (sameActivation : frameCallerActivation base.activation frame = base.activation)
    (included : ∀ binder ∈ nodeDeclaredBinders inner, binder ∈ nodeDeclaredBinders outer)
    (reservations : pendingMachineBinders { base with control := .expression inner, stack := frame :: base.stack } =
      pendingMachineBinders { base with control := .expression outer })
    (typed : TypedMachine globals storage types { base with control := .expression outer }) :
    TypedMachine globals storage types { base with control := .expression inner, stack := frame :: base.stack } := by
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | expression ground closed fresh =>
    rename_i context ty runtimeType
    have schemas : SchemaMatches context.scope context.solution base.schemas := by
      simpa [ActiveScopeTyped, TypedDemand.context] using active.2.2.1
    obtain ⟨innerType, innerGround, frameTyped⟩ := decompose context ty ground schemas
    have newPending : PendingBindingsSafe { base with control := .expression inner, stack := frame :: base.stack } := by
      constructor
      · rw [reservations]
        exact pending.unique
      · intro binding member
        apply pending.fresh binding
        rw [← reservations]
        exact member
    have oldStack : StackTyped { base with control := .expression inner, stack := frame :: base.stack }
        (.expression context ty) output base.stack := by
      apply stack_typing_survives_pending step stack
      intro binding member
      apply newPending.fresh binding
      simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
      right
      right
      simpa [activation, sameActivation] using member
    have innerFresh : FutureNodeBindingsFresh base.store context.activation inner := by
      intro binder member
      exact fresh binder (included binder member)
    obtain ⟨innerRuntime, innerClosed⟩ := ground_node_is_closed innerGround
    refine ⟨.expression context innerType, output, runtimeSolution, activation, storageEqual,
      proofSorted, runtimeSorted, ?_, ⟨innerRuntime, innerClosed⟩, outputClosed,
      .expression innerGround innerClosed innerFresh, .cons frameTyped inputClosed oldStack, newPending⟩
    simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_propagation_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Node) (error : MonoType)
    (typed : TypedMachine globals storage types { base with control := .expression (.propagate value error) }) :
    TypedMachine globals storage types
      { base with control := .expression value, stack := .propagate :: base.stack } := by
  apply typed_machine_expression_push globals storage types base (.propagate value error) value
    .propagate .internal (Step.propagateStart base value error) ?_ rfl ?_ ?_ typed
  · intro context ty ground _schemas
    obtain ⟨failure, inner, frame⟩ := ground_propagation_opens_frame ground
    exact ⟨_, inner, frame⟩
  · intro binder member
    simpa [nodeDeclaredBinders] using member
  · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      nodeDeclaredBinders, frameCallerActivation]

theorem typed_machine_binary_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (operator : Xsh.Syntax.BinaryOp) (left right : Node)
    (typed : TypedMachine globals storage types { base with control := .expression (.binary operator left right) }) :
    TypedMachine globals storage types
      { base with control := .expression left, stack := .binaryLeft operator right :: base.stack } := by
  apply typed_machine_expression_push globals storage types base (.binary operator left right) left
    (.binaryLeft operator right) .internal (Step.binary base operator left right) ?_ rfl ?_ ?_ typed
  · intro context ty ground _schemas
    cases ground with
    | binary leftTyped rightTyped operation closed => exact ⟨_, leftTyped, .binaryLeft rightTyped operation⟩
  · intro binder member
    simp [nodeDeclaredBinders, member]
  · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      nodeDeclaredBinders, frameCallerActivation, List.map_append, List.append_assoc]

theorem typed_machine_field_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (value : Node) (name : NameId)
    (typed : TypedMachine globals storage types { base with control := .expression (.field value name) }) :
    TypedMachine globals storage types
      { base with control := .expression value, stack := .field name :: base.stack } := by
  apply typed_machine_expression_push globals storage types base (.field value name) value
    (.field name) .internal (Step.fieldStart base value name) ?_ rfl ?_ ?_ typed
  · intro context ty ground _schemas
    cases ground with
    | field inner occurs closed => exact ⟨_, inner, .field occurs⟩
  · intro binder member
    simpa [nodeDeclaredBinders] using member
  · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      nodeDeclaredBinders, frameCallerActivation]

theorem typed_machine_call_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (callee : Node) (arguments : List Node)
    (typed : TypedMachine globals storage types { base with control := .expression (.call callee arguments) }) :
    TypedMachine globals storage types
      { base with control := .expression callee, stack := .callTarget arguments :: base.stack } := by
  apply typed_machine_expression_push globals storage types base (.call callee arguments) callee
    (.callTarget arguments) .internal (Step.callStart base callee arguments) ?_ rfl ?_ ?_ typed
  · intro context ty ground _schemas
    cases ground with
    | call target arguments closed => exact ⟨_, target, .callTarget arguments⟩
  · intro binder member
    simp [nodeDeclaredBinders, member]
  · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      nodeDeclaredBinders, frameCallerActivation, List.map_append, List.append_assoc]

theorem typed_machine_conditional_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (condition branch : Node) (rest : List (Node × Node)) (otherwise : Node)
    (typed : TypedMachine globals storage types
      { base with control := .expression (.conditional ((condition, branch) :: rest) otherwise) }) :
    TypedMachine globals storage types
      { base with control := .expression condition, stack := .conditional ((condition, branch) :: rest) otherwise :: base.stack } := by
  apply typed_machine_expression_push globals storage types base (.conditional ((condition, branch) :: rest) otherwise)
    condition (.conditional ((condition, branch) :: rest) otherwise) .internal
    (Step.conditionalStart base condition branch rest otherwise) ?_ rfl ?_ ?_ typed
  · intro context ty ground _schemas
    cases ground with
    | conditional branches fallback closed =>
      cases branches with
      | cons conditionTyped branchTyped restTyped =>
        exact ⟨_, conditionTyped, .conditional (by simp) (.cons conditionTyped branchTyped restTyped) fallback⟩
  · intro binder member
    simp [nodeDeclaredBinders, expressionBranchBinders, member]
  · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      nodeDeclaredBinders, expressionBranchBinders, frameCallerActivation, List.map_append, List.append_assoc]

theorem typed_machine_record_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (name : NameId) (value : Node) (rest : List (NameId × Node))
    (typed : TypedMachine globals storage types { base with control := .expression (.record ((name, value) :: rest)) }) :
    TypedMachine globals storage types
      { base with control := .expression value, stack := .recordFields [] name rest :: base.stack } := by
  apply typed_machine_expression_push globals storage types base (.record ((name, value) :: rest)) value
    (.recordFields [] name rest) .internal (Step.recordStart base name value rest) ?_ rfl ?_ ?_ typed
  · intro context ty ground _schemas
    cases ground with
    | record fields unique closed =>
      cases fields with
      | cons valueTyped restTyped =>
        exact ⟨_, valueTyped, .recordFields .nil restTyped (by simpa using unique)⟩
  · intro binder member
    simp [nodeDeclaredBinders, fieldBinders, member]
  · simp [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
      nodeDeclaredBinders, fieldBinders, frameCallerActivation, List.map_append, List.append_assoc]

end Xsh.Semantics.Connected
