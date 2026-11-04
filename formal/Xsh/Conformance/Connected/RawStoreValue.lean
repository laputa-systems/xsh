import Xsh.Conformance.Connected.RawExpression
import Xsh.Conformance.Connected.RawStoreFixture

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected

def rawStoreReplacementCheck (fuel : Nat) (names : RawNameBytes)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (slots : List (BinderId × Nat)) (occurrences : List ScopedOccurrence)
    (document : ScopedBodyDocument) (entry : Nat × Nat) : Bool :=
  match rawStoreReference rows functions owner row, document.operations[entry.2]? with
  | some (_, _, value, _), some (.store store) =>
      match occurrences.find? (fun occurrence => decide (occurrence.site = store.site)) with
      | some occurrence => decide (value < row) &&
          rawValueCorrespondence fuel names slots rows functions owner value occurrence.candidate
      | none => false
  | _, _ => false

def checkRawStoreValueFact (fuel : Nat) (names : TypeNames) (unit : SourceUnit)
    (result : MonoType) (types : BinderTypes) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (slots : List (BinderId × Nat)) (occurrences : List ScopedOccurrence)
    (document : ScopedBodyDocument) (entry : Nat × Nat) : Bool :=
  checkRawStoreFact fuel names result types principal requirements fields rows functions owner row slots
    occurrences document entry &&
    rawStoreReplacementCheck fuel (sourceRawNameBytes unit) rows functions owner row slots occurrences document entry

theorem checked_raw_store_value_types_every_solution (fuel : Nat) (names : TypeNames) (unit : SourceUnit)
    (result : MonoType) (types : BinderTypes) (principal later : Substitution)
    (requirements : List Constraint) (fields : FieldResults)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (slots : List (BinderId × Nat)) (occurrences : List ScopedOccurrence)
    (document : ScopedBodyDocument) (entry : Nat × Nat)
    (checked : checkRawStoreValueFact fuel names unit result types principal requirements fields rows functions
      owner row slots occurrences document entry = true)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ∃ slot value site store occurrence binder binding tree,
      rawStoreReference rows functions owner row = some (slot, entry.2, value, site) ∧
      value < row ∧
      document.operations[entry.2]? = some (.store store) ∧
      occurrences.find? (fun item => decide (item.site = store.site)) = some occurrence ∧
      occurrence.storeTarget = some (binder, site) ∧
      slots.find? (fun item => decide (item.1 = binder)) = some (binder, slot) ∧
      readRawValueTree rows functions owner (slots.map Prod.snd) fuel value = some tree ∧
      nodeRawValueTree (sourceRawNameBytes unit) slots fuel occurrence.candidate = some tree ∧
      occurrence.scope.find? (fun item => decide (item.binder = binder)) = some binding ∧
      binding.kind = .mutable ∧
      StoreCompatible (substitute (compose later principal) binding.scheme.body) (substitute later store.replacement) ∧
      ExpressionHasTypeIn (compose later principal) (some (names, substitute later result)) occurrence.scope
        occurrence.source (substitute later store.replacement) := by
  simp only [checkRawStoreValueFact, Bool.and_eq_true] at checked
  obtain ⟨slot, value, site, store, occurrence, binder, binding, raw, operation, found, target, mapped,
      selected, mutable, compatible, typed⟩ :=
    checked_raw_store_correspondence_types_every_solution fuel names result types principal later requirements
      fields rows functions owner row slots occurrences document entry checked.1 sorted solves
  have replacement := checked.2
  simp only [rawStoreReplacementCheck, raw, operation, found, Bool.and_eq_true, decide_eq_true_eq] at replacement
  obtain ⟨tree, actual, expected⟩ :=
    raw_value_correspondence_sound fuel (sourceRawNameBytes unit) slots rows functions owner value occurrence.candidate replacement.2
  exact ⟨slot, value, site, store, occurrence, binder, binding, tree, raw, replacement.1, operation, found,
    target, mapped, actual, expected, selected, mutable, compatible, typed⟩

def scopedReplacementRows (readSlot : Nat) : RawInstructionRows :=
  let rows := projectedScopedRows 0 2 2 "StmtAssignPath" .set
  { rows with data := rows.data.set 8 ⟨rows.extra.length, 1⟩, extra := rows.extra ++ [readSlot] }

def scopedReplacementCheck (readSlot : Nat) : Bool :=
  match actualScopedCheckDecoded scopedUnit scopedDocument scopedScopes,
      sourceScopedOccurrences 128 scopedFunction Xsh.Semantics.Connected.scopedFixtureParameters [] []
        Xsh.Semantics.Connected.scopedFixtureBinders Xsh.Semantics.Connected.scopedFixtureCandidate with
  | .ok principal, .ok occurrences =>
      checkRawStoreValueFact 128 Xsh.Semantics.Connected.scopedFixtureNames scopedUnit
        (substitute principal (.variable Xsh.Semantics.Connected.scopedFixtureTypeVar))
        Xsh.Semantics.Connected.scopedFixtureBinders principal
        Xsh.Semantics.Connected.scopedFixtureScheme.requirements (fun _ => none)
        (scopedReplacementRows readSlot) projectedScopedBlocks 1 9
        [(Xsh.Semantics.Connected.scopedFixtureInput, 1), (Xsh.Semantics.Connected.scopedFixtureLocal, 2)]
        occurrences ⟨scopedBodyDocument, [.store scopedStore]⟩ (1, 0)
  | _, _ => false

set_option maxRecDepth 10000 in
theorem scoped_store_rejects_a_different_well_typed_replacement :
    projectedScopedStoreCheck 0 2 2 "StmtAssignPath" .set = true ∧
    scopedReplacementCheck 1 = true ∧
    scopedReplacementCheck 2 = false ∧
    scopedReplacementCheck 3 = false := by
  decide +kernel

def literalReplacementRows (low high : Nat) : RawInstructionRows :=
  ⟨[⟨"ExprInt", 20⟩], [false], [false], [⟨0, 2⟩], [low, high], [], [], [], [], [], []⟩

theorem raw_store_replacement_rejects_changed_literal_bits :
    rawValueCorrespondence 4 (fun _ => none) [] (literalReplacementRows 1 0) ⟨[], []⟩ 0 0 (.integer "1") = true ∧
    rawValueCorrespondence 4 (fun _ => none) [] (literalReplacementRows 2 0) ⟨[], []⟩ 0 0 (.integer "1") = false ∧
    rawValueCorrespondence 4 (fun _ => none) [] (literalReplacementRows 4294967297 0) ⟨[], []⟩ 0 0 (.integer "1") = false ∧
    rawValueCorrespondence 4 (fun _ => none) [] (literalReplacementRows 4294967297 0) ⟨[], []⟩ 0 0 (.integer "invalid") = false := by
  decide +kernel

def literalReplacementBlockRows (value temporary : Nat) (tail : RawTag) : RawInstructionRows :=
  ⟨[⟨"ExprInt", 20⟩, ⟨"StmtLet", 115⟩, ⟨"ExprParam", 31⟩, tail, ⟨"ExprValueBlock", 73⟩],
    [false, false, false, false, false], [false, false, false, false, false],
    [⟨0, 2⟩, ⟨2, 3⟩, ⟨5, 1⟩, ⟨6, 1⟩, ⟨7, 2⟩], [value, 0, temporary, 0, 0, 3, 2, 1, 1, 2, 1, 3],
    [], [], [⟨0, 1⟩], [0], [], []⟩

def literalReplacementBlockFunctions : RawFunctionRows :=
  ⟨[⟨⟨9, 3⟩, 4294967295, 0, 1, (0, 0, 0)⟩], []⟩

theorem raw_value_block_binds_fresh_temporaries :
    rawValueCorrespondence 8 (fun _ => none) [] (literalReplacementBlockRows 1 3 ⟨"StmtValue", 126⟩)
      literalReplacementBlockFunctions 0 4 (.integer "1") = true ∧
    rawValueCorrespondence 8 (fun _ => none) [] (literalReplacementBlockRows 2 3 ⟨"StmtValue", 126⟩)
      literalReplacementBlockFunctions 0 4 (.integer "1") = false ∧
    rawValueCorrespondence 8 (fun _ => none) [] (literalReplacementBlockRows 1 4 ⟨"StmtValue", 126⟩)
      literalReplacementBlockFunctions 0 4 (.integer "1") = false ∧
    rawValueCorrespondence 8 (fun _ => none) [(Xsh.Semantics.Connected.scopedFixtureLocal, 3)]
      (literalReplacementBlockRows 1 3 ⟨"StmtValue", 126⟩) literalReplacementBlockFunctions 0 4 (.integer "1") = false ∧
    rawValueCorrespondence 8 (fun _ => none) [] (literalReplacementBlockRows 1 3 ⟨"StmtReturn", 150⟩)
      literalReplacementBlockFunctions 0 4 (.integer "1") = false := by
  decide +kernel

end Xsh.Conformance.Connected
