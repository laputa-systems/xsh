/-!
A finite map-presence model over ordinal keys. The outer option represents
whether a key exists; `EntryValue.null` is an ordinary present value.
This fragment does not model key ordering, validation, or XSH runtime behavior.
-/

namespace Xsh.Values.MapPresence

universe u

/-- A map value can be null or carry a payload of the map's value type. -/
inductive EntryValue (Payload : Type u) where
  | null
  | payload (value : Payload)
  deriving DecidableEq

/-- A finite map stores optional entries over ordinal keys. -/
structure FiniteMap (domainSize : Nat) (Payload : Type u) where
  entries : Fin domainSize → Option (EntryValue Payload)

/-- The result of `.get`: only an absent key is an error. -/
inductive GetResult (Payload : Type u) where
  | missing
  | found (value : EntryValue Payload)
  deriving DecidableEq

/-- Read the optional entry for one key. -/
def lookup {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    Option (EntryValue Payload) :=
  map.entries key

/-- Return an error for an absent key and success for every present value. -/
def get {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    GetResult Payload :=
  match lookup map key with
  | none => .missing
  | some value => .found value

/-- Test entry presence without inspecting the stored value. -/
def contains {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) : Bool :=
  match lookup map key with
  | none => false
  | some _ => true

/-- The map with no present keys. -/
def empty (domainSize : Nat) (Payload : Type u) :
    FiniteMap domainSize Payload :=
  ⟨fun _ => none⟩

/-- Store a value at one key, replacing any earlier value there. -/
def set {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize)
    (value : EntryValue Payload) : FiniteMap domainSize Payload :=
  ⟨fun candidate => if candidate = key then some value else lookup map candidate⟩

/-- Remove one key while preserving every other entry. -/
def remove {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    FiniteMap domainSize Payload :=
  ⟨fun candidate => if candidate = key then none else lookup map candidate⟩

/-- The faulty interpretation that treats a stored null as a missing key. -/
def nullAsAbsenceLookup {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    Option (EntryValue Payload) :=
  match lookup map key with
  | some .null => none
  | result => result

private theorem ext {domainSize : Nat} {Payload : Type u}
    {left right : FiniteMap domainSize Payload}
    (sameEntries : ∀ key, lookup left key = lookup right key) :
    left = right := by
  cases left with
  | mk leftEntries =>
    cases right with
    | mk rightEntries =>
      have same : leftEntries = rightEntries := by
        funext key
        exact sameEntries key
      cases same
      rfl

/-- A missing lookup is exactly the error case reported by `.get`. -/
theorem lookup_absent {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    lookup map key = none ↔ get map key = .missing := by
  cases found : lookup map key <;> simp [get, found]

/-- Setting null creates a present lookup entry rather than absence. -/
theorem lookup_present_null {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    lookup (set map key .null) key = some .null ∧
      contains (set map key .null) key = true := by
  simp [contains, set, lookup]

/-- `.get` succeeds when the selected entry is null. -/
theorem get_present_null {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    get (set map key .null) key = .found .null := by
  simp [get, set, lookup]

/-- `.get` reports a missing-key error exactly for an absent lookup. -/
theorem get_missing_error {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize)
    (absent : lookup map key = none) :
    get map key = .missing := by
  simp [get, absent]

/-- Presence is equivalent to having some entry, independently of its value. -/
theorem contains_iff_lookup_exists {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    contains map key = true ↔ ∃ value, lookup map key = some value := by
  cases found : lookup map key <;> simp [contains, found]

/-- Setting either value at a key makes that key present. -/
theorem contains_set_payload_independent {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize)
    (leftValue rightValue : EntryValue Payload) :
    contains (set map key leftValue) key =
      contains (set map key rightValue) key := by
  simp [contains, set, lookup]

/-- Setting a key replaces its previous entry with the selected value. -/
theorem set_updates_selected {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize)
    (value : EntryValue Payload) :
    lookup (set map key value) key = some value := by
  simp [set, lookup]

/-- Setting one key leaves a distinct key's lookup unchanged. -/
theorem set_preserves_other {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key other : Fin domainSize)
    (value : EntryValue Payload) (distinct : other ≠ key) :
    lookup (set map key value) other = lookup map other := by
  simp [set, lookup, distinct]

/-- Removing a key makes its lookup absent. -/
theorem remove_clears_selected {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize) :
    lookup (remove map key) key = none := by
  simp [remove, lookup]

/-- Removing one key leaves a distinct key's lookup unchanged. -/
theorem remove_preserves_other {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key other : Fin domainSize)
    (distinct : other ≠ key) :
    lookup (remove map key) other = lookup map other := by
  simp [remove, lookup, distinct]

/-- Removing a key that is already absent preserves the whole map. -/
theorem remove_absent_identity {domainSize : Nat} {Payload : Type u}
    (map : FiniteMap domainSize Payload) (key : Fin domainSize)
    (absent : lookup map key = none) :
    remove map key = map := by
  apply ext
  intro candidate
  by_cases sameKey : candidate = key
  · subst candidate
    simpa [remove, lookup] using absent.symm
  · simp [remove, lookup, sameKey]

private def nullWitnessKey : Fin 1 := ⟨0, by decide⟩

private def nullWitnessMap : FiniteMap 1 Bool :=
  set (empty 1 Bool) nullWitnessKey .null

/-- A concrete nonempty map distinguishes null-as-absence from real presence. -/
theorem null_as_absence_mutation_detected :
    lookup nullWitnessMap nullWitnessKey = some .null ∧
      contains nullWitnessMap nullWitnessKey = true ∧
      nullAsAbsenceLookup nullWitnessMap nullWitnessKey = none := by
  decide

end Xsh.Values.MapPresence
