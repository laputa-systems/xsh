import Xsh.Conformance.Connected.RawStoreValue
import Xsh.Conformance.Connected.RawDeclarations

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected

def checkDerivedRawStoreValueFact (fuel : Nat) (names : TypeNames) (unit : SourceUnit)
    (result : MonoType) (types : BinderTypes) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (parameters : List Binding) (declarations : RawLocalDeclarations)
    (occurrences : List ScopedOccurrence) (document : ScopedBodyDocument) (entry : Nat × Nat) : Bool :=
  match deriveRawSlots fuel (sourceRawNameBytes unit) rows functions owner parameters declarations occurrences with
  | some slots =>
      checkRawStoreValueFact fuel names unit result types principal requirements fields rows functions owner row
        slots occurrences document entry
  | none => false

theorem checked_derived_raw_store_value_uses_derived_slots (fuel : Nat) (names : TypeNames) (unit : SourceUnit)
    (result : MonoType) (types : BinderTypes) (principal : Substitution)
    (requirements : List Constraint) (fields : FieldResults)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (parameters : List Binding) (declarations : RawLocalDeclarations)
    (occurrences : List ScopedOccurrence) (document : ScopedBodyDocument) (entry : Nat × Nat)
    (checked : checkDerivedRawStoreValueFact fuel names unit result types principal requirements fields rows
      functions owner row parameters declarations occurrences document entry = true) :
    ∃ slots, deriveRawSlots fuel (sourceRawNameBytes unit) rows functions owner parameters declarations
        occurrences = some slots ∧
      checkRawStoreValueFact fuel names unit result types principal requirements fields rows functions owner row
        slots occurrences document entry = true := by
  unfold checkDerivedRawStoreValueFact at checked
  cases derived : deriveRawSlots fuel (sourceRawNameBytes unit) rows functions owner parameters declarations occurrences with
  | none => simp [derived] at checked
  | some slots =>
      simp only [derived] at checked
      exact ⟨slots, rfl, checked⟩

theorem checked_derived_raw_store_value_types_every_solution (fuel : Nat) (names : TypeNames) (unit : SourceUnit)
    (result : MonoType) (types : BinderTypes) (principal later : Substitution)
    (requirements : List Constraint) (fields : FieldResults)
    (rows : RawInstructionRows) (functions : RawFunctionRows) (owner row : Nat)
    (parameters : List Binding) (declarations : RawLocalDeclarations)
    (occurrences : List ScopedOccurrence) (document : ScopedBodyDocument) (entry : Nat × Nat)
    (checked : checkDerivedRawStoreValueFact fuel names unit result types principal requirements fields rows
      functions owner row parameters declarations occurrences document entry = true)
    (sorted : WellSorted later) (solves : Solves requirements later) :
    ∃ slots, deriveRawSlots fuel (sourceRawNameBytes unit) rows functions owner parameters declarations
        occurrences = some slots ∧
      (slots.map Prod.fst).Nodup ∧ (slots.map Prod.snd).Nodup ∧
      (∀ index binder, (parameters.map Binding.binder)[index]? = some binder → (binder, index) ∈ slots) ∧
      (∀ declaration ∈ sourceLocals occurrences, ∃ evidence,
        localEvidence fuel (sourceRawNameBytes unit) rows functions owner declarations slots declaration = some evidence ∧
        (declaration.binder, evidence.slot) ∈ slots ∧
        claimedLocalRow declarations declaration = some evidence.row ∧
        rawLetDeclaration rows evidence.row = some (evidence.slot, evidence.value) ∧
        evidence.row ∈ rawFunctionStatements rows functions owner ∧
        (∃ fact ∈ declarations.facts, fact.instruction = evidence.row ∧ fact.mutable = declaration.mutable) ∧
        nodeRawValueTree (sourceRawNameBytes unit) slots fuel declaration.initializer = some evidence.tree) ∧
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
  obtain ⟨slots, derived, valued⟩ := checked_derived_raw_store_value_uses_derived_slots fuel names unit result
    types principal requirements fields rows functions owner row parameters declarations occurrences document entry checked
  obtain ⟨_, uniqueBinders, uniqueSlots, parametersHeld, localsHeld⟩ :=
    derive_raw_slots_sound fuel (sourceRawNameBytes unit) rows functions owner parameters declarations occurrences
      slots derived
  refine ⟨slots, derived, uniqueBinders, uniqueSlots, parametersHeld, ?_,
    checked_raw_store_value_types_every_solution fuel names unit result types principal later requirements fields
      rows functions owner row slots occurrences document entry valued sorted solves⟩
  intro declaration member
  obtain ⟨evidence, found, held⟩ := localsHeld declaration member
  obtain ⟨rowEq, slotEq, letEq, treeEq⟩ := local_evidence_sound fuel (sourceRawNameBytes unit) rows functions owner
    declarations slots declaration evidence found
  obtain ⟨row', value', rowAgain, letAgain, statements, factWitness⟩ :=
    claimed_local_slot_sound rows functions owner declarations declaration evidence.slot slotEq
  rw [rowEq] at rowAgain
  cases rowAgain
  exact ⟨evidence, found, held, rowEq, letEq, statements, factWitness, treeEq⟩

end Xsh.Conformance.Connected
