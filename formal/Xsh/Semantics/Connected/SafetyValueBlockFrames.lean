import Xsh.Semantics.Connected.SafetyValueBlockWorld
import Xsh.Semantics.Connected.SafetyResult

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected
open Xsh.Semantics.Environments

theorem record_value_shape (value : Value) (row : RuntimeType)
    (typed : valueType value = some (.record row)) : ∃ fields, value = .record fields := by
  cases value with
  | record fields => exact ⟨fields, rfl⟩
  | scalar scalar => cases scalar <;> simp [valueType] at typed
  | ok value failure | error value failure => cases inner : valueType value <;> simp [valueType, inner] at typed
  | nominal identity base value => cases inner : valueType value <;> simp [valueType, inner] at typed
  | _ => simp [valueType] at typed

theorem erase_nominal_retains_a_type (value : Value) (ty : RuntimeType)
    (typed : valueType value = some ty) : ∃ erasedType, valueType (eraseNominal value) = some erasedType := by
  cases value with
  | nominal identity base inner =>
    cases found : valueType inner with
    | none => simp [valueType, found] at typed
    | some innerType =>
      exact erase_nominal_retains_a_type inner innerType found
  | _ => exact ⟨ty, typed⟩
termination_by value

theorem typed_guard_decision_exists (guard : Guard) (value : Value) (ty : RuntimeType)
    (typed : valueType value = some ty) : ∃ decision, guardDecision guard value = some decision := by
  obtain ⟨erasedType, erasedTyped⟩ := erase_nominal_retains_a_type value ty typed
  cases guard with
  | integerRange minimum maximum =>
    cases erased : eraseNominal value with
    | scalar scalar =>
      cases scalar with
      | integer integer =>
        exact ⟨decide (minimum.val ≤ integer.val ∧ integer.val ≤ maximum.val), by simp [guardDecision, erased, guardPlain]⟩
      | _ => exact ⟨false, by simp [guardDecision, erased, guardPlain]⟩
    | _ => exact ⟨false, by simp [guardDecision, erased, guardPlain]⟩
  | recordShape row =>
    cases erased : eraseNominal value with
    | record fields =>
      rw [erased] at erasedTyped
      exact ⟨typeFits (.record row) erasedType, by simp [guardDecision, erased, guardPlain, erasedTyped]⟩
    | _ => exact ⟨false, by simp [guardDecision, erased, guardPlain]⟩

def PermittedControl : Control → Prop
  | .fault (.integer _) => True
  | .fault _ => False
  | _ => True

theorem typed_value_frame_progress (state : Machine) (context : TypingContext) (input : MonoType)
    (value : Value) (runtimeType : RuntimeType) (frame : Frame) (rest : List Frame) (output : TypedDemand)
    (typed : valueType value = some runtimeType) (closed : monoRuntime input = some runtimeType)
    (scope : ScopeStateAgrees context.scope { state with solution := context.solution })
    (activation : context.activation = state.activation)
    (frameTyped : FrameTyped state (.expression context input) output frame) (outputClosed : DemandClosed output) :
    ∃ next, Step { state with control := .value value, stack := frame :: rest } .internal next ∧ PermittedControl next.control := by
  let base : Machine := { state with stack := rest }
  cases frame with
  | binaryLeft operator right =>
    cases frameTyped with
    | binaryLeft rightTyped operation =>
      by_cases andOperator : operator = .and
      · subst operator
        cases operation
        have booleanType : valueType value = some .boolean := by simpa [monoRuntime] using typed.trans closed.symm
        obtain ⟨boolean, rfl⟩ := boolean_value_shape value booleanType
        cases boolean with
        | false => exact ⟨_, by simpa [base] using Step.andFalse base right, by trivial⟩
        | true => exact ⟨_, by simpa [base] using Step.andTrue base right, by trivial⟩
      · by_cases orOperator : operator = .or
        · subst operator
          cases operation
          have booleanType : valueType value = some .boolean := by simpa [monoRuntime] using typed.trans closed.symm
          obtain ⟨boolean, rfl⟩ := boolean_value_shape value booleanType
          cases boolean with
          | false => exact ⟨_, by simpa [base] using Step.orFalse base right, by trivial⟩
          | true => exact ⟨_, by simpa [base] using Step.orTrue base right, by trivial⟩
        · exact ⟨_, by simpa [base] using Step.binaryLeft base operator value right ⟨andOperator, orOperator⟩, by trivial⟩
  | binaryRight operator left =>
    cases frameTyped with
    | binaryRight leftTyped leftClosed callable operation =>
      obtain ⟨resultRuntime, control, resultClosed, step, safe⟩ := typed_binary_frame_progress base operator
        _ _ _ left value operation (leftTyped.trans leftClosed.symm) (typed.trans closed.symm)
      refine ⟨_, by simpa [base] using step, ?_⟩
      rcases safe with ⟨candidate, rfl, candidateTyped⟩ | ⟨fault, rfl⟩ <;> trivial
  | callTarget arguments =>
    cases frameTyped with
    | callTarget argumentsTyped =>
      cases arguments with
      | nil => exact ⟨_, by simpa [base] using Step.callWithoutArguments base value, by trivial⟩
      | cons first tail => exact ⟨_, by simpa [base] using Step.callFirstArgument base value first tail, by trivial⟩
  | callArguments target done remainingNodes =>
    cases frameTyped with
    | callArguments doneTyped remaining targetTyped callable =>
      cases remainingNodes with
      | nil => exact ⟨_, by simpa [base] using Step.callArgumentsComplete base target value done, by trivial⟩
      | cons next tail => exact ⟨_, by simpa [base] using Step.callNextArgument base target value done next tail, by trivial⟩
  | recordFields done name remainingFields =>
    cases frameTyped with
    | recordFields doneTyped remaining unique =>
      cases remainingFields with
      | nil => exact ⟨_, by simpa [base] using Step.recordFinish base done name value, by trivial⟩
      | cons next tail =>
        obtain ⟨nextName, nextNode⟩ := next
        exact ⟨_, by simpa [base] using Step.recordNext base done name nextName value nextNode tail, by trivial⟩
  | field name =>
    cases frameTyped with
    | field occurs =>
      rename_i row result
      obtain ⟨runtimeResult, resultClosed⟩ := outputClosed
      cases rowClosed : monoRuntime row with
      | none => simp [monoRuntime, rowClosed] at closed
      | some runtimeRow =>
        simp [monoRuntime, rowClosed] at closed
        subst runtimeType
        obtain ⟨fields, rfl⟩ := record_value_shape value runtimeRow typed
        obtain ⟨selected, step, selectedTyped⟩ := typed_field_frame_progress base fields name row result runtimeRow runtimeResult
          typed occurs rowClosed resultClosed
        exact ⟨_, by simpa [base] using step, by trivial⟩
  | conditional branches otherwise =>
    cases frameTyped with
    | conditional nonempty branchTypes fallback =>
      have booleanType : valueType value = some .boolean := by simpa [monoRuntime] using typed.trans closed.symm
      obtain ⟨boolean, rfl⟩ := boolean_value_shape value booleanType
      cases branches with
      | nil => exact False.elim (nonempty rfl)
      | cons branch tail =>
        obtain ⟨condition, selected⟩ := branch
        cases boolean with
        | false => exact ⟨_, by simpa [base] using Step.conditionalFalse base condition selected tail otherwise, by trivial⟩
        | true => exact ⟨_, by simpa [base] using Step.conditionalTrue base condition selected tail otherwise, by trivial⟩
  | propagate =>
    cases frameTyped with
    | propagate expected =>
      rename_i success failure result
      cases successClosed : monoRuntime success with
      | none => simp [monoRuntime, successClosed] at closed
      | some runtimeSuccess =>
        cases failureClosed : monoRuntime failure with
        | none => simp [monoRuntime, successClosed, failureClosed] at closed
        | some runtimeFailure =>
          simp [monoRuntime, successClosed, failureClosed] at closed
          subst runtimeType
          rcases result_value_shape value runtimeSuccess runtimeFailure typed with ⟨inner, rfl, innerTyped⟩ | ⟨inner, rfl, innerTyped⟩
          · exact ⟨_, by simpa [base] using Step.propagateOk base inner runtimeFailure, by trivial⟩
          · exact ⟨_, by simpa [base] using Step.propagateError base inner runtimeSuccess, by trivial⟩
  | requireGuard identity guard =>
    cases frameTyped with
    | requireGuard baseMatches =>
      obtain ⟨decision, decided⟩ := typed_guard_decision_exists guard value runtimeType typed
      cases decision with
      | false => exact ⟨_, by simpa [base] using Step.requireFailure base identity guard value decided, by trivial⟩
      | true => exact ⟨_, by simpa [base] using Step.requireSuccess base identity guard value decided, by trivial⟩
  | bind binder kind following =>
    cases frameTyped with
    | bind kindValid ordinaryBinding freshScope freshCell restTyped freshRest =>
      rw [activation] at freshCell
      obtain ⟨next, entry, allocated, present, mutability, tag, contents⟩ :=
        allocate_fresh_typed_cell state.store ⟨state.activation, binder⟩
          (if kind = .mutable then .mutable else .immutable) value runtimeType freshCell typed
      exact ⟨_, by simpa [base] using Step.bindingCommit base binder kind value following next allocated, by trivial⟩
  | assign binder following =>
    cases frameTyped with
    | assign found mutable restTyped freshRest =>
      rename_i name binding
      obtain ⟨unique, values, mutableCells, distinct, live⟩ := scope
      obtain ⟨quantified, requirements, cell, current, actualType, active, present, writable, tag, actualClosed⟩ :=
        mutableCells name binding found mutable
      rw [closed] at actualClosed
      cases actualClosed
      have member : HasType current.contents.typeTag value := by
        unfold HasType
        rw [tag]
        exact typed
      obtain ⟨next, written⟩ := (write_accepted_iff (fun _ _ => inferInstance) state.store cell value).mpr
        ⟨current, present, writable, member⟩
      exact ⟨_, by simpa [base] using Step.assignmentCommit base binding.binder value following cell next active written, by trivial⟩
  | discard following =>
    cases frameTyped with
    | discard typedRest freshRest => exact ⟨_, by simpa [base] using Step.discarded base value following, by trivial⟩
  | returnValue =>
    cases frameTyped with
    | returnValue => exact ⟨_, by simpa [base] using Step.returnValue base value, by trivial⟩
  | tailValue =>
    cases frameTyped with
    | tailValue accepts => exact ⟨_, by simpa [base] using Step.tailValue base value, by trivial⟩
  | branch branches otherwise following =>
    cases frameTyped with
    | branch nonempty branchTypes fallback restTyped freshBranches freshFallback freshRest =>
      have booleanType : valueType value = some .boolean := by simpa [monoRuntime] using typed.trans closed.symm
      obtain ⟨boolean, rfl⟩ := boolean_value_shape value booleanType
      cases branches with
      | nil => exact False.elim (nonempty rfl)
      | cons branch tail =>
        obtain ⟨condition, body⟩ := branch
        cases boolean with
        | false => exact ⟨_, by simpa [base] using Step.statementConditionalFalse base condition body tail otherwise following, by trivial⟩
        | true => exact ⟨_, by simpa [base] using Step.statementConditionalTrue base condition body tail otherwise following, by trivial⟩
  | continueBlock => cases frameTyped
  | valueBlock => cases frameTyped
  | caller => cases frameTyped

theorem typed_value_stack_progress (state : Machine) (context : TypingContext) (ty : MonoType)
    (value : Value) (runtimeType : RuntimeType) (stack : List Frame) (output : TypedDemand)
    (typed : valueType value = some runtimeType) (closed : monoRuntime ty = some runtimeType)
    (scope : ScopeStateAgrees context.scope { state with solution := context.solution })
    (activation : context.activation = state.activation)
    (typedStack : StackTyped state (.expression context ty) output stack) :
    stack = [] ∨ ∃ next, Step { state with control := .value value, stack := stack } .internal next ∧ PermittedControl next.control := by
  cases typedStack with
  | nil => exact Or.inl rfl
  | @cons input middle output frame rest typedFrame middleClosed remaining =>
    exact Or.inr (typed_value_frame_progress state context ty value runtimeType frame rest middle
      typed closed scope activation typedFrame middleClosed)

theorem typed_machine_value_progress (state : Machine) (globals : Scope) (storage : BindingStorageMap)
    (types : CellTypes) (value : Value) (invariant : TypedMachine globals storage types state)
    (current : state.control = .value value) :
    state.stack = [] ∨ ∃ next, Step state .internal next ∧ PermittedControl next.control := by
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := invariant.focus
  rw [current] at control
  cases control with
  | value typed closed callables =>
    rename_i context ty runtimeType
    have activeScope : ScopeStateAgrees context.scope { state with solution := context.solution } ∧
        SchemaMatches context.scope context.solution state.schemas ∧
        SchemeCellsImmutable state.cells state.store ∧ StorageAgreement state.cells state.store context.storage := by
      simpa [ActiveScopeTyped, current, TypedDemand.context] using active
    have sameActivation : context.activation = state.activation := by simpa [TypedDemand.context] using activation
    rcases typed_value_stack_progress state context ty value runtimeType state.stack output typed closed activeScope.1
      sameActivation stack with empty | ⟨next, step, safe⟩
    · exact Or.inl empty
    · have sameState : { state with control := .value value, stack := state.stack } = state := by
        cases state
        simp_all
      rw [sameState] at step
      exact Or.inr ⟨next, step, safe⟩

end Xsh.Semantics.Connected
