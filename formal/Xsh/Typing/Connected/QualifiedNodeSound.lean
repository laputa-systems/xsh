import Xsh.Typing.Connected.QualifiedContextSound

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

theorem validated_qualified_node_instantiates (fuel : Nat) (scope : Scope)
    (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (source : Expr) (candidate : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression fuel scope principal requirements fields source candidate = .ok ty)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    NodeHasType scope (compose later principal) candidate (substitute later ty) :=
  qualified_node_instantiates scope principal later requirements candidate ty
    ((c_validate_qualified_node_all_sound fuel).1 none scope principal requirements fields source candidate ty accepted)
    sorted solves

theorem accepted_qualified_node_has_type (fuel : Nat) (scope : Scope) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (source : Expr) (candidate : Node) (ty : MonoType)
    (accepted : validateQualifiedExpression fuel scope principal requirements fields source candidate = .ok ty) :
    QualifiedNode scope principal requirements candidate ty :=
  (c_validate_qualified_node_all_sound fuel).1 none scope principal requirements fields source candidate ty accepted

end Xsh.Typing.Connected
