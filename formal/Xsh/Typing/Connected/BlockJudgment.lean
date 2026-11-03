import Xsh.Typing.Connected.Judgment

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def AnnotationAgrees (names : TypeNames) (solution : Substitution)
    (declared : Option TypeExpr) (ty : MonoType) : Prop :=
  match declared with
  | none => True
  | some source => ∃ annotated, annotation names source = some annotated ∧
      substitute solution annotated = ty

def bindingKind (kind : Xsh.Syntax.BindingKind) : BindingKind :=
  if kind = .var then .mutable else .immutable

mutual
  inductive BlockHasType (names : TypeNames) (solution : Substitution) :
      Scope → MonoType → List Statement → Prop where
    | nil {scope result} : BlockHasType names solution scope result []
    | binding {scope result kind name declared initializer rest binder rawType} :
        ExpressionHasType scope solution initializer (substitute solution rawType) →
        AnnotationAgrees names solution declared (substitute solution rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        BlockHasType names solution
          (⟨name, binder, bindingKind kind, ⟨[], rawType, []⟩⟩ :: scope) result rest →
        BlockHasType names solution scope result
          (.binding kind (.name name) declared initializer :: rest)
    | assignment {scope result name value rest binding} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .mutable →
        ExpressionHasType scope solution value (substitute solution binding.scheme.body) →
        BlockHasType names solution scope result rest →
        BlockHasType names solution scope result (.assign (.name name) .set value :: rest)
    | returned {scope result value rest} :
        ExpressionHasType scope solution value result →
        BlockHasType names solution scope result rest →
        BlockHasType names solution scope result (.returnValue (some value) :: rest)
    | tailName {scope result name} :
        ExpressionHasType scope solution (.ident name) result →
        BlockHasType names solution scope result [.tailName name]
    | expression {scope result value rest ty} :
        ExpressionHasType scope solution value ty →
        (rest = [] → ty = result) →
        BlockHasType names solution scope result rest →
        BlockHasType names solution scope result (.expression value :: rest)
    | conditionalWithoutFallback {scope result branches rest} :
        StatementBranchesHaveType names solution scope result branches →
        BlockHasType names solution scope result rest →
        BlockHasType names solution scope result (.ifStmt branches none :: rest)
    | conditionalWithFallback {scope result branches fallback rest} :
        StatementBranchesHaveType names solution scope result branches →
        BlockHasType names solution scope result fallback →
        BlockHasType names solution scope result rest →
        BlockHasType names solution scope result (.ifStmt branches (some fallback) :: rest)

  inductive StatementBranchesHaveType (names : TypeNames) (solution : Substitution) :
      Scope → MonoType → List (Expr × List Statement) → Prop where
    | nil {scope result} : StatementBranchesHaveType names solution scope result []
    | cons {scope result condition body rest} :
        ExpressionHasType scope solution condition (.primitive .boolean) →
        BlockHasType names solution scope result body →
        StatementBranchesHaveType names solution scope result rest →
        StatementBranchesHaveType names solution scope result ((condition, body) :: rest)
end

end Xsh.Typing.Connected
