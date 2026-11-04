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

end Xsh.Semantics.Connected
