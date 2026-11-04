import Xsh.Semantics.Connected.Values

namespace Xsh.Semantics.Connected

open Xsh.Foundation

theorem record_types_preserve_names (fields : List (NameId × Value))
    (types : List (NameId × RuntimeType)) (typed : recordTypes fields = some types) :
    types.map Prod.fst = fields.map Prod.fst := by
  induction fields generalizing types with
  | nil => simp [recordTypes] at typed; subst types; rfl
  | cons field rest ih =>
    obtain ⟨name, value⟩ := field
    cases first : valueType value with
    | none => simp [recordTypes, first] at typed
    | some ty =>
      cases remaining : recordTypes rest with
      | none => simp [recordTypes, first, remaining] at typed
      | some tail =>
        simp [recordTypes, first, remaining] at typed
        subst types
        simp [ih tail remaining]

theorem record_typed_field_lookup (fields : List (NameId × Value))
    (types : List (NameId × RuntimeType)) (name : NameId) (ty : RuntimeType)
    (typed : recordTypes fields = some types) (unique : (fields.map Prod.fst).Nodup)
    (member : (name, ty) ∈ types) :
    ∃ value, fields.lookup name = some value ∧ valueType value = some ty := by
  induction fields generalizing types with
  | nil => simp [recordTypes] at typed; subst types; simp at member
  | cons field rest ih =>
    obtain ⟨head, value⟩ := field
    cases first : valueType value with
    | none => simp [recordTypes, first] at typed
    | some fieldType =>
      cases remaining : recordTypes rest with
      | none => simp [recordTypes, first, remaining] at typed
      | some tail =>
        simp [recordTypes, first, remaining] at typed
        subst types
        simp only [List.map_cons, List.nodup_cons] at unique
        simp only [List.mem_cons] at member
        rcases member with same | later
        · cases same
          exact ⟨value, by simp, first⟩
        · have different : name ≠ head := by
            intro same
            subst name
            have inNames : head ∈ tail.map Prod.fst := List.mem_map.mpr ⟨(head, ty), later, rfl⟩
            rw [record_types_preserve_names rest tail remaining] at inNames
            exact unique.1 inNames
          obtain ⟨found, lookup, foundType⟩ := ih tail remaining unique.2 later
          refine ⟨found, ?_, foundType⟩
          simp only [List.lookup_cons]
          have unequal : (name == head) = false := by
            change decide (name = head) = false
            simp [different]
          rw [unequal]
          exact lookup

end Xsh.Semantics.Connected
