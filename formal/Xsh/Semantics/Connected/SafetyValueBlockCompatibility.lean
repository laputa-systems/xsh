import Xsh.Semantics.Connected.SafetyValueBlockStatements

namespace Xsh.Semantics.Connected

theorem boolean_fit_is_boolean (actual : RuntimeType) (fits : typeFits .boolean actual = true) : actual = .boolean := by
  cases actual <;> simp_all [typeFits]

theorem integer_fit_is_integer (actual : RuntimeType) (fits : typeFits .integer actual = true) : actual = .integer := by
  cases actual <;> simp_all [typeFits]

theorem string_fit_is_string (actual : RuntimeType) (fits : typeFits .string actual = true) : actual = .string := by
  cases actual <;> simp_all [typeFits]

theorem result_fit_has_components (expectedValue expectedError actual : RuntimeType)
    (fits : typeFits (.result expectedValue expectedError) actual = true) :
    ∃ actualValue actualError, actual = .result actualValue actualError ∧
      typeFits expectedValue actualValue = true ∧ typeFits expectedError actualError = true := by
  cases actual <;> simp_all [typeFits]
  case result value failure =>
    rcases fits with equal | components
    · obtain ⟨rfl, rfl⟩ := equal
      exact ⟨expectedValue, expectedError, ⟨rfl, rfl⟩,
        by cases expectedValue <;> simp [typeFits], by cases expectedError <;> simp [typeFits]⟩
    · exact ⟨value, failure, ⟨rfl, rfl⟩, components.1, components.2⟩

theorem record_fit_has_row (expected actual : RuntimeType)
    (fits : typeFits (.record expected) actual = true) :
    ∃ row, actual = .record row ∧ (expected = row ∨ rowFits expected row = true) := by
  cases actual <;> simp_all [typeFits]

theorem row_fit_preserves_field (expected actual : RuntimeType) (name : Xsh.Foundation.NameId) (expectedType : RuntimeType)
    (fits : rowFits expected actual = true) (present : rowFieldType name expected = some expectedType) :
    ∃ actualType, rowFieldType name actual = some actualType ∧ typeFits expectedType actualType = true := by
  induction expected with
  | rowField field value tail valueIh tailIh =>
    cases found : rowFieldType field actual with
    | none => simp [rowFits, found] at fits
    | some actualValue =>
      have both : typeFits value actualValue = true ∧ rowFits tail actual = true := by
        simpa [rowFits, found] using fits
      by_cases same : name = field
      · subst name
        simp [rowFieldType] at present
        subst expectedType
        exact ⟨actualValue, found, both.1⟩
      · have tailPresent : rowFieldType name tail = some expectedType := by
          simpa [rowFieldType, same] using present
        exact tailIh both.2 tailPresent
  | _ => simp [rowFieldType] at present

theorem compatible_record_field_progress (state : Machine) (fields : List (Xsh.Foundation.NameId × Value))
    (name : Xsh.Foundation.NameId) (row fieldType : Xsh.Typing.Connected.MonoType)
    (expectedRow expectedField actualRow : RuntimeType)
    (typed : valueType (.record fields) = some (.record actualRow))
    (field : Xsh.Typing.Connected.FieldOccurs name fieldType row)
    (rowClosed : monoRuntime row = some expectedRow) (fieldClosed : monoRuntime fieldType = some expectedField)
    (compatible : rowFits expectedRow actualRow = true) :
    ∃ value actualField, Step { state with control := .value (.record fields), stack := .field name :: state.stack }
        .internal { state with control := .value value } ∧ valueType value = some actualField ∧
      typeFits expectedField actualField = true := by
  have expectedAt := field_occurrence_runtime field fieldClosed rowClosed
  obtain ⟨actualField, actualAt, fieldFits⟩ := row_fit_preserves_field expectedRow actualRow name expectedField compatible expectedAt
  by_cases unique : (fields.map Prod.fst).Nodup
  · cases recordType : recordTypes fields with
    | none => simp [valueType, unique, recordType] at typed
    | some fieldTypes =>
      simp [valueType, unique, recordType] at typed
      subst actualRow
      have member := runtime_row_field_member (canonicalFields fieldTypes) name actualField actualAt
      have actualMember : (name, actualField) ∈ fieldTypes := (runtime_canonical_fields_perm fieldTypes).mem_iff.mp member
      obtain ⟨value, found, valueTyped⟩ := record_typed_field_lookup fields fieldTypes name actualField recordType unique actualMember
      exact ⟨value, actualField, .fieldValue state name fields value found, valueTyped, fieldFits⟩
  · simp [valueType, unique] at typed

end Xsh.Semantics.Connected
