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

def CompletionAccepts (expected : Option MonoType) (actual : MonoType) : Prop :=
  expected = none ∨ expected = some actual

mutual
  inductive SourceBlockHasCompletion (names : TypeNames) (solution : Substitution) :
      Scope → MonoType → Option MonoType → List Statement → Prop where
    | nil {scope result completion} :
        CompletionAccepts completion (.primitive .unit) →
        SourceBlockHasCompletion names solution scope result completion []
    | binding {scope result completion kind name declared initializer rest binder rawType} :
        ExpressionHasType scope solution initializer (substitute solution rawType) →
        AnnotationAgrees names solution declared (substitute solution rawType) →
        (∀ binding ∈ scope, binding.binder ≠ binder) →
        SourceBlockHasCompletion names solution
          (⟨name, binder, bindingKind kind, ⟨[], rawType, []⟩⟩ :: scope) result completion rest →
        SourceBlockHasCompletion names solution scope result completion
          (.binding kind (.name name) declared initializer :: rest)
    | assignment {scope result completion name value rest binding} :
        scope.find? (fun item => decide (item.name = name)) = some binding →
        binding.kind = .mutable →
        ExpressionHasType scope solution value (substitute solution binding.scheme.body) →
        SourceBlockHasCompletion names solution scope result completion rest →
        SourceBlockHasCompletion names solution scope result completion (.assign (.name name) .set value :: rest)
    | returned {scope result completion value rest} :
        ExpressionHasType scope solution value result →
        SourceBlockHasCompletion names solution scope result none rest →
        SourceBlockHasCompletion names solution scope result completion (.returnValue (some value) :: rest)
    | tailName {scope result completion name ty} :
        ExpressionHasType scope solution (.ident name) ty → CompletionAccepts completion ty →
        SourceBlockHasCompletion names solution scope result completion [.tailName name]
    | expressionTail {scope result completion value ty} :
        ExpressionHasType scope solution value ty → CompletionAccepts completion ty →
        SourceBlockHasCompletion names solution scope result completion [.expression value]
    | expressionNext {scope result completion value next rest ty} :
        ExpressionHasType scope solution value ty →
        SourceBlockHasCompletion names solution scope result completion (next :: rest) →
        SourceBlockHasCompletion names solution scope result completion (.expression value :: next :: rest)
    | scopedTail {scope result completion body} :
        SourceBlockHasCompletion names solution scope result completion body →
        SourceBlockHasCompletion names solution scope result completion [.expression (.valueBlock body)]
    | scopedNext {scope result completion body next rest} :
        SourceBlockHasCompletion names solution scope result none body →
        SourceBlockHasCompletion names solution scope result completion (next :: rest) →
        SourceBlockHasCompletion names solution scope result completion (.expression (.valueBlock body) :: next :: rest)
    | conditionalTail {scope result completion branches fallback} :
        SourceBranchesHaveCompletion names solution scope result completion branches →
        SourceBlockHasCompletion names solution scope result completion fallback →
        SourceBlockHasCompletion names solution scope result completion [.ifStmt branches (some fallback)]
    | conditionalNoFallbackTail {scope result completion branches} :
        SourceBranchesHaveCompletion names solution scope result completion branches →
        CompletionAccepts completion (.primitive .unit) →
        SourceBlockHasCompletion names solution scope result completion [.ifStmt branches none]
    | conditionalNext {scope result completion branches fallback next rest} :
        SourceBranchesHaveCompletion names solution scope result none branches →
        SourceBlockHasCompletion names solution scope result none fallback →
        SourceBlockHasCompletion names solution scope result completion (next :: rest) →
        SourceBlockHasCompletion names solution scope result completion (.ifStmt branches (some fallback) :: next :: rest)
    | conditionalNoFallbackNext {scope result completion branches next rest} :
        SourceBranchesHaveCompletion names solution scope result none branches →
        SourceBlockHasCompletion names solution scope result completion (next :: rest) →
        SourceBlockHasCompletion names solution scope result completion (.ifStmt branches none :: next :: rest)

  inductive SourceBranchesHaveCompletion (names : TypeNames) (solution : Substitution) :
      Scope → MonoType → Option MonoType → List (Expr × List Statement) → Prop where
    | nil {scope result completion} : SourceBranchesHaveCompletion names solution scope result completion []
    | cons {scope result completion condition body rest} :
        ExpressionHasType scope solution condition (.primitive .boolean) →
        SourceBlockHasCompletion names solution scope result completion body →
        SourceBranchesHaveCompletion names solution scope result completion rest →
        SourceBranchesHaveCompletion names solution scope result completion ((condition, body) :: rest)
end

abbrev BlockHasType (names : TypeNames) (solution : Substitution) (scope : Scope)
    (result : MonoType) (source : List Statement) : Prop :=
  SourceBlockHasCompletion names solution scope result (some result) source

abbrev StatementBranchesHaveType (names : TypeNames) (solution : Substitution) (scope : Scope)
    (result : MonoType) (branches : List (Expr × List Statement)) : Prop :=
  SourceBranchesHaveCompletion names solution scope result (some result) branches

end Xsh.Typing.Connected
