import Xsh.Validation.Core.Widen

namespace Xsh.Validation.Core.Erase

open Xsh.Core
open Xsh.Validation.Core
open Xsh.Validation.Core.Widen

variable {Declaration Identity Source : Type}

theorem erase_nominal_member (world : MembershipWorld Declaration Identity Source)
    {declaration : Declaration} {fields : RecordType Declaration} {candidate : Value Declaration Identity Source}
    (member : Membership.value world (.nominal declaration fields) candidate) :
    ∃ contents, candidate = .nominal declaration contents ∧
      eraseValidated (.nominal declaration fields) candidate = contents := by
  cases candidate with
  | nominal actual contents =>
    simp only [Membership.value] at member
    exact ⟨contents, by rw [member.1], rfl⟩
  | _ => simp [Membership.value] at member

theorem erase_keeps_unwrapped (type : ValueType Declaration) (candidate : Value Declaration Identity Source)
    (notNominal : ∀ declaration fields, type ≠ .nominal declaration fields) :
    eraseValidated type candidate = candidate := by
  cases type <;> try rfl
  exact absurd rfl (notNominal _ _)

theorem erase_member (world : MembershipWorld Declaration Identity Source)
    {validated baseType : ValueType Declaration} {candidate : Value Declaration Identity Source}
    (hasBase : base validated = some baseType) (member : Membership.value world validated candidate) :
    Membership.value world baseType (eraseValidated validated candidate) := by
  cases validated
  case nominal declaration fields =>
    obtain ⟨contents, rfl, erased⟩ := erase_nominal_member world member
    rw [erased]
    simp only [base, Option.some.injEq] at hasBase
    subst hasBase
    simp only [Membership.value] at member ⊢
    simpa [Membership.recordFields] using member.2
  all_goals
    rw [erase_keeps_unwrapped _ _ (fun _ _ => by simp)]
    exact base_member world hasBase member

theorem erase_references (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    (eraseValidated type candidate).references = candidate.references := by
  cases type <;> try rfl
  cases candidate <;> first | rfl | simp [eraseValidated, Value.references]

theorem erase_reads_as_base (type : ValueType Declaration) (candidate : Value Declaration Identity Source) :
    (eraseValidated type candidate).eraseNominal = candidate.eraseNominal := by
  cases type <;> try rfl
  cases candidate <;> first | rfl | simp [eraseValidated, Value.eraseNominal]

/-- Validated erasure over every Core validated type and value. Reading a
member of a validated type at its base yields a member of the base. Erasure
changes nothing for `NonEmpty`, `RelPath` and bounded integers, which are not
runtime wrappers; for a nominal type it removes exactly the declaration
wrapper the member carries. Erasure keeps every reference and every
observation made through the base view. -/
theorem validated_erasure (world : MembershipWorld Declaration Identity Source) :
    (∀ (validated baseType : ValueType Declaration) (candidate : Value Declaration Identity Source),
      base validated = some baseType → Membership.value world validated candidate →
        Membership.value world baseType (eraseValidated validated candidate)) ∧
    (∀ (type : ValueType Declaration) (candidate : Value Declaration Identity Source),
      (∀ declaration fields, type ≠ .nominal declaration fields) → eraseValidated type candidate = candidate) ∧
    (∀ declaration (fields : RecordType Declaration) (candidate : Value Declaration Identity Source),
      Membership.value world (.nominal declaration fields) candidate →
        ∃ contents, candidate = .nominal declaration contents ∧
          eraseValidated (.nominal declaration fields) candidate = contents) ∧
    (∀ (type : ValueType Declaration) (candidate : Value Declaration Identity Source),
      (eraseValidated type candidate).references = candidate.references) ∧
    (∀ (type : ValueType Declaration) (candidate : Value Declaration Identity Source),
      (eraseValidated type candidate).eraseNominal = candidate.eraseNominal) :=
  ⟨fun _ _ _ hasBase member => erase_member world hasBase member, erase_keeps_unwrapped,
    fun _ _ _ member => erase_nominal_member world member, erase_references, erase_reads_as_base⟩

namespace Witness

open Xsh.Validation.Core.Widen.Witness

def packageFields : RecordType Unit := .rowField "name" (.primitive .string) .rowEmpty

def packageRecord : WitnessValue := .record [("name", .scalar (.string "xsh"))]

def package : WitnessValue := .nominal () packageRecord

theorem erased_package_is_its_record_and_loses_identity :
    Membership.value openWorld (.nominal () packageFields) package ∧
      eraseValidated (.nominal () packageFields) package = packageRecord ∧
      Membership.value openWorld (.record packageFields) packageRecord ∧
      ¬ Membership.value openWorld (.nominal () packageFields) packageRecord := by
  refine ⟨?_, rfl, ?_, ?_⟩ <;>
    simp [Membership.value, Membership.row, Membership.recordFields, Membership.primitive, package,
      packageRecord, packageFields, Value.eraseNominal]

theorem erased_relative_path_is_the_same_path :
    eraseValidated (Identity := Unit) (Source := Unit) (.relativePath : WitnessType) relative = relative ∧
      Membership.value openWorld (.primitive .path) relative ∧
      Membership.value openWorld (.primitive .path) absolute ∧
      ¬ Membership.value openWorld .relativePath absolute :=
  ⟨rfl, by simp [Membership.value, Membership.primitive, relative, pathValue, Value.eraseNominal],
    by simp [Membership.value, Membership.primitive, absolute, pathValue, Value.eraseNominal],
    distinct_validations_never_view.2.2.2⟩

end Witness

end Xsh.Validation.Core.Erase
