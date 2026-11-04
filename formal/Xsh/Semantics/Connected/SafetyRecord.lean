import Xsh.Semantics.Connected.Machine

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

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

theorem runtime_insert_field_perm (field : NameId × RuntimeType) (fields : List (NameId × RuntimeType)) :
    (insertField field fields).Perm (field :: fields) := by
  induction fields with
  | nil => rfl
  | cons head rest ih =>
    simp only [insertField]
    split
    · rfl
    · exact (List.Perm.cons head ih).trans (List.Perm.swap field head rest)

theorem runtime_canonical_fields_perm (fields : List (NameId × RuntimeType)) :
    (canonicalFields fields).Perm fields := by
  induction fields with
  | nil => rfl
  | cons field rest ih =>
    exact (runtime_insert_field_perm field (canonicalFields rest)).trans (List.Perm.cons field ih)

theorem runtime_row_field_member (fields : List (NameId × RuntimeType)) (name : NameId) (ty : RuntimeType)
    (found : rowFieldType name (fields.foldr (fun (name, ty) row => .rowField name ty row) .rowEmpty) = some ty) :
    (name, ty) ∈ fields := by
  induction fields with
  | nil => simp [rowFieldType] at found
  | cons field rest ih =>
    obtain ⟨other, value⟩ := field
    simp only [List.foldr_cons, rowFieldType] at found
    split at found
    · rename_i same
      subst other
      cases found
      simp
    · exact List.mem_cons_of_mem _ (ih found)

theorem field_occurrence_runtime {name : NameId} {value row : MonoType}
    {runtimeValue runtimeRow : RuntimeType} (field : FieldOccurs name value row)
    (valueClosed : monoRuntime value = some runtimeValue) (rowClosed : monoRuntime row = some runtimeRow) :
    rowFieldType name runtimeRow = some runtimeValue := by
  induction field generalizing runtimeRow with
  | @here value tail =>
    cases tailClosed : monoRuntime tail with
    | none => simp [monoRuntime, valueClosed, tailClosed] at rowClosed
    | some runtimeTail =>
      simp [monoRuntime, valueClosed, tailClosed] at rowClosed
      subst runtimeRow
      simp [rowFieldType]
  | @later other value head tail different tailField ih =>
    cases headClosed : monoRuntime head with
    | none => simp [monoRuntime, headClosed] at rowClosed
    | some runtimeHead =>
      cases tailClosed : monoRuntime tail with
      | none => simp [monoRuntime, headClosed, tailClosed] at rowClosed
      | some runtimeTail =>
        simp [monoRuntime, headClosed, tailClosed] at rowClosed
        subst runtimeRow
        simp [rowFieldType, different, ih valueClosed tailClosed]

theorem row_fits_field (expected actual : RuntimeType) (name : NameId) (ty : RuntimeType)
    (fits : rowFits expected actual = true) (found : rowFieldType name expected = some ty) :
    ∃ actualTy, rowFieldType name actual = some actualTy ∧ typeFits ty actualTy = true := by
  induction expected with
  | rowField head value tail valueIH tailIH =>
    by_cases same : name = head
    · subst name
      simp [rowFieldType] at found
      subst ty
      cases actualField : rowFieldType head actual with
      | none => simp [rowFits, actualField] at fits
      | some actualTy =>
        have parts : typeFits value actualTy = true ∧ rowFits tail actual = true := by
          simpa [rowFits, actualField] using fits
        exact ⟨actualTy, rfl, parts.1⟩
    · have tailFound : rowFieldType name tail = some ty := by simpa [rowFieldType, same] using found
      cases actualField : rowFieldType head actual with
      | none => simp [rowFits, actualField] at fits
      | some actualTy =>
        have parts : typeFits value actualTy = true ∧ rowFits tail actual = true := by
          simpa [rowFits, actualField] using fits
        exact tailIH parts.2 tailFound
  | _ => simp [rowFieldType] at found

theorem compatible_field_lookup (fields : List (NameId × Value)) (name : NameId)
    (row valueType' : MonoType) (runtimeRow runtimeValue : RuntimeType)
    (typed : HasType (.record runtimeRow) (.record fields))
    (field : FieldOccurs name valueType' row) (rowClosed : monoRuntime row = some runtimeRow)
    (valueClosed : monoRuntime valueType' = some runtimeValue) :
    ∃ value, fields.lookup name = some value ∧ HasType runtimeValue value := by
  have expectedField := field_occurrence_runtime field valueClosed rowClosed
  by_cases unique : (fields.map Prod.fst).Nodup
  · cases recordType : recordTypes fields with
    | none => simp [HasType, valueType, unique, recordType] at typed
    | some types =>
      let actualRow := (canonicalFields types).foldr
        (fun (name, ty) row => RuntimeType.rowField name ty row) RuntimeType.rowEmpty
      have fits : typeFits (.record runtimeRow) (.record actualRow) = true := by
        simpa [HasType, valueType, unique, recordType, actualRow] using typed
      have actualField : ∃ actualTy, rowFieldType name actualRow = some actualTy ∧
          typeFits runtimeValue actualTy = true := by
        by_cases same : runtimeRow = actualRow
        · subst runtimeRow
          exact ⟨runtimeValue, expectedField, by cases runtimeValue <;> simp [typeFits]⟩
        · have rows : rowFits runtimeRow actualRow = true := by
            simpa [typeFits, same] using fits
          exact row_fits_field runtimeRow actualRow name runtimeValue rows expectedField
      obtain ⟨actualTy, found, compatible⟩ := actualField
      have member := runtime_row_field_member (canonicalFields types) name actualTy found
      have actualMember : (name, actualTy) ∈ types := (runtime_canonical_fields_perm types).mem_iff.mp member
      obtain ⟨value, lookup, valueTyped⟩ := record_typed_field_lookup fields types name actualTy recordType unique actualMember
      exact ⟨value, lookup, by simpa [HasType, valueTyped] using compatible⟩
  · simp [HasType, valueType, unique] at typed

theorem typed_field_frame_progress (state : Machine) (fields : List (NameId × Value)) (name : NameId)
    (row valueType' : MonoType) (runtimeRow runtimeValue : RuntimeType)
    (typed : valueType (.record fields) = some (.record runtimeRow))
    (field : FieldOccurs name valueType' row) (rowClosed : monoRuntime row = some runtimeRow)
    (valueClosed : monoRuntime valueType' = some runtimeValue) :
    ∃ value, Step { state with control := .value (.record fields), stack := .field name :: state.stack }
        .internal { state with control := .value value } ∧ valueType value = some runtimeValue := by
  have runtimeField := field_occurrence_runtime field valueClosed rowClosed
  by_cases unique : (fields.map Prod.fst).Nodup
  · cases recordType : recordTypes fields with
    | none => simp [valueType, unique, recordType] at typed
    | some types =>
      simp [valueType, unique, recordType] at typed
      subst runtimeRow
      have member := runtime_row_field_member (canonicalFields types) name runtimeValue runtimeField
      have actualMember : (name, runtimeValue) ∈ types := (runtime_canonical_fields_perm types).mem_iff.mp member
      obtain ⟨value, found, valueTyped⟩ := record_typed_field_lookup fields types name runtimeValue recordType unique actualMember
      exact ⟨value, .fieldValue state name fields value found, valueTyped⟩
  · simp [valueType, unique] at typed

end Xsh.Semantics.Connected
