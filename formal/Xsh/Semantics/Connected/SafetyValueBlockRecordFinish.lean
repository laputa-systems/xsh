import Xsh.Semantics.Connected.SafetyValueBlockRecord

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

def runtimeFields : List (NameId × MonoType) → Option (List (NameId × RuntimeType))
  | [] => some []
  | (name, ty) :: rest => do
      let runtimeType ← monoRuntime ty
      let tail ← runtimeFields rest
      some ((name, runtimeType) :: tail)

theorem runtime_fields_insert (name : NameId) (ty : MonoType) (runtimeType : RuntimeType)
    (fields : List (NameId × MonoType)) (runtime : List (NameId × RuntimeType))
    (closed : monoRuntime ty = some runtimeType) (converted : runtimeFields fields = some runtime) :
    runtimeFields (Xsh.Typing.Connected.insertField (name, ty) fields) =
      some (insertField (name, runtimeType) runtime) := by
  induction fields generalizing runtime with
  | nil =>
    simp [runtimeFields] at converted
    subst runtime
    simp [Xsh.Typing.Connected.insertField, insertField, runtimeFields, closed]
  | cons field rest ih =>
    obtain ⟨other, head⟩ := field
    cases headClosed : monoRuntime head with
    | none => simp [runtimeFields, headClosed] at converted
    | some headRuntime =>
      cases tailClosed : runtimeFields rest with
      | none => simp [runtimeFields, headClosed, tailClosed] at converted
      | some tailRuntime =>
        simp [runtimeFields, headClosed, tailClosed] at converted
        subst runtime
        by_cases order : name.value ≤ other.value
        · simp [Xsh.Typing.Connected.insertField, insertField, order, runtimeFields, closed, headClosed, tailClosed]
        · have recursive := ih tailRuntime tailClosed
          simp [Xsh.Typing.Connected.insertField, insertField, order, runtimeFields, headClosed, recursive]

theorem runtime_fields_canonical (fields : List (NameId × MonoType)) (runtime : List (NameId × RuntimeType))
    (converted : runtimeFields fields = some runtime) :
    runtimeFields (Xsh.Typing.Connected.canonicalFields fields) = some (canonicalFields runtime) := by
  induction fields generalizing runtime with
  | nil => simp [runtimeFields] at converted; subst runtime; rfl
  | cons field rest ih =>
    obtain ⟨name, ty⟩ := field
    cases closed : monoRuntime ty with
    | none => simp [runtimeFields, closed] at converted
    | some runtimeType =>
      cases tail : runtimeFields rest with
      | none => simp [runtimeFields, closed, tail] at converted
      | some runtimeRest =>
        simp [runtimeFields, closed, tail] at converted
        subst runtime
        simp only [Xsh.Typing.Connected.canonicalFields, canonicalFields]
        exact runtime_fields_insert name ty runtimeType _ _ closed (ih runtimeRest tail)

theorem runtime_fields_row (fields : List (NameId × MonoType)) (runtime : List (NameId × RuntimeType))
    (converted : runtimeFields fields = some runtime) :
    monoRuntime (fields.foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty) =
      some (runtime.foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty) := by
  induction fields generalizing runtime with
  | nil => simp [runtimeFields] at converted; subst runtime; rfl
  | cons field rest ih =>
    obtain ⟨name, ty⟩ := field
    cases closed : monoRuntime ty with
    | none => simp [runtimeFields, closed] at converted
    | some runtimeType =>
      cases tail : runtimeFields rest with
      | none => simp [runtimeFields, closed, tail] at converted
      | some runtimeRest =>
        simp [runtimeFields, closed, tail] at converted
        subst runtime
        simp [monoRuntime, closed, ih runtimeRest tail]

theorem typed_fields_runtime (functions : FunctionTable) (values : List (NameId × Value)) (types : List (NameId × MonoType))
    (typed : TypedFields functions values types) :
    ∃ runtime, recordTypes values = some runtime ∧ runtimeFields types = some runtime ∧
      FieldCallables functions values ∧ values.map Prod.fst = types.map Prod.fst := by
  induction typed with
  | nil => exact ⟨[], rfl, rfl, .nil, rfl⟩
  | @cons name value values ty types runtimeType valueTyped closed callable tail ih =>
    obtain ⟨runtime, valuesConverted, typesConverted, fieldsCallable, names⟩ := ih
    refine ⟨(name, runtimeType) :: runtime, ?_, ?_, .cons callable fieldsCallable, ?_⟩
    · simp [recordTypes, valueTyped, valuesConverted]
    · simp [runtimeFields, closed, typesConverted]
    · simpa using congrArg (List.cons name) names

theorem typed_fields_record (functions : FunctionTable) (values : List (NameId × Value)) (types : List (NameId × MonoType))
    (typed : TypedFields functions values types) (unique : (types.map Prod.fst).Nodup) :
    ∃ runtimeType,
      monoRuntime (.record ((Xsh.Typing.Connected.canonicalFields types).foldr
        (fun (name, ty) row => .rowField name ty row) .rowEmpty)) = some runtimeType ∧
      valueType (.record values) = some runtimeType ∧ ValueCallables functions (.record values) := by
  obtain ⟨runtime, valuesConverted, typesConverted, fieldsCallable, names⟩ := typed_fields_runtime functions values types typed
  have uniqueValues : (values.map Prod.fst).Nodup := by rw [names]; exact unique
  have rowClosed := runtime_fields_row _ _ (runtime_fields_canonical _ _ typesConverted)
  refine ⟨.record ((canonicalFields runtime).foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty), ?_, ?_, .record fieldsCallable⟩
  · simp [monoRuntime, rowClosed]
  · simp [valueType, uniqueValues, valuesConverted]

theorem typed_machine_record_finish (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (done : List (NameId × Value)) (name : NameId) (value : Value)
    (typed : TypedMachine globals storage types { base with control := .value value, stack := .recordFields done name [] :: base.stack }) :
    TypedMachine globals storage types { base with control := .value (.record (done ++ [(name, value)])) } := by
  have step := Step.recordFinish base done name value
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
      | recordFields doneTyped restTyped unique =>
        rename_i context inputType runtimeType doneTypes restTypes
        cases restTyped with
        | nil =>
          simp only [List.append_nil] at remaining middleClosed
          have fieldsTyped := typed_fields_append _ _ _ doneTyped name value inputType runtimeType valueTyped valueClosed callable
          obtain ⟨recordRuntime, recordClosed, recordTyped, recordCallable⟩ :=
            typed_fields_record base.functions (done ++ [(name, value)]) (doneTypes ++ [(name, inputType)]) fieldsTyped (by simpa using unique)
          have newPending : PendingBindingsSafe { base with control := .value (.record (done ++ [(name, value)])) } := by
            constructor
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation, fieldBinders] using pending.unique
            · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
                frameCallerActivation, fieldBinders] using pending.fresh
          have oldStack : StackTyped { base with control := .value (.record (done ++ [(name, value)])) }
              (.expression context (.record ((Xsh.Typing.Connected.canonicalFields (doneTypes ++ [(name, inputType)])).foldr
                (fun (name, ty) row => .rowField name ty row) .rowEmpty))) output base.stack := by
            apply stack_typing_survives_with_pending step remaining newPending
            · simpa [TypedDemand.context] using activation.symm
            · rfl
          refine ⟨.expression context (.record ((Xsh.Typing.Connected.canonicalFields (doneTypes ++ [(name, inputType)])).foldr
              (fun (name, ty) row => .rowField name ty row) .rowEmpty)), output,
            runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, middleClosed, outputClosed, .value recordTyped recordClosed recordCallable, oldStack, newPending⟩
          simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
