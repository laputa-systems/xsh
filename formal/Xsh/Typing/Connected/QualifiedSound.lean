import Xsh.Typing.Connected.QualifiedContextSound

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

theorem validated_qualified_expression_instantiates (fuel : Nat) (scope : Scope)
    (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : Expr) (candidate : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression fuel scope principal requirements fields source candidate = .ok ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ExpressionHasType scope (compose later principal) source (substitute later ty) :=
  qualified_expression_instantiates scope principal later requirements source ty
    ((c_validate_qualified_all_sound fuel).1 none scope principal requirements fields source candidate ty accepted)
    sorted solves

theorem accepted_qualified_expression_has_type (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : Expr) (candidate : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression fuel scope principal requirements fields source candidate = .ok ty) :
    QualifiedExpression scope principal requirements source ty :=
  (c_validate_qualified_all_sound fuel).1 none scope principal requirements fields source candidate ty accepted

end Xsh.Typing.Connected
