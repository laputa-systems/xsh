import Xsh.Semantics.Connected.Machine

namespace Xsh.Semantics.Connected
open Xsh.Foundation Xsh.Typing.Connected Xsh.Semantics.Environments Xsh.Values

theorem runtime_field_occurs {name : NameId} {value row : MonoType}
    (found : FieldOccurs name value row) :
    ∀ runtimeRow, monoRuntime row = some runtimeRow →
      ∃ runtimeValue, monoRuntime value = some runtimeValue ∧ rowFieldType name runtimeRow = some runtimeValue := by
  induction found with
  | @here value tail =>
    intro runtimeRow converted
    cases v : monoRuntime value with
    | none => simp [monoRuntime, v] at converted
    | some actualValue =>
      cases t : monoRuntime tail with
      | none => simp [monoRuntime, v, t] at converted
      | some actualTail =>
        simp [monoRuntime, v, t] at converted
        subst runtimeRow
        exact ⟨actualValue, rfl, by simp [rowFieldType]⟩
  | @later other value head tail different found ih =>
    intro runtimeRow converted
    cases h : monoRuntime head with
    | none => simp [monoRuntime, h] at converted
    | some actualHead =>
      cases t : monoRuntime tail with
      | none => simp [monoRuntime, h, t] at converted
      | some actualTail =>
        simp [monoRuntime, h, t] at converted
        subst runtimeRow
        obtain ⟨runtimeValue, v, present⟩ := ih actualTail t
        exact ⟨runtimeValue, v, by simpa [rowFieldType, different] using present⟩

theorem store_compatible_runtime {expected actual : MonoType}
    (compatible : StoreCompatible expected actual) :
    ∀ expectedRuntime actualRuntime,
      monoRuntime expected = some expectedRuntime → monoRuntime actual = some actualRuntime →
      typeFits expectedRuntime actualRuntime = true := by
  induction compatible using StoreCompatible.rec
    (motive_2 := fun expected actual _ => ∀ expectedRuntime actualRuntime,
      monoRuntime expected = some expectedRuntime → monoRuntime actual = some actualRuntime →
      rowFits expectedRuntime actualRuntime = true) with
  | same ty =>
    intro expectedRuntime actualRuntime left right
    rw [left] at right
    cases right
    cases expectedRuntime <;> simp [typeFits]
  | @result expectedValue expectedError actualValue actualError cv ce left right =>
    intro expectedRuntime actualRuntime expected actual
    cases ev : monoRuntime expectedValue <;> cases av : monoRuntime actualValue <;>
      cases ee : monoRuntime expectedError <;> cases ae : monoRuntime actualError <;>
      simp_all [monoRuntime]
    all_goals subst_vars; simp_all [typeFits]
  | @record expectedRow actualRow compatible row =>
    intro expectedRuntime actualRuntime expected actual
    cases er : monoRuntime expectedRow <;> cases ar : monoRuntime actualRow <;>
      simp_all [monoRuntime]
    all_goals subst_vars; simp_all [typeFits]
  | @nominal identity expectedBase actualBase compatible base =>
    intro expectedRuntime actualRuntime expected actual
    cases eb : monoRuntime expectedBase <;> cases ab : monoRuntime actualBase <;>
      simp_all [monoRuntime]
    all_goals subst_vars; simp_all [typeFits]
  | empty =>
    simp_all [monoRuntime]
    all_goals subst_vars; simp_all [rowFits]
  | @field name expectedValue expectedTail actualRow selected found cv cr value rest =>
    rename_i expectedRuntime actualRuntime expected actual
    cases ev : monoRuntime expectedValue with
    | none => simp [monoRuntime, ev] at expected
    | some runtimeValue =>
      cases et : monoRuntime expectedTail with
      | none => simp [monoRuntime, ev, et] at expected
      | some runtimeTail =>
        simp [monoRuntime, ev, et] at expected
        subst expectedRuntime
        obtain ⟨runtimeSelected, selectedClosed, selectedAt⟩ := runtime_field_occurs found actualRuntime actual
        have fieldFits := value runtimeValue runtimeSelected ev selectedClosed
        have restFits := rest runtimeTail actualRuntime et actual
        simp [rowFits, selectedAt, fieldFits, restFits]

def widthFieldA : NameId := ⟨⟨0, by decide⟩⟩
def widthFieldB : NameId := ⟨⟨1, by decide⟩⟩
def widthTag : RuntimeType := .record (.rowField widthFieldA .boolean .rowEmpty)
def widthInitial : Value := .record [(widthFieldA, .scalar (.boolean true)), (widthFieldB, .scalar (.boolean false))]
def widthReplacement : Value := .record [(widthFieldA, .scalar (.boolean false))]
def widthBad : Value := .record [(widthFieldB, .scalar (.boolean true))]
def widthCell : CellId := ⟨0, ⟨⟨0, by decide⟩⟩⟩
def widthStore : Store := fun _ => none

def keepsWiderValueAndAcceptsNarrowReplacement : Bool :=
  match allocate widthStore widthCell .mutable widthTag widthInitial with
  | none => false
  | some initial =>
    match initial widthCell, write initial widthCell widthReplacement, write initial widthCell widthBad with
    | some entry, .accepted final, .rejected _ =>
      match entry.contents.value, final widthCell with
      | .record [(first, .scalar (.boolean true)), (second, .scalar (.boolean false))], some updated =>
        match updated.contents.value with
        | .record [(name, .scalar (.boolean false))] =>
          decide (first = widthFieldA ∧ second = widthFieldB ∧ name = widthFieldA ∧
            entry.contents.typeTag = widthTag ∧ updated.contents.typeTag = widthTag)
        | _ => false
      | _, _ => false
    | _, _, _ => false

theorem declared_storage_preserves_value_and_controls_writes :
    keepsWiderValueAndAcceptsNarrowReplacement = true := by
  decide +kernel


end Xsh.Semantics.Connected
