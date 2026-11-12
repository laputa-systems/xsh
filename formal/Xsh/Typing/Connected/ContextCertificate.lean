import Xsh.Typing.Connected.Derivation

namespace Xsh.Typing.Connected

open Xsh.Syntax

def propagationConstraints (context : Option MonoType) (node : Node) : Except ValidationError (List Constraint) :=
  match context with
  | none => if (propagationErrors node).isEmpty then .ok [] else .error .invalidContext
  | some error => .ok ((propagationErrors node).map fun actual => .equal actual error)

def validateContextualExpression (fuel : Nat) (scope : Scope) (context : Option MonoType)
    (source : Expr) (candidate : Node) (steps : List DerivationStep) : Except ValidationError MonoType := do
  let (generated, state) ← ((expression fuel scope source).run (initialState scope)).mapError generationFailure
  let extra ← propagationConstraints context generated.node
  let solution ← checkDerivation steps (state.constraints ++ extra)
  requireCheck (checksPropagationErrors (context.map (substitute solution)) solution (propagationErrors candidate))
  let ty ← validateExpression fuel scope solution source candidate
  requireCheck (decide (ty = substitute solution generated.ty))
  pure ty

theorem contextual_expression_certificate_sound (fuel : Nat) (scope : Scope) (context : Option MonoType)
    (source : Expr) (candidate : Node) (steps : List DerivationStep) (ty : MonoType)
    (accepted : validateContextualExpression fuel scope context source candidate steps = .ok ty) :
    ∃ generated state extra solution,
      (expression fuel scope source).run (initialState scope) = .ok (generated, state) ∧
      propagationConstraints context generated.node = .ok extra ∧
      checkDerivation steps (state.constraints ++ extra) = .ok solution ∧
      KindedPrincipal (state.constraints ++ extra) solution ∧
      PropagationMatches (context.map (substitute solution)) solution (propagationErrors candidate) ∧
      ExpressionHasType scope solution source ty ∧ ty = substitute solution generated.ty := by
  unfold validateContextualExpression at accepted
  cases found : (expression fuel scope source).run (initialState scope) with
  | error error =>
    simp only [found, Except.mapError] at accepted
    cases accepted
  | ok output =>
    obtain ⟨generated, state⟩ := output
    simp [found, Except.mapError, except_bind_ok, except_map_ok, exists_require_ok,
      checks_propagation_errors_iff] at accepted
    obtain ⟨extra, extraFound, solution, derived, matching, checked, same⟩ := accepted
    exact ⟨generated, state, extra, solution, rfl, extraFound, derived,
      check_derivation_principal steps _ _ derived, matching,
      accepted_expression_has_type fuel scope solution source candidate ty checked, same⟩

end Xsh.Typing.Connected
