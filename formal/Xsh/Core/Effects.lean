import Xsh.Effects.Lattice
import Init.Data.Vector.OfFn
import Init.Data.Vector.DecidableEq
import Init.Data.Vector.Lemmas
import Init.Data.List.Nat.Pairwise

namespace Xsh.Core

open Xsh.Syntax Xsh.Semantics.Effects

def effectPosition (effect : Effect) : Fin allEffects.length :=
  ⟨allEffects.idxOf effect, by cases effect <;> decide⟩

structure EffectSet where
  membership : Vector Bool allEffects.length
  deriving DecidableEq

def EffectSet.contains (effects : EffectSet) (effect : Effect) : Bool :=
  effects.membership.get (effectPosition effect)

def EffectSet.ofList (effects : List Effect) : EffectSet :=
  ⟨Vector.ofFn fun position => decide (allEffects[position] ∈ effects)⟩

def EffectSet.denotation (effects : EffectSet) : FiniteEffectSet :=
  fun effect => effects.contains effect = true

theorem EffectSet.ofList_contains (effects : List Effect) (effect : Effect) :
    (EffectSet.ofList effects).contains effect = decide (effect ∈ effects) := by
  simp [EffectSet.ofList, EffectSet.contains, Vector.get, Fin.cast, effectPosition]

theorem EffectSet.ofList_denotation (effects : List Effect) :
    (EffectSet.ofList effects).denotation = Xsh.Semantics.Effects.ofList effects := by
  funext effect
  simp [EffectSet.denotation, EffectSet.ofList_contains, Xsh.Semantics.Effects.ofList]

namespace EffectSet

def empty : EffectSet := ⟨Vector.replicate _ false⟩

def full : EffectSet := ⟨Vector.replicate _ true⟩

def union (left right : EffectSet) : EffectSet :=
  ⟨Vector.zipWith (· || ·) left.membership right.membership⟩

def intersection (left right : EffectSet) : EffectSet :=
  ⟨Vector.zipWith (· && ·) left.membership right.membership⟩

def without (left right : EffectSet) : EffectSet :=
  ⟨Vector.zipWith (fun present removed => present && !removed) left.membership right.membership⟩

theorem ext {left right : EffectSet}
    (equal : ∀ index : Fin allEffects.length, left.membership.get index = right.membership.get index) :
    left = right := by
  cases left with
  | mk left =>
    cases right with
    | mk right =>
      congr 1
      apply Vector.ext
      intro index bounded
      exact equal ⟨index, bounded⟩

theorem ext_contains {left right : EffectSet}
    (equal : ∀ effect, left.contains effect = right.contains effect) : left = right := by
  apply ext
  intro index
  have position : effectPosition allEffects[index] = index := by
    apply Fin.ext
    exact listed_effects_are_distinct.idxOf_getElem index.val index.isLt
  have result := equal allEffects[index]
  simpa only [contains, position] using result

@[simp] theorem contains_empty (effect : Effect) : empty.contains effect = false := by
  simp [empty, contains, Vector.get]

@[simp] theorem contains_full (effect : Effect) : full.contains effect = true := by
  simp [full, contains, Vector.get]

@[simp] theorem contains_union (left right : EffectSet) (effect : Effect) :
    (union left right).contains effect = (left.contains effect || right.contains effect) := by
  simp [union, contains, Vector.get, Fin.cast]

@[simp] theorem contains_intersection (left right : EffectSet) (effect : Effect) :
    (intersection left right).contains effect = (left.contains effect && right.contains effect) := by
  simp [intersection, contains, Vector.get, Fin.cast]

@[simp] theorem contains_without (left right : EffectSet) (effect : Effect) :
    (without left right).contains effect = (left.contains effect && !right.contains effect) := by
  simp [without, contains, Vector.get, Fin.cast]

theorem ext_denotation {left right : EffectSet}
    (equal : ∀ effect, left.denotation effect ↔ right.denotation effect) : left = right := by
  apply ext_contains
  intro effect
  exact Bool.eq_iff_iff.mpr (equal effect)

@[simp] theorem union_empty (effects : EffectSet) : union effects empty = effects := by
  apply ext_contains
  intro effect
  simp

@[simp] theorem empty_union (effects : EffectSet) : union empty effects = effects := by
  apply ext_contains
  intro effect
  simp

theorem union_comm (left right : EffectSet) : union left right = union right left := by
  apply ext_contains
  intro effect
  simp [Bool.or_comm]

theorem union_assoc (first second third : EffectSet) :
    union (union first second) third = union first (union second third) := by
  apply ext_contains
  intro effect
  simp [Bool.or_assoc]

@[simp] theorem union_self (effects : EffectSet) : union effects effects = effects := by
  apply ext_contains
  intro effect
  simp

@[simp] theorem intersection_empty (effects : EffectSet) : intersection effects empty = empty := by
  apply ext_contains
  intro effect
  simp

@[simp] theorem empty_intersection (effects : EffectSet) : intersection empty effects = empty := by
  apply ext_contains
  intro effect
  simp

@[simp] theorem intersection_full (effects : EffectSet) : intersection effects full = effects := by
  apply ext_contains
  intro effect
  simp

@[simp] theorem full_intersection (effects : EffectSet) : intersection full effects = effects := by
  apply ext_contains
  intro effect
  simp

theorem intersection_comm (left right : EffectSet) :
    intersection left right = intersection right left := by
  apply ext_contains
  intro effect
  simp [Bool.and_comm]

theorem intersection_assoc (first second third : EffectSet) :
    intersection (intersection first second) third = intersection first (intersection second third) := by
  apply ext_contains
  intro effect
  simp [Bool.and_assoc]

@[simp] theorem intersection_self (effects : EffectSet) : intersection effects effects = effects := by
  apply ext_contains
  intro effect
  simp

theorem intersection_union (first second third : EffectSet) :
    intersection first (union second third) = union (intersection first second) (intersection first third) := by
  apply ext_contains
  intro effect
  simp [Bool.and_or_distrib_left]

theorem union_intersection (first second third : EffectSet) :
    intersection (union first second) third = union (intersection first third) (intersection second third) := by
  rw [intersection_comm, intersection_union]
  rw [intersection_comm third first, intersection_comm third second]

theorem empty_iff (effects : EffectSet) :
    effects = empty ↔ ∀ effect, effects.denotation effect → False := by
  constructor
  · rintro rfl effect
    simp [denotation]
  · intro absent
    apply ext_denotation
    intro effect
    simp only [denotation, contains_empty, Bool.false_eq_true, iff_false]
    exact absent effect

def included (left right : EffectSet) : Bool :=
  allEffects.all fun effect => !left.contains effect || right.contains effect

theorem included_iff (left right : EffectSet) :
    left.included right = true ↔ le left.denotation right.denotation := by
  simp only [included, List.all_eq_true, Bool.or_eq_true,
    denotation, le]
  constructor
  · intro accepted effect required
    rcases accepted effect (every_effect_is_listed effect) with absent | present
    · simp [required] at absent
    · exact present
  · intro included effect _
    by_cases required : left.contains effect = true
    · exact Or.inr (included effect required)
    · exact Or.inl (by simpa using required)

private def closureDecision (effects : EffectSet) (effect : Effect) :
    Decidable (closure effects.denotation effect) := by
  cases effect <;> dsimp [closure, ioComponent, denotation] <;> infer_instance

def permissionClosure (effects : EffectSet) : EffectSet :=
  ⟨Vector.ofFn fun position =>
    letI := closureDecision effects allEffects[position]
    decide (closure effects.denotation allEffects[position])⟩

theorem permissionClosure_denotation (effects : EffectSet) :
    effects.permissionClosure.denotation = closure effects.denotation := by
  funext effect
  apply propext
  have position : allEffects[(effectPosition effect).val] = effect := by
    exact List.getElem_idxOf (effectPosition effect).isLt
  simp [permissionClosure, denotation, contains, Vector.get, Fin.cast, position]

end EffectSet

inductive EffectBound where
  | known (required : EffectSet)
  | unrestricted
  | unknown
  deriving DecidableEq

def EffectBound.denotation : EffectBound → EffectContract
  | .known required => .checked required.denotation
  | .unrestricted => .unrestricted
  | .unknown => .unknown

end Xsh.Core
