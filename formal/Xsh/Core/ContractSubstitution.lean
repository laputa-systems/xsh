import Xsh.Core.SymbolicEffects

namespace Xsh.Core.SymbolicContract

private def compareKnown (relation : EffectExpression parameters → EffectExpression parameters → Bool) :
    SymbolicContract parameters → SymbolicContract parameters → Bool
  | _, .unknown => true
  | .unknown, .known _ => false
  | .known left, .known right => relation left right

def included (left right : SymbolicContract parameters) : Bool :=
  compareKnown EffectExpression.included left right

def covered (left right : SymbolicContract parameters) : Bool :=
  compareKnown EffectExpression.covered left right

private theorem substituteTerms_congr (expression : EffectExpression source)
    (left right : Fin source → SymbolicContract target) (parameters : List (Fin source))
    (agree : ∀ parameter ∈ parameters,
      expression.coefficients.get parameter ≠ EffectSet.empty → left parameter = right parameter) :
    substituteTerms expression left parameters = substituteTerms expression right parameters := by
  induction parameters with
  | nil => rfl
  | cons parameter rest induction =>
    have tail : substituteTerms expression left rest = substituteTerms expression right rest :=
      induction (fun index member live => agree index (List.mem_cons_of_mem parameter member) live)
    simp only [substituteTerms, List.foldr_cons]
    rw [← substituteTerms, ← substituteTerms, tail]
    by_cases absent : expression.coefficients.get parameter = EffectSet.empty
    · simp [absent]
    · rw [agree parameter (List.mem_cons_self ..) absent]

theorem substitution_congr (expression : EffectExpression source)
    (left right : Fin source → SymbolicContract target)
    (agree : ∀ parameter,
      expression.coefficients.get parameter ≠ EffectSet.empty → left parameter = right parameter) :
    (known expression).substitute left = (known expression).substitute right := by
  exact substituteTerms_congr expression left right (List.finRange source)
    (fun parameter _ live => agree parameter live)

private def knownOrEmpty : SymbolicContract parameters → EffectExpression parameters
  | .unknown => .finite EffectSet.empty
  | .known expression => expression

private theorem substitute_known_part (expression : EffectExpression source)
    (replacement : Fin source → SymbolicContract target)
    (resolved : (known expression).substitute replacement ≠ unknown) :
    (known expression).substitute replacement =
      known (expression.substitute (fun parameter => knownOrEmpty (replacement parameter))) := by
  rw [← substitution_known]
  apply substitution_congr
  intro parameter live
  cases actual : replacement parameter with
  | unknown =>
    exact False.elim (resolved ((substitution_unknown_iff expression replacement).mpr
      ⟨parameter, live, actual⟩))
  | known expression => rfl

private theorem substitution_comparison_iff
    (left right : EffectExpression source) (replacement : Fin source → SymbolicContract target)
    (relation : EffectExpression target → EffectExpression target → Bool)
    (preserved : relation
      (left.substitute (fun parameter => knownOrEmpty (replacement parameter)))
      (right.substitute (fun parameter => knownOrEmpty (replacement parameter))) = true) :
    compareKnown relation ((known left).substitute replacement) ((known right).substitute replacement) = true ↔
      ((known left).substitute replacement = unknown → (known right).substitute replacement = unknown) := by
  cases rightResult : (known right).substitute replacement with
  | unknown => simp [compareKnown]
  | known actualRight =>
    cases leftResult : (known left).substitute replacement with
    | unknown => simp [compareKnown]
    | known actualLeft =>
      have leftPart := substitute_known_part left replacement (by simp [leftResult])
      have rightPart := substitute_known_part right replacement (by simp [rightResult])
      rw [leftResult] at leftPart
      rw [rightResult] at rightPart
      cases SymbolicContract.known.inj leftPart
      cases SymbolicContract.known.inj rightPart
      simp [compareKnown, preserved]

theorem substitution_included_iff
    (left right : SymbolicContract source) (included : left.included right = true)
    (replacement : Fin source → SymbolicContract target) :
    (left.substitute replacement).included (right.substitute replacement) = true ↔
      (left.substitute replacement = unknown → right.substitute replacement = unknown) := by
  cases right with
  | unknown => simp [substitute, SymbolicContract.included, compareKnown]
  | known right =>
    cases left with
    | unknown => simp [SymbolicContract.included, compareKnown] at included
    | known left =>
      exact substitution_comparison_iff left right replacement EffectExpression.included
        (EffectExpression.substitution_preserves_inclusion included _)

theorem substitution_covered_iff
    (left right : SymbolicContract source) (covered : left.covered right = true)
    (replacement : Fin source → SymbolicContract target) :
    (left.substitute replacement).covered (right.substitute replacement) = true ↔
      (left.substitute replacement = unknown → right.substitute replacement = unknown) := by
  cases right with
  | unknown => simp [substitute, SymbolicContract.covered, compareKnown]
  | known right =>
    cases left with
    | unknown => simp [SymbolicContract.covered, compareKnown] at covered
    | known left =>
      exact substitution_comparison_iff left right replacement EffectExpression.covered
        (EffectExpression.substitution_preserves_coverage covered _)

private theorem unknown_preservation_iff_support
    (left right : EffectExpression source) :
    (∀ target (replacement : Fin source → SymbolicContract target),
      (known left).substitute replacement = unknown → (known right).substitute replacement = unknown) ↔
      ∀ parameter, left.coefficients.get parameter ≠ EffectSet.empty →
        right.coefficients.get parameter ≠ EffectSet.empty := by
  constructor
  · intro preserved parameter live
    let replacement : Fin source → SymbolicContract 0 := fun index =>
      if index = parameter then unknown else finite EffectSet.empty
    have leftUnknown := (substitution_unknown_iff left replacement).mpr
      ⟨parameter, live, by simp [replacement]⟩
    obtain ⟨index, retained, actual⟩ := (substitution_unknown_iff right replacement).mp
      (preserved 0 replacement leftUnknown)
    by_cases equal : index = parameter
    · simpa [equal] using retained
    · simp [replacement, equal, finite] at actual
  · intro retained target replacement leftUnknown
    obtain ⟨parameter, live, actual⟩ := (substitution_unknown_iff left replacement).mp leftUnknown
    exact (substitution_unknown_iff right replacement).mpr
      ⟨parameter, retained parameter live, actual⟩

theorem every_substitution_preserves_inclusion_iff
    (left right : EffectExpression source) (included : left.included right = true) :
    (∀ target (replacement : Fin source → SymbolicContract target),
      ((known left).substitute replacement).included ((known right).substitute replacement) = true) ↔
      ∀ parameter, left.coefficients.get parameter ≠ EffectSet.empty →
        right.coefficients.get parameter ≠ EffectSet.empty := by
  constructor
  · intro preserved
    apply (unknown_preservation_iff_support left right).mp
    intro target replacement
    exact (substitution_included_iff (known left) (known right) included replacement).mp
      (preserved target replacement)
  · intro retained target replacement
    exact (substitution_included_iff (known left) (known right) included replacement).mpr
      ((unknown_preservation_iff_support left right).mpr retained target replacement)

theorem every_substitution_preserves_coverage_iff
    (left right : EffectExpression source) (covered : left.covered right = true) :
    (∀ target (replacement : Fin source → SymbolicContract target),
      ((known left).substitute replacement).covered ((known right).substitute replacement) = true) ↔
      ∀ parameter, left.coefficients.get parameter ≠ EffectSet.empty →
        right.coefficients.get parameter ≠ EffectSet.empty := by
  constructor
  · intro preserved
    apply (unknown_preservation_iff_support left right).mp
    intro target replacement
    exact (substitution_covered_iff (known left) (known right) covered replacement).mp
      (preserved target replacement)
  · intro retained target replacement
    exact (substitution_covered_iff (known left) (known right) covered replacement).mpr
      ((unknown_preservation_iff_support left right).mpr retained target replacement)

theorem unknown_substitution_can_break_inclusion :
    let direct : SymbolicContract 1 := finite (EffectSet.ofList [.fs])
    let redundant := direct.union (known ((EffectExpression.parameter 0).masked (EffectSet.ofList [.fs])))
    redundant.included direct = true ∧
      (redundant.substitute (fun _ => (unknown : SymbolicContract 0))).included
        (direct.substitute (fun _ => (unknown : SymbolicContract 0))) = false := by
  decide +kernel

end Xsh.Core.SymbolicContract
