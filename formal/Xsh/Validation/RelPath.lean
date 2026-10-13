/-!
An independent model of the byte-lexical `RelPath` boundary. Paths are lists
of bytes, components are separated by `/`, and only the depth of `..`
components matters. Validation wraps the exact input bytes in a subtype.

The laws and witnesses here do not establish checker/runtime correspondence,
Rust conformance, filesystem confinement through symlinks, or full validated
type closure.
-/

namespace Xsh.Validation.RelPath

abbrev PathBytes := List UInt8

def slashByte : UInt8 := UInt8.ofNat 47
def dotByte : UInt8 := UInt8.ofNat 46

def advanceComponent (depth : Nat) (component : List UInt8) : Option Nat :=
  if component = [] || component = [dotByte] then
    some depth
  else if component = [dotByte, dotByte] then
    if depth = 0 then none else some (depth - 1)
  else
    some (depth + 1)

def scanComponents : List (List UInt8) → Nat → Option Nat
  | [], depth => some depth
  | component :: remaining, depth =>
      match advanceComponent depth component with
      | none => none
      | some nextDepth => scanComponents remaining nextDepth

def scanPath (bytes : PathBytes) : Option Nat :=
  if bytes.isEmpty || bytes.head? = some slashByte then
    none
  else
    scanComponents (bytes.splitOn slashByte) 0

def isRelPath (bytes : PathBytes) : Prop := scanPath bytes ≠ none

instance (bytes : PathBytes) : Decidable (isRelPath bytes) := by
  unfold isRelPath
  infer_instance

abbrev RelPathValue := { bytes : PathBytes // isRelPath bytes }

def introduce (bytes : PathBytes) (accepted : isRelPath bytes) : RelPathValue :=
  ⟨bytes, accepted⟩

def erase (value : RelPathValue) : PathBytes := value.val

def require (bytes : PathBytes) : Option RelPathValue :=
  if accepted : isRelPath bytes then
    some (introduce bytes accepted)
  else
    none

theorem empty_component_preserves_depth (depth : Nat) :
    advanceComponent depth [] = some depth := by
  simp [advanceComponent]

theorem dot_component_preserves_depth (depth : Nat) :
    advanceComponent depth [dotByte] = some depth := by
  simp [advanceComponent]

theorem parent_at_start_is_rejected :
    advanceComponent 0 [dotByte, dotByte] = none := by
  simp [advanceComponent]

theorem parent_with_depth_decreases (depth : Nat) :
    advanceComponent (depth + 1) [dotByte, dotByte] = some depth := by
  simp [advanceComponent]

theorem empty_path_is_rejected : ¬ isRelPath [] := by
  decide

theorem absolute_path_is_rejected (tail : PathBytes) :
    ¬ isRelPath (slashByte :: tail) := by
  simp [isRelPath, scanPath]

def asciiBytes (values : List Nat) : PathBytes := values.map UInt8.ofNat

theorem dot_path_is_accepted : isRelPath (asciiBytes [46]) := by
  decide

theorem empty_and_dot_components_are_accepted :
    isRelPath (asciiBytes [97, 47, 46, 47, 47, 98, 47]) := by
  decide

theorem cancelling_parent_is_accepted :
    isRelPath (asciiBytes [97, 47, 46, 46, 47, 98]) := by
  decide

theorem escaping_parent_is_rejected :
    ¬ isRelPath (asciiBytes [97, 47, 46, 46, 47, 46, 46]) := by
  decide

theorem non_utf8_name_is_accepted :
    isRelPath [UInt8.ofNat 97, slashByte, UInt8.ofNat 255, slashByte,
      UInt8.ofNat 98] := by
  decide

theorem introduce_erases_exactly (bytes : PathBytes) (accepted : isRelPath bytes) :
    erase (introduce bytes accepted) = bytes := rfl

theorem require_accepts (bytes : PathBytes) (accepted : isRelPath bytes) :
    require bytes = some (introduce bytes accepted) := by
  unfold require
  rw [dite_eq_left accepted]

theorem require_rejects (bytes : PathBytes) (rejected : ¬ isRelPath bytes) :
    require bytes = none := by
  unfold require
  rw [dite_eq_right rejected]

theorem require_rejected_iff_invalid (bytes : PathBytes) :
    require bytes = none ↔ ¬ isRelPath bytes := by
  constructor
  · intro rejected accepted
    rw [require_accepts bytes accepted] at rejected
    cases rejected
  · exact require_rejects bytes

theorem require_success_preserves_bytes (bytes : PathBytes) (value : RelPathValue)
    (success : require bytes = some value) : erase value = bytes := by
  by_cases accepted : isRelPath bytes
  · rw [require_accepts bytes accepted] at success
    cases success
    rfl
  · rw [require_rejects bytes accepted] at success
    cases success

theorem require_success_establishes_boundary (bytes : PathBytes) (value : RelPathValue)
    (success : require bytes = some value) : isRelPath bytes := by
  rw [← require_success_preserves_bytes bytes value success]
  exact value.property

def advanceComponentWithMutation (depth : Nat) (component : List UInt8) : Option Nat :=
  if component = [dotByte, dotByte] then
    some (depth + 1)
  else
    advanceComponent depth component

def scanComponentsWith (advance : Nat → List UInt8 → Option Nat) :
    List (List UInt8) → Nat → Option Nat
  | [], depth => some depth
  | component :: remaining, depth =>
      match advance depth component with
      | none => none
      | some nextDepth => scanComponentsWith advance remaining nextDepth

def scanPathWithMutation (bytes : PathBytes) : Option Nat :=
  if bytes.isEmpty || bytes.head? = some slashByte then
    none
  else
    scanComponentsWith advanceComponentWithMutation (bytes.splitOn slashByte) 0

def mutationAccepts (bytes : PathBytes) : Prop := scanPathWithMutation bytes ≠ none

instance (bytes : PathBytes) : Decidable (mutationAccepts bytes) := by
  unfold mutationAccepts
  infer_instance

theorem treating_parent_as_name_is_detected :
    ¬ isRelPath (asciiBytes [46, 46]) ∧ mutationAccepts (asciiBytes [46, 46]) := by
  decide

end Xsh.Validation.RelPath
