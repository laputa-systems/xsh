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
  case result value failure => exact ⟨value, failure, ⟨rfl, rfl⟩, fits.1, fits.2⟩

theorem record_fit_has_row (expected actual : RuntimeType)
    (fits : typeFits (.record expected) actual = true) :
    ∃ row, actual = .record row ∧ rowFits expected row = true := by
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

end Xsh.Semantics.Connected
