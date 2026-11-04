import Xsh.Semantics.Connected.SafetyValueBlockProjection

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected
open Xsh.Syntax.Foundation

theorem read_preserves_callable_provenance (functions : FunctionTable) (store : Store) (cells : ActiveCells)
    (safe : StoreCallables functions store) (binder : Xsh.Foundation.BinderId) (value : Value)
    (found : Connected.read store cells binder = some value) : ValueCallables functions value := by
  unfold Connected.read at found
  cases active : cells binder with
  | none => simp [active] at found
  | some cell =>
    cases present : store cell with
    | none => simp [active, present] at found
    | some entry =>
      simp [active, present] at found
      subst value
      exact safe cell entry present

theorem instantiated_scheme_is_callable (functions : FunctionTable) (declaration : DeclarationId)
    (scheme : Scheme) (captured : Substitution) (arguments : List (Variable × MonoType))
    (parameters result : RuntimeType) (normalized : List (Variable × MonoType))
    (safe : ValueCallables functions (.schemeFunction declaration scheme)) (sorted : WellSorted captured)
    (instantiated : instantiateFunction captured scheme arguments = some (parameters, result, normalized)) :
    ValueCallables functions (.instantiatedFunction declaration parameters result captured arguments) := by
  cases safe with
  | scheme correct =>
    rcases correct with ⟨function, found, sameScheme⟩ | ⟨found, shape⟩ | ⟨found, shape⟩ | ⟨found, shape⟩
    · cases sameScheme
      exact .instantiatedFunction found sorted instantiated
    · obtain ⟨success, failure, parametersSame, resultSame⟩ := shape captured arguments parameters result normalized instantiated
      subst parameters
      subst result
      exact .instantiatedOk found
    · obtain ⟨success, failure, parametersSame, resultSame⟩ := shape captured arguments parameters result normalized instantiated
      subst parameters
      subst result
      exact .instantiatedError found
    · obtain ⟨parametersSame, resultSame⟩ := shape captured arguments parameters result normalized instantiated
      subst parameters
      subst result
      exact .instantiatedTime found

theorem typed_machine_read (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (binder : Xsh.Foundation.BinderId) (arguments : List (Variable × MonoType)) (value : Value)
    (found : Connected.read base.store base.cells binder = some value) (ordinary : ordinaryValue value = true)
    (typed : TypedMachine globals storage types { base with control := .expression (.read binder arguments) }) :
    TypedMachine globals storage types { base with control := .value value } := by
  have step := Step.read base binder arguments value found ordinary
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | expression ground closed fresh =>
    rename_i context ty runtimeType
    have valueTyped : HasType runtimeType value := by
      cases ground with
      | read nodeTyped groundClosed contract =>
        cases nodeTyped with
        | read lookup notSchema keys sorts instantiated solved wellFormed sort =>
          rename_i name binding raw requirements
          obtain ⟨scope, writable, schemas, schemes, storageAgrees⟩ := active
          obtain ⟨other, readOther, agrees⟩ := scope.2.1 name binding lookup notSchema
          rw [found] at readOther
          cases readOther
          rcases agrees with ⟨declaration, parameters, result, same, body⟩ | ordinaryAgrees
          · cases same
            simp [ordinaryValue] at ordinary
          · exact ordinaryAgrees.2 arguments raw requirements runtimeType instantiated solved closed
    have callable := read_preserves_callable_provenance base.functions base.store base.cells typed.storedValues binder value found
    have newPending : PendingBindingsSafe { base with control := .value value } := by
      constructor
      · simpa [pendingMachineBinders, pendingControlBinders, nodeDeclaredBinders] using pending.unique
      · simpa [pendingMachineBinders, pendingControlBinders, nodeDeclaredBinders] using pending.fresh
    have oldStack : StackTyped { base with control := .value value } (.expression context ty) output base.stack := by
      apply stack_typing_survives_with_pending step stack newPending
      · simpa [TypedDemand.context] using activation.symm
      · rfl
    refine ⟨.expression context ty, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
      ?_, inputClosed, outputClosed, .value valueTyped closed callable, oldStack, newPending⟩
    simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
