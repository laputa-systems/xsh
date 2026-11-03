import Xsh.Typing.Connected.NodeBlockSound
import Xsh.Typing.Connected.Qualified

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

theorem accepted_function_instance_has_node_type (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (scheme : Scheme) (instantiation : Substitution) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (principal : Substitution)
    (accepted : validateQualifiedFunctionInstance fuel names parameters globals result body ordered
      steps solvedCount scheme instantiation candidate binderTypes = .ok principal) :
    NodeBlockHasType (compose instantiation principal) (parameters ++ globals)
      (substitute (compose instantiation principal) result) candidate := by
  cases complete : guaranteesFunctionResult candidate with
  | false =>
    cases supported : supportedStatementTails true candidate <;>
      simp [validateQualifiedFunctionInstance, except_bind_ok, except_map_ok,
        exists_require_ok, complete, supported] at accepted
  | true =>
    cases supported : supportedStatementTails true candidate with
    | false =>
      simp [validateQualifiedFunctionInstance, except_bind_ok, supported] at accepted
    | true =>
      simp [validateQualifiedFunctionInstance, except_bind_ok, except_map_ok,
        exists_require_ok, supported, complete] at accepted
      obtain ⟨chosen, found, requirements, unique, propagation, checked, same⟩ := accepted
      subst chosen
      obtain ⟨resultUnit, checked⟩ := checked
      cases resultUnit
      exact accepted_node_block_has_type fuel names (parameters ++ globals) (compose instantiation principal)
        binderTypes (substitute (compose instantiation principal) result) body candidate checked

theorem qualified_function_instance_types_source_and_nodes (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (scheme : Scheme) (instantiation : Substitution) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (principal : Substitution)
    (sorted : WellSorted instantiation)
    (accepted : validateQualifiedFunctionInstance fuel names parameters globals result body ordered
      steps solvedCount scheme instantiation candidate binderTypes = .ok principal) :
    (∃ generated state,
      (block fuel names (parameters ++ globals) result body).run
        (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
      QualifiedScheme .generalize (globalEnvironment globals)
        (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
      Solves state.constraints (compose instantiation principal) ∧
      (declaredBinders candidate).Nodup ∧
      PropagationMatches (propagationContext (substitute (compose instantiation principal) result))
        (compose instantiation principal) (blockPropagationErrors candidate) ∧
      BlockHasType names (compose instantiation principal) (parameters ++ globals)
        (substitute (compose instantiation principal) result) body) ∧
    NodeBlockHasType (compose instantiation principal) (parameters ++ globals)
      (substitute (compose instantiation principal) result) candidate := by
  exact ⟨qualified_function_instance_sound fuel names parameters globals result body ordered steps solvedCount
      scheme instantiation candidate binderTypes principal sorted accepted,
    accepted_function_instance_has_node_type fuel names parameters globals result body ordered steps solvedCount
      scheme instantiation candidate binderTypes principal accepted⟩

end Xsh.Typing.Connected
