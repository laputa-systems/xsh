import Xsh.Values.OrderedSet

/-!
Algebra laws for finite ordered sets over ordinal keys. Every law compares the
canonical `toList` result, which scans keys in increasing ordinal order, so the
results preserve the model's observable iteration order as well as membership.
They do not model first-insertion history.
-/

namespace Xsh.Sets.Algebra

open Xsh.Values.OrderedSet

private theorem same_iteration_of_same_membership {domainSize : Nat}
    {left right : OrderedSet domainSize}
    (sameMembership : ∀ key, left.contains key = right.contains key) :
    toList left = toList right := by
  have sameSet : left = right := by
    cases left with
    | mk leftContains =>
      cases right with
      | mk rightContains =>
        have sameFunction : leftContains = rightContains := by
          funext key
          exact sameMembership key
        cases sameFunction
        rfl
  rw [sameSet]

/-- Union is commutative in the canonical ordered iteration. -/
theorem union_commutativity {domainSize : Nat}
    (left right : OrderedSet domainSize) :
    toList (union left right) = toList (union right left) := by
  apply same_iteration_of_same_membership
  intro key
  cases leftMember : left.contains key <;>
    cases rightMember : right.contains key <;>
    simp [union, leftMember, rightMember]

/-- Union is associative in the canonical ordered iteration. -/
theorem union_associativity {domainSize : Nat}
    (first second third : OrderedSet domainSize) :
    toList (union (union first second) third) =
      toList (union first (union second third)) := by
  apply same_iteration_of_same_membership
  intro key
  cases firstMember : first.contains key <;>
    cases secondMember : second.contains key <;>
    cases thirdMember : third.contains key <;>
    simp [union, firstMember, secondMember, thirdMember]

/-- Union with itself preserves the canonical ordered iteration. -/
theorem union_idempotence {domainSize : Nat}
    (set : OrderedSet domainSize) :
    toList (union set set) = toList set := by
  apply same_iteration_of_same_membership
  intro key
  cases member : set.contains key <;> simp [union, member]

/-- The empty set is a left identity for union. -/
theorem union_empty_left {domainSize : Nat} (set : OrderedSet domainSize) :
    toList (union (empty domainSize) set) = toList set := by
  apply same_iteration_of_same_membership
  intro key
  cases member : set.contains key <;> simp [union, empty, member]

/-- The empty set is a right identity for union. -/
theorem union_empty_right {domainSize : Nat} (set : OrderedSet domainSize) :
    toList (union set (empty domainSize)) = toList set := by
  apply same_iteration_of_same_membership
  intro key
  cases member : set.contains key <;> simp [union, empty, member]

/-- Intersection is commutative in the canonical ordered iteration. -/
theorem intersection_commutativity {domainSize : Nat}
    (left right : OrderedSet domainSize) :
    toList (intersection left right) = toList (intersection right left) := by
  apply same_iteration_of_same_membership
  intro key
  cases leftMember : left.contains key <;>
    cases rightMember : right.contains key <;>
    simp [intersection, leftMember, rightMember]

/-- Intersection is associative in the canonical ordered iteration. -/
theorem intersection_associativity {domainSize : Nat}
    (first second third : OrderedSet domainSize) :
    toList (intersection (intersection first second) third) =
      toList (intersection first (intersection second third)) := by
  apply same_iteration_of_same_membership
  intro key
  cases firstMember : first.contains key <;>
    cases secondMember : second.contains key <;>
    cases thirdMember : third.contains key <;>
    simp [intersection, firstMember, secondMember, thirdMember]

/-- Intersecting a set with itself preserves the canonical ordered iteration. -/
theorem intersection_idempotence {domainSize : Nat}
    (set : OrderedSet domainSize) :
    toList (intersection set set) = toList set := by
  apply same_iteration_of_same_membership
  intro key
  cases member : set.contains key <;> simp [intersection, member]

/-- The empty set annihilates intersection. -/
theorem intersection_empty_annihilation {domainSize : Nat}
    (set : OrderedSet domainSize) :
    toList (intersection set (empty domainSize)) =
      toList (empty domainSize) := by
  apply same_iteration_of_same_membership
  intro key
  simp [intersection, empty]

/-- Removing every member of a set produces the empty canonical iteration. -/
theorem difference_self_empty {domainSize : Nat}
    (set : OrderedSet domainSize) :
    toList (difference set set) = toList (empty domainSize) := by
  apply same_iteration_of_same_membership
  intro key
  simp [difference, empty]

/-- Subtracting the empty set preserves the canonical ordered iteration. -/
theorem difference_empty_identity {domainSize : Nat}
    (set : OrderedSet domainSize) :
    toList (difference set (empty domainSize)) = toList set := by
  apply same_iteration_of_same_membership
  intro key
  cases member : set.contains key <;> simp [difference, empty, member]

/-- A singleton distinguishes difference from union by its ordered output. -/
theorem difference_union_mutation_witness :
    toList (difference
      (ofList [(⟨0, by decide⟩ : Fin 1)])
      (ofList [(⟨0, by decide⟩ : Fin 1)])) ≠
    toList (union
      (ofList [(⟨0, by decide⟩ : Fin 1)])
      (ofList [(⟨0, by decide⟩ : Fin 1)])) := by
  decide

end Xsh.Sets.Algebra
