import Xsh.Typing.Connected.QualifiedValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def validateQualifiedBlock (fuel : Nat) (names : TypeNames) (scope : Scope) (solution : Substitution)
    (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes) (result : MonoType)
    (source : List Statement) (candidate : List StatementNode) : Except ValidationError Unit :=
  validateQualifiedBlockCompletion fuel names scope solution requirements fields binderTypes result (some result) source candidate

def validateQualifiedStatementBranches (fuel : Nat) (names : TypeNames) (scope : Scope)
    (solution : Substitution) (requirements : List Constraint) (fields : FieldResults) (binderTypes : BinderTypes)
    (result : MonoType) (source : List (Expr × List Statement))
    (candidate : List (Node × List StatementNode)) : Except ValidationError Unit :=
  validateQualifiedBranchesCompletion fuel names scope solution requirements fields binderTypes result (some result) source candidate

end Xsh.Typing.Connected
