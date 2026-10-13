import Init.Data.List.Nat.Pairwise

/-!
A finite ordered-set model over ordinal keys. Membership is extensional, while
iteration scans the key domain in increasing order. This fragment specifies
these operations independently of XSH's checker and runtime.
-/

namespace Xsh.Values.OrderedSet

/-- A set over the finite ordinal key domain `Fin domainSize`. -/
structure OrderedSet (domainSize : Nat) where
  contains : Fin domainSize → Bool

/-- Extensional equality for finite ordered sets. -/
private theorem ext {domainSize : Nat} {left right : OrderedSet domainSize}
    (sameMembership : ∀ key, left.contains key = right.contains key) :
    left = right := by
  cases left with
  | mk leftContains =>
    cases right with
    | mk rightContains =>
      have same : leftContains = rightContains := by
        funext key
        exact sameMembership key
      cases same
      rfl

/-- The empty set has no member in the finite key domain. -/
def empty (domainSize : Nat) : OrderedSet domainSize :=
  ⟨fun _ => false⟩

/-- Build a set by membership, collapsing any repeated input keys. -/
def ofList {domainSize : Nat} (keys : List (Fin domainSize)) :
    OrderedSet domainSize :=
  ⟨fun key => decide (key ∈ keys)⟩

/-- Add a key while preserving the set's extensional membership. -/
def insert {domainSize : Nat} (set : OrderedSet domainSize)
    (key : Fin domainSize) : OrderedSet domainSize :=
  ⟨fun candidate => if candidate = key then true else set.contains candidate⟩

/-- Remove a key while preserving the set's extensional membership. -/
def remove {domainSize : Nat} (set : OrderedSet domainSize)
    (key : Fin domainSize) : OrderedSet domainSize :=
  ⟨fun candidate => if candidate = key then false else set.contains candidate⟩

/-- Union holds a key when either operand holds it. -/
def union {domainSize : Nat} (left right : OrderedSet domainSize) :
    OrderedSet domainSize :=
  ⟨fun key => left.contains key || right.contains key⟩

/-- Intersection holds a key only when both operands hold it. -/
def intersection {domainSize : Nat} (left right : OrderedSet domainSize) :
    OrderedSet domainSize :=
  ⟨fun key => left.contains key && right.contains key⟩

/-- Difference holds a left key only when the right operand does not hold it. -/
def difference {domainSize : Nat} (left right : OrderedSet domainSize) :
    OrderedSet domainSize :=
  ⟨fun key => left.contains key && !right.contains key⟩

/-- Iterate held keys in the ordinal order, independently of construction order. -/
def toList {domainSize : Nat} (set : OrderedSet domainSize) :
    List (Fin domainSize) :=
  (List.finRange domainSize).filter set.contains

/-- Membership of a constructed set is exactly input-list membership. -/
theorem ofList_membership {domainSize : Nat} (keys : List (Fin domainSize))
    (key : Fin domainSize) :
    (ofList keys).contains key = true ↔ key ∈ keys := by
  simp [ofList]

/-- Canonical iteration never repeats a key. -/
theorem toList_nodup {domainSize : Nat} (set : OrderedSet domainSize) :
    (toList set).Nodup := by
  exact (List.pairwise_lt_finRange domainSize).filter set.contains |>.imp Fin.ne_of_lt

/-- Every earlier canonical iteration key is smaller in ordinal order. -/
theorem toList_key_ordered {domainSize : Nat} (set : OrderedSet domainSize) :
    (toList set).Pairwise (· < ·) := by
  exact (List.pairwise_lt_finRange domainSize).filter set.contains

/-- Inserting a key already present preserves the set. -/
theorem insert_existing_identity {domainSize : Nat}
    (set : OrderedSet domainSize) (key : Fin domainSize)
    (present : set.contains key = true) :
    insert set key = set := by
  apply ext
  intro candidate
  by_cases sameKey : candidate = key
  · subst candidate
    simp [insert, present]
  · simp [insert, sameKey]

/-- Removing a key absent from the set preserves the set. -/
theorem remove_absent_identity {domainSize : Nat}
    (set : OrderedSet domainSize) (key : Fin domainSize)
    (absent : set.contains key = false) :
    remove set key = set := by
  apply ext
  intro candidate
  by_cases sameKey : candidate = key
  · subst candidate
    simp [remove, absent]
  · simp [remove, sameKey]

/-- Union membership is the disjunction of operand membership. -/
theorem union_membership_iff {domainSize : Nat}
    (left right : OrderedSet domainSize) (key : Fin domainSize) :
    (union left right).contains key = true ↔
      left.contains key = true ∨ right.contains key = true := by
  cases leftHas : left.contains key <;> cases rightHas : right.contains key <;>
    simp [union, leftHas, rightHas]

/-- Intersection membership is the conjunction of operand membership. -/
theorem intersection_membership_iff {domainSize : Nat}
    (left right : OrderedSet domainSize) (key : Fin domainSize) :
    (intersection left right).contains key = true ↔
      left.contains key = true ∧ right.contains key = true := by
  cases leftHas : left.contains key <;> cases rightHas : right.contains key <;>
    simp [intersection, leftHas, rightHas]

/-- Difference membership is left membership without right membership. -/
theorem difference_membership_iff {domainSize : Nat}
    (left right : OrderedSet domainSize) (key : Fin domainSize) :
    (difference left right).contains key = true ↔
      left.contains key = true ∧ right.contains key = false := by
  cases leftHas : left.contains key <;> cases rightHas : right.contains key <;>
    simp [difference, leftHas, rightHas]

/-- Permuting construction input preserves the canonical iteration. -/
theorem ofList_permutation_preserves_iteration {domainSize : Nat}
    {left right : List (Fin domainSize)} (sameKeys : left.Perm right) :
    toList (ofList left) = toList (ofList right) := by
  have sameSet : ofList left = ofList right := by
    apply ext
    intro key
    simp [ofList, sameKeys.mem_iff]
  rw [sameSet]

/-- Adding another occurrence of a key does not change the constructed set. -/
theorem ofList_duplicate_append_preserves_iteration {domainSize : Nat}
    (keys : List (Fin domainSize)) (key : Fin domainSize) :
    toList (ofList (keys ++ [key, key])) =
      toList (ofList (keys ++ [key])) := by
  have sameSet : ofList (keys ++ [key, key]) = ofList (keys ++ [key]) := by
    apply ext
    intro candidate
    simp [ofList]
  rw [sameSet]

/-- This history-order mutation keeps insertion order instead of scanning keys. -/
def insertionOrderIteration {domainSize : Nat} (history : List (Fin domainSize))
    (set : OrderedSet domainSize) : List (Fin domainSize) :=
  history.filter set.contains

def reversedKeyHistory : List (Fin 3) :=
  [⟨2, by decide⟩, ⟨0, by decide⟩]

/-- A nonempty reverse-ordered history distinguishes the faulty iterator from
the canonical key-ordered iteration. -/
theorem insertion_order_mutation_detected :
    insertionOrderIteration reversedKeyHistory (ofList reversedKeyHistory) =
        reversedKeyHistory ∧
      toList (ofList reversedKeyHistory) = [⟨0, by decide⟩, ⟨2, by decide⟩] ∧
      insertionOrderIteration reversedKeyHistory (ofList reversedKeyHistory) ≠
        toList (ofList reversedKeyHistory) := by
  decide

end Xsh.Values.OrderedSet
