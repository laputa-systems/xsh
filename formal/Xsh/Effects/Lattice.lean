import Xsh.Syntax.Core

/-!
A finite, independent model of XSH effect clauses and unknown contracts.

Effect clauses are sets, so their semantics does not depend on list order or
duplicate spellings. `io` closes over the four host effects it implies; no
other effect is introduced by closure. Unknown and unrestricted contracts are
kept distinct from a known empty set, since neither can justify a restricted
call.
-/

namespace Xsh.Semantics.Effects

open Xsh.Syntax

/-- The complete effect vocabulary represented by the core syntax. -/
def allEffects : List Effect :=
  [.fs, .net, .process, .env, .time, .error, .io]

theorem every_effect_is_listed (effect : Effect) : effect ∈ allEffects := by
  cases effect <;> simp [allEffects]

theorem listed_effects_are_distinct : allEffects.Nodup := by
  decide

/-- A canonical set over the finite effect vocabulary. -/
abbrev FiniteEffectSet := Effect → Prop

def ofList (effects : List Effect) : FiniteEffectSet := fun effect =>
  effect ∈ effects

def bottom : FiniteEffectSet := fun _ => False

def top : FiniteEffectSet := fun _ => True

def le (left right : FiniteEffectSet) : Prop :=
  ∀ effect, left effect → right effect

def join (left right : FiniteEffectSet) : FiniteEffectSet :=
  fun effect => left effect ∨ right effect

def meet (left right : FiniteEffectSet) : FiniteEffectSet :=
  fun effect => left effect ∧ right effect

theorem le_refl (effects : FiniteEffectSet) : le effects effects := by
  intro _ member
  exact member

theorem le_trans {left middle right : FiniteEffectSet}
    (first : le left middle) (second : le middle right) : le left right := by
  intro effect member
  exact second effect (first effect member)

theorem le_antisymm {left right : FiniteEffectSet}
    (first : le left right) (second : le right left) : left = right := by
  funext effect
  apply propext
  exact ⟨first effect, second effect⟩

theorem bottom_le (effects : FiniteEffectSet) : le bottom effects := by
  intro _ impossible
  exact False.elim impossible

theorem le_top (effects : FiniteEffectSet) : le effects top := by
  intro _ _
  trivial

theorem join_upper_left (left right : FiniteEffectSet) : le left (join left right) := by
  intro effect member
  exact Or.inl member

theorem join_upper_right (left right : FiniteEffectSet) : le right (join left right) := by
  intro effect member
  exact Or.inr member

theorem join_least {left right upper : FiniteEffectSet}
    (leftBelow : le left upper) (rightBelow : le right upper) :
    le (join left right) upper := by
  intro effect member
  rcases member with leftMember | rightMember
  · exact leftBelow effect leftMember
  · exact rightBelow effect rightMember

theorem meet_lower_left (left right : FiniteEffectSet) : le (meet left right) left := by
  intro effect member
  exact member.1

theorem meet_lower_right (left right : FiniteEffectSet) : le (meet left right) right := by
  intro effect member
  exact member.2

theorem meet_greatest {lower left right : FiniteEffectSet}
    (belowLeft : le lower left) (belowRight : le lower right) :
    le lower (meet left right) := by
  intro effect member
  exact ⟨belowLeft effect member, belowRight effect member⟩

theorem join_comm (left right : FiniteEffectSet) : join left right = join right left := by
  funext effect
  apply propext
  exact or_comm

theorem join_assoc (first second third : FiniteEffectSet) :
    join (join first second) third = join first (join second third) := by
  funext effect
  apply propext
  exact or_assoc

theorem meet_comm (left right : FiniteEffectSet) : meet left right = meet right left := by
  funext effect
  apply propext
  exact and_comm

theorem meet_assoc (first second third : FiniteEffectSet) :
    meet (meet first second) third = meet first (meet second third) := by
  funext effect
  apply propext
  exact and_assoc

theorem ofList_order_independent (first second : Effect) (rest : List Effect) :
    ofList (first :: second :: rest) = ofList (second :: first :: rest) := by
  funext effect
  apply propext
  simp only [ofList, List.mem_cons]
  exact or_left_comm

/-- The host effects implied by a clause entry of `io`. -/
def ioComponent : Effect → Prop
  | .fs | .net | .process | .env => True
  | .time | .error | .io => False

/-- The effects justified by a clause after implication closure. -/
def closure (effects : FiniteEffectSet) : FiniteEffectSet := fun required =>
  effects required ∨ (effects .io ∧ ioComponent required)

theorem closure_extensive (effects : FiniteEffectSet) : le effects (closure effects) := by
  intro effect member
  exact Or.inl member

theorem closure_mono {left right : FiniteEffectSet} (included : le left right) :
    le (closure left) (closure right) := by
  intro effect justified
  rcases justified with direct | implied
  · exact Or.inl (included effect direct)
  · exact Or.inr ⟨included .io implied.1, implied.2⟩

theorem closure_idempotent (effects : FiniteEffectSet) :
    closure (closure effects) = closure effects := by
  funext required
  apply propext
  cases required <;> simp [closure, ioComponent]

theorem closure_is_least_closed (effects closed : FiniteEffectSet)
    (containsEffects : le effects closed)
    (containsIoComponents : ∀ required, ioComponent required → closed .io → closed required) :
    le (closure effects) closed := by
  intro required justified
  rcases justified with direct | implied
  · exact containsEffects required direct
  · exact containsIoComponents required implied.2 (containsEffects .io implied.1)

theorem io_clause_covers_filesystem (effects : FiniteEffectSet)
    (includesIo : effects .io) : closure effects .fs := by
  exact Or.inr ⟨includesIo, by simp [ioComponent]⟩

theorem io_clause_covers_network (effects : FiniteEffectSet)
    (includesIo : effects .io) : closure effects .net := by
  exact Or.inr ⟨includesIo, by simp [ioComponent]⟩

theorem io_clause_covers_process (effects : FiniteEffectSet)
    (includesIo : effects .io) : closure effects .process := by
  exact Or.inr ⟨includesIo, by simp [ioComponent]⟩

theorem io_clause_covers_environment (effects : FiniteEffectSet)
    (includesIo : effects .io) : closure effects .env := by
  exact Or.inr ⟨includesIo, by simp [ioComponent]⟩

theorem io_does_not_imply_time : ¬ closure (ofList [.io]) .time := by
  simp [closure, ofList, ioComponent]

theorem io_does_not_imply_error : ¬ closure (ofList [.io]) .error := by
  simp [closure, ofList, ioComponent]

theorem filesystem_does_not_imply_io : ¬ closure (ofList [.fs]) .io := by
  simp [closure, ofList, ioComponent]

/-- A valid `without` component rejects `io`; excluding `io` is exact. -/
def withoutBlocks (excluded required : Effect) : Prop :=
  excluded ≠ .error ∧
    (excluded = required ∨ (required = .io ∧ ioComponent excluded))

theorem without_component_blocks_io (component : Effect)
    (isComponent : ioComponent component) : withoutBlocks component .io := by
  have notError : component ≠ .error := by
    intro equal
    subst component
    simp [ioComponent] at isComponent
  exact ⟨notError, Or.inr ⟨rfl, isComponent⟩⟩

theorem without_io_does_not_block_filesystem : ¬ withoutBlocks .io .fs := by
  simp [withoutBlocks, ioComponent]

theorem without_io_does_not_block_network : ¬ withoutBlocks .io .net := by
  simp [withoutBlocks, ioComponent]

theorem without_io_does_not_block_process : ¬ withoutBlocks .io .process := by
  simp [withoutBlocks, ioComponent]

theorem without_io_does_not_block_environment : ¬ withoutBlocks .io .env := by
  simp [withoutBlocks, ioComponent]

theorem without_never_blocks_error (excluded : Effect) :
    ¬ withoutBlocks excluded .error := by
  cases excluded <;> simp [withoutBlocks, ioComponent]

/-- A summary retains known effects even when an unresolved dependency exists. -/
structure EffectSummary where
  known : FiniteEffectSet
  unresolved : Bool

def joinSummary (left right : EffectSummary) : EffectSummary :=
  ⟨join left.known right.known, left.unresolved || right.unresolved⟩

/-- Unknown summaries and unrestricted contracts both fail a restricted call. -/
inductive EffectContract where
  | checked (required : FiniteEffectSet)
  | unrestricted
  | unknown

def contractOfSummary (summary : EffectSummary) : EffectContract :=
  if summary.unresolved then .unknown else .checked summary.known

def restrictedCallPermitted (bound : FiniteEffectSet) (contract : EffectContract) : Prop :=
  match contract with
  | .checked required => le required (closure bound)
  | .unrestricted => False
  | .unknown => False

/-- An absent clause is unrestricted, while a present clause is checked. -/
def clauseCallPermitted (bound : Option FiniteEffectSet) (contract : EffectContract) : Prop :=
  match bound with
  | none => True
  | some effects => restrictedCallPermitted effects contract

/-- Whether one valid lexical exclusion blocks a known required effect. -/
def excludedRequirement (bounds : List Effect) (required : Effect) : Prop :=
  ∃ excluded, excluded ∈ bounds ∧ withoutBlocks excluded required

/-- Unknown or unrestricted contracts cannot be justified inside a `without` region. -/
def withoutCallPermitted (bounds : List Effect) (contract : EffectContract) : Prop :=
  match bounds with
  | [] => True
  | _ :: _ =>
    match contract with
    | .checked required => ∀ effect, required effect → ¬ excludedRequirement bounds effect
    | .unrestricted => False
    | .unknown => False

def emptySummary : EffectSummary := ⟨bottom, false⟩

def unknownSummary : EffectSummary := ⟨bottom, true⟩

theorem joinSummary_preserves_known (left right : EffectSummary) (effect : Effect) :
    (joinSummary left right).known effect ↔ left.known effect ∨ right.known effect := Iff.rfl

theorem joinSummary_preserves_unknown (left right : EffectSummary) :
    (joinSummary left right).unresolved = true ↔
      left.unresolved = true ∨ right.unresolved = true := by
  cases left with
  | mk leftKnown leftUnknown =>
    cases right with
    | mk rightKnown rightUnknown =>
      cases leftUnknown <;> cases rightUnknown <;> simp [joinSummary]

theorem unknown_is_not_an_empty_contract :
    contractOfSummary unknownSummary ≠ contractOfSummary emptySummary := by
  simp [contractOfSummary, unknownSummary, emptySummary]

theorem restricted_call_rejects_unknown (bound : FiniteEffectSet) :
    ¬ restrictedCallPermitted bound .unknown := by
  simp [restrictedCallPermitted]

theorem restricted_call_rejects_unrestricted (bound : FiniteEffectSet) :
    ¬ restrictedCallPermitted bound .unrestricted := by
  simp [restrictedCallPermitted]

theorem no_clause_allows_any_contract (contract : EffectContract) :
    clauseCallPermitted none contract := by
  trivial

theorem without_region_rejects_unknown (excluded : Effect) (rest : List Effect) :
    ¬ withoutCallPermitted (excluded :: rest) .unknown := by
  simp [withoutCallPermitted]

theorem without_region_rejects_unrestricted (excluded : Effect) (rest : List Effect) :
    ¬ withoutCallPermitted (excluded :: rest) .unrestricted := by
  simp [withoutCallPermitted]

theorem without_fs_blocks_known_io :
    excludedRequirement [.fs] .io := by
  exact ⟨.fs, by simp, by simp [withoutBlocks, ioComponent]⟩

theorem without_io_allows_known_filesystem :
    withoutCallPermitted [.io] (.checked (ofList [.fs])) := by
  intro effect required
  simp only [ofList, List.mem_cons, List.mem_nil_iff, or_false] at required
  subst effect
  intro blocked
  rcases blocked with ⟨excluded, member, denies⟩
  simp only [List.mem_cons, List.mem_nil_iff, or_false] at member
  subst excluded
  exact (without_io_does_not_block_filesystem denies)

theorem without_region_allows_known_empty (excluded : Effect) (rest : List Effect) :
    withoutCallPermitted (excluded :: rest) (.checked bottom) := by
  intro effect impossible
  simp [bottom] at impossible

end Xsh.Semantics.Effects
