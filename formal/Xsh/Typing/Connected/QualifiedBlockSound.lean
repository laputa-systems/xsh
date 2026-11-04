import Xsh.Typing.Connected.QualifiedContextSound
import Xsh.Typing.Connected.QualifiedBlockValidate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def QualifiedBlockSound (fuel : Nat) : Prop :=
  ∀ names scope principal requirements fields binderTypes result source candidate,
    validateQualifiedBlock fuel names scope principal requirements fields binderTypes result source candidate = .ok () →
    QualifiedBlock names principal requirements scope result source

def QualifiedStatementBranchesSound (fuel : Nat) : Prop :=
  ∀ names scope principal requirements fields binderTypes result source candidate,
    validateQualifiedStatementBranches fuel names scope principal requirements fields binderTypes result source candidate = .ok () →
    QualifiedStatementBranches names principal requirements scope result source

theorem validate_qualified_blocks_sound (fuel : Nat) : QualifiedBlockSound fuel ∧ QualifiedStatementBranchesSound fuel := by
  constructor
  · intro names scope principal requirements fields binderTypes result source candidate accepted
    exact (c_validate_qualified_all_sound fuel).2.2.2.2.1 names scope principal requirements fields binderTypes
      result (some result) source candidate accepted
  · intro names scope principal requirements fields binderTypes result source candidate accepted
    exact (c_validate_qualified_all_sound fuel).2.2.2.2.2 names scope principal requirements fields binderTypes
      result (some result) source candidate accepted

theorem validated_qualified_block_instantiates (fuel : Nat) (names : TypeNames) (scope : Scope)
    (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (binderTypes : BinderTypes) (result : MonoType) (source : List Statement) (candidate : List StatementNode)
    (accepted : validateQualifiedBlock fuel names scope principal requirements fields binderTypes result source candidate = .ok ())
    (sorted : WellSorted later) (solves : Solves requirements later) :
    BlockHasType names (compose later principal) scope (substitute later result) source :=
  qualified_block_instantiates names principal later requirements scope result source
    ((validate_qualified_blocks_sound fuel).1 names scope principal requirements fields binderTypes result source candidate accepted)
    sorted solves

end Xsh.Typing.Connected
