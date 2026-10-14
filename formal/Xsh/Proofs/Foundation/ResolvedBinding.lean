import Xsh.Proofs.Foundation.ObservedMachine
import Xsh.Semantics.Foundation.ResolvedBinding

namespace Xsh.Proofs.Foundation.ResolvedBinding

open Xsh.Foundation
open Xsh.Values
open Xsh.Semantics.Environments
open Xsh.Semantics.BaseMachine
open Xsh.Semantics.Foundation
open Xsh.Syntax
open Xsh.Syntax.Foundation

private def sourceName : RawName := ⟨⟨7, by decide⟩⟩
private def outerBinder : BinderId := ⟨⟨11, by decide⟩⟩
private def innerBinder : BinderId := ⟨⟨12, by decide⟩⟩

private def outerDeclaration : DeclarationId := ⟨outerBinder⟩
private def innerDeclaration : DeclarationId := ⟨innerBinder⟩

private def outerOccurrence : OccurrenceId := ⟨⟨1, by decide⟩⟩
private def innerOccurrence : OccurrenceId := ⟨⟨2, by decide⟩⟩

private def outerUse : ResolvedNameOccurrence :=
  ⟨⟨outerOccurrence, sourceName⟩, outerDeclaration⟩

private def innerUse : ResolvedNameOccurrence :=
  ⟨⟨innerOccurrence, sourceName⟩, innerDeclaration⟩

private def wrongOuterUse : ResolvedNameOccurrence :=
  { outerUse with declaration := innerDeclaration }

private def outerCell : CellId := ⟨1, outerBinder⟩
private def innerCell : CellId := ⟨2, innerBinder⟩

private def activationMutatedBinding : BindingId :=
  { BindingId.ofCell outerCell with activation := 3 }

private def activeCells : ActivationEnvironment :=
  bindCell (bindCell ([] : ActivationEnvironment) outerBinder outerCell)
    innerBinder innerCell

private def outerNames : LexicalEnvironment :=
  bind emptyLexicalEnvironment sourceName outerBinder

private def innerNames : LexicalEnvironment :=
  bind emptyLexicalEnvironment sourceName innerBinder

private def outerCellValue : StoreCell BaseType CompositeValue hasBaseType :=
  ⟨typedInteger intOne, .mutable⟩

private def innerCellValue : StoreCell BaseType CompositeValue hasBaseType :=
  ⟨typedInteger intZero, .mutable⟩

private def identityStore : BaseStore := fun cell =>
  if cell = outerCell then some outerCellValue
  else if cell = innerCell then some innerCellValue
  else none

private def outerMachine : MachineConfig :=
  initialConfig outerNames activeCells identityStore (.identifier sourceName)

private def innerMachine : MachineConfig :=
  initialConfig innerNames activeCells identityStore (.identifier sourceName)

private def outerObserved : Configuration :=
  Configuration.start outerMachine (.identifier sourceName) rfl

private def innerObserved : Configuration :=
  Configuration.start innerMachine (.identifier sourceName) rfl

private def intTwo : IntValue :=
  ⟨2, by unfold signed64Min signed64Limit; omega⟩

private def updatedOuterCellValue : StoreCell BaseType CompositeValue hasBaseType :=
  ⟨typedInteger intTwo, .mutable⟩

private def wrongTypeCandidate : CompositeValue := .string "not an integer"

theorem dynamic_binding_identity_round_trips_through_machine_cell (cell : CellId) :
    BindingId.toCell (BindingId.ofCell cell) = cell ∧
      (BindingId.ofCell outerCell).declaration = activationMutatedBinding.declaration ∧
      activationMutatedBinding.activation ≠ outerCell.activation ∧
      resolveCell activeCells outerBinder = some outerCell ∧
      resolveCell activeCells outerBinder ≠
        some (BindingId.toCell activationMutatedBinding) ∧
      identityStore (BindingId.toCell activationMutatedBinding) = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩
  · cases cell
    rfl
  · rfl
  · decide
  · decide
  · decide
  · decide

theorem equal_name_syntax_has_distinct_occurrence_and_declaration_ids :
    outerUse.site.name = innerUse.site.name ∧
      BaseExpr.identifier outerUse.site.name = BaseExpr.identifier innerUse.site.name ∧
      outerUse.site.occurrence ≠ innerUse.site.occurrence ∧
      outerUse.declaration ≠ innerUse.declaration := by
  constructor
  · rfl
  constructor
  · rfl
  constructor <;> decide

theorem accepted_observed_uses_resolve_to_their_active_cells :
    Xsh.Semantics.Foundation.wellFormed outerObserved ∧
      Xsh.Semantics.Foundation.wellFormed innerObserved ∧
      checkBaseExpr outerNames activeCells identityStore (.identifier sourceName) = true ∧
      checkBaseExpr innerNames activeCells identityStore (.identifier sourceName) = true ∧
      readResolved? outerObserved outerUse = some (typedInteger intOne) ∧
      readResolved? innerObserved innerUse = some (typedInteger intZero) := by
  constructor
  · apply Xsh.Proofs.Foundation.ObservedMachine.initial_configuration_is_wellFormed
    exact Xsh.Proofs.BaseMachine.accepted_initial_expression_is_wellFormed_fragment
      outerNames activeCells identityStore (.identifier sourceName) (by decide)
  constructor
  · apply Xsh.Proofs.Foundation.ObservedMachine.initial_configuration_is_wellFormed
    exact Xsh.Proofs.BaseMachine.accepted_initial_expression_is_wellFormed_fragment
      innerNames activeCells identityStore (.identifier sourceName) (by decide)
  constructor
  · decide
  constructor
  · decide
  constructor
  · rfl
  · rfl

theorem changing_a_use_to_an_unresolved_binding_is_rejected :
    resolvedBinding? outerObserved wrongOuterUse = none ∧
      readResolved? outerObserved wrongOuterUse = none ∧
      writeResolved? outerObserved wrongOuterUse (.integer intZero) = none := by
  have outerNameResolves : resolve outerNames sourceName = some outerBinder :=
    resolve_after_bind emptyLexicalEnvironment sourceName outerBinder
  have mismatchedBinding : resolvedBinding? outerObserved wrongOuterUse = none := by
    simp only [resolvedBinding?, wrongOuterUse, outerUse, innerDeclaration,
      outerObserved, Configuration.start, outerMachine, initialConfig]
    rw [outerNameResolves]
    decide
  refine ⟨mismatchedBinding, ?_, ?_⟩
  · simp [readResolved?, mismatchedBinding]
  · simp [writeResolved?, mismatchedBinding]

theorem resolved_write_updates_only_the_selected_binding_cell :
    writeResolved? outerObserved outerUse (.integer intTwo) =
        some (.accepted (fun cell =>
          if cell = outerCell then some updatedOuterCellValue else identityStore cell)) ∧
      (fun cell =>
        if cell = outerCell then some updatedOuterCellValue else identityStore cell) outerCell =
        some updatedOuterCellValue ∧
      (fun cell =>
        if cell = outerCell then some updatedOuterCellValue else identityStore cell) innerCell =
        some innerCellValue ∧
      identityStore outerCell = some outerCellValue := by
  refine ⟨rfl, rfl, rfl, rfl⟩

theorem incompatible_value_write_is_rejected_without_store_change :
    writeResolved? outerObserved outerUse wrongTypeCandidate =
      some (.rejected identityStore) := by
  rfl

end Xsh.Proofs.Foundation.ResolvedBinding
