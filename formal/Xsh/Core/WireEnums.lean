import Xsh.Core.Types

namespace Xsh.Core

/-- The declared wire strings of each Str-backed enum, as `(variant, wire)`
pairs in declaration order. Ordinary enums, tagged unions and error families
declare none. Uniqueness of variants and wire strings is a separate
well-formedness judgment. -/
structure WireEnums (Declaration : Type) where
  strings : Declaration → Option (List (String × String))

namespace WireEnums

variable {Declaration : Type}

def empty : WireEnums Declaration := ⟨fun _ => none⟩

def variantOf (mapping : List (String × String)) (text : String) : Option String :=
  (mapping.find? (fun entry => entry.2 == text)).map Prod.fst

theorem variantOf_mem {mapping : List (String × String)} {text variant : String}
    (found : variantOf mapping text = some variant) : (variant, text) ∈ mapping := by
  unfold variantOf at found
  cases hit : mapping.find? (fun entry => entry.2 == text) with
  | none => simp [hit] at found
  | some entry =>
    simp only [hit, Option.map_some, Option.some.injEq] at found
    have present := List.mem_of_find?_eq_some hit
    have matched := List.find?_some hit
    simp only [beq_iff_eq] at matched
    subst found
    rw [← matched]
    exact present

theorem variantOf_none {mapping : List (String × String)} {text : String}
    (missing : variantOf mapping text = none) : ∀ variant, (variant, text) ∉ mapping := by
  unfold variantOf at missing
  cases hit : mapping.find? (fun entry => entry.2 == text) with
  | none =>
    intro variant present
    have := List.find?_eq_none.mp hit (variant, text) present
    simp at this
  | some entry => simp [hit] at missing

end WireEnums

end Xsh.Core
