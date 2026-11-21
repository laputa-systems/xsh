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
  schema, nominal schema and error payload row lists each field once (Rust's
  `check.duplicate-record-field`), and every module contract names each export
  once. -/
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

end Xsh.Validation.Core.WellFormed
