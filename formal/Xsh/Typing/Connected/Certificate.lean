import Xsh.Typing.Connected.BlockSound

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def generationFailure : GenerationError → ValidationError
  | .exhausted => .exhausted
  | .unresolvedName => .unresolvedName
  | _ => .unsupported

def validateGeneratedExpression (fuel : Nat) (scope : Scope) (solution : Substitution)
    (source : Expr) (candidate : Node) : Except ValidationError MonoType := do
  let (generated, state) ← ((expression fuel scope source).run (initialState scope)).mapError generationFailure
  requireCheck (checksConstraints solution state.constraints)
  let actual ← validateExpression fuel scope solution source candidate
  requireCheck (decide (actual = substitute solution generated.ty))
  pure actual

theorem generated_expression_certificate_sound (fuel : Nat) (scope : Scope)
    (solution : Substitution) (source : Expr) (candidate : Node) (ty : MonoType)
    (accepted : validateGeneratedExpression fuel scope solution source candidate = .ok ty) :
    ExpressionHasType scope solution source ty ∧
      ∃ generated state,
        (expression fuel scope source).run (initialState scope) = .ok (generated, state) ∧
        Solves state.constraints solution ∧ ty = substitute solution generated.ty := by
  unfold validateGeneratedExpression at accepted
  cases found : (expression fuel scope source).run (initialState scope) with
  | error error =>
    simp only [found, Except.mapError] at accepted
    cases accepted
  | ok output =>
    obtain ⟨generated, state⟩ := output
    simp [found, Except.mapError, except_bind_ok, except_map_ok, exists_require_ok, checks_constraints_iff] at accepted
    obtain ⟨solved, checked, same⟩ := accepted
    exact ⟨accepted_expression_has_type fuel scope solution source candidate _ checked,
      generated, state, rfl, solved, same⟩


def generatedIdentityCandidate : Node :=
  .call (.read identityBinding.binder [(identityVariable, .variable ⟨1, .value⟩)]) [.integer "7"]

def generatedIdentitySolution (id : Variable) : MonoType :=
  if id = ⟨1, .value⟩ ∨ id = ⟨2, .value⟩ then .primitive .integer else .variable id

def generatedIdentityAccepted : Bool :=
  match validateGeneratedExpression 12 [identityBinding] generatedIdentitySolution
      identityCallSource generatedIdentityCandidate with
  | .ok (.primitive .integer) => true
  | _ => false

def generatedIdentityRejected (solution : Substitution) (candidate : Node) : Bool :=
  match validateGeneratedExpression 12 [identityBinding] solution identityCallSource candidate with
  | .error .mismatch => true
  | _ => false

def generatedIdentityExhausted : Bool :=
  match validateGeneratedExpression 0 [identityBinding] generatedIdentitySolution
      identityCallSource generatedIdentityCandidate with
  | .error .exhausted => true
  | _ => false

theorem generated_identity_certificate_is_inhabited : generatedIdentityAccepted = true := by
  decide +kernel

theorem generated_identity_rejects_wrong_solution_and_binding :
    generatedIdentityRejected (fun id => .variable id) generatedIdentityCandidate = true ∧
    generatedIdentityRejected generatedIdentitySolution changedIdentityBinding = true := by
  constructor <;> decide +kernel

theorem generated_expression_exhaustion_is_not_a_rejection : generatedIdentityExhausted = true := by
  decide +kernel

end Xsh.Typing.Connected
