import Xsh.Semantics.Connected.SafetyValueBlockCallArguments

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem typed_machine_empty_record (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (typed : TypedMachine globals storage types { base with control := .expression (.record []) }) :
    TypedMachine globals storage types { base with control := .value (.record []) } := by
  apply typed_machine_expression_value globals storage types base (.record []) (.record []) .internal (Step.recordEmpty base) ?_ typed
  intro context ty runtimeType ground closed
  cases ground with
  | record fields unique recordClosed =>
    cases fields with
    | nil =>
      simp [Xsh.Typing.Connected.canonicalFields, monoRuntime] at closed
      subst runtimeType
      exact ⟨rfl, .record .nil⟩

theorem typed_fields_append (functions : FunctionTable) (values : List (NameId × Value)) (types : List (NameId × MonoType))
    (typed : TypedFields functions values types) (name : NameId) (value : Value) (ty : MonoType) (runtimeType : RuntimeType)
    (valueTyped : HasType runtimeType value) (closed : monoRuntime ty = some runtimeType)
    (callable : ValueCallables functions value) :
    TypedFields functions (values ++ [(name, value)]) (types ++ [(name, ty)]) := by
  induction typed with
  | nil => exact .cons valueTyped closed callable .nil
  | cons headTyped headClosed headCallable tail ih => exact .cons headTyped headClosed headCallable ih

theorem typed_machine_record_next (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (done : List (NameId × Value)) (name nextName : NameId)
    (value : Value) (next : Node) (rest : List (NameId × Node))
    (typed : TypedMachine globals storage types
      { base with control := .value value, stack := .recordFields done name ((nextName, next) :: rest) :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .expression next, stack := .recordFields (done ++ [(name, value)]) nextName rest :: base.stack } := by
  have step := Step.recordNext base done name nextName value next rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed callable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | recordFields doneTyped nextFields unique =>
        rename_i context inputType runtimeType doneTypes restTypes
        simp only [TypedDemand.context] at activation
        cases nextFields with
        | cons nextTyped restTyped =>
          rename_i nextType afterTypes
          have newPending : PendingBindingsSafe
              { base with control := .expression next, stack := .recordFields (done ++ [(name, value)]) nextName rest :: base.stack } := by
            constructor
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation, fieldBinders, List.map_append, List.append_assoc] using pending.unique
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation, fieldBinders, List.map_append, List.append_assoc] using pending.fresh
          have nextFresh : FutureNodeBindingsFresh base.store context.activation next := by
            intro binder member
            apply newPending.fresh (context.activation, binder)
            simp [pendingMachineBinders, pendingControlBinders, activation, member]
          have oldStack : StackTyped
              { base with control := .expression next, stack := .recordFields (done ++ [(name, value)]) nextName rest :: base.stack }
              (.expression context (.record ((Xsh.Typing.Connected.canonicalFields (doneTypes ++ [(name, inputType)] ++ (nextName, nextType) :: afterTypes)).foldr
                (fun (name, ty) row => .rowField name ty row) .rowEmpty))) output base.stack := by
            apply stack_typing_survives_pending step remaining
            intro binding member
            apply newPending.fresh binding
            simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
            right
            right
            simpa [TypedDemand.context, activation, frameCallerActivation] using member
          have frame : FrameTyped
              { base with control := .expression next, stack := .recordFields (done ++ [(name, value)]) nextName rest :: base.stack }
              (.expression context nextType)
              (.expression context (.record ((Xsh.Typing.Connected.canonicalFields (doneTypes ++ [(name, inputType)] ++ (nextName, nextType) :: afterTypes)).foldr
                (fun (name, ty) row => .rowField name ty row) .rowEmpty)))
              (.recordFields (done ++ [(name, value)]) nextName rest) := by
            have frame' : FrameTyped
                { base with control := .expression next, stack := .recordFields (done ++ [(name, value)]) nextName rest :: base.stack }
                (.expression context nextType)
                (.expression context (.record ((Xsh.Typing.Connected.canonicalFields ((doneTypes ++ [(name, inputType)]) ++ [(nextName, nextType)] ++ afterTypes)).foldr
                  (fun (name, ty) row => .rowField name ty row) .rowEmpty)))
                (.recordFields (done ++ [(name, value)]) nextName rest) := by
              exact .recordFields (typed_fields_append _ _ _ doneTyped name value inputType runtimeType valueTyped valueClosed callable)
                restTyped (by simpa [List.append_assoc] using unique)
            simpa [List.append_assoc] using frame'
          obtain ⟨nextRuntime, nextClosed⟩ := ground_node_is_closed nextTyped
          refine ⟨.expression context nextType, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, ⟨nextRuntime, nextClosed⟩, outputClosed, .expression nextTyped nextClosed nextFresh,
            .cons frame middleClosed oldStack, newPending⟩
          simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
