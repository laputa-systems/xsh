import Xsh.Semantics.Connected.StoreRuntime
import Xsh.Semantics.Connected.SafetyRecord

namespace Xsh.Semantics.Connected
open Xsh.Foundation Xsh.Typing.Connected

theorem store_compatible_composes {expected actual : MonoType}
    (compatible : StoreCompatible expected actual) :
    ∀ expectedRuntime actualRuntime target,
      monoRuntime expected = some expectedRuntime → monoRuntime actual = some actualRuntime →
      typeFits actualRuntime target = true → typeFits expectedRuntime target = true := by
  induction compatible using StoreCompatible.rec
    (motive_2 := fun expected actual _ => ∀ expectedRuntime actualRuntime target,
      monoRuntime expected = some expectedRuntime → monoRuntime actual = some actualRuntime →
      rowFits actualRuntime target = true → rowFits expectedRuntime target = true) with
  | same ty =>
    intro expectedRuntime actualRuntime target expected actual fits
    rw [expected] at actual
    cases actual
    exact fits
  | @result expectedValue expectedError actualValue actualError cv ce valueIH errorIH =>
    intro expectedRuntime actualRuntime target expected actual fits
    cases ev : monoRuntime expectedValue <;> cases av : monoRuntime actualValue <;>
      cases ee : monoRuntime expectedError <;> cases ae : monoRuntime actualError <;>
      simp_all [monoRuntime]
    all_goals subst_vars
    all_goals cases target <;> simp_all [typeFits]
    rcases fits with ⟨rfl, rfl⟩ | ⟨valueFits, errorFits⟩
    · exact Or.inr ⟨store_compatible_runtime cv _ _ ev av, store_compatible_runtime ce _ _ ee ae⟩
    · exact Or.inr ⟨valueIH _ _ _ rfl rfl valueFits, errorIH _ _ _ rfl rfl errorFits⟩
  | @record expectedRow actualRow compatible rowIH =>
    intro expectedRuntime actualRuntime target expected actual fits
    cases er : monoRuntime expectedRow <;> cases ar : monoRuntime actualRow <;> simp_all [monoRuntime]
    all_goals subst_vars
    all_goals cases target <;> simp_all [typeFits]
    rcases fits with same | rows
    · subst_vars
      rename_i expectedRowRuntime actualRowRuntime
      have bridge := store_compatible_runtime (StoreCompatible.record compatible)
        (.record expectedRowRuntime) (.record actualRowRuntime)
        (by simp [monoRuntime, er]) (by simp [monoRuntime, ar])
      simpa [typeFits] using bridge
    · exact Or.inr (rowIH _ _ _ rfl rfl rows)
  | @nominal identity expectedBase actualBase compatible baseIH =>
    intro expectedRuntime actualRuntime target expected actual fits
    cases eb : monoRuntime expectedBase <;> cases ab : monoRuntime actualBase <;> simp_all [monoRuntime]
    all_goals subst_vars
    all_goals cases target <;> simp_all [typeFits]
    rcases fits with ⟨sameId, sameBase⟩ | ⟨sameId, baseFits⟩
    · subst_vars
      exact Or.inr ⟨rfl, store_compatible_runtime compatible _ _ eb ab⟩
    · exact Or.inr ⟨sameId, baseIH _ _ _ rfl rfl baseFits⟩
  | empty =>
    rename_i expectedRuntime actualRuntime target expected actual fits
    simp [monoRuntime] at expected
    subst expectedRuntime
    rfl
  | @field name expectedValue expectedTail actualRow selected found cv cr valueIH restIH =>
    rename_i expectedRuntime actualRuntime target expected actual fits
    cases ev : monoRuntime expectedValue with
    | none => simp [monoRuntime, ev] at expected
    | some runtimeValue =>
      cases et : monoRuntime expectedTail with
      | none => simp [monoRuntime, ev, et] at expected
      | some runtimeTail =>
        simp [monoRuntime, ev, et] at expected
        subst expectedRuntime
        obtain ⟨runtimeSelected, selectedClosed, selectedAt⟩ := runtime_field_occurs found actualRuntime actual
        obtain ⟨targetValue, targetAt, targetFits⟩ := row_fits_field actualRuntime target name runtimeSelected fits selectedAt
        have valueFits := valueIH runtimeValue runtimeSelected targetValue ev selectedClosed targetFits
        have tailFits := restIH runtimeTail actualRuntime target et actual fits
        simp [rowFits, targetAt, valueFits, tailFits]

end Xsh.Semantics.Connected
