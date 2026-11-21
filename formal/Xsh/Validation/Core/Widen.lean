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

theorem row_iff (world : MembershipWorld Declaration Identity Source) :
    ∀ (row : RecordType Declaration) (fields : List (String × Value Declaration Identity Source)),
      Membership.row world row fields ↔
        ∀ entry ∈ rowEntries row, ∃ found, fields.lookup entry.1 = some found ∧
          Membership.value world entry.2 found
  | .variable id, _ => nomatch id
  | .rowEmpty, _ => by simp [Membership.row, rowEntries]
  | .rowField name type rest, fields => by
    simp only [Membership.row, rowEntries, List.mem_cons, forall_eq_or_imp, row_iff world rest fields]
    cases fields.lookup name <;> simp

theorem members_iff (world : MembershipWorld Declaration Identity Source) :
    ∀ (rest : UnionTail Declaration) (candidate : Value Declaration Identity Source),
      Membership.members world rest candidate ↔ ∃ type ∈ tailMembers rest, Membership.value world type candidate
  | .variable id, _ => nomatch id
  | .membersEmpty, _ => by simp [Membership.members, tailMembers]
  | .member type tail, candidate => by
    simp only [Membership.members, tailMembers, List.mem_cons, exists_eq_or_imp]
    rw [members_iff world tail candidate]

theorem union_member_iff (world : MembershipWorld Declaration Identity Source) {union : ValueType Declaration}
    {members : List (ValueType Declaration)} (listed : unionMembers union = some members)
    (candidate : Value Declaration Identity Source) :
    Membership.value world union candidate ↔ ∃ type ∈ members, Membership.value world type candidate := by
  cases union <;> simp only [unionMembers, reduceCtorEq, Option.some.injEq] at listed
  subst listed
  simp only [Membership.value, List.mem_cons, exists_eq_or_imp, members_iff]

theorem signature_view_kind {source target : SignatureType Declaration} (view : SignatureView source target) :
    Membership.signatureKind target = Membership.signatureKind source := by
  cases view
  rfl

mutual
  theorem view_member (world : MembershipWorld Declaration Identity Source) :
      ∀ {source target : ValueType Declaration}, BaseView source target →
        ∀ candidate : Value Declaration Identity Source,
          Membership.value world source candidate → Membership.value world target candidate
    | _, _, .refl _, _, member => member
    | _, _, .validated hasBase view, candidate, member =>
        view_member world view candidate (base_member world hasBase member)
    | _, _, .nonEmpty view, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | exact ⟨member.1, fun item present => view_member world view _ (member.2 item present)⟩
    | _, _, .list view, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | exact fun item present => view_member world view _ (member item present)
    | _, _, .map key view, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | exact fun entry present => ⟨(member entry present).1, view_member world view _ (member entry present).2⟩
          | exact ⟨member.1, fun field present => view_member world view _ (member.2 field present)⟩
    | _, _, .stream view, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | (obtain ⟨retained, held, chain⟩ := member; exact ⟨retained, held, .step chain view⟩)
    | _, _, .record covered _, candidate, member => by
        simp only [Membership.value] at member ⊢
        cases fields : Membership.recordFields world candidate <;> simp only [fields] at member ⊢
        all_goals first
          | exact member.elim
          | exact (row_iff world _ _).mpr
              (entries_member world covered _ ((row_iff world _ _).mp member))
    | _, _, .optional view, candidate, member => by
        simp only [Membership.value] at member ⊢
        rcases member with isNull | member
        · exact Or.inl isNull
        · exact Or.inr (view_member world view candidate member)
    | _, _, .result valueView errorView, candidate, member => by
        cases erased : candidate.eraseNominal <;> simp only [Membership.value, erased] at member ⊢
        all_goals first
          | exact member.elim
          | exact view_member world valueView _ member
          | exact view_member world errorView _ member
    | _, _, .union covered _ _ _, candidate, member => by
        have sourceListed := (union_member_iff world (union := .union _ _ _) rfl candidate).mp member
        exact (union_member_iff world (union := .union _ _ _) rfl candidate).mpr
          (members_member world covered candidate sourceListed)
    | _, _, .callable view, candidate, member => by
        simp only [Membership.value, Membership.callable] at member ⊢
        cases erased : candidate.eraseNominal <;> simp only [erased] at member ⊢
        all_goals first
          | exact member.elim
          | (obtain ⟨kind, retained, held, chain⟩ := member
             exact ⟨kind.trans (signature_view_kind view).symm, retained, held, .step chain view⟩)

  theorem entry_member (world : MembershipWorld Declaration Identity Source) :
      ∀ {entry : String × ValueType Declaration} {sources : List (String × ValueType Declaration)},
        EntryCovered entry sources → ∀ fields : List (String × Value Declaration Identity Source),
          (∀ source ∈ sources, ∃ found, fields.lookup source.1 = some found ∧ Membership.value world source.2 found) →
            ∃ found, fields.lookup entry.1 = some found ∧ Membership.value world entry.2 found
    | _, _, @EntryCovered.here _ name source _ rest view, fields, members => by
        obtain ⟨found, lookup, member⟩ := members (name, source) (by simp)
        exact ⟨found, lookup, view_member world view found member⟩
    | _, _, .there other covered, fields, members =>
        entry_member world covered fields (fun source present => members source (List.mem_cons_of_mem _ present))

  theorem entries_member (world : MembershipWorld Declaration Identity Source) :
      ∀ {targets sources : List (String × ValueType Declaration)}, EntriesCovered targets sources →
        ∀ fields : List (String × Value Declaration Identity Source),
          (∀ source ∈ sources, ∃ found, fields.lookup source.1 = some found ∧ Membership.value world source.2 found) →
            ∀ target ∈ targets, ∃ found, fields.lookup target.1 = some found ∧ Membership.value world target.2 found
    | _, _, .nil _, _, _ => by simp
    | _, _, .cons here rest, fields, members => by
        intro target present
        simp only [List.mem_cons] at present
        rcases present with same | later
        · exact same ▸ entry_member world here fields members
        · exact entries_member world rest fields members target later

  theorem member_member (world : MembershipWorld Declaration Identity Source) :
      ∀ {source : ValueType Declaration} {targets : List (ValueType Declaration)}, MemberCovered source targets →
        ∀ candidate : Value Declaration Identity Source, Membership.value world source candidate →
          ∃ target ∈ targets, Membership.value world target candidate
    | _, _, .here rest view, candidate, member => ⟨_, by simp, view_member world view candidate member⟩
    | _, _, .there other covered, candidate, member => by
        obtain ⟨target, present, targetMember⟩ := member_member world covered candidate member
        exact ⟨target, List.mem_cons_of_mem _ present, targetMember⟩

  theorem members_member (world : MembershipWorld Declaration Identity Source) :
      ∀ {sources targets : List (ValueType Declaration)}, MembersCovered sources targets →
        ∀ candidate : Value Declaration Identity Source, (∃ source ∈ sources, Membership.value world source candidate) →
          ∃ target ∈ targets, Membership.value world target candidate
    | _, _, .nil _, _, found => by simp at found
    | _, _, .cons here rest, candidate, found => by
        obtain ⟨source, present, member⟩ := found
        simp only [List.mem_cons] at present
        rcases present with same | later
        · exact member_member world here candidate (same ▸ member)
        · exact members_member world rest candidate ⟨source, later, member⟩
end

theorem base_is_unvalidated {validated baseType : ValueType Declaration}
    (hasBase : base validated = some baseType) : validationTag baseType = none := by
  cases validated <;> simp only [base, reduceCtorEq, Option.some.injEq] at hasBase <;> subst hasBase <;>
    try rfl
  rename_i bounds
  cases bounds.base <;> rfl

theorem validated_has_tag {validated baseType : ValueType Declaration}
    (hasBase : base validated = some baseType) : validationTag validated ≠ none := by
  cases validated <;> simp [base, validationTag] at hasBase ⊢

theorem view_from_unvalidated {source target : ValueType Declaration} (view : BaseView source target)
    (unvalidated : validationTag source = none) : validationTag target = none := by
  cases view with
  | refl => exact unvalidated
  | validated hasBase _ => exact absurd unvalidated (validated_has_tag hasBase)
  | nonEmpty _ => simp [validationTag] at unvalidated
  | _ => rfl

theorem view_into_validated {source target : ValueType Declaration} (view : BaseView source target)
    {tag : ValidationTag Declaration} (targetTag : validationTag target = some tag) :
    validationTag source = some tag ∧
      ∀ sourceBase targetBase, base source = some sourceBase → base target = some targetBase →
        BaseView sourceBase targetBase := by
  cases view with
  | refl =>
    refine ⟨targetTag, fun sourceBase targetBase sourceHas targetHas => ?_⟩
    rw [sourceHas, Option.some.injEq] at targetHas
    subst targetHas
    exact .refl _
  | validated hasBase rest =>
    rw [view_from_unvalidated rest (base_is_unvalidated hasBase)] at targetTag
    cases targetTag
  | nonEmpty elementView =>
    refine ⟨targetTag, fun sourceBase targetBase sourceHas targetHas => ?_⟩
    simp only [base, Option.some.injEq] at sourceHas targetHas
    subst sourceHas
    subst targetHas
    exact .list elementView
  | _ => simp [validationTag] at targetTag

theorem base_view {validated baseType : ValueType Declaration} (hasBase : base validated = some baseType) :
    BaseView validated baseType :=
  .validated hasBase (.refl _)

theorem base_never_views_validated {validated baseType : ValueType Declaration}
    (hasBase : base validated = some baseType) : ¬ BaseView baseType validated := by
  intro view
  obtain ⟨tag, tagged⟩ := Option.ne_none_iff_exists'.mp (validated_has_tag hasBase)
  have := (view_into_validated view tagged).1
  rw [base_is_unvalidated hasBase] at this
  cases this

theorem chain_member (world : MembershipWorld Declaration Identity Source) {source target : ValueType Declaration}
    (chain : ViewChain source target) (candidate : Value Declaration Identity Source)
    (member : Membership.value world source candidate) : Membership.value world target candidate := by
  induction chain with
  | refl => exact member
  | step _ view ih => exact view_member world view candidate ih

theorem equivalent_members (world : MembershipWorld Declaration Identity Source)
    {source target : ValueType Declaration} (equiv : source ≈ target) (candidate : Value Declaration Identity Source) :
    Membership.value world source candidate ↔ Membership.value world target candidate :=
  ⟨chain_member world (erasedEquiv_mutual_views equiv).1 candidate,
    chain_member world (erasedEquiv_mutual_views equiv).2 candidate⟩

/-- Validated widening over every Core type, value and world. A base view
replaces validated positions by their bases at any depth: inside `NonEmpty`,
lists, maps, streams, optionals, results, callable returns, record fields in
any order and union members in any order. Every view carries each member of
the source type to a member of the target, because retained stream and
callable contracts are read upward-closed under views. A view into a validated
type starts from a type with the same validation tag whose base views to the
target's base, so distinct validations, including nested intervals, never fit.
Every validated type views as its base, and no base views as its validated
type. A view between types whose rows have distinct labels is precise: with
validations erased, the two types are equivalent, differing at most in the
order of record fields and union members, and equivalent types have the same
members in every world. Such a view also introduces no validation at any
depth: for each validation tag, counted over every position, the target has at
most as many occurrences as the source, so a nested interval never widens to
another interval. -/
theorem validated_widening (world : MembershipWorld Declaration Identity Source) :
    (∀ (source target : ValueType Declaration) candidate, BaseView source target →
      Membership.value world source candidate → Membership.value world target candidate) ∧
    (∀ (source target : ValueType Declaration) tag, BaseView source target → validationTag target = some tag →
      validationTag source = some tag ∧
        ∀ sourceBase targetBase, base source = some sourceBase → base target = some targetBase →
          BaseView sourceBase targetBase) ∧
    (∀ validated baseType : ValueType Declaration, base validated = some baseType →
      BaseView validated baseType ∧ ¬ BaseView baseType validated) ∧
    (∀ source target : ValueType Declaration, RowsDistinct source → RowsDistinct target →
      BaseView source target → eraseValidations source ≈ eraseValidations target) ∧
    (∀ (source target : ValueType Declaration) candidate, source ≈ target →
      (Membership.value world source candidate ↔ Membership.value world target candidate)) ∧
    (∀ source target : ValueType Declaration, RowsDistinct source → RowsDistinct target →
      BaseView source target → Submultiset (validationTags target) (validationTags source)) :=
  ⟨fun _ _ candidate view member => view_member world view candidate member,
    fun _ _ _ view tagged => view_into_validated view tagged,
    fun _ _ hasBase => ⟨base_view hasBase, base_never_views_validated hasBase⟩,
    fun _ _ sourceDistinct targetDistinct view => view_precision sourceDistinct targetDistinct view,
    fun _ _ candidate equiv => equivalent_members world equiv candidate,
    fun _ _ sourceDistinct targetDistinct view => view_tags sourceDistinct targetDistinct view⟩

namespace Witness

abbrev WitnessValue := Value Unit Unit Unit
abbrev WitnessType := ValueType Unit

def openWorld : MembershipWorld Unit Unit Unit where
  entryFields := fun _ => []
  stream := fun _ _ => True
  callable := fun _ _ => True
  scheme := fun _ _ => False
  generic := fun _ _ => False

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

theorem relative_member : Membership.value openWorld .relativePath relative := by
  simp [Membership.value, relative, pathValue, Value.eraseNominal]
  decide

theorem nonempty_relative_widens_to_nonempty_path :
    BaseView (Declaration := Unit) (.nonEmpty .relativePath) (.nonEmpty (.primitive .path)) ∧
      Membership.value openWorld (.nonEmpty (.primitive .path)) (.list [relative]) := by
  have view : BaseView (Declaration := Unit) (.nonEmpty .relativePath) (.nonEmpty (.primitive .path)) :=
    .nonEmpty (.validated rfl (.refl _))
  refine ⟨view, view_member openWorld view _ ?_⟩
  simp only [Membership.value, Value.eraseNominal]
  have relativeAt : Membership.value openWorld .relativePath relative := relative_member
  refine ⟨by simp, fun item present => ?_⟩
  simp only [List.mem_singleton] at present
  subst present
  simpa only [Membership.value] using relativeAt

theorem reordered_union_and_record_are_views :
    BaseView (Declaration := Unit) (.union .relativePath (.primitive .string) .membersEmpty)
        (.union (.primitive .string) (.primitive .path) .membersEmpty) ∧
      BaseView (Declaration := Unit)
        (.record (.rowField "a" .relativePath (.rowField "b" (.primitive .string) .rowEmpty)))
        (.record (.rowField "b" (.primitive .string) (.rowField "a" (.primitive .path) .rowEmpty))) := by
  constructor
  · refine .union ?_ (by simp [tailMembers]) ?_ ?_
    · simp only [tailMembers]
      exact .cons (.there _ (.here _ (.validated rfl (.refl _)))) (.cons (.here _ (.refl _)) (.nil _))
    · simp only [tailMembers]
      exact .cons (.there _ (.here _ (.refl _))) (.cons (.here _ (.validated rfl (.refl _))) (.nil _))
    · simp only [tailMembers]
      exact .cons (.validated rfl (.refl _)) (.there _ (.here _ _))
        (.cons (.refl _) (.here _ _) .nil)
  · refine .record ?_ (by simp [rowEntries])
    simp only [rowEntries]
    exact .cons (.there _ (.here _ _ (.refl _))) (.cons (.here _ _ (.validated rfl (.refl _))) (.nil _))

theorem retained_relative_stream_is_a_path_stream :
    Membership.value relativeStreamWorld (.stream .relativePath) (.stream ⟨()⟩ : WitnessValue) ∧
      Membership.value relativeStreamWorld (.stream (.primitive .path)) (.stream ⟨()⟩ : WitnessValue) ∧
      ¬ Membership.value relativeStreamWorld (.stream (.primitive .string)) (.stream ⟨()⟩ : WitnessValue) := by
  have source : Membership.value relativeStreamWorld (.stream .relativePath) (.stream ⟨()⟩ : WitnessValue) := by
    simp only [Membership.value, Value.eraseNominal]
    exact ⟨.relativePath, rfl, .refl _⟩
  refine ⟨source, view_member relativeStreamWorld (.stream (.validated rfl (.refl _))) _ source, ?_⟩
  simp only [Membership.value, Value.eraseNominal]
  rintro ⟨retained, held, chain⟩
  simp only [relativeStreamWorld] at held
  subst held
  have : ∀ target : WitnessType, ViewChain .relativePath target →
      target = .relativePath ∨ target = .primitive .path := by
    intro target chain
    induction chain with
    | refl => exact Or.inl rfl
    | step _ view ih =>
      rcases ih with same | same <;> subst same
      · cases view with
        | refl => exact Or.inl rfl
        | validated hasBase rest =>
          simp only [base, Option.some.injEq] at hasBase
          subst hasBase
          exact Or.inr (view_from_primitive rest)
      · exact Or.inr (view_from_primitive view)
  rcases this _ chain with same | same <;> cases same

theorem distinct_validations_never_view :
    Xsh.Validation.BoundedRefinements.widensTo narrowPort widePort ∧
      ¬ BaseView (Declaration := Unit) (.boundedInteger narrowPort) (.boundedInteger widePort) ∧
      ¬ BaseView (Declaration := Unit) (.primitive .path) .relativePath ∧
      ¬ Membership.value openWorld .relativePath absolute := by
  refine ⟨by unfold Xsh.Validation.BoundedRefinements.widensTo; decide, ?_, base_never_views_validated rfl, ?_⟩
  · intro view
    have := (view_into_validated view (tag := .bounded widePort) rfl).1
    simp [validationTag, narrowPort, widePort] at this
  · simp [Membership.value, absolute, pathValue, Value.eraseNominal]
    decide

/-- The nested-interval mutant: a list of one interval read as a list of a
wider one. The real relation rejects it, and so does the tag clause of
`validated_widening`, so a view relation with an extra nested-interval rule
could not satisfy that theorem. -/
theorem nested_interval_never_views :
    ¬ BaseView (Declaration := Unit) (.list (.boundedInteger narrowPort)) (.list (.boundedInteger widePort)) ∧
      ¬ Submultiset (validationTags (.list (.boundedInteger widePort) : WitnessType))
        (validationTags (.list (.boundedInteger narrowPort) : WitnessType)) := by
  constructor
  · intro view
    cases view with
    | validated hasBase _ => simp [base] at hasBase
    | list inner =>
      have := (view_into_validated inner (tag := .bounded widePort) rfl).1
      simp [validationTag, narrowPort, widePort] at this
  · intro within
    have counted := within.count_le (.bounded widePort)
    revert counted
    decide

def pureReturning (result : WitnessType) : WitnessType :=
  .callable (.signature .pure .argumentsEmpty result)

def duplicateRow : WitnessType :=
  .record (.rowField "a" .relativePath (.rowField "a" (.primitive .integer) .rowEmpty))

def pathRow : WitnessType := .record (.rowField "a" (.primitive .path) .rowEmpty)

theorem duplicate_row_breaks_precision :
    BaseView duplicateRow pathRow ∧ ¬ RowsDistinct duplicateRow ∧
      ¬ eraseValidations duplicateRow ≈ eraseValidations pathRow := by
  refine ⟨.record ?_ (by simp [rowEntries]), ?_, ?_⟩
  · simp only [rowEntries]
    exact .cons (.here _ _ (.validated rfl (.refl _))) (.nil _)
  · simp [duplicateRow, RowsDistinct, rowEntries]
  · intro equiv
    have widths := erasedEquiv_recordWidth equiv
    simp [duplicateRow, pathRow, eraseValidations, recordWidth] at widths

theorem reordered_views_are_precise :
    eraseValidations (Declaration := Unit) (.union .relativePath (.primitive .string) .membersEmpty) ≈
        eraseValidations (Declaration := Unit) (.union (.primitive .string) (.primitive .path) .membersEmpty) ∧
      eraseValidations (Declaration := Unit)
          (.record (.rowField "a" .relativePath (.rowField "b" (.primitive .string) .rowEmpty))) ≈
        eraseValidations (Declaration := Unit)
          (.record (.rowField "b" (.primitive .string) (.rowField "a" (.primitive .path) .rowEmpty))) :=
  ⟨view_precision ⟨trivial, trivial, trivial⟩ ⟨trivial, trivial, trivial⟩ reordered_union_and_record_are_views.1,
    view_precision (by simp [RowsDistinct, rowEntries]) (by simp [RowsDistinct, rowEntries])
      reordered_union_and_record_are_views.2⟩

theorem union_cannot_gain_member :
    ¬ BaseView (Declaration := Unit)
      (.list (.union (.primitive .integer) (.primitive .string) .membersEmpty))
      (.list (.union (.primitive .integer) (.primitive .string) (.member (.primitive .path) .membersEmpty))) := by
  intro view
  cases view with
  | validated hasBase _ => simp [base] at hasBase
  | list inner =>
    cases inner with
    | validated hasBase _ => simp [base] at hasBase
    | union _ length _ _ => simp [tailMembers] at length

theorem record_cannot_drop_field :
    ¬ BaseView (Declaration := Unit)
      (.record (.rowField "a" .relativePath (.rowField "b" (.primitive .integer) .rowEmpty)))
      (.record (.rowField "a" (.primitive .path) .rowEmpty)) := by
  intro view
  cases view with
  | validated hasBase _ => simp [base] at hasBase
  | record _ names =>
    have present := names "b" (by simp [rowEntries])
    simp [rowEntries] at present

theorem list_string_cannot_view_list_any :
    ¬ BaseView (Declaration := Unit) (.list (.primitive .string)) (.list (.primitive .any)) := by
  intro view
  cases view with
  | validated hasBase _ => simp [base] at hasBase
  | list inner =>
    have := view_from_primitive inner
    cases this

theorem callable_return_view_is_derived :
    BaseView (Declaration := Unit) (.list (pureReturning .relativePath)) (.list (pureReturning (.primitive .path))) :=
  .list (.callable (.result _ _ (.validated rfl (.refl _))))

end Witness

end Xsh.Validation.Core.Widen
