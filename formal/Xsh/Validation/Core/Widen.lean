import Xsh.Validation.Core.Validated
import Xsh.Validation.BoundedRefinements

namespace Xsh.Validation.Core.Widen

open Xsh.Core
open Xsh.Validation.Core

variable {Declaration Identity Source : Type}

theorem base_member (world : MembershipWorld Declaration Identity Source)
    {validated baseType : ValueType Declaration} {candidate : Value Declaration Identity Source}
    (hasBase : base validated = some baseType) (member : Membership.value world validated candidate) :
    Membership.value world baseType candidate := by
  cases validated <;> simp only [base, reduceCtorEq, Option.some.injEq] at hasBase <;> subst hasBase
  case nonEmpty element =>
    revert member
    simp only [Membership.value]
    split <;> simp_all
  case relativePath =>
    revert member
    simp only [Membership.value, Membership.primitive]
    split <;> simp_all
  case nominal declaration fields =>
    cases candidate with
    | nominal actual contents =>
      simp only [Membership.value] at member ⊢
      simpa [Membership.recordFields, Value.eraseNominal] using member.2
    | _ => simp [Membership.value] at member
  case boundedInteger bounds =>
    simp only [Membership.value] at member
    cases erased : candidate.eraseNominal <;> simp only [erased] at member
    rename_i scalar
    cases scalar <;> simp only at member
    rename_i integer
    simp only [Xsh.Validation.BoundedSpec.accepts] at member
    cases baseKind : bounds.base <;>
      simp [IntegerBaseType, baseKind, Membership.value, Membership.primitive, erased]
    rw [baseKind] at member
    exact member.2.2.1

mutual
  theorem view_member (world : MembershipWorld Declaration Identity Source) (closed : ViewClosed world) :
      ∀ {source target : ValueType Declaration}, BaseView source target →
        ∀ candidate : Value Declaration Identity Source,
          Membership.value world source candidate → Membership.value world target candidate
    | _, _, .refl _, _, member => member
    | _, _, .validated hasBase view, candidate, member =>
        view_member world closed view candidate (base_member world hasBase member)
    | _, _, .list view, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | exact fun item present => view_member world closed view _ (member item present)
    | _, _, .map key view, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | exact fun entry present => ⟨(member entry present).1, view_member world closed view _ (member entry present).2⟩
          | exact ⟨member.1, fun field present => view_member world closed view _ (member.2 field present)⟩
    | _, _, .stream view, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | exact closed.stream _ _ _ view member
    | _, _, .record view, candidate, member => by
        simp only [Membership.value] at member ⊢
        cases fields : Membership.recordFields world candidate <;> simp only [fields] at member ⊢
        all_goals first
          | exact member.elim
          | exact row_view_member world closed view _ member
    | _, _, .optional view, candidate, member => by
        simp only [Membership.value] at member ⊢
        rcases member with isNull | member
        · exact Or.inl isNull
        · exact Or.inr (view_member world closed view candidate member)
    | _, _, .result valueView errorView, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | exact view_member world closed valueView _ member
          | exact view_member world closed errorView _ member
    | _, _, .union firstView secondView restView, candidate, member => by
        simp only [Membership.value] at member ⊢
        rcases member with first | second | rest
        · exact Or.inl (view_member world closed firstView candidate first)
        · exact Or.inr (Or.inl (view_member world closed secondView candidate second))
        · exact Or.inr (Or.inr (members_view_member world closed restView candidate rest))
    | _, _, .callable view, candidate, member => by
        simp only [Membership.value, Membership.callable] at member ⊢
        cases view with
        | result policy parameters resultView =>
          cases erased : candidate.eraseNominal <;> simp only [erased] at member ⊢
          all_goals first
            | exact member.elim
            | exact ⟨by simpa [Membership.signatureKind] using member.1,
                closed.callable _ _ _ (.result policy parameters resultView) member.2⟩

  theorem row_view_member (world : MembershipWorld Declaration Identity Source) (closed : ViewClosed world) :
      ∀ {source target : RecordType Declaration}, RowView source target →
        ∀ fields : List (String × Value Declaration Identity Source),
          Membership.row world source fields → Membership.row world target fields
    | _, _, .empty, _, member => member
    | _, _, .field name view restView, fields, member => by
        simp only [Membership.row] at member ⊢
        refine ⟨?_, row_view_member world closed restView fields member.2⟩
        have found := member.1
        cases lookup : fields.lookup name <;> simp only [lookup] at found ⊢
        all_goals first
          | exact found.elim
          | exact view_member world closed view _ found

  theorem members_view_member (world : MembershipWorld Declaration Identity Source) (closed : ViewClosed world) :
      ∀ {source target : UnionTail Declaration}, MembersView source target →
        ∀ candidate : Value Declaration Identity Source,
          Membership.members world source candidate → Membership.members world target candidate
    | _, _, .empty, _, member => member
    | _, _, .member view restView, candidate, member => by
        simp only [Membership.members] at member ⊢
        rcases member with here | later
        · exact Or.inl (view_member world closed view candidate here)
        · exact Or.inr (members_view_member world closed restView candidate later)
end

theorem base_is_unvalidated {validated baseType : ValueType Declaration}
    (hasBase : base validated = some baseType) : validation baseType = none := by
  cases validated <;> simp only [base, reduceCtorEq, Option.some.injEq] at hasBase <;> subst hasBase <;>
    try rfl
  rename_i bounds
  cases bounds.base <;> rfl

theorem validated_has_identity {validated baseType : ValueType Declaration}
    (hasBase : base validated = some baseType) : validation validated ≠ none := by
  cases validated <;> simp [base, validation] at hasBase ⊢

theorem view_from_unvalidated {source target : ValueType Declaration} (view : BaseView source target)
    (unvalidated : validation source = none) : validation target = none := by
  cases view with
  | refl => exact unvalidated
  | validated hasBase _ => exact absurd unvalidated (validated_has_identity hasBase)
  | _ => rfl

theorem view_into_validated_is_identity {source target : ValueType Declaration}
    (view : BaseView source target) (validatedTarget : validation target ≠ none) : source = target := by
  cases view with
  | refl => rfl
  | validated hasBase rest =>
    exact absurd (view_from_unvalidated rest (base_is_unvalidated hasBase)) validatedTarget
  | _ => exact absurd rfl validatedTarget

theorem base_view {validated baseType : ValueType Declaration} (hasBase : base validated = some baseType) :
    BaseView validated baseType :=
  .validated hasBase (.refl _)

theorem base_never_views_validated {validated baseType : ValueType Declaration}
    (hasBase : base validated = some baseType) : ¬ BaseView baseType validated := by
  intro view
  have same := view_into_validated_is_identity view (validated_has_identity hasBase)
  subst same
  exact validated_has_identity hasBase (base_is_unvalidated hasBase)

/-- Validated widening over every Core type. In a world whose retained stream
and callable contracts are closed under base views, a base view carries every
member of the source type to the target: a validated value to its base, and a
validated position at any depth of a collection, record, optional, result,
union, stream element or callable return to its base. A view into a
validated type is the identity, so a validated-to-validated assignment requires
the exact same validation identity, and two validations with distinct
identities never fit even when one interval contains the other. Every
validated type views as its base, and no base views as its validated type. -/
theorem validated_widening (world : MembershipWorld Declaration Identity Source) (closed : ViewClosed world) :
    (∀ (source target : ValueType Declaration) candidate, BaseView source target →
      Membership.value world source candidate → Membership.value world target candidate) ∧
    (∀ source target : ValueType Declaration, BaseView source target →
      ∀ sourceIdentity targetIdentity, validation source = some sourceIdentity →
        validation target = some targetIdentity → sourceIdentity = targetIdentity ∧ source = target) ∧
    (∀ source target : ValueType Declaration, BaseView source target → validation target ≠ none →
      source = target) ∧
    (∀ validated baseType : ValueType Declaration, base validated = some baseType →
      BaseView validated baseType ∧ ¬ BaseView baseType validated) := by
  refine ⟨fun source target candidate view member => view_member world closed view candidate member, ?_,
    fun _ _ view validatedTarget => view_into_validated_is_identity view validatedTarget,
    fun _ _ hasBase => ⟨base_view hasBase, base_never_views_validated hasBase⟩⟩
  intro source target view sourceIdentity targetIdentity sourceValidated targetValidated
  have same := view_into_validated_is_identity view (by simp [targetValidated])
  subst same
  rw [sourceValidated] at targetValidated
  exact ⟨Option.some.inj targetValidated, rfl⟩

namespace Witness

abbrev WitnessValue := Value Unit Unit Unit
abbrev WitnessType := ValueType Unit

def openWorld : MembershipWorld Unit Unit Unit where
  entryFields := fun _ => []
  stream := fun _ _ => True
  callable := fun _ _ => True
  scheme := fun _ _ => False
  generic := fun _ _ => False

theorem openWorld_closed : ViewClosed openWorld := ⟨fun _ _ _ _ _ => trivial, fun _ _ _ _ _ => trivial⟩

def relativeStreamWorld : MembershipWorld Unit Unit Unit where
  entryFields := fun _ => []
  stream := fun _ element => element = .relativePath
  callable := fun _ _ => False
  scheme := fun _ _ => False
  generic := fun _ _ => False

def pathValue (bytes : List UInt8) (noNul : ∀ byte, byte ∈ bytes → byte ≠ 0) : WitnessValue :=
  .scalar (.path ⟨bytes, noNul⟩)

def relative : WitnessValue := pathValue [97] (by decide)
def absolute : WitnessValue := pathValue [47, 97] (by decide)

def narrowPort : Xsh.Validation.BoundedSpec :=
  ⟨.int, 1, 10, by decide, by unfold Xsh.Validation.IntegerBase.accepts; decide,
    by unfold Xsh.Validation.IntegerBase.accepts; decide⟩

def widePort : Xsh.Validation.BoundedSpec :=
  ⟨.int, 0, 100, by decide, by unfold Xsh.Validation.IntegerBase.accepts; decide,
    by unfold Xsh.Validation.IntegerBase.accepts; decide⟩

theorem nested_relative_list_widens_to_path_list :
    BaseView (Declaration := Unit) (.list .relativePath) (.list (.primitive .path)) ∧
      Membership.value openWorld (.list .relativePath) (.list [relative]) ∧
      Membership.value openWorld (.list (.primitive .path)) (.list [relative]) := by
  have view : BaseView (Declaration := Unit) (.list .relativePath) (.list (.primitive .path)) :=
    .list (.validated rfl (.refl _))
  have member : Membership.value openWorld (.list .relativePath) (.list [relative]) := by
    simp [Membership.value, relative, pathValue, Value.eraseNominal]
    decide
  exact ⟨view, member, view_member openWorld openWorld_closed view _ member⟩

theorem path_never_views_relative_path :
    ¬ BaseView (Declaration := Unit) (.primitive .path) .relativePath ∧
      Membership.value openWorld (.primitive .path) absolute ∧
      ¬ Membership.value openWorld .relativePath absolute := by
  refine ⟨base_never_views_validated rfl, ?_, ?_⟩
  · simp [Membership.value, Membership.primitive, absolute, pathValue, Value.eraseNominal]
  · simp [Membership.value, absolute, pathValue, Value.eraseNominal]
    decide

theorem contained_interval_is_not_a_view :
    Xsh.Validation.BoundedRefinements.widensTo narrowPort widePort ∧
      ¬ BaseView (Declaration := Unit) (.boundedInteger narrowPort) (.boundedInteger widePort) := by
  refine ⟨by unfold Xsh.Validation.BoundedRefinements.widensTo; decide, ?_⟩
  intro view
  have same := view_into_validated_is_identity view (by simp [validation])
  simp [narrowPort, widePort] at same

theorem stream_view_needs_closed_contracts :
    BaseView (Declaration := Unit) (.stream .relativePath) (.stream (.primitive .path)) ∧
      Membership.value relativeStreamWorld (.stream .relativePath) (.stream ⟨()⟩) ∧
      ¬ Membership.value relativeStreamWorld (.stream (.primitive .path)) (.stream ⟨()⟩) := by
  refine ⟨.stream (.validated rfl (.refl _)), ?_, ?_⟩
  · simp [Membership.value, relativeStreamWorld, Value.eraseNominal]
  · simp [Membership.value, relativeStreamWorld, Value.eraseNominal]

end Witness

end Xsh.Validation.Core.Widen
