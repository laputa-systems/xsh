import Xsh.Semantics.Connected.SafetyValueBlockInvariant

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem ground_monomorphic_read (scope : Scope) (solution runtime : Substitution)
    (storage : BindingStorageMap) (functionResult : MonoType) (name : NameId) (binder : BinderId)
    (kind : BindingKind) (raw ty : MonoType) (tag : RuntimeType)
    (typed : NodeHasTypeIn solution (some functionResult) scope (.read binder []) ty)
    (unique : (scope.map Binding.binder).Nodup)
    (member : (⟨name, binder, kind, ⟨[], raw, []⟩⟩ : Binding) ∈ scope)
    (closed : monoRuntime (substitute solution raw) = some tag)
    (ordinary : storage binder = .ordinary) :
    GroundNode solution runtime storage scope functionResult (.read binder []) ty := by
  have closedType : monoRuntime ty = some tag := by
    cases typed with
    | read found notSchema keys sorts instantiated solved wellFormed sort =>
      rename_i foundName binding actual requirements
      have same := scope_binding_unique scope binding _ unique (List.mem_of_find?_eq_some found) member rfl
      have schemeSame : binding.scheme = ⟨[], raw, []⟩ := congrArg Binding.scheme same
      rw [schemeSame] at instantiated
      obtain ⟨actualEq, requirementsEq⟩ := instantiated
      have emptySubstitution : schemeSubstitution ⟨[], raw, []⟩ (witnessSubstitution []) = identity := by
        funext id
        simp [schemeSubstitution, identity]
      rw [emptySubstitution, substitute_identity] at actualEq
      rw [actualEq]
      exact closed
  refine .read typed closedType ?_
  intro foundName binding found same isScheme
  rw [ordinary] at isScheme
  cases isScheme

theorem contextual_runtime_read_contract (scope : Scope) (runtimeSolution : Substitution)
    (storage : BindingStorageMap) (context : Option MonoType) (binder : BinderId)
    (arguments : List (Variable × MonoType)) (runtimeTy proofTy parameters result : MonoType)
    (runtimeParameters runtimeResult : RuntimeType)
    (typed : NodeHasTypeIn runtimeSolution context scope (.read binder arguments) runtimeTy)
    (unique : (scope.map Binding.binder).Nodup)
    (functionType : runtimeTy = .function parameters result)
    (parametersClosed : monoRuntime parameters = some runtimeParameters)
    (resultClosed : monoRuntime result = some runtimeResult)
    (proofClosed : monoRuntime proofTy = some (.function runtimeParameters runtimeResult)) :
    ReadContract scope runtimeSolution storage binder arguments proofTy := by
  intro sought candidate found sameBinder isScheme
  cases typed with
  | read runtimeFound notSchema keys sorts instantiated solved wellFormed sort =>
    rename_i name binding raw requirements
    have same : candidate = binding := scope_binding_unique scope candidate binding unique
      (List.mem_of_find?_eq_some found) (List.mem_of_find?_eq_some runtimeFound) sameBinder
    subst candidate
    obtain ⟨rfl, rfl⟩ := instantiated
    have constraints : checksConstraints runtimeSolution
        (binding.scheme.requirements.map (mapConstraint (schemeSubstitution binding.scheme (witnessSubstitution arguments)))) = true :=
      (checks_constraints_iff _ _).mpr solved
    refine ⟨runtimeParameters, runtimeResult,
      arguments.map (fun (id, ty) => (id, substitute runtimeSolution ty)), ?_, proofClosed⟩
    simp [instantiateFunction, keys, sorts, constraints, functionType, parametersClosed, resultClosed]

end Xsh.Semantics.Connected

namespace Xsh.Semantics.Connected
open Xsh.Foundation
open Xsh.Typing.Connected

theorem closed_runtime_type_substitution_invariant (ty : MonoType) (runtime : RuntimeType)
    (closed : monoRuntime ty = some runtime) (solution : Substitution) : substitute solution ty = ty := by
  induction ty generalizing runtime with
  | «variable» id => simp [monoRuntime] at closed
  | primitive primitive => rfl
  | argumentsEmpty | rowEmpty => rfl
  | record row ih | nominal _ row ih =>
    cases found : monoRuntime row with
    | none => simp [monoRuntime, found] at closed
    | some actual => simp [substitute, ih actual found]
  | function left right leftIH rightIH | argument left right leftIH rightIH | result left right leftIH rightIH | rowField _ left right leftIH rightIH =>
    cases first : monoRuntime left with
    | none => simp [monoRuntime, first] at closed
    | some leftRuntime =>
      cases second : monoRuntime right with
      | none => simp [monoRuntime, first, second] at closed
      | some rightRuntime => simp [substitute, leftIH leftRuntime first, rightIH rightRuntime second]

theorem runtime_closed_storage_survives_extension (extension runtime : Substitution) (raw : MonoType)
    (tag : RuntimeType) (closed : monoRuntime (substitute runtime raw) = some tag) :
    monoRuntime (substitute (compose extension runtime) raw) = some tag := by
  rw [substitute_compose, closed_runtime_type_substitution_invariant _ tag closed extension]
  exact closed

theorem contextual_selected_read_contract (scope : Scope) (runtime extension : Substitution)
    (storage : BindingStorageMap) (context : Option MonoType) (binder : Xsh.Foundation.BinderId)
    (arguments : List (Variable × MonoType)) (raw parameters result : MonoType)
    (runtimeParameters runtimeResult : RuntimeType)
    (typed : NodeHasTypeIn runtime context scope (.read binder arguments) (substitute runtime raw))
    (unique : (scope.map Binding.binder).Nodup)
    (functionType : substitute runtime raw = .function parameters result)
    (parametersClosed : monoRuntime parameters = some runtimeParameters)
    (resultClosed : monoRuntime result = some runtimeResult) :
    ReadContract scope runtime storage binder arguments (substitute (compose extension runtime) raw) := by
  have runtimeClosed : monoRuntime (substitute runtime raw) = some (.function runtimeParameters runtimeResult) := by
    rw [functionType]
    simp [monoRuntime, parametersClosed, resultClosed]
  have proofClosed := runtime_closed_storage_survives_extension extension runtime raw _ runtimeClosed
  exact contextual_runtime_read_contract scope runtime storage context binder arguments _ _ parameters result
    runtimeParameters runtimeResult typed unique functionType parametersClosed resultClosed proofClosed

theorem ground_ordinary_read_from_runtime_type (scope : Scope) (runtime extension : Substitution)
    (storage : BindingStorageMap) (functionResult : MonoType) (binder : BinderId)
    (arguments : List (Variable × MonoType)) (raw : MonoType) (tag : RuntimeType)
    (proofTyped : NodeHasTypeIn (compose extension runtime) (some functionResult) scope
      (.read binder arguments) (substitute (compose extension runtime) raw))
    (runtimeClosed : monoRuntime (substitute runtime raw) = some tag)
    (ordinary : storage binder = .ordinary) :
    GroundNode (compose extension runtime) runtime storage scope functionResult (.read binder arguments)
      (substitute (compose extension runtime) raw) := by
  refine .read proofTyped (runtime_closed_storage_survives_extension extension runtime raw tag runtimeClosed) ?_
  intro name binding found same isScheme
  rw [ordinary] at isScheme
  cases isScheme

theorem ground_selected_function_read (scope : Scope) (runtime extension : Substitution)
    (storage : BindingStorageMap) (functionResult : MonoType) (binder : Xsh.Foundation.BinderId)
    (arguments : List (Variable × MonoType)) (raw parameters result : MonoType)
    (runtimeParameters runtimeResult : RuntimeType)
    (proofTyped : NodeHasTypeIn (compose extension runtime) (some functionResult) scope
      (.read binder arguments) (substitute (compose extension runtime) raw))
    (runtimeTyped : NodeHasTypeIn runtime (some functionResult) scope
      (.read binder arguments) (substitute runtime raw))
    (unique : (scope.map Binding.binder).Nodup)
    (functionType : substitute runtime raw = .function parameters result)
    (parametersClosed : monoRuntime parameters = some runtimeParameters)
    (resultClosed : monoRuntime result = some runtimeResult) :
    GroundNode (compose extension runtime) runtime storage scope functionResult (.read binder arguments)
      (substitute (compose extension runtime) raw) := by
  have runtimeClosed : monoRuntime (substitute runtime raw) = some (.function runtimeParameters runtimeResult) := by
    rw [functionType]
    simp [monoRuntime, parametersClosed, resultClosed]
  exact .read proofTyped (runtime_closed_storage_survives_extension extension runtime raw _ runtimeClosed)
    (contextual_selected_read_contract scope runtime extension storage (some functionResult) binder arguments raw
      parameters result runtimeParameters runtimeResult runtimeTyped unique functionType parametersClosed resultClosed)

theorem ground_binding_from_runtime_storage (scope : Scope) (runtime extension : Substitution)
    (storage : BindingStorageMap) (functionResult : MonoType) (completion : Option MonoType)
    (name : NameId) (binder : BinderId) (kind : BindingKind) (raw initializerType : MonoType)
    (initializer : Node) (rest : List StatementNode) (tag : RuntimeType)
    (initializerTyped : GroundNode (compose extension runtime) runtime storage scope functionResult initializer initializerType)
    (compatible : StoreCompatible (substitute (compose extension runtime) raw) initializerType)
    (runtimeClosed : monoRuntime (substitute runtime raw) = some tag)
    (valid : kind = .immutable ∨ kind = .mutable) (ordinary : storage binder = .ordinary)
    (fresh : ∀ binding ∈ scope, binding.binder ≠ binder)
    (tail : GroundBlock (compose extension runtime) runtime storage
      (⟨name, binder, kind, ⟨[], raw, []⟩⟩ :: scope) functionResult completion rest) :
    GroundBlock (compose extension runtime) runtime storage scope functionResult completion
      (.binding binder kind raw initializer :: rest) :=
  .binding initializerTyped compatible (runtime_closed_storage_survives_extension extension runtime raw tag runtimeClosed)
    runtimeClosed valid ordinary fresh tail

end Xsh.Semantics.Connected
