import Xsh.Semantics.Connected.TypedBinary
import Xsh.Typing.Connected.NodeBlockJudgment

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem typed_expression_conditional_frame (state : Machine) (scope : Scope)
    (condition branch : Node) (rest : List (Node × Node)) (otherwise : Node) (result : MonoType) (value : Value)
    (branches : NodeBranchesHaveType scope state.solution ((condition, branch) :: rest) result)
    (fallback : NodeHasType scope state.solution otherwise result)
    (typed : valueType value = some .boolean) :
    ∃ next, Step { state with control := .value value, stack := .conditional ((condition, branch) :: rest) otherwise :: state.stack }
        .internal { state with control := .expression next } ∧ NodeHasType scope state.solution next result := by
  obtain ⟨boolean, rfl⟩ := boolean_value_shape value typed
  cases branches with
  | cons conditionTyped branchTyped restTyped =>
    cases boolean with
    | false => exact ⟨.conditional rest otherwise, .conditionalFalse state condition branch rest otherwise,
        .conditional restTyped fallback⟩
    | true => exact ⟨branch, .conditionalTrue state condition branch rest otherwise, branchTyped⟩

theorem typed_statement_conditional_frame (state : Machine) (scope : Scope)
    (condition : Node) (body : List StatementNode) (branches : List (Node × List StatementNode))
    (otherwise rest : List StatementNode) (result : MonoType) (value : Value)
    (branchTypes : NodeBranchesHaveCompletion state.solution scope result
      (if rest.isEmpty then some result else none) ((condition, body) :: branches))
    (fallback : NodeBlockHasCompletion state.solution scope result
      (if rest.isEmpty then some result else none) otherwise)
    (following : rest ≠ [] → NodeBlockHasType state.solution scope result rest)
    (typed : valueType value = some .boolean) :
    (Step { state with control := .value value, stack := .branch ((condition, body) :: branches) otherwise rest :: state.stack }
        .internal { state with control := .statements body, stack := .continueBlock state.cells rest :: state.stack } ∧
      NodeBlockHasCompletion state.solution scope result (if rest.isEmpty then some result else none) body) ∨
    (Step { state with control := .value value, stack := .branch ((condition, body) :: branches) otherwise rest :: state.stack }
        .internal { state with control := .statements (.conditional branches otherwise :: rest) } ∧
      NodeBlockHasType state.solution scope result (.conditional branches otherwise :: rest)) := by
  obtain ⟨boolean, rfl⟩ := boolean_value_shape value typed
  cases branchTypes with
  | cons conditionTyped bodyTyped branchesTyped =>
    cases boolean with
    | false =>
      refine Or.inr ⟨.statementConditionalFalse state condition body branches otherwise rest, ?_⟩
      cases rest with
      | nil => exact .conditionalTail branchesTyped fallback
      | cons next tail => exact .conditionalNext branchesTyped fallback (following (by simp))
    | true => exact Or.inl ⟨.statementConditionalTrue state condition body branches otherwise rest, bodyTyped⟩

end Xsh.Semantics.Connected
