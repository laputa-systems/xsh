import Xsh.Typing.Connected.BlockJudgment
import Xsh.Typing.Connected.Validate

namespace Xsh.Typing.Connected

open Xsh.Syntax

def validateBlock (fuel : Nat) (names : TypeNames) (scope : Scope) (solution : Substitution)
    (binderTypes : BinderTypes) (result : MonoType) (source : List Statement)
    (candidate : List StatementNode) : Except ValidationError Unit :=
  validateBlockCompletion fuel names scope solution binderTypes result (some result) source candidate

def validateStatementBranches (fuel : Nat) (names : TypeNames) (scope : Scope) (solution : Substitution)
    (binderTypes : BinderTypes) (result : MonoType) (source : List (Expr × List Statement))
    (candidate : List (Node × List StatementNode)) : Except ValidationError Unit :=
  validateStatementBranchesCompletion fuel names scope solution binderTypes result (some result) source candidate

def BlockSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result source candidate,
    validateBlock fuel names scope solution binderTypes result source candidate = .ok () →
    BlockHasType names solution scope result source

def StatementBranchesSound (fuel : Nat) : Prop :=
  ∀ names scope solution binderTypes result source candidate,
    validateStatementBranches fuel names scope solution binderTypes result source candidate = .ok () →
    StatementBranchesHaveType names solution scope result source

end Xsh.Typing.Connected
