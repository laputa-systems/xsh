import Xsh.Core.Effects
import Init.Data.List.FinRange

namespace Xsh.Core

open Xsh.Syntax

structure EffectExpression (parameters : Nat) where
  direct : EffectSet
  coefficients : Vector EffectSet parameters
  deriving DecidableEq

namespace EffectExpression

theorem ext {left right : EffectExpression parameters}
    (direct : left.direct = right.direct)
    (coefficients : ∀ parameter, left.coefficients.get parameter = right.coefficients.get parameter) :
    left = right := by
  cases left with
  | mk leftDirect leftCoefficients =>
    cases right with
    | mk rightDirect rightCoefficients =>
      have equal : leftCoefficients = rightCoefficients := by
        apply Vector.ext
        intro index bounded
        exact coefficients ⟨index, bounded⟩
      cases equal
      cases direct
      rfl

def finite (effects : EffectSet) : EffectExpression parameters :=
  ⟨effects, Vector.replicate parameters EffectSet.empty⟩

def parameter (parameter : Fin parameters) : EffectExpression parameters :=
  ⟨EffectSet.empty, Vector.ofFn fun index => if index = parameter then EffectSet.full else EffectSet.empty⟩

def union (left right : EffectExpression parameters) : EffectExpression parameters :=
  ⟨left.direct.union right.direct, Vector.zipWith EffectSet.union left.coefficients right.coefficients⟩

def masked (expression : EffectExpression parameters) (mask : EffectSet) : EffectExpression parameters :=
  ⟨expression.direct.intersection mask, expression.coefficients.map (·.intersection mask)⟩

def denote (expression : EffectExpression parameters)
    (assignment : Fin parameters → Xsh.Semantics.Effects.FiniteEffectSet) (effect : Effect) : Prop :=
  expression.direct.denotation effect ∨
    ∃ parameter, (expression.coefficients.get parameter).denotation effect ∧ assignment parameter effect

def comparison (left right : EffectExpression parameters) (available : EffectSet) : Bool :=
  left.direct.included available && (List.finRange parameters).all fun parameter =>
    (left.coefficients.get parameter).included (available.union (right.coefficients.get parameter))

def included (left right : EffectExpression parameters) : Bool :=
  comparison left right right.direct

def covered (left right : EffectExpression parameters) : Bool :=
  comparison left right right.direct.permissionClosure

theorem comparison_iff (left right : EffectExpression parameters) (available : EffectSet) :
    comparison left right available = true ↔
      Xsh.Semantics.Effects.le left.direct.denotation available.denotation ∧
      ∀ parameter, Xsh.Semantics.Effects.le (left.coefficients.get parameter).denotation
        (available.union (right.coefficients.get parameter)).denotation := by
  simp [comparison, EffectSet.included_iff]

theorem denote_empty (expression : EffectExpression parameters) (effect : Effect) :
    expression.denote (fun _ _ => False) effect ↔ expression.direct.denotation effect := by
  simp [denote]

theorem denote_single (expression : EffectExpression parameters) (parameter : Fin parameters)
    (selected effect : Effect) :
    expression.denote (fun index actual => index = parameter ∧ actual = selected) effect ↔
      expression.direct.denotation effect ∨
        (expression.coefficients.get parameter).denotation effect ∧ effect = selected := by
  constructor
  · rintro (direct | ⟨index, coefficient, equal, selected⟩)
    · exact Or.inl direct
    · subst index
      exact Or.inr ⟨coefficient, selected⟩
  · rintro (direct | ⟨coefficient, selected⟩)
    · exact Or.inl direct
    · exact Or.inr ⟨parameter, coefficient, rfl, selected⟩

theorem included_iff (left right : EffectExpression parameters) :
    left.included right = true ↔
      ∀ assignment, Xsh.Semantics.Effects.le (left.denote assignment) (right.denote assignment) := by
  rw [included, comparison_iff]
  constructor
  · rintro ⟨direct, coefficients⟩ assignment effect (required | ⟨parameter, required, actual⟩)
    · exact Or.inl (direct effect required)
    · have accepted := coefficients parameter effect required
      simp only [EffectSet.denotation, EffectSet.contains_union, Bool.or_eq_true] at accepted
      rcases accepted with available | retained
      · exact Or.inl available
      · exact Or.inr ⟨parameter, retained, actual⟩
  · intro included
    constructor
    · intro effect required
      exact (denote_empty right effect).mp
        (included (fun _ _ => False) effect (Or.inl required))
    · intro parameter effect required
      have accepted := included (fun index actual => index = parameter ∧ actual = effect)
        effect (Or.inr ⟨parameter, required, rfl, rfl⟩)
      rw [denote_single] at accepted
      simpa only [EffectSet.denotation, EffectSet.contains_union, Bool.or_eq_true, and_true] using accepted

theorem covered_iff (left right : EffectExpression parameters) :
    left.covered right = true ↔
      ∀ assignment, Xsh.Semantics.Effects.le (left.denote assignment)
        (Xsh.Semantics.Effects.closure (right.denote assignment)) := by
  rw [covered, comparison_iff, EffectSet.permissionClosure_denotation]
  constructor
  · rintro ⟨direct, coefficients⟩ assignment effect (required | ⟨parameter, required, actual⟩)
    · exact Xsh.Semantics.Effects.closure_mono
        (fun _ member => Or.inl member) effect (direct effect required)
    · have accepted := coefficients parameter effect required
      simp only [EffectSet.denotation, EffectSet.contains_union, Bool.or_eq_true] at accepted
      rcases accepted with available | retained
      · rw [← EffectSet.denotation, EffectSet.permissionClosure_denotation] at available
        exact Xsh.Semantics.Effects.closure_mono
          (fun _ member => Or.inl member) effect available
      · exact Or.inl (Or.inr ⟨parameter, retained, actual⟩)
  · intro covered
    constructor
    · intro effect required
      have accepted := covered (fun _ _ => False) effect (Or.inl required)
      simpa only [Xsh.Semantics.Effects.closure, denote_empty] using accepted
    · intro parameter effect required
      have accepted := covered (fun index actual => index = parameter ∧ actual = effect)
        effect (Or.inr ⟨parameter, required, rfl, rfl⟩)
      simp only [Xsh.Semantics.Effects.closure, denote_single] at accepted
      have accepted' : Xsh.Semantics.Effects.closure right.direct.denotation effect ∨
          (right.coefficients.get parameter).denotation effect := by
        rcases accepted with (direct | ⟨retained, _⟩) | ⟨direct | ⟨_, equal⟩, implied⟩
        · exact Or.inl (Or.inl direct)
        · exact Or.inr retained
        · exact Or.inl (Or.inr ⟨direct, implied⟩)
        · subst effect
          simp [Xsh.Semantics.Effects.ioComponent] at implied
      simpa only [EffectSet.denotation, EffectSet.contains_union, Bool.or_eq_true,
        ← EffectSet.permissionClosure_denotation] using accepted'

def substituteTerms (expression : EffectExpression source)
    (replacement : Fin source → EffectExpression target) (parameters : List (Fin source)) : EffectExpression target :=
  parameters.foldr (fun parameter rest =>
    ((replacement parameter).masked (expression.coefficients.get parameter)).union rest) (finite expression.direct)

def substitute (expression : EffectExpression source)
    (replacement : Fin source → EffectExpression target) : EffectExpression target :=
  expression.substituteTerms replacement (List.finRange source)

@[simp] theorem union_empty (expression : EffectExpression parameters) :
    expression.union (finite EffectSet.empty) = expression := by
  apply ext
  · simp [union, finite]
  · intro parameter
    simp [union, finite, Vector.get, Fin.cast]

@[simp] theorem empty_union (expression : EffectExpression parameters) :
    (finite EffectSet.empty).union expression = expression := by
  apply ext
  · simp [union, finite]
  · intro parameter
    simp [union, finite, Vector.get, Fin.cast]

@[simp] theorem masked_empty (expression : EffectExpression parameters) :
    expression.masked EffectSet.empty = finite EffectSet.empty := by
  apply ext
  · simp [masked, finite]
  · intro parameter
    simp [masked, finite, Vector.get, Fin.cast]

@[simp] theorem masked_finite (effects mask : EffectSet) :
    (finite effects : EffectExpression parameters).masked mask = finite (effects.intersection mask) := by
  apply ext
  · rfl
  · intro parameter
    simp [masked, finite, Vector.get, Fin.cast]

@[simp] theorem masked_full (expression : EffectExpression parameters) :
    expression.masked EffectSet.full = expression := by
  apply ext
  · simp [masked]
  · intro parameter
    simp [masked, Vector.get, Fin.cast]

theorem masked_union (left right : EffectExpression parameters) (mask : EffectSet) :
    (left.union right).masked mask = (left.masked mask).union (right.masked mask) := by
  apply ext
  · exact EffectSet.union_intersection _ _ _
  · intro parameter
    simp [union, masked, Vector.get, Fin.cast, EffectSet.union_intersection]

theorem masked_masked (expression : EffectExpression parameters) (first second : EffectSet) :
    (expression.masked first).masked second = expression.masked (first.intersection second) := by
  apply ext
  · exact EffectSet.intersection_assoc _ _ _
  · intro parameter
    simp [masked, Vector.get, Fin.cast, EffectSet.intersection_assoc]

theorem denote_finite (effects : EffectSet)
    (assignment : Fin parameters → Xsh.Semantics.Effects.FiniteEffectSet) (effect : Effect) :
    (finite effects).denote assignment effect ↔ effects.denotation effect := by
  simp [denote, finite, EffectSet.denotation, Vector.get]

theorem denote_parameter (parameter : Fin parameters)
    (assignment : Fin parameters → Xsh.Semantics.Effects.FiniteEffectSet) (effect : Effect) :
    (EffectExpression.parameter parameter).denote assignment effect ↔ assignment parameter effect := by
  simp [denote, EffectExpression.parameter, EffectSet.denotation, Vector.get, Fin.cast]
  constructor
  · rintro ⟨index, coefficient, actual⟩
    by_cases equal : index = parameter
    · subst index
      exact actual
    · simp [equal] at coefficient
  · intro actual
    exact ⟨parameter, by simp, actual⟩

theorem denote_union (left right : EffectExpression parameters)
    (assignment : Fin parameters → Xsh.Semantics.Effects.FiniteEffectSet) (effect : Effect) :
    (left.union right).denote assignment effect ↔ left.denote assignment effect ∨ right.denote assignment effect := by
  simp only [denote, union, EffectSet.denotation, EffectSet.contains_union,
    Bool.or_eq_true, Vector.get, Vector.getElem_toArray, Vector.getElem_zipWith, Fin.cast]
  constructor
  · rintro ((left | right) | ⟨parameter, (left | right), actual⟩)
    · exact Or.inl (Or.inl left)
    · exact Or.inr (Or.inl right)
    · exact Or.inl (Or.inr ⟨parameter, left, actual⟩)
    · exact Or.inr (Or.inr ⟨parameter, right, actual⟩)
  · rintro ((direct | ⟨parameter, coefficient, actual⟩) | (direct | ⟨parameter, coefficient, actual⟩))
    · exact Or.inl (Or.inl direct)
    · exact Or.inr ⟨parameter, Or.inl coefficient, actual⟩
    · exact Or.inl (Or.inr direct)
    · exact Or.inr ⟨parameter, Or.inr coefficient, actual⟩

theorem denote_masked (expression : EffectExpression parameters) (mask : EffectSet)
    (assignment : Fin parameters → Xsh.Semantics.Effects.FiniteEffectSet) (effect : Effect) :
    (expression.masked mask).denote assignment effect ↔ expression.denote assignment effect ∧ mask.denotation effect := by
  simp only [denote, masked, EffectSet.denotation, EffectSet.contains_intersection,
    Bool.and_eq_true, Vector.get, Vector.getElem_toArray, Vector.getElem_map, Fin.cast]
  constructor
  · rintro (⟨direct, permitted⟩ | ⟨parameter, ⟨coefficient, permitted⟩, actual⟩)
    · exact ⟨Or.inl direct, permitted⟩
    · exact ⟨Or.inr ⟨parameter, coefficient, actual⟩, permitted⟩
  · rintro ⟨direct | ⟨parameter, coefficient, actual⟩, permitted⟩
    · exact Or.inl ⟨direct, permitted⟩
    · exact Or.inr ⟨parameter, ⟨coefficient, permitted⟩, actual⟩

theorem denote_substituteTerms (expression : EffectExpression source)
    (replacement : Fin source → EffectExpression target) (parameters : List (Fin source))
    (assignment : Fin target → Xsh.Semantics.Effects.FiniteEffectSet) (effect : Effect) :
    (expression.substituteTerms replacement parameters).denote assignment effect ↔
      expression.direct.denotation effect ∨
        ∃ parameter ∈ parameters,
          (expression.coefficients.get parameter).denotation effect ∧ (replacement parameter).denote assignment effect := by
  induction parameters with
  | nil => simp [substituteTerms, denote_finite]
  | cons parameter rest induction =>
    simp only [substituteTerms, List.foldr_cons, denote_union, denote_masked]
    rw [← substituteTerms, induction]
    simp only [List.mem_cons, exists_eq_or_imp]
    constructor
    · rintro (⟨actual, coefficient⟩ | direct | ⟨parameter, member, coefficient, actual⟩)
      · exact Or.inr (Or.inl ⟨coefficient, actual⟩)
      · exact Or.inl direct
      · exact Or.inr (Or.inr ⟨parameter, member, coefficient, actual⟩)
    · rintro (direct | ⟨coefficient, actual⟩ | ⟨parameter, member, coefficient, actual⟩)
      · exact Or.inr (Or.inl direct)
      · exact Or.inl ⟨actual, coefficient⟩
      · exact Or.inr (Or.inr ⟨parameter, member, coefficient, actual⟩)

theorem substitution_semantics (expression : EffectExpression source)
    (replacement : Fin source → EffectExpression target)
    (assignment : Fin target → Xsh.Semantics.Effects.FiniteEffectSet) (effect : Effect) :
    (expression.substitute replacement).denote assignment effect ↔
      expression.denote (fun parameter => (replacement parameter).denote assignment) effect := by
  simpa only [substitute, denote, List.mem_finRange, true_and] using
    denote_substituteTerms expression replacement (List.finRange source) assignment effect

theorem masked_substitution_semantics (expression : EffectExpression source)
    (replacement : Fin source → EffectExpression target) (mask : EffectSet)
    (assignment : Fin target → Xsh.Semantics.Effects.FiniteEffectSet) (effect : Effect) :
    ((expression.masked mask).substitute replacement).denote assignment effect ↔
      ((expression.substitute replacement).masked mask).denote assignment effect := by
  simp only [substitution_semantics, denote_masked]

theorem included_refl (expression : EffectExpression parameters) :
    expression.included expression = true := by
  rw [included_iff]
  intro assignment
  exact Xsh.Semantics.Effects.le_refl _

theorem included_trans {left middle right : EffectExpression parameters}
    (first : left.included middle = true) (second : middle.included right = true) :
    left.included right = true := by
  rw [included_iff] at *
  intro assignment
  exact Xsh.Semantics.Effects.le_trans (first assignment) (second assignment)

theorem included_implies_covered {left right : EffectExpression parameters}
    (included : left.included right = true) : left.covered right = true := by
  rw [included_iff] at included
  rw [covered_iff]
  intro assignment
  exact Xsh.Semantics.Effects.le_trans (included assignment)
    (Xsh.Semantics.Effects.closure_extensive _)

theorem covered_refl (expression : EffectExpression parameters) :
    expression.covered expression = true :=
  included_implies_covered (included_refl expression)

theorem covered_trans {left middle right : EffectExpression parameters}
    (first : left.covered middle = true) (second : middle.covered right = true) :
    left.covered right = true := by
  rw [covered_iff] at *
  intro assignment
  have transit := Xsh.Semantics.Effects.le_trans (first assignment)
    (Xsh.Semantics.Effects.closure_mono (second assignment))
  simpa only [Xsh.Semantics.Effects.closure_idempotent] using transit

theorem union_included_iff (left right upper : EffectExpression parameters) :
    (left.union right).included upper = true ↔
      left.included upper = true ∧ right.included upper = true := by
  simp only [included_iff]
  constructor
  · intro included
    exact ⟨fun assignment effect member => included assignment effect
      ((denote_union left right assignment effect).mpr (Or.inl member)),
      fun assignment effect member => included assignment effect
      ((denote_union left right assignment effect).mpr (Or.inr member))⟩
  · rintro ⟨leftBelow, rightBelow⟩ assignment effect member
    rcases (denote_union left right assignment effect).mp member with left | right
    · exact leftBelow assignment effect left
    · exact rightBelow assignment effect right

theorem substitution_preserves_inclusion {left right : EffectExpression source}
    (included : left.included right = true) (replacement : Fin source → EffectExpression target) :
    (left.substitute replacement).included (right.substitute replacement) = true := by
  rw [included_iff] at *
  intro assignment effect member
  exact (substitution_semantics right replacement assignment effect).mpr
    (included _ effect ((substitution_semantics left replacement assignment effect).mp member))

theorem substitution_preserves_coverage {left right : EffectExpression source}
    (covered : left.covered right = true) (replacement : Fin source → EffectExpression target) :
    (left.substitute replacement).covered (right.substitute replacement) = true := by
  rw [covered_iff] at *
  intro assignment effect member
  have accepted := covered _ effect
    ((substitution_semantics left replacement assignment effect).mp member)
  have equal : (right.substitute replacement).denote assignment =
      right.denote (fun parameter => (replacement parameter).denote assignment) := by
    funext effect
    exact propext (substitution_semantics right replacement assignment effect)
  simpa only [equal] using accepted

theorem coverage_controls :
    let filesystem : EffectExpression 1 :=
      (parameter 0).masked (EffectSet.ofList [.fs])
    let symbolicIo : EffectExpression 1 :=
      (parameter 0).masked (EffectSet.ofList [.io])
    let directIo : EffectExpression 1 := finite (EffectSet.ofList [.io])
    filesystem.covered directIo = true ∧
      filesystem.included directIo = false ∧ filesystem.covered symbolicIo = false := by
  decide +kernel

end EffectExpression

inductive SymbolicContract (parameters : Nat) where
  | unknown
  | known (expression : EffectExpression parameters)
  deriving DecidableEq

namespace SymbolicContract

def finite (effects : EffectSet) : SymbolicContract parameters := .known (.finite effects)

def union : SymbolicContract parameters → SymbolicContract parameters → SymbolicContract parameters
  | .known left, .known right => .known (left.union right)
  | _, _ => .unknown

def masked : SymbolicContract parameters → EffectSet → SymbolicContract parameters
  | .unknown, _ => .unknown
  | .known expression, mask => .known (expression.masked mask)

def captureError (contract : SymbolicContract parameters) : SymbolicContract parameters :=
  contract.masked (EffectSet.full.without (EffectSet.ofList [.error]))

def substituteTerms (expression : EffectExpression source)
    (replacement : Fin source → SymbolicContract target) (parameters : List (Fin source)) : SymbolicContract target :=
  parameters.foldr (fun parameter rest =>
    if expression.coefficients.get parameter = EffectSet.empty then rest
    else ((replacement parameter).masked (expression.coefficients.get parameter)).union rest) (finite expression.direct)

def substitute (contract : SymbolicContract source)
    (replacement : Fin source → SymbolicContract target) : SymbolicContract target :=
  match contract with
  | .unknown => .unknown
  | .known expression => substituteTerms expression replacement (List.finRange source)

theorem union_unknown_iff (left right : SymbolicContract parameters) :
    left.union right = .unknown ↔ left = .unknown ∨ right = .unknown := by
  cases left <;> cases right <;> simp [union]

theorem masked_unknown_iff (contract : SymbolicContract parameters) (mask : EffectSet) :
    contract.masked mask = .unknown ↔ contract = .unknown := by
  cases contract <;> simp [masked]

theorem substituteTerms_unknown_iff (expression : EffectExpression source)
    (replacement : Fin source → SymbolicContract target) (parameters : List (Fin source)) :
    substituteTerms expression replacement parameters = .unknown ↔
      ∃ parameter ∈ parameters,
        expression.coefficients.get parameter ≠ EffectSet.empty ∧ replacement parameter = .unknown := by
  induction parameters with
  | nil => simp [substituteTerms, finite]
  | cons parameter rest induction =>
    simp only [substituteTerms, List.foldr_cons]
    by_cases absent : expression.coefficients.get parameter = EffectSet.empty
    · simp only [absent, ↓reduceIte]
      rw [← substituteTerms, induction]
      simp [List.mem_cons, exists_eq_or_imp, absent]
    · simp only [absent, ↓reduceIte, union_unknown_iff, masked_unknown_iff]
      rw [← substituteTerms, induction]
      simp [List.mem_cons, exists_eq_or_imp, absent]

theorem substitution_unknown_iff (expression : EffectExpression source)
    (replacement : Fin source → SymbolicContract target) :
    (known expression).substitute replacement = .unknown ↔
      ∃ parameter, expression.coefficients.get parameter ≠ EffectSet.empty ∧ replacement parameter = .unknown := by
  simpa only [substitute, List.mem_finRange, true_and] using
    substituteTerms_unknown_iff expression replacement (List.finRange source)

theorem substituteTerms_known (expression : EffectExpression source)
    (replacement : Fin source → EffectExpression target) (parameters : List (Fin source)) :
    substituteTerms expression (fun parameter => .known (replacement parameter)) parameters =
      .known (expression.substituteTerms replacement parameters) := by
  induction parameters with
  | nil => rfl
  | cons parameter rest induction =>
    simp only [substituteTerms, EffectExpression.substituteTerms, List.foldr_cons]
    rw [← substituteTerms, induction]
    by_cases absent : expression.coefficients.get parameter = EffectSet.empty
    · simp [absent, EffectExpression.masked_empty, EffectExpression.substituteTerms]
    · simp [absent, masked, union, EffectExpression.substituteTerms]

theorem substitution_known (expression : EffectExpression source)
    (replacement : Fin source → EffectExpression target) :
    (known expression).substitute (fun parameter => .known (replacement parameter)) =
      .known (expression.substitute replacement) :=
  substituteTerms_known expression replacement (List.finRange source)

theorem capture_unknown : (unknown : SymbolicContract parameters).captureError = unknown := rfl

theorem capture_empty : (finite EffectSet.empty : SymbolicContract parameters).captureError = finite EffectSet.empty := by
  simp [captureError, masked, finite]

theorem unknown_ne_empty : (unknown : SymbolicContract parameters) ≠ finite EffectSet.empty := by
  simp [finite]

theorem erased_dependency_unknown_counterexample :
    let expression : EffectExpression 1 := (EffectExpression.parameter 0).masked (EffectSet.ofList [.error])
    ((known expression).captureError.substitute (fun _ => (unknown : SymbolicContract 0))) = finite EffectSet.empty ∧
      (((known expression).substitute (fun _ => (unknown : SymbolicContract 0))).captureError) = unknown := by
  decide +kernel

end SymbolicContract

inductive EffectBound (parameters : Nat) where
  | known (required : EffectExpression parameters)
  | unrestricted
  | unknown
  deriving DecidableEq

namespace EffectBound

def interpret (bound : EffectBound parameters)
    (assignment : Fin parameters → Xsh.Semantics.Effects.FiniteEffectSet) :
    Xsh.Semantics.Effects.EffectContract :=
  match bound with
  | .known required => .checked (required.denote assignment)
  | .unrestricted => .unrestricted
  | .unknown => .unknown

def denotation (bound : EffectBound 0) : Xsh.Semantics.Effects.EffectContract :=
  bound.interpret Fin.elim0

def substitute (bound : EffectBound source)
    (replacement : Fin source → SymbolicContract target) : EffectBound target :=
  match bound with
  | .known required =>
      match (SymbolicContract.known required).substitute replacement with
      | .known actual => .known actual
      | .unknown => .unknown
  | .unrestricted => .unrestricted
  | .unknown => .unknown

theorem interpret_known_substitution (bound : EffectBound source)
    (replacement : Fin source → EffectExpression target)
    (assignment : Fin target → Xsh.Semantics.Effects.FiniteEffectSet) :
    (bound.substitute (fun parameter => .known (replacement parameter))).interpret assignment =
      bound.interpret (fun parameter => (replacement parameter).denote assignment) := by
  cases bound with
  | known required =>
    simp only [substitute, SymbolicContract.substitution_known, interpret]
    congr 1
    funext effect
    exact propext (EffectExpression.substitution_semantics required replacement assignment effect)
  | unrestricted => rfl
  | unknown => rfl

theorem substitution_unknown_iff (required : EffectExpression source)
    (replacement : Fin source → SymbolicContract target) :
    (known required).substitute replacement = .unknown ↔
      ∃ parameter, required.coefficients.get parameter ≠ EffectSet.empty ∧ replacement parameter = .unknown := by
  have unknown := SymbolicContract.substitution_unknown_iff required replacement
  cases substituted : (SymbolicContract.known required).substitute replacement <;>
    simp_all [substitute]

end EffectBound

end Xsh.Core
