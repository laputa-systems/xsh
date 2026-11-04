import Xsh.Semantics.Connected.SafetyValueBlockRecordFinish
import Xsh.Semantics.Connected.SafetyValueBlockCompatibility

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem field_lookup_preserves_callables (functions : FunctionTable) (fields : List (NameId × Value))
    (safe : FieldCallables functions fields) (name : NameId) (value : Value)
    (found : fields.lookup name = some value) : ValueCallables functions value := by
  induction fields with
  | nil => simp at found
  | cons field rest ih =>
    obtain ⟨fieldName, fieldValue⟩ := field
    cases safe with
    | cons callable restSafe =>
      by_cases same : name = fieldName
      · subst name
        simp at found
        cases found
        exact callable
      · simp only [List.lookup_cons] at found
        have unequal : (name == fieldName) = false := by
          change decide (name = fieldName) = false
          simp [same]
        rw [unequal] at found
        exact ih restSafe found

theorem compatible_record_field_is_callable (functions : FunctionTable) (fields : List (NameId × Value))
    (safe : ValueCallables functions (.record fields)) (name : NameId) (value : Value)
    (found : fields.lookup name = some value) : ValueCallables functions value := by
  cases safe with
  | record fieldSafe => exact field_lookup_preserves_callables functions fields fieldSafe name value found

theorem typed_machine_field_value (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (name : NameId) (fields : List (NameId × Value)) (value : Value)
    (found : fields.lookup name = some value)
    (typed : TypedMachine globals storage types
      { base with control := .value (.record fields), stack := .field name :: base.stack }) :
    TypedMachine globals storage types { base with control := .value value } := by
  have step := Step.fieldValue base name fields value found
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value recordTyped recordClosed recordCallable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | field occurs =>
        rename_i context recordRuntime row result
        obtain ⟨fieldRuntime, fieldClosed⟩ := middleClosed
        cases rowClosed : monoRuntime row with
        | none => simp [monoRuntime, rowClosed] at recordClosed
        | some runtimeRow =>
          simp [monoRuntime, rowClosed] at recordClosed
          subst recordRuntime
          obtain ⟨actual, actualFound, actualTyped⟩ := compatible_field_lookup fields name row result
            runtimeRow fieldRuntime recordTyped occurs rowClosed fieldClosed
          rw [found] at actualFound
          cases actualFound
          have callable := compatible_record_field_is_callable base.functions fields recordCallable name value found
          have newPending : PendingBindingsSafe { base with control := .value value } := by
            constructor
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation] using pending.unique
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation] using pending.fresh
          have oldStack : StackTyped { base with control := .value value } (.expression context result) output base.stack := by
            apply stack_typing_survives_with_pending step remaining newPending
            · simpa [TypedDemand.context] using activation.symm
            · rfl
          refine ⟨.expression context result, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, ⟨fieldRuntime, fieldClosed⟩, outputClosed, .value actualTyped fieldClosed callable, oldStack, newPending⟩
          simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
