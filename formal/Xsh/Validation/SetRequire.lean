import Xsh.Collections.KeyOrder
import Xsh.Values.OrderedSet
import Lean.Elab.Tactic.Omega

/-!
An independent finite model of `.require(Set[T])`. A finite ordinal indexes
primitive keys in an explicitly order-preserving key table. Existing sets are
accepted only when every stored key matches the target element predicate.
Decoded arrays convert only after every item is a matching key and no key
repeats. The output set iterates in primitive key order.

Nested arrays and other non-key values remain representable as array members
and are rejected as a whole. This fragment does not model diagnostic text,
source checking, JSON parsing, or Rust correspondence.
-/

namespace Xsh.Validation.SetRequire

open Xsh.Collections.KeyOrder
open Xsh.Values.OrderedSet

/-- An explicit finite enumeration whose ordinal order agrees with key order. -/
structure KeyOrderTable (domain : KeyDomain) (size : Nat) where
  keyAt : Fin size → Key domain
  orderPreserving : ∀ left right, left < right →
    keyLE domain (keyAt left) (keyAt right)
  injective : Function.Injective keyAt

/-- Raw decoded members keep non-key and nested values representable. -/
inductive Member (size : Nat) where
  | key (index : Fin size)
  | nestedArray (members : List (Member size))
  | other (tag : Nat)

/-- A set candidate is either an existing set or a decoded array. -/
inductive Candidate (size : Nat) where
  | existing (set : OrderedSet size)
  | decodedArray (members : List (Member size))

/-- Array failures carry an index; an existing set fails as a whole. -/
inductive Failure where
  | wrongMember (position : Nat)
  | duplicateMember (position : Nat)
  | existingSetMismatch
  deriving DecidableEq, Repr

/-- A rejection has no converted set, so conversion cannot publish partial data. -/
inductive RequireResult (size : Nat) where
  | accepted (set : OrderedSet size)
  | rejected (failure : Failure)

def memberKey? : Member size → Option (Fin size)
  | .key index => some index
  | .nestedArray _ | .other _ => none

def memberMatches (table : KeyOrderTable domain size)
    (accepts : Key domain → Bool) : Member size → Bool
  | .key index => accepts (table.keyAt index)
  | .nestedArray _ | .other _ => false

def allMembersMatch (table : KeyOrderTable domain size)
    (accepts : Key domain → Bool) : List (Member size) → Bool
  | [] => true
  | member :: rest => memberMatches table accepts member &&
      allMembersMatch table accepts rest

def arrayKeys (members : List (Member size)) : List (Fin size) :=
  members.filterMap memberKey?

def firstWrongKeyIndex (table : KeyOrderTable domain size)
    (accepts : Key domain → Bool) : List (Fin size) → Nat → Option Nat
  | [], _ => none
  | key :: rest, position =>
      if accepts (table.keyAt key) then
        firstWrongKeyIndex table accepts rest (position + 1)
      else
        some position

def firstArrayFailure (table : KeyOrderTable domain size)
    (accepts : Key domain → Bool) : List (Member size) → List (Fin size) → Nat →
      Option Failure
  | [], _, _ => none
  | member :: rest, seen, position =>
      match member with
      | .key index =>
          if accepts (table.keyAt index) then
            if index ∈ seen then some (.duplicateMember position)
            else firstArrayFailure table accepts rest (index :: seen) (position + 1)
          else
            some (.wrongMember position)
      | .nestedArray _ | .other _ => some (.wrongMember position)

private theorem arrayKeys_key_cons (index : Fin size)
    (rest : List (Member size)) :
    arrayKeys (.key index :: rest) = index :: arrayKeys rest := by
  simp [arrayKeys, memberKey?]

private theorem firstArrayFailure_none_iff
    (table : KeyOrderTable domain size) (accepts : Key domain → Bool)
    (members : List (Member size)) (seen : List (Fin size)) (position : Nat) :
    firstArrayFailure table accepts members seen position = none ↔
      allMembersMatch table accepts members = true ∧
        (arrayKeys members).Nodup ∧
        ∀ key, key ∈ arrayKeys members → key ∉ seen := by
  induction members generalizing seen position with
  | nil => simp [firstArrayFailure, allMembersMatch, arrayKeys]
  | cons member rest ih =>
      cases member with
      | nestedArray nested =>
          simp [firstArrayFailure, allMembersMatch, memberMatches, arrayKeys,
            memberKey?]
      | other tag =>
          simp [firstArrayFailure, allMembersMatch, memberMatches, arrayKeys,
            memberKey?]
      | key index =>
          by_cases keyMatches : accepts (table.keyAt index)
          · by_cases repeated : index ∈ seen
            · simp [firstArrayFailure, keyMatches, repeated, allMembersMatch,
                memberMatches, arrayKeys, memberKey?]
            · have restIff := ih (index :: seen) (position + 1)
              simp [firstArrayFailure, keyMatches, repeated]
              rw [arrayKeys_key_cons]
              rw [restIff]
              simp [allMembersMatch, memberMatches, List.nodup_cons,
                List.mem_cons]
              constructor
              · rintro ⟨allRest, nodupRest, restDisjoint⟩
                have indexNotInKeys : index ∉ arrayKeys rest := by
                  intro inKeys
                  exact (restDisjoint index inKeys).1 rfl
                refine ⟨⟨keyMatches, allRest⟩,
                  ⟨indexNotInKeys, nodupRest⟩, repeated, ?_⟩
                intro key inKeys
                exact (restDisjoint key inKeys).2
              · rintro ⟨⟨_, allRest⟩, ⟨indexNotInKeys, nodupRest⟩,
                  _, restDisjoint⟩
                refine ⟨allRest, nodupRest, ?_⟩
                intro key inKeys
                refine ⟨?_, restDisjoint key inKeys⟩
                intro same
                subst key
                exact indexNotInKeys inKeys
          · simp [firstArrayFailure, keyMatches, allMembersMatch, memberMatches,
              arrayKeys, memberKey?]

def require (table : KeyOrderTable domain size) (accepts : Key domain → Bool) :
    Candidate size → RequireResult size
  | .existing set =>
      match firstWrongKeyIndex table accepts (toList set) 0 with
      | none => .accepted set
      | some _ => .rejected .existingSetMismatch
  | .decodedArray members =>
      match firstArrayFailure table accepts members [] 0 with
      | none => .accepted (ofList (arrayKeys members))
      | some failure => .rejected failure

private theorem decoded_array_acceptance_iff_typed_nodup
    (table : KeyOrderTable domain size) (accepts : Key domain → Bool)
    (members : List (Member size)) :
    (∃ set, require table accepts (.decodedArray members) = .accepted set) ↔
      allMembersMatch table accepts members = true ∧ (arrayKeys members).Nodup := by
  constructor
  · rintro ⟨set, accepted⟩
    unfold require at accepted
    cases checked : firstArrayFailure table accepts members [] 0 with
    | some failure => simp [checked] at accepted
    | none =>
        have properties :=
          (firstArrayFailure_none_iff table accepts members [] 0).mp checked
        exact ⟨properties.1, properties.2.1⟩
  · rintro ⟨allMatch, noDuplicates⟩
    have noFailure : firstArrayFailure table accepts members [] 0 = none :=
      (firstArrayFailure_none_iff table accepts members [] 0).mpr
        ⟨allMatch, noDuplicates, by simp⟩
    refine ⟨ofList (arrayKeys members), ?_⟩
    simp [require, noFailure]

private theorem firstWrongKeyIndex_none_iff
    (table : KeyOrderTable domain size) (accepts : Key domain → Bool)
    (keys : List (Fin size)) (position : Nat) :
    firstWrongKeyIndex table accepts keys position = none ↔
      ∀ key, key ∈ keys → accepts (table.keyAt key) = true := by
  induction keys generalizing position with
  | nil => simp [firstWrongKeyIndex]
  | cons head tail inductionHypothesis =>
      constructor
      · intro noWrong
        have headMatches : accepts (table.keyAt head) = true := by
          cases result : accepts (table.keyAt head) with
          | false => simp [firstWrongKeyIndex, result] at noWrong
          | true => rfl
        have tailNoWrong : firstWrongKeyIndex table accepts tail (position + 1) = none := by
          simpa [firstWrongKeyIndex, headMatches] using noWrong
        intro key keyIn
        rcases List.mem_cons.mp keyIn with same | inTail
        · simpa [same] using headMatches
        · exact (inductionHypothesis (position + 1)).mp tailNoWrong key inTail
      · intro allMatch
        have headMatches : accepts (table.keyAt head) = true :=
          allMatch head (by simp)
        have tailMatches : ∀ key, key ∈ tail → accepts (table.keyAt key) = true := by
          intro key inTail
          exact allMatch key (by simp [inTail])
        have tailNoWrong : firstWrongKeyIndex table accepts tail (position + 1) = none :=
          (inductionHypothesis (position + 1)).mpr tailMatches
        simpa [firstWrongKeyIndex, headMatches] using tailNoWrong

theorem existing_set_acceptance_iff_all_members_match
    (table : KeyOrderTable domain size) (accepts : Key domain → Bool)
    (set : OrderedSet size) :
    require table accepts (.existing set) = .accepted set ↔
      ∀ key, key ∈ toList set → accepts (table.keyAt key) = true := by
  unfold require
  cases checked : firstWrongKeyIndex table accepts (toList set) 0 with
  | none =>
      constructor
      · intro _
        exact (firstWrongKeyIndex_none_iff table accepts (toList set) 0).mp checked
      · intro _
        simp [checked]
  | some position =>
      constructor
      · intro impossible
        simp [checked] at impossible
      · intro allMatches
        have noWrong := (firstWrongKeyIndex_none_iff table accepts (toList set) 0).mpr allMatches
        rw [checked] at noWrong
        contradiction

theorem array_acceptance_preserves_membership
    (table : KeyOrderTable domain size) (accepts : Key domain → Bool)
    (members : List (Member size)) (set : OrderedSet size)
    (accepted : require table accepts (.decodedArray members) = .accepted set) :
    ∀ key, set.contains key = true ↔ key ∈ arrayKeys members := by
  intro key
  unfold require at accepted
  cases failed : firstArrayFailure table accepts members [] 0 with
  | some failure => simp [failed] at accepted
  | none =>
      simp only [failed] at accepted
      cases accepted
      exact ofList_membership (arrayKeys members) key

theorem accepted_array_iteration_uses_primitive_key_order
    (table : KeyOrderTable domain size) (accepts : Key domain → Bool)
    (members : List (Member size)) (set : OrderedSet size)
    (accepted : require table accepts (.decodedArray members) = .accepted set) :
    (toList set |>.map table.keyAt).Pairwise
      (fun left right => keyLE domain left right ∧ left ≠ right) := by
  unfold require at accepted
  cases failed : firstArrayFailure table accepts members [] 0 with
  | some failure => simp [failed] at accepted
  | none =>
      simp only [failed] at accepted
      cases accepted
      exact (toList_key_ordered (ofList (arrayKeys members))).map
        table.keyAt (by
          intro left right before
          constructor
          · exact table.orderPreserving left right before
          · intro sameKey
            exact (Fin.ne_of_lt before) (table.injective sameKey))

/-- False and true enumerate the finite Boolean key domain in key order. -/
def boolAt (index : Fin 2) : Bool := if index.val = 1 then true else false

def boolKeyTable : KeyOrderTable .bool 2 where
  keyAt := boolAt
  orderPreserving := by
    intro left right ordered
    have leftBound : left.val < 2 := left.isLt
    have rightBound : right.val < 2 := right.isLt
    have ordinalOrder : left.val < right.val := ordered
    by_cases leftTrue : left.val = 1
    · have rightTrue : right.val = 1 := by omega
      simp [boolAt, leftTrue, rightTrue, keyLE]
    · have leftFalse : left.val = 0 := by omega
      by_cases rightTrue : right.val = 1
      · simp [boolAt, leftFalse, rightTrue, keyLE]
      · have rightFalse : right.val = 0 := by omega
        simp [boolAt, leftFalse, rightFalse, keyLE]
  injective := by
    intro left right sameKey
    have leftBound : left.val < 2 := left.isLt
    have rightBound : right.val < 2 := right.isLt
    have sameIndex : left.val = right.val := by
      by_cases leftTrue : left.val = 1 <;> by_cases rightTrue : right.val = 1
      · omega
      · simp [boolAt, leftTrue, rightTrue] at sameKey
      · simp [boolAt, leftTrue, rightTrue] at sameKey
      · omega
    exact Fin.ext sameIndex

def boolIndexFalse : Fin 2 := ⟨0, by decide⟩
def boolIndexTrue : Fin 2 := ⟨1, by decide⟩

def allBooleanKeys : Key .bool → Bool := fun _ => true

def falseOnly : Key .bool → Bool := fun key => !key

def repeatedBooleanArray : List (Member 2) := [
  .key boolIndexTrue,
  .key boolIndexFalse,
  .key boolIndexTrue
]

def wrongBooleanArray : List (Member 2) := [
  .key boolIndexFalse,
  .other 7
]

def nestedInvalidBooleanArray : List (Member 2) := [
  .key boolIndexFalse,
  .nestedArray [.other 9]
]

theorem existing_set_is_accepted_without_membership_change :
    require boolKeyTable allBooleanKeys
        (.existing (ofList [boolIndexTrue, boolIndexFalse])) =
      .accepted (ofList [boolIndexTrue, boolIndexFalse]) := by
  rfl

theorem existing_set_rejects_when_member_outside_target_type :
    require boolKeyTable falseOnly
        (.existing (ofList [boolIndexFalse, boolIndexTrue])) =
      .rejected .existingSetMismatch := by
  rfl

theorem decoded_array_converts_only_after_unique_members_match :
    require boolKeyTable allBooleanKeys
        (.decodedArray [.key boolIndexTrue, .key boolIndexFalse]) =
      .accepted (ofList [boolIndexTrue, boolIndexFalse]) := by
  rfl

theorem invalid_member_rejects_whole_array_at_its_position :
    require boolKeyTable allBooleanKeys (.decodedArray wrongBooleanArray) =
      .rejected (.wrongMember 1) := by
  rfl

theorem duplicate_member_reports_repeated_position :
    require boolKeyTable allBooleanKeys (.decodedArray repeatedBooleanArray) =
      .rejected (.duplicateMember 2) := by
  rfl

theorem nested_invalid_member_rejects_whole_array :
    require boolKeyTable allBooleanKeys (.decodedArray nestedInvalidBooleanArray) =
      .rejected (.wrongMember 1) := by
  rfl

def deduplicateMutation (table : KeyOrderTable domain size)
    (accepts : Key domain → Bool) : Candidate size → RequireResult size
  | candidate@(.existing _) => require table accepts candidate
  | .decodedArray members =>
      if members.all (memberMatches table accepts) then
        .accepted (ofList (arrayKeys members))
      else
        require table accepts (.decodedArray members)

theorem deduplication_mutation_is_detected_by_duplicate_array :
    require boolKeyTable allBooleanKeys (.decodedArray repeatedBooleanArray) =
      .rejected (.duplicateMember 2) ∧
    deduplicateMutation boolKeyTable allBooleanKeys (.decodedArray repeatedBooleanArray) =
      .accepted (ofList [boolIndexTrue, boolIndexFalse, boolIndexTrue]) ∧
    (toList (ofList [boolIndexTrue, boolIndexFalse, boolIndexTrue]) =
      [boolIndexFalse, boolIndexTrue]) := by
  refine ⟨rfl, rfl, by decide⟩

end Xsh.Validation.SetRequire
