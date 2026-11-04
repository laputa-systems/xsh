import Xsh.Semantics.Connected.TypedScope

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem typed_expression_progress (state : Machine) (scope : Scope) (node : Node)
    (ty : MonoType) (runtimeType : RuntimeType)
    (scopeAgrees : ScopeStateAgrees scope state)
    (schemasAgree : SchemaMatches scope state.solution state.schemas)
    (typing : NodeHasType scope state.solution node ty)
    (closed : monoRuntime ty = some runtimeType) :
    ∃ event next, Step { state with control := .expression node } event next := by
  cases typing with
  | integer parsed => exact ⟨.internal, _, .integer state _ _ parsed⟩
  | boolean => exact ⟨.internal, _, .boolean state _⟩
  | string => exact ⟨.internal, _, .string state _⟩
  | read found notSchema keys sorts instantiated solved wellFormed sort =>
    obtain ⟨value, step, _⟩ := typed_scope_read_progress state scope _ _ _ runtimeType scopeAgrees
      (.read found notSchema keys sorts instantiated solved wellFormed sort) closed
    exact ⟨.internal, _, step⟩
  | binary => exact ⟨.internal, _, .binary state _ _ _⟩
  | call => exact ⟨.internal, _, .callStart state _ _⟩
  | record typed unique =>
    rename_i fields types
    cases fields with
    | nil => exact ⟨.internal, _, .recordEmpty state⟩
    | cons field rest =>
      obtain ⟨name, value⟩ := field
      exact ⟨.internal, _, .recordStart state name value rest⟩
  | field => exact ⟨.internal, _, .fieldStart state _ _⟩
  | conditional branchesTyped otherwiseTyped =>
    rename_i branches otherwise
    cases branches with
    | nil => exact ⟨.internal, _, .conditionalEmpty state otherwise⟩
    | cons branch rest =>
      obtain ⟨condition, value⟩ := branch
      exact ⟨.internal, _, .conditionalStart state condition value rest otherwise⟩
  | requireValue found kind quantified requirements body wellFormed typed =>
    obtain ⟨guard, guardFound, _⟩ := schemasAgree _ _ _ _ found kind body
    exact ⟨.internal, _, .requireStart state _ _ _ guard guardFound⟩
  | propagate => exact ⟨.internal, _, .propagateStart state _ _⟩

end Xsh.Semantics.Connected
