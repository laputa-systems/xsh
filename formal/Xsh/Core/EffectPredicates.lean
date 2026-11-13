import Xsh.Core.SymbolicEffects

namespace Xsh.Core

open Xsh.Syntax Xsh.Semantics.Effects

namespace EffectExpression

def closed (expression : EffectExpression n) : Bool :=
  (List.finRange n).all fun parameter => expression.coefficients.get parameter == EffectSet.empty

theorem closed_iff (expression : EffectExpression n) :
    expression.closed = true ↔ ∀ parameter, expression.coefficients.get parameter = EffectSet.empty := by
  simp [closed]

theorem closed_denote (expression : EffectExpression n) (closed : expression.closed = true)
    (assignment : Fin n → FiniteEffectSet) (effect : Effect) :
    expression.denote assignment effect ↔ expression.direct.denotation effect := by
  have absent := (closed_iff expression).mp closed
  simp [denote, absent, EffectSet.denotation]

end EffectExpression

def EffectSet.prohibited (excluded : EffectSet) : EffectSet :=
  ⟨Vector.ofFn fun position =>
    allEffects.any fun effect => excluded.contains effect &&
      (EffectSet.ofList [allEffects[position]]).permissionClosure.contains effect⟩

theorem EffectSet.prohibited_contains (excluded : EffectSet) (required : Effect) :
    excluded.prohibited.contains required =
      (allEffects.any fun permission => excluded.contains permission &&
        (EffectSet.ofList [required]).permissionClosure.contains permission) := by
  cases required <;> rfl

theorem EffectSet.prohibited_iff (excluded : EffectSet) (required : Effect) :
    excluded.prohibited.denotation required ↔ ∃ permission,
      excluded.denotation permission ∧ closure (Xsh.Semantics.Effects.ofList [required]) permission := by
  unfold EffectSet.denotation
  rw [EffectSet.prohibited_contains]
  simp only [List.any_eq_true, Bool.and_eq_true]
  constructor
  · rintro ⟨permission, _, present, covered⟩
    refine ⟨permission, present, ?_⟩
    change (EffectSet.ofList [required]).permissionClosure.denotation permission at covered
    rw [EffectSet.permissionClosure_denotation, EffectSet.ofList_denotation] at covered
    exact covered
  · rintro ⟨permission, present, covered⟩
    refine ⟨permission, every_effect_is_listed permission, present, ?_⟩
    change (EffectSet.ofList [required]).permissionClosure.denotation permission
    rw [EffectSet.permissionClosure_denotation, EffectSet.ofList_denotation]
    exact covered

inductive EffectPredicate (n : Nat) where
  | equal (left right : SymbolicContract n)
  | included (left right : SymbolicContract n)
  | covered (left right : SymbolicContract n)
  | excluded (contract : SymbolicContract n) (effects : EffectSet)
  deriving DecidableEq

namespace EffectPredicate

def satisfaction : EffectPredicate n → Option Bool
  | .equal left right =>
    if left = right then some true
    else match left, right with
      | .known l, .known r =>
        if l.closed && r.closed then some (l.direct == r.direct) else none
      | _, _ => some false
  | .included left right =>
    match left, right with
    | _, .unknown => some true
    | .unknown, _ => some false
    | .known l, .known r =>
      if l = r then some true
      else if l.included r then some true
      else if l.closed && r.closed then some false else none
  | .covered left right =>
    match left, right with
    | _, .unknown => some true
    | .unknown, _ => some false
    | .known l, .known r =>
      if l = r then some true
      else if l.covered r then some true
      else if l.closed && r.closed then some false else none
  | .excluded contract mask =>
    match contract with
    | .unknown => some false
    | .known expression =>
      let prohibited := mask.prohibited
      if expression.direct.intersection prohibited != EffectSet.empty then some false
      else if (List.finRange n).all (fun p => (expression.coefficients.get p).intersection prohibited == EffectSet.empty)
        then some true else none

theorem included_known (left right : EffectExpression n) :
    (EffectPredicate.included (.known left) (.known right)).satisfaction = some true ↔
      ∀ assignment, le (left.denote assignment) (right.denote assignment) := by
  rw [← EffectExpression.included_iff]
  by_cases same : left = right
  · subst right
    simp [satisfaction, EffectExpression.included_refl]
  · simp [satisfaction, same]

theorem covered_known (left right : EffectExpression n) :
    (EffectPredicate.covered (.known left) (.known right)).satisfaction = some true ↔
      ∀ assignment, le (left.denote assignment) (closure (right.denote assignment)) := by
  rw [← EffectExpression.covered_iff]
  by_cases same : left = right
  · subst right
    simp [satisfaction, EffectExpression.covered_refl]
  · simp [satisfaction, same]

theorem included_rejected_witness (left right : EffectExpression n)
    (rejected : (included (.known left) (.known right)).satisfaction = some false) :
    ∃ assignment effect, left.denote assignment effect ∧ ¬ right.denote assignment effect := by
  classical
  apply Classical.byContradiction
  intro missing
  have accepted := (included_known left right).2 (fun assignment effect member =>
    Classical.byContradiction (fun absent => missing ⟨assignment, effect, member, absent⟩))
  rw [rejected] at accepted
  contradiction

theorem covered_rejected_witness (left right : EffectExpression n)
    (rejected : (covered (.known left) (.known right)).satisfaction = some false) :
    ∃ assignment effect, left.denote assignment effect ∧ ¬ closure (right.denote assignment) effect := by
  classical
  apply Classical.byContradiction
  intro missing
  have accepted := (covered_known left right).2 (fun assignment effect member =>
    Classical.byContradiction (fun absent => missing ⟨assignment, effect, member, absent⟩))
  rw [rejected] at accepted
  contradiction

theorem equal_known_sound (left right : EffectExpression n)
    (accepted : (equal (.known left) (.known right)).satisfaction = some true) :
    ∀ assignment effect, left.denote assignment effect ↔ right.denote assignment effect := by
  by_cases same : left = right
  · subst right
    exact fun _ _ => Iff.rfl
  · simp only [satisfaction, SymbolicContract.known.injEq, same, ↓reduceIte] at accepted
    split at accepted
    · rename_i closed
      have direct : left.direct = right.direct := by simpa using accepted
      have both : left.closed = true ∧ right.closed = true := by simpa only [Bool.and_eq_true] using closed
      intro assignment effect
      rw [left.closed_denote both.1, right.closed_denote both.2, direct]
    · contradiction

private theorem equal_known_rejected_closed (left right : EffectExpression n)
    (rejected : (equal (.known left) (.known right)).satisfaction = some false) :
    left.closed = true ∧ right.closed = true := by
  by_cases same : left = right
  · subst right
    simp [satisfaction] at rejected
  · simp only [satisfaction, SymbolicContract.known.injEq, same, ↓reduceIte] at rejected
    split at rejected
    · rename_i closed
      simpa only [Bool.and_eq_true] using closed
    · contradiction

theorem equal_rejected_witness (left right : EffectExpression n)
    (rejected : (equal (.known left) (.known right)).satisfaction = some false) :
    ∃ assignment effect,
      (left.denote assignment effect ∧ ¬ right.denote assignment effect) ∨
      (right.denote assignment effect ∧ ¬ left.denote assignment effect) := by
  classical
  apply Classical.byContradiction
  intro missing
  have equalDenotations : ∀ assignment effect,
      left.denote assignment effect ↔ right.denote assignment effect := by
    intro assignment effect
    constructor
    · intro present
      exact Classical.byContradiction (fun absent =>
        missing ⟨assignment, effect, Or.inl ⟨present, absent⟩⟩)
    · intro present
      exact Classical.byContradiction (fun absent =>
        missing ⟨assignment, effect, Or.inr ⟨present, absent⟩⟩)
  have both := equal_known_rejected_closed left right rejected
  have direct : left.direct = right.direct := by
    apply EffectSet.ext_denotation
    intro effect
    exact (left.closed_denote both.1 (fun _ _ => False) effect).symm.trans
      ((equalDenotations (fun _ _ => False) effect).trans
        (right.closed_denote both.2 (fun _ _ => False) effect))
  have same : left = right := EffectExpression.ext direct (fun parameter => by
    rw [(EffectExpression.closed_iff left).mp both.1 parameter,
      (EffectExpression.closed_iff right).mp both.2 parameter])
  subst right
  simp [satisfaction] at rejected

def Avoids (expression : EffectExpression n) (excluded : EffectSet)
    (assignment : Fin n → FiniteEffectSet) : Prop :=
  ∀ effect, expression.denote assignment effect → ¬ excluded.prohibited.denotation effect

private theorem disjoint_iff (left right : EffectSet) :
    left.intersection right = EffectSet.empty ↔
      ∀ effect, left.denotation effect → ¬ right.denotation effect := by
  rw [EffectSet.empty_iff]
  simp only [EffectSet.denotation, EffectSet.contains_intersection, Bool.and_eq_true, and_imp]

private theorem avoids_empty_assignment (expression : EffectExpression n) (excluded : EffectSet)
    (direct : expression.direct.intersection excluded.prohibited = EffectSet.empty) :
    Avoids expression excluded (fun _ _ => False) := by
  intro effect member prohibited
  have present : expression.direct.denotation effect :=
    (EffectExpression.denote_empty expression effect).mp member
  exact (disjoint_iff expression.direct excluded.prohibited).mp direct effect present prohibited

private theorem universal_avoids_iff (expression : EffectExpression n) (excluded : EffectSet) :
    (∀ assignment, Avoids expression excluded assignment) ↔
      expression.direct.intersection excluded.prohibited = EffectSet.empty ∧
      ∀ parameter, (expression.coefficients.get parameter).intersection excluded.prohibited = EffectSet.empty := by
  constructor
  · intro safe
    constructor
    · apply (disjoint_iff expression.direct excluded.prohibited).mpr
      intro effect direct prohibited
      exact safe (fun _ _ => False) effect (Or.inl direct) prohibited
    · intro parameter
      apply (disjoint_iff _ _).mpr
      intro effect coefficient prohibited
      exact safe (fun index actual => index = parameter ∧ actual = effect) effect
        (Or.inr ⟨parameter, coefficient, rfl, rfl⟩) prohibited
  · rintro ⟨direct, coefficients⟩ assignment effect member prohibited
    rcases member with present | ⟨parameter, present, _⟩
    · exact (disjoint_iff _ _).mp direct effect present prohibited
    · exact (disjoint_iff _ _).mp (coefficients parameter) effect present prohibited

private theorem excluded_accepts_iff (expression : EffectExpression n) (excluded : EffectSet) :
    (EffectPredicate.excluded (.known expression) excluded).satisfaction = some true ↔
      expression.direct.intersection excluded.prohibited = EffectSet.empty ∧
      ∀ parameter, (expression.coefficients.get parameter).intersection excluded.prohibited = EffectSet.empty := by
  by_cases direct : expression.direct.intersection excluded.prohibited = EffectSet.empty
  · simp [satisfaction, direct]
  · simp [satisfaction, direct]

private theorem excluded_rejects_iff (expression : EffectExpression n) (excluded : EffectSet) :
    (EffectPredicate.excluded (.known expression) excluded).satisfaction = some false ↔
      expression.direct.intersection excluded.prohibited ≠ EffectSet.empty := by
  by_cases direct : expression.direct.intersection excluded.prohibited = EffectSet.empty
  · simp [satisfaction, direct]
  · simp [satisfaction, direct]

theorem excluded_known (expression : EffectExpression n) (excluded : EffectSet) :
    (EffectPredicate.excluded (.known expression) excluded).satisfaction = some true ↔
      ∀ assignment, Avoids expression excluded assignment :=
  (excluded_accepts_iff expression excluded).trans (universal_avoids_iff expression excluded).symm

theorem excluded_rejected (expression : EffectExpression n) (excluded : EffectSet) :
    (EffectPredicate.excluded (.known expression) excluded).satisfaction = some false ↔
      ∀ assignment, ¬ Avoids expression excluded assignment := by
  rw [excluded_rejects_iff]
  constructor
  · intro prohibited assignment safe
    apply prohibited
    apply (disjoint_iff _ _).mpr
    intro effect member blocked
    exact safe effect (Or.inl member) blocked
  · intro alwaysBlocked direct
    exact alwaysBlocked (fun _ _ => False) (avoids_empty_assignment expression excluded direct)

theorem excluded_unresolved_witness (expression : EffectExpression n) (excluded : EffectSet)
    (unresolved : (EffectPredicate.excluded (.known expression) excluded).satisfaction = none) :
    (∃ assignment, Avoids expression excluded assignment) ∧
      (∃ assignment, ¬ Avoids expression excluded assignment) := by
  classical
  have direct : expression.direct.intersection excluded.prohibited = EffectSet.empty := by
    apply Classical.byContradiction
    intro blocked
    have rejected := (excluded_rejects_iff expression excluded).2 blocked
    rw [unresolved] at rejected
    contradiction
  constructor
  · exact ⟨fun _ _ => False, avoids_empty_assignment expression excluded direct⟩
  · apply Classical.byContradiction
    intro missing
    have accepted := (excluded_known expression excluded).2 (fun assignment =>
      Classical.byContradiction (fun violates => missing ⟨assignment, violates⟩))
    rw [unresolved] at accepted
    contradiction

theorem unknown_policy (contract : SymbolicContract n) (known : EffectExpression n) (effects : EffectSet) :
    (included contract .unknown).satisfaction = some true ∧
    (covered contract .unknown).satisfaction = some true ∧
    (excluded .unknown effects : EffectPredicate n).satisfaction = some false ∧
    (equal .unknown .unknown : EffectPredicate n).satisfaction = some true ∧
    (equal .unknown (.known known)).satisfaction = some false ∧
    (equal (.known known) .unknown).satisfaction = some false ∧
    (included .unknown (.known known)).satisfaction = some false ∧
    (covered .unknown (.known known)).satisfaction = some false := by
  cases contract <;> simp [satisfaction]

theorem satisfaction_contract (left right : EffectExpression n) (excluded : EffectSet) :
    ((included (.known left) (.known right)).satisfaction = some true ↔
      ∀ assignment, le (left.denote assignment) (right.denote assignment)) ∧
    ((covered (.known left) (.known right)).satisfaction = some true ↔
      ∀ assignment, le (left.denote assignment) (closure (right.denote assignment))) ∧
    ((equal (.known left) (.known right)).satisfaction = some true →
      ∀ assignment effect, left.denote assignment effect ↔ right.denote assignment effect) ∧
    ((included (.known left) (.known right)).satisfaction = some false →
      ∃ assignment effect, left.denote assignment effect ∧ ¬ right.denote assignment effect) ∧
    ((covered (.known left) (.known right)).satisfaction = some false →
      ∃ assignment effect, left.denote assignment effect ∧ ¬ closure (right.denote assignment) effect) ∧
    ((equal (.known left) (.known right)).satisfaction = some false →
      ∃ assignment effect,
        (left.denote assignment effect ∧ ¬ right.denote assignment effect) ∨
        (right.denote assignment effect ∧ ¬ left.denote assignment effect)) ∧
    ((EffectPredicate.excluded (.known left) excluded).satisfaction = some true ↔
      ∀ assignment, Avoids left excluded assignment) ∧
    ((EffectPredicate.excluded (.known left) excluded).satisfaction = some false ↔
      ∀ assignment, ¬ Avoids left excluded assignment) ∧
    ((EffectPredicate.excluded (.known left) excluded).satisfaction = none →
      (∃ assignment, Avoids left excluded assignment) ∧
      (∃ assignment, ¬ Avoids left excluded assignment)) :=
  ⟨included_known left right, covered_known left right,
    equal_known_sound left right, included_rejected_witness left right,
    covered_rejected_witness left right, equal_rejected_witness left right,
    excluded_known left excluded, excluded_rejected left excluded,
    excluded_unresolved_witness left excluded⟩

theorem finite_controls :
    let empty : SymbolicContract 1 := .finite EffectSet.empty
    let fs : SymbolicContract 1 := .finite (EffectSet.ofList [.fs])
    let io : SymbolicContract 1 := .finite (EffectSet.ofList [.io])
    let subject : SymbolicContract 1 := .known (EffectExpression.parameter 0)
    (equal .unknown .unknown : EffectPredicate 1).satisfaction = some true ∧
    (equal .unknown empty).satisfaction = some false ∧
    (equal subject empty).satisfaction = none ∧
    (included fs fs).satisfaction = some true ∧
    (included fs empty).satisfaction = some false ∧
    (included subject empty).satisfaction = none ∧
    (covered fs io).satisfaction = some true ∧
    (covered fs empty).satisfaction = some false ∧
    (covered subject empty).satisfaction = none ∧
    (excluded empty (EffectSet.ofList [.fs])).satisfaction = some true ∧
    (excluded fs (EffectSet.ofList [.fs])).satisfaction = some false ∧
    (excluded subject (EffectSet.ofList [.fs])).satisfaction = none := by
  decide +kernel

theorem exclusion_controls :
    let fs : SymbolicContract 0 := .finite (EffectSet.ofList [.fs])
    let io : SymbolicContract 0 := .finite (EffectSet.ofList [.io])
    let error : SymbolicContract 0 := .finite (EffectSet.ofList [.error])
    (excluded io (EffectSet.ofList [.fs])).satisfaction = some false ∧
    (excluded fs (EffectSet.ofList [.io])).satisfaction = some true ∧
    (excluded error (EffectSet.ofList [.error])).satisfaction = some false := by
  decide +kernel

theorem masked_controls :
    let subject : SymbolicContract 1 := .known (EffectExpression.parameter 0)
    let fsMask := EffectSet.ofList [.fs]
    let ioMask := EffectSet.ofList [.io]
    let errorMask := EffectSet.ofList [.error]
    (covered (subject.masked fsMask) (subject.masked ioMask)).satisfaction = none ∧
    (excluded subject errorMask).satisfaction = none ∧
    (excluded subject.captureError errorMask).satisfaction = some true ∧
    (excluded (.unknown : SymbolicContract 1).captureError errorMask).satisfaction = some false := by
  decide +kernel

theorem equal_unresolved_can_agree :
    let fs := EffectSet.ofList [.fs]
    let finite : EffectExpression 1 := .finite fs
    let redundant : EffectExpression 1 := ⟨fs, Vector.replicate 1 fs⟩
    (equal (.known redundant) (.known finite)).satisfaction = none ∧
      ∀ assignment effect, redundant.denote assignment effect ↔ finite.denote assignment effect := by
  dsimp
  constructor
  · decide +kernel
  · intro assignment effect
    have closed : (EffectExpression.finite (EffectSet.ofList [.fs]) : EffectExpression 1).closed = true := by
      decide +kernel
    rw [EffectExpression.closed_denote _ closed]
    constructor
    · rintro (present | ⟨parameter, present, _⟩)
      · exact present
      · have zero := Fin.fin_one_eq_zero parameter
        subst parameter
        exact present
    · exact Or.inl

end EffectPredicate
end Xsh.Core
