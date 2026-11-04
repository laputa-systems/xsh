import Xsh.Semantics.Connected.SafetyFunctionEntry

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem returned_caller_restores_scope (start state : Machine) (events : List Label)
    (scope : Scope) (cells : ActiveCells) (activation : Nat) (solution : Substitution)
    (result : RuntimeType) (value : Value)
    (execution : Execution start events
      { state with control := .returned value, stack := .caller cells activation solution result :: state.stack })
    (agrees : ScopeStateAgrees scope { start with cells := cells, solution := solution })
    (schemes : SchemeCellsImmutable cells start.store)
    (contract : ScopeWritableContract scope solution cells start.store) (typed : HasType result value) :
    Step { state with control := .returned value, stack := .caller cells activation solution result :: state.stack }
      .internal { state with control := .value value, cells := cells, activation := activation, solution := solution } ∧
    ScopeStateAgrees scope { state with control := .value value, cells := cells, activation := activation, solution := solution } ∧
    SchemeCellsImmutable cells state.store := by
  have restored := saved_scope_survives_execution start _ events scope solution cells execution agrees schemes contract
  exact ⟨.returnedToCaller state value cells activation solution result typed, restored.1, restored.2⟩

theorem completed_caller_restores_scope (start state : Machine) (events : List Label)
    (scope : Scope) (cells : ActiveCells) (activation : Nat) (solution : Substitution)
    (result : RuntimeType) (value : Value)
    (execution : Execution start events
      { state with control := .completed value, stack := .caller cells activation solution result :: state.stack })
    (agrees : ScopeStateAgrees scope { start with cells := cells, solution := solution })
    (schemes : SchemeCellsImmutable cells start.store)
    (contract : ScopeWritableContract scope solution cells start.store) (typed : HasType result value) :
    Step { state with control := .completed value, stack := .caller cells activation solution result :: state.stack }
      .internal { state with control := .value value, cells := cells, activation := activation, solution := solution } ∧
    ScopeStateAgrees scope { state with control := .value value, cells := cells, activation := activation, solution := solution } ∧
    SchemeCellsImmutable cells state.store := by
  have restored := saved_scope_survives_execution start _ events scope solution cells execution agrees schemes contract
  exact ⟨.completedToCaller state value cells activation solution result typed, restored.1, restored.2⟩

theorem propagated_caller_restores_scope (start state : Machine) (events : List Label)
    (scope : Scope) (cells : ActiveCells) (activation : Nat) (solution : Substitution)
    (success failure oldSuccess : RuntimeType) (error : Value)
    (execution : Execution start events { state with control := .propagated (.error error oldSuccess), stack := .caller cells activation solution (.result success failure) :: state.stack })
    (agrees : ScopeStateAgrees scope { start with cells := cells, solution := solution })
    (schemes : SchemeCellsImmutable cells start.store)
    (contract : ScopeWritableContract scope solution cells start.store) (typed : HasType failure error) :
    Step { state with control := .propagated (.error error oldSuccess), stack := .caller cells activation solution (.result success failure) :: state.stack }
      .internal { state with control := .value (.error error success), cells := cells, activation := activation, solution := solution } ∧
    ScopeStateAgrees scope { state with control := .value (.error error success), cells := cells, activation := activation, solution := solution } ∧
    SchemeCellsImmutable cells state.store := by
  have restored := saved_scope_survives_execution start _ events scope solution cells execution agrees schemes contract
  exact ⟨.propagatedToCaller state error oldSuccess success failure cells activation solution typed, restored.1, restored.2⟩

end Xsh.Semantics.Connected
