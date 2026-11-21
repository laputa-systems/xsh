import Xsh.Core.Membership
import Xsh.Core.WireEnums

namespace Xsh.Validation.Core.WellFormed

open Xsh.Core

variable {Declaration : Type}

def rowNames : RecordType Declaration → List String
  | .rowField name _ rest => name :: rowNames rest
  | _ => []

def rowLookup : RecordType Declaration → String → Option (ValueType Declaration)
  | .rowField name type rest, wanted => if name = wanted then some type else rowLookup rest wanted
  | _, _ => none

mutual
  /-- Type well-formedness for the positions a value can occupy: every record
  and nominal schema lists each field once (Rust's
  `check.duplicate-record-field`), and every module contract names each export
  once (Rust's `check.duplicate-name`). -/
  def WellFormed : ValueType Declaration → Prop
    | .list element | .nonEmpty element | .optional element | .set element | .stream element => WellFormed element
    | .map key element => WellFormed key ∧ WellFormed element
    | .record fields | .nominal _ fields => RowWellFormed fields
    | .union first second rest => WellFormed first ∧ WellFormed second ∧ MembersWellFormed rest
    | .result success failure => WellFormed success ∧ WellFormed failure
    | .module _ exports => ExportsWellFormed exports
    | .callable signature => SignatureWellFormed signature
    | _ => True

  def RowWellFormed : RecordType Declaration → Prop
    | .rowField name type rest => name ∉ rowNames rest ∧ WellFormed type ∧ RowWellFormed rest
    | _ => True

  def MembersWellFormed : UnionTail Declaration → Prop
    | .member type rest => WellFormed type ∧ MembersWellFormed rest
    | _ => True

  def ExportsWellFormed : ExportType Declaration → Prop
    | .exportValue name _ type rest => name ∉ Membership.exportNames rest ∧ WellFormed type ∧ ExportsWellFormed rest
    | .exportCallable name _ signature rest =>
        name ∉ Membership.exportNames rest ∧ SignatureWellFormed signature ∧ ExportsWellFormed rest
    | .exportGenericCallable name _ _ rest => name ∉ Membership.exportNames rest ∧ ExportsWellFormed rest
    | _ => True

  def SignatureWellFormed : SignatureType Declaration → Prop
    | .signature _ parameters output => ArgumentsWellFormed parameters ∧ WellFormed output
    | _ => True

  def ArgumentsWellFormed : TypeVariableFreeTerm 0 String Declaration .arguments → Prop
    | .parameter _ _ type rest => WellFormed type ∧ ArgumentsWellFormed rest
    | .restParameter _ element => WellFormed element
    | _ => True
end

/-- A Str-backed enum lists at least one variant, each variant once, and each
wire string once. -/
def WireWellFormed (enums : WireEnums Declaration) : Prop :=
  ∀ declaration mapping, enums.strings declaration = some mapping →
    mapping ≠ [] ∧ (mapping.map Prod.fst).Nodup ∧ (mapping.map Prod.snd).Nodup

theorem rowLookup_mem_names : ∀ {row : RecordType Declaration} {name : String} {type : ValueType Declaration},
    rowLookup row name = some type → name ∈ rowNames row
  | .variable id, _, _, _ => nomatch id
  | .rowEmpty, _, _, found => by simp [rowLookup] at found
  | .rowField other _ rest, name, type, found => by
    simp only [rowLookup] at found
    by_cases same : other = name
    · simp [rowNames, same]
    · simp only [same, ite_false] at found
      simp [rowNames, rowLookup_mem_names found]

theorem rowLookup_none : ∀ {row : RecordType Declaration} {name : String},
    name ∉ rowNames row → rowLookup row name = none
  | .variable id, _, _ => nomatch id
  | .rowEmpty, _, _ => by simp [rowLookup]
  | .rowField other _ rest, name, absent => by
    simp only [rowNames, List.mem_cons, not_or] at absent
    simp [rowLookup, Ne.symm absent.1, rowLookup_none absent.2]

theorem variant_unique_of_nodup {first second text : String} :
    ∀ {mapping : List (String × String)}, (mapping.map Prod.snd).Nodup →
      (first, text) ∈ mapping → (second, text) ∈ mapping → first = second
  | [], _, present, _ => by simp at present
  | head :: tail, distinct, firstPresent, secondPresent => by
    simp only [List.map_cons, List.nodup_cons, List.mem_map, not_exists, not_and] at distinct
    simp only [List.mem_cons] at firstPresent secondPresent
    rcases firstPresent with firstHead | firstRest <;> rcases secondPresent with secondHead | secondRest
    · rw [← firstHead] at secondHead
      exact (Prod.mk.inj secondHead).1.symm
    · exact absurd (by rw [← firstHead]) (distinct.1 (second, text) secondRest)
    · exact absurd (by rw [← secondHead]) (distinct.1 (first, text) firstRest)
    · exact variant_unique_of_nodup distinct.2 firstRest secondRest

theorem wire_variant_unique {enums : WireEnums Declaration} (wellFormed : WireWellFormed enums)
    {declaration : Declaration} {mapping : List (String × String)}
    (declared : enums.strings declaration = some mapping) {first second text : String}
    (firstPresent : (first, text) ∈ mapping) (secondPresent : (second, text) ∈ mapping) : first = second :=
  variant_unique_of_nodup (wellFormed declaration mapping declared).2.2 firstPresent secondPresent

theorem rowNames_eq_rowEntries : ∀ row : RecordType Declaration, rowNames row = (rowEntries row).map Prod.fst
  | .variable id => nomatch id
  | .rowEmpty => by simp [rowNames, rowEntries]
  | .rowField name _ rest => by simp only [rowNames, rowEntries, List.map_cons, rowNames_eq_rowEntries rest]

mutual
  theorem rowsDistinct_of_wellFormed : ∀ type : ValueType Declaration, WellFormed type → RowsDistinct type
    | .variable id, _ => nomatch id
    | .primitive _, _ | .functionClass _, _ | .schemeCallable _, _ | .tagged _, _ | .errorFamily _, _
    | .errorVariant _ _, _ | .errorFacet _, _ | .relativePath, _ | .boundedInteger _, _ => trivial
    | .list element, wellFormed | .nonEmpty element, wellFormed | .optional element, wellFormed
    | .set element, wellFormed | .stream element, wellFormed => by
      simp only [WellFormed] at wellFormed
      simp only [RowsDistinct]
      exact rowsDistinct_of_wellFormed element wellFormed
    | .map key element, wellFormed | .result key element, wellFormed => by
      simp only [WellFormed] at wellFormed
      simp only [RowsDistinct]
      exact ⟨rowsDistinct_of_wellFormed key wellFormed.1, rowsDistinct_of_wellFormed element wellFormed.2⟩
    | .record fields, wellFormed | .nominal _ fields, wellFormed => by
      simp only [WellFormed] at wellFormed
      simp only [RowsDistinct]
      exact row_rowsDistinct_of_wellFormed fields wellFormed
    | .union first second rest, wellFormed => by
      simp only [WellFormed] at wellFormed
      simp only [RowsDistinct]
      exact ⟨rowsDistinct_of_wellFormed first wellFormed.1, rowsDistinct_of_wellFormed second wellFormed.2.1,
        members_rowsDistinct_of_wellFormed rest wellFormed.2.2⟩
    | .module _ exports, wellFormed => by
      simp only [WellFormed] at wellFormed
      simp only [RowsDistinct]
      exact exports_rowsDistinct_of_wellFormed exports wellFormed
    | .callable signature, wellFormed => by
      simp only [WellFormed] at wellFormed
      simp only [RowsDistinct]
      exact signature_rowsDistinct_of_wellFormed signature wellFormed

  theorem row_rowsDistinct_of_wellFormed : ∀ row : RecordType Declaration, RowWellFormed row → RowsDistinct row
    | .variable id, _ => nomatch id
    | .rowEmpty, _ => trivial
    | .rowField name type rest, wellFormed => by
      simp only [RowWellFormed] at wellFormed
      simp only [RowsDistinct]
      exact ⟨rowNames_eq_rowEntries rest ▸ wellFormed.1, rowsDistinct_of_wellFormed type wellFormed.2.1,
        row_rowsDistinct_of_wellFormed rest wellFormed.2.2⟩

  theorem members_rowsDistinct_of_wellFormed : ∀ members : UnionTail Declaration,
      MembersWellFormed members → RowsDistinct members
    | .variable id, _ => nomatch id
    | .membersEmpty, _ => trivial
    | .member type rest, wellFormed => by
      simp only [MembersWellFormed] at wellFormed
      simp only [RowsDistinct]
      exact ⟨rowsDistinct_of_wellFormed type wellFormed.1, members_rowsDistinct_of_wellFormed rest wellFormed.2⟩

  theorem exports_rowsDistinct_of_wellFormed : ∀ exports : ExportType Declaration,
      ExportsWellFormed exports → RowsDistinct exports
    | .variable id, _ => nomatch id
    | .exportsEmpty, _ => trivial
    | .exportValue _ _ type rest, wellFormed => by
      simp only [ExportsWellFormed] at wellFormed
      simp only [RowsDistinct]
      exact ⟨rowsDistinct_of_wellFormed type wellFormed.2.1, exports_rowsDistinct_of_wellFormed rest wellFormed.2.2⟩
    | .exportCallable _ _ signature rest, wellFormed => by
      simp only [ExportsWellFormed] at wellFormed
      simp only [RowsDistinct]
      exact ⟨signature_rowsDistinct_of_wellFormed signature wellFormed.2.1,
        exports_rowsDistinct_of_wellFormed rest wellFormed.2.2⟩
    | .exportGenericCallable _ _ _ rest, wellFormed => by
      simp only [ExportsWellFormed] at wellFormed
      simp only [RowsDistinct]
      exact exports_rowsDistinct_of_wellFormed rest wellFormed.2

  theorem signature_rowsDistinct_of_wellFormed : ∀ signature : SignatureType Declaration,
      SignatureWellFormed signature → RowsDistinct signature
    | .variable id, _ => nomatch id
    | .signature _ parameters output, wellFormed => by
      simp only [SignatureWellFormed] at wellFormed
      simp only [RowsDistinct]
      exact ⟨arguments_rowsDistinct_of_wellFormed parameters wellFormed.1,
        rowsDistinct_of_wellFormed output wellFormed.2⟩

  theorem arguments_rowsDistinct_of_wellFormed : ∀ arguments : TypeVariableFreeTerm 0 String Declaration .arguments,
      ArgumentsWellFormed arguments → RowsDistinct arguments
    | .variable id, _ => nomatch id
    | .argumentsEmpty, _ => trivial
    | .parameter _ _ type rest, wellFormed => by
      simp only [ArgumentsWellFormed] at wellFormed
      simp only [RowsDistinct]
      exact ⟨rowsDistinct_of_wellFormed type wellFormed.1, arguments_rowsDistinct_of_wellFormed rest wellFormed.2⟩
    | .restParameter _ element, wellFormed => by
      simp only [ArgumentsWellFormed] at wellFormed
      simp only [RowsDistinct]
      exact rowsDistinct_of_wellFormed element wellFormed
end

end Xsh.Validation.Core.WellFormed
