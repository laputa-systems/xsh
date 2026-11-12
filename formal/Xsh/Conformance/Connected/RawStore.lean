import Xsh.Conformance.Connected.RawRows
import Xsh.Conformance.Connected.ScopedOccurrences

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected

def rawStoreReference (rows : RawInstructionRows) (functions : RawFunctionRows)
    (owner row : Nat) : Option (Nat × Nat × Nat × SourceSite) := do
  let instruction ← readRawInstruction rows row
  if instruction.tag = ⟨"StmtAssignPath", 123⟩ then pure () else none
  let .assignPath slot [.storeValue operation] .set value site ← rawStatementShape rows functions owner instruction
    | none
  pure (slot, operation, value, site)

def rawStoreCorrespondence (rows : RawInstructionRows) (functions : RawFunctionRows)
    (owner row : Nat) (slots : List (BinderId × Nat)) (occurrences : List ScopedOccurrence)
    (document : ScopedBodyDocument) (entry : Nat × Nat) : Bool :=
  match rawStoreReference rows functions owner row, document.operations[entry.2]? with
  | some (slot, operation, _, site), some (.store store) =>
    match occurrences.find? (fun occurrence => decide (occurrence.site = store.site)) with
    | some occurrence => match occurrence.storeTarget with
      | some (binder, statementSite) =>
        decide (operation = entry.2) && decide (site = statementSite) &&
          decide ((slots.map Prod.fst).Nodup) && decide ((slots.map Prod.snd).Nodup) &&
          decide (slots.find? (fun item => decide (item.1 = binder)) = some (binder, slot))
      | none => false
    | none => false
  | _, _ => false

def checkRawStoreFact (fuel : Nat) (names : TypeNames) (result : MonoType) (types : BinderTypes)
    (principal : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (slots : List (BinderId × Nat)) (occurrences : List ScopedOccurrence)
    (document : ScopedBodyDocument) (entry : Nat × Nat) : Bool :=
  rawStoreCorrespondence rows functions owner row slots occurrences document entry &&
    scopedOperationSiteCheck fuel names result types principal requirements fields occurrences document entry

theorem raw_store_correspondence_sound (rows : RawInstructionRows) (functions : RawFunctionRows)
    (owner row : Nat) (slots : List (BinderId × Nat)) (occurrences : List ScopedOccurrence)
    (document : ScopedBodyDocument) (entry : Nat × Nat)
    (accepted : rawStoreCorrespondence rows functions owner row slots occurrences document entry = true) :
    ∃ slot value site store occurrence binder,
      rawStoreReference rows functions owner row = some (slot, entry.2, value, site) ∧
      document.operations[entry.2]? = some (.store store) ∧
      occurrences.find? (fun item => decide (item.site = store.site)) = some occurrence ∧
      occurrence.storeTarget = some (binder, site) ∧
      (slots.map Prod.fst).Nodup ∧ (slots.map Prod.snd).Nodup ∧
      slots.find? (fun item => decide (item.1 = binder)) = some (binder, slot) := by
  cases raw : rawStoreReference rows functions owner row with
  | none => simp [rawStoreCorrespondence, raw] at accepted
  | some data =>
    obtain ⟨slot, ordinal, value, site⟩ := data
    cases op : document.operations[entry.2]? with
    | none => simp [rawStoreCorrespondence, raw, op] at accepted
    | some operation =>
      cases operation with
      | field field => simp [rawStoreCorrespondence, raw, op] at accepted
      | store store =>
        cases occurrence : occurrences.find? (fun item => decide (item.site = store.site)) with
        | none => simp [rawStoreCorrespondence, raw, op, occurrence] at accepted
        | some found =>
          cases target : found.storeTarget with
          | none => simp [rawStoreCorrespondence, raw, op, occurrence, target] at accepted
          | some pair =>
            obtain ⟨binder, statementSite⟩ := pair
            simp [rawStoreCorrespondence, raw, op, occurrence, target, Bool.and_eq_true] at accepted
            obtain ⟨⟨⟨⟨ordinalSame, siteSame⟩, uniqueBinders⟩, uniqueSlots⟩, mapped⟩ := accepted
            subst ordinal
            subst statementSite
            exact ⟨slot, value, site, store, found, binder, rfl, rfl, occurrence, target,
              uniqueBinders, uniqueSlots, mapped⟩

theorem checked_raw_store_fact_types_every_solution (fuel : Nat) (names : TypeNames) (result : MonoType)
    (types : BinderTypes) (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (slots : List (BinderId × Nat)) (occurrences : List ScopedOccurrence)
    (document : ScopedBodyDocument) (entry : Nat × Nat) (store : StoreOperation)
    (operation : document.operations[entry.2]? = some (.store store))
    (checked : checkRawStoreFact fuel names result types principal requirements fields rows functions owner row
      slots occurrences document entry = true)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    rawStoreCorrespondence rows functions owner row slots occurrences document entry = true ∧
    ∃ occurrence binder statementSite binding,
      occurrences.find? (fun item => decide (item.site = store.site)) = some occurrence ∧
      occurrence.storeTarget = some (binder, statementSite) ∧
      occurrence.scope.find? (fun item => decide (item.binder = binder)) = some binding ∧
      binding.kind = .mutable ∧
      StoreCompatible (substitute (compose later principal) binding.scheme.body) (substitute later store.replacement) ∧
      ExpressionHasTypeIn (compose later principal) (some (names, substitute later result)) occurrence.scope
        occurrence.source (substitute later store.replacement) := by
  simp only [checkRawStoreFact, Bool.and_eq_true] at checked
  exact ⟨checked.1, checked_store_site_types_every_solution fuel names result types principal later requirements
    fields occurrences document entry store operation checked.2 sorted solves⟩

theorem checked_raw_store_correspondence_types_every_solution (fuel : Nat) (names : TypeNames) (result : MonoType)
    (types : BinderTypes) (principal later : Substitution) (requirements : List Constraint) (fields : FieldResults)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (slots : List (BinderId × Nat)) (occurrences : List ScopedOccurrence)
    (document : ScopedBodyDocument) (entry : Nat × Nat)
    (checked : checkRawStoreFact fuel names result types principal requirements fields rows functions owner row
      slots occurrences document entry = true)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ∃ slot value site store occurrence binder binding,
      rawStoreReference rows functions owner row = some (slot, entry.2, value, site) ∧
      document.operations[entry.2]? = some (.store store) ∧
      occurrences.find? (fun item => decide (item.site = store.site)) = some occurrence ∧
      occurrence.storeTarget = some (binder, site) ∧
      slots.find? (fun item => decide (item.1 = binder)) = some (binder, slot) ∧
      occurrence.scope.find? (fun item => decide (item.binder = binder)) = some binding ∧
      binding.kind = .mutable ∧
      StoreCompatible (substitute (compose later principal) binding.scheme.body) (substitute later store.replacement) ∧
      ExpressionHasTypeIn (compose later principal) (some (names, substitute later result)) occurrence.scope
        occurrence.source (substitute later store.replacement) := by
  simp only [checkRawStoreFact, Bool.and_eq_true] at checked
  obtain ⟨slot, value, site, store, occurrence, binder, raw, operation, found, target, _, _, mapped⟩ :=
    raw_store_correspondence_sound rows functions owner row slots occurrences document entry checked.1
  obtain ⟨typedOccurrence, typedBinder, typedSite, binding, typedFound, typedTarget, selected, mutable, compatible, typed⟩ :=
    checked_store_site_types_every_solution fuel names result types principal later requirements fields occurrences
      document entry store operation checked.2 sorted solves
  rw [found] at typedFound
  cases typedFound
  rw [target] at typedTarget
  cases typedTarget
  exact ⟨slot, value, site, store, occurrence, binder, binding, raw, operation, found, target,
    mapped, selected, mutable, compatible, typed⟩

end Xsh.Conformance.Connected
