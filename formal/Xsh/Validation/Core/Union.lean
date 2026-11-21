import Xsh.Validation.Core.Widen

namespace Xsh.Validation.Core.Union

open Xsh.Core
open Xsh.Validation.Core
open Xsh.Validation.Core.Widen

variable {Declaration Identity Source : Type}

def tailMembers : UnionTail Declaration → List (ValueType Declaration)
  | .membersEmpty => []
  | .member type rest => type :: tailMembers rest

def tailOf : List (ValueType Declaration) → UnionTail Declaration
  | [] => .membersEmpty
  | type :: rest => .member type (tailOf rest)

def unionMembers : ValueType Declaration → Option (List (ValueType Declaration))
  | .union first second rest => some (first :: second :: tailMembers rest)
  | _ => none

def unionOf : List (ValueType Declaration) → Option (ValueType Declaration)
  | [] => none
  | [type] => some type
  | first :: second :: rest => some (.union first second (tailOf rest))

theorem tailMembers_tailOf (types : List (ValueType Declaration)) : tailMembers (tailOf types) = types := by
  induction types with
  | nil => simp [tailOf, tailMembers]
  | cons head rest ih => simp [tailOf, tailMembers, ih]

theorem members_iff (world : MembershipWorld Declaration Identity Source) (rest : UnionTail Declaration)
    (candidate : Value Declaration Identity Source) :
    Membership.members world rest candidate ↔ ∃ type ∈ tailMembers rest, Membership.value world type candidate := by
  match rest with
  | .variable id => exact nomatch id
  | .membersEmpty => simp [Membership.members, tailMembers]
  | .member type tail =>
    simp only [Membership.members, tailMembers, List.mem_cons, exists_eq_or_imp]
    rw [members_iff world tail candidate]

theorem union_member_iff (world : MembershipWorld Declaration Identity Source) {union : ValueType Declaration}
    {members : List (ValueType Declaration)} (listed : unionMembers union = some members)
    (candidate : Value Declaration Identity Source) :
    Membership.value world union candidate ↔ ∃ type ∈ members, Membership.value world type candidate := by
  cases union <;> simp only [unionMembers, reduceCtorEq, Option.some.injEq] at listed
  subst listed
  simp only [Membership.value, List.mem_cons, exists_eq_or_imp, members_iff]

theorem unionOf_member_iff (world : MembershipWorld Declaration Identity Source)
    {members : List (ValueType Declaration)} {union : ValueType Declaration} (built : unionOf members = some union)
    (candidate : Value Declaration Identity Source) :
    Membership.value world union candidate ↔ ∃ type ∈ members, Membership.value world type candidate := by
  match members, built with
  | [type], built =>
    simp only [unionOf, Option.some.injEq] at built
    subst built
    simp
  | first :: second :: rest, built =>
    simp only [unionOf, Option.some.injEq] at built
    subst built
    exact union_member_iff world (by simp [unionMembers, tailMembers_tailOf]) candidate

def classify (world : MembershipWorld Declaration Identity Source) [DecidableEq Declaration]
    (contracts : ContractDecisions world) :
    List (ValueType Declaration) → Value Declaration Identity Source → Option Nat
  | [], _ => none
  | type :: rest, candidate =>
    letI := Membership.decideTerm world contracts type candidate
    if Membership.value world type candidate then some 0 else (classify world contracts rest candidate).map (· + 1)

theorem classify_some_iff (world : MembershipWorld Declaration Identity Source) [DecidableEq Declaration]
    (contracts : ContractDecisions world) (members : List (ValueType Declaration))
    (candidate : Value Declaration Identity Source) (index : Nat) :
    classify world contracts members candidate = some index ↔
      ∃ selected : index < members.length, Membership.value world members[index] candidate ∧
        ∀ earlier (before : earlier < index),
          ¬ Membership.value world (members[earlier]'(Nat.lt_trans before selected)) candidate := by
  induction members generalizing index with
  | nil => simp [classify]
  | cons type rest ih =>
    letI := Membership.decideTerm world contracts type candidate
    by_cases accepted : Membership.value world type candidate
    · simp only [classify, accepted, ite_true, Option.some.injEq]
      constructor
      · intro same
        subst same
        exact ⟨by simp, accepted, fun earlier before => absurd before (Nat.not_lt_zero _)⟩
      · rintro ⟨selected, member, none⟩
        cases index with
        | zero => rfl
        | succ index => exact absurd accepted (none 0 (Nat.succ_pos _))
    · simp only [classify, accepted, ite_false]
      cases index with
      | zero =>
        constructor
        · intro found
          obtain ⟨_, _, impossible⟩ := Option.map_eq_some_iff.mp found
          omega
        · rintro ⟨_, member, _⟩
          exact absurd (by simpa using member) accepted
      | succ index =>
        simp only [Option.map_eq_some_iff, Nat.add_right_cancel_iff, exists_eq_right, ih]
        constructor
        · rintro ⟨selected, member, none⟩
          refine ⟨by simpa using selected, by simpa using member, ?_⟩
          intro earlier before
          cases earlier with
          | zero => simpa using accepted
          | succ earlier => simpa using none earlier (by omega)
        · rintro ⟨selected, member, none⟩
          refine ⟨by simpa using selected, by simpa using member, ?_⟩
          intro earlier before
          simpa using none (earlier + 1) (by omega)

theorem classify_none_iff (world : MembershipWorld Declaration Identity Source) [DecidableEq Declaration]
    (contracts : ContractDecisions world) (members : List (ValueType Declaration))
    (candidate : Value Declaration Identity Source) :
    classify world contracts members candidate = none ↔
      ∀ type ∈ members, ¬ Membership.value world type candidate := by
  induction members with
  | nil => simp [classify]
  | cons type rest ih =>
    letI := Membership.decideTerm world contracts type candidate
    by_cases accepted : Membership.value world type candidate
    · simp [classify, accepted]
    · simp [classify, accepted, ih]

theorem union_view_member (world : MembershipWorld Declaration Identity Source) (closed : ViewClosed world)
    {source target : ValueType Declaration} {sourceMembers targetMembers : List (ValueType Declaration)}
    (sourceListed : unionMembers source = some sourceMembers) (targetListed : unionMembers target = some targetMembers)
    (covered : ∀ member ∈ sourceMembers, ∃ wider ∈ targetMembers, BaseView member wider)
    (candidate : Value Declaration Identity Source) (member : Membership.value world source candidate) :
    Membership.value world target candidate := by
  obtain ⟨found, present, foundMember⟩ := (union_member_iff world sourceListed candidate).mp member
  obtain ⟨wider, widerPresent, view⟩ := covered found present
  exact (union_member_iff world targetListed candidate).mpr
    ⟨wider, widerPresent, view_member world closed view candidate foundMember⟩

/-- Union membership and selection over every Core union. A value is a member
of a union exactly when it is a member of one listed member type, the union is
not a runtime wrapper, and runtime selection returns the first member in
written order that accepts the value, or none when no member does. A union
fits another union when each of its members views as one of the other's,
whatever the order. -/
theorem union_semantics (world : MembershipWorld Declaration Identity Source) [DecidableEq Declaration]
    (contracts : ContractDecisions world) (closed : ViewClosed world) :
    (∀ (union : ValueType Declaration) members candidate, unionMembers union = some members →
      (Membership.value world union candidate ↔ ∃ type ∈ members, Membership.value world type candidate)) ∧
    (∀ (members : List (ValueType Declaration)) union candidate, unionOf members = some union →
      (Membership.value world union candidate ↔ ∃ type ∈ members, Membership.value world type candidate)) ∧
    (∀ (members : List (ValueType Declaration)) candidate index,
      classify world contracts members candidate = some index ↔
        ∃ selected : index < members.length, Membership.value world members[index] candidate ∧
          ∀ earlier (before : earlier < index),
            ¬ Membership.value world (members[earlier]'(Nat.lt_trans before selected)) candidate) ∧
    (∀ (members : List (ValueType Declaration)) candidate,
      classify world contracts members candidate = none ↔
        ∀ type ∈ members, ¬ Membership.value world type candidate) ∧
    (∀ (source target : ValueType Declaration) sourceMembers targetMembers candidate,
      unionMembers source = some sourceMembers → unionMembers target = some targetMembers →
      (∀ member ∈ sourceMembers, ∃ wider ∈ targetMembers, BaseView member wider) →
      Membership.value world source candidate → Membership.value world target candidate) :=
  ⟨fun _ _ candidate listed => union_member_iff world listed candidate,
    fun _ _ candidate built => unionOf_member_iff world built candidate,
    classify_some_iff world contracts, classify_none_iff world contracts,
    fun _ _ _ _ candidate sourceListed targetListed covered member =>
      union_view_member world closed sourceListed targetListed covered candidate member⟩

namespace Witness

open Xsh.Validation.Core.Widen.Witness

def openContracts : ContractDecisions openWorld where
  stream := fun _ _ => instDecidableTrue
  callable := fun _ _ => instDecidableTrue
  scheme := fun _ _ => instDecidableFalse
  generic := fun _ _ => instDecidableFalse

def five : WitnessValue := .scalar (.integer ⟨5, by decide⟩)
def minusFive : WitnessValue := .scalar (.integer ⟨-5, by decide⟩)

def overlapping : List WitnessType := [.primitive .unsignedInteger, .primitive .integer]

theorem overlapping_union_selects_the_first_accepting_member :
    classify openWorld openContracts overlapping five = some 0 ∧
      classify openWorld openContracts overlapping minusFive = some 1 ∧
      classify openWorld openContracts [.primitive .string, .primitive .boolean] five = none := by
  refine ⟨?_, ?_, ?_⟩ <;>
    simp [classify, overlapping, five, minusFive, Membership.value, Membership.primitive, Value.eraseNominal]

theorem reordered_union_still_fits_and_member_alone_does_not :
    Membership.value openWorld (.union .relativePath (.primitive .string) .membersEmpty) relative ∧
      Membership.value openWorld (.union (.primitive .string) (.primitive .path) .membersEmpty) relative ∧
      Membership.value openWorld (.union (.primitive .string) (.primitive .path) .membersEmpty)
        (.scalar (.string "a")) ∧
      ¬ Membership.value openWorld (.primitive .path) (.scalar (.string "a") : WitnessValue) := by
  have member : Membership.value openWorld (.union .relativePath (.primitive .string) .membersEmpty) relative := by
    simp [Membership.value, Membership.members, Membership.primitive, relative, pathValue, Value.eraseNominal]
    decide
  refine ⟨member, union_view_member openWorld openWorld_closed rfl rfl ?_ _ member, ?_, ?_⟩
  · intro type present
    simp only [List.mem_cons, tailMembers, List.not_mem_nil, or_false] at present
    rcases present with same | same <;> subst same
    · exact ⟨.primitive .path, by simp, .validated rfl (.refl _)⟩
    · exact ⟨.primitive .string, by simp, .refl _⟩
  · simp [Membership.value, Membership.primitive, Value.eraseNominal]
  · simp [Membership.value, Membership.primitive, Value.eraseNominal]

end Witness

end Xsh.Validation.Core.Union
