import Xsh.Validation.RelPath

/-!
Path component queries read native path bytes lexically. Repeated separators,
trailing separators, and interior `.` components disappear; a leading `.` and
every `..` component remain as written. Root is represented separately from
name bytes so component comparisons cannot confuse it with a name.

This model covers component reading and component-sequence prefix/suffix
queries. It does not model filesystem access, normalization, name or parent
queries, RelPath validation, or Rust correspondence.
-/

namespace Xsh.Paths.Components

open Xsh.Validation.RelPath

abbrev PathBytes := Xsh.Validation.RelPath.PathBytes

/-- Root is distinct from every native-byte name. -/
inductive Component where
  | root
  | name (bytes : PathBytes)
  deriving DecidableEq, Repr

/-- A path is absolute exactly when its first native byte is `/`. -/
def isAbsolute (bytes : PathBytes) : Bool :=
  bytes.head? == some slashByte

/-- Collect component names in order while preserving each byte slice. -/
def collectNames : List PathBytes → Bool → List PathBytes
  | [], _ => []
  | part :: remaining, seenComponent =>
      if part.isEmpty then
        collectNames remaining seenComponent
      else if part == [dotByte] && seenComponent then
        collectNames remaining seenComponent
      else
        part :: collectNames remaining true

/-- Drop the empty fields produced by leading separators on an absolute path. -/
def pathParts (bytes : PathBytes) : List PathBytes :=
  let parts := bytes.splitOn slashByte
  if isAbsolute bytes then parts.dropWhile (fun part => part.isEmpty) else parts

/-- Read root and native-byte name components from a path. -/
def components (bytes : PathBytes) : List Component :=
  let absolute := isAbsolute bytes
  let names := (collectNames (pathParts bytes) absolute).map Component.name
  if absolute then Component.root :: names else names

/-- Compare a leading component run against a complete component sequence. -/
def listHasPrefix : List Component → List Component → Bool
  | [], _ => true
  | _ :: _, [] => false
  | wanted :: wantedRest, found :: foundRest =>
      if wanted = found then listHasPrefix wantedRest foundRest else false

/-- Compare a trailing component run against a complete component sequence. -/
def listHasSuffix (suffix full : List Component) : Bool :=
  listHasPrefix suffix (full.drop (full.length - suffix.length))

/-- `path` starts with `prefix` when all prefix components match. -/
def startsWith (path wanted : PathBytes) : Bool :=
  listHasPrefix (components wanted) (components path)

/-- `path` ends with `suffix` when all suffix components match. -/
def endsWith (path wanted : PathBytes) : Bool :=
  if isAbsolute wanted then
    if components path = components wanted then true else false
  else
    listHasSuffix (components wanted) (components path)

/-- Compare a byte prefix without component boundaries. -/
def rawStartsWith : PathBytes → PathBytes → Bool
  | [], _ => true
  | _ :: _, [] => false
  | wanted :: wantedRest, found :: foundRest =>
      if wanted = found then rawStartsWith wantedRest foundRest else false

private theorem listHasPrefix_self (values : List Component) :
    listHasPrefix values values = true := by
  induction values with
  | nil => rfl
  | cons value rest inductionHypothesis =>
      simp [listHasPrefix, inductionHypothesis]

theorem empty_path_has_no_components :
    components [] = [] := by
  decide

theorem absolute_root_is_one_component :
    components [slashByte] = [.root] := by
  decide

theorem repeated_and_trailing_separators_disappear :
    components (asciiBytes [47, 117, 115, 114, 47, 47, 108, 105, 98, 47]) =
      [.root, .name (asciiBytes [117, 115, 114]), .name (asciiBytes [108, 105, 98])] := by
  decide

theorem leading_dot_component_is_preserved :
    components (asciiBytes [46, 47, 97]) =
      [.name (asciiBytes [46]), .name (asciiBytes [97])] := by
  decide

theorem interior_dot_component_is_omitted :
    components (asciiBytes [97, 47, 46, 47, 98]) =
      [.name (asciiBytes [97]), .name (asciiBytes [98])] := by
  decide

theorem parent_component_is_preserved :
    components (asciiBytes [97, 47, 46, 46, 47, 98]) =
      [.name (asciiBytes [97]), .name (asciiBytes [46, 46]), .name (asciiBytes [98])] := by
  decide

theorem non_utf8_name_bytes_are_preserved :
    components [slashByte, UInt8.ofNat 255, slashByte, UInt8.ofNat 97] =
      [.root, .name [UInt8.ofNat 255], .name [UInt8.ofNat 97]] := by
  decide

theorem component_prefix_is_reflexive (path : PathBytes) :
    startsWith path path = true := by
  exact listHasPrefix_self (components path)

theorem complete_components_match_as_prefix :
    startsWith (asciiBytes [47, 117, 115, 114, 47, 108, 105, 98])
      (asciiBytes [47, 117, 115, 114]) = true := by
  decide

theorem component_suffix_is_reflexive (path : PathBytes) :
    endsWith path path = true := by
  cases absolute : isAbsolute path
  · simp only [endsWith, absolute, ↓reduceIte, listHasSuffix, Nat.sub_self, List.drop_zero]
    exact listHasPrefix_self (components path)
  · simp [endsWith, absolute]

theorem complete_component_matches_as_suffix :
    endsWith (asciiBytes [97, 47, 98, 46, 116, 120, 116])
      (asciiBytes [98, 46, 116, 120, 116]) = true := by
  decide

theorem absolute_suffix_matches_exactly_equal_components
    (path suffix : PathBytes) (absolute : isAbsolute suffix = true) :
    endsWith path suffix = true ↔ components path = components suffix := by
  simp [endsWith, absolute]

theorem root_prefix_matches_exactly_absolute_paths (path : PathBytes) :
    startsWith path [slashByte] = isAbsolute path := by
  unfold startsWith
  rw [absolute_root_is_one_component]
  have noRootPrefix (names : List PathBytes) :
      listHasPrefix [.root] (names.map Component.name) = false := by
    cases names <;> simp [listHasPrefix]
  cases h : isAbsolute path <;>
    simp [components, h, listHasPrefix, noRootPrefix]

theorem raw_byte_prefix_mutation_is_detected :
    rawStartsWith (asciiBytes [47, 117, 115])
      (asciiBytes [47, 117, 115, 114, 47, 108, 105, 98]) = true ∧
    startsWith (asciiBytes [47, 117, 115, 114, 47, 108, 105, 98])
      (asciiBytes [47, 117, 115]) = false := by
  decide

end Xsh.Paths.Components
