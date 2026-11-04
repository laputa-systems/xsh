import Xsh.Semantics.Connected.SafetyValueBlockRecordFinish
import Xsh.Semantics.Connected.SafetyValueBlockCompatibility

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Typing.Connected

theorem field_lookup_preserves_callables (functions : FunctionTable) (fields : List (NameId × Value))
    (safe : FieldCallables functions fields) (name : NameId) (value : Value)
    (found : fields.lookup name = some value) : ValueCallables functions value := by
  induction fields with
  | nil => simp at found
  | cons field rest ih =>
    obtain ⟨fieldName, fieldValue⟩ := field
    cases safe with
    | cons callable restSafe =>
      by_cases same : name = fieldName
      · subst name
        simp at found
        cases found
        exact callable
      · simp only [List.lookup_cons] at found
        have unequal : (name == fieldName) = false := by
          change decide (name = fieldName) = false
          simp [same]
        rw [unequal] at found
        exact ih restSafe found

theorem compatible_record_field_is_callable (functions : FunctionTable) (fields : List (NameId × Value))
    (safe : ValueCallables functions (.record fields)) (name : NameId) (value : Value)
    (found : fields.lookup name = some value) : ValueCallables functions value := by
  cases safe with
  | record fieldSafe => exact field_lookup_preserves_callables functions fields fieldSafe name value found

end Xsh.Semantics.Connected
