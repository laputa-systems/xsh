import Xsh.Validation.RelPath

/-!
Lexical normalization scans native-byte path components separated by `/`.
The absolute-root bit stays separate from the component list: `..` cancels an
ordinary component, remains at the front of a relative path, and is ignored
above an absolute root. Rendering joins the resulting byte components without
decoding or changing their bytes.

Inputs here are arbitrary byte lists; the model does not establish the runtime
`Path` NUL-byte admission check. Its repeated-normalization theorem below is a
concrete example, not a general idempotence law.

This model excludes working-directory resolution, filesystem lookup,
symlink-aware resolution, path construction and interpolation, and field
queries. It proves properties of this independent byte-list model, not Rust
correspondence or filesystem confinement.
-/

namespace Xsh.Paths.Normalize

open Xsh.Validation.RelPath

abbrev PathBytes := Xsh.Validation.RelPath.PathBytes

/-- A path is absolute exactly when its first native byte is `/`. -/
def isAbsolute (bytes : PathBytes) : Bool :=
  bytes.head? == some slashByte

/-- The reversed component stack changes only when a path component requires it. -/
def advanceComponent (absolute : Bool) (stack : List PathBytes) (component : PathBytes) : List PathBytes :=
  if component.isEmpty || component == [dotByte] then
    stack
  else if component == [dotByte, dotByte] then
    match stack with
    | top :: remaining =>
        if !top.isEmpty && top != [dotByte] && top != [dotByte, dotByte] then
          remaining
        else if absolute then
          stack
        else
          component :: stack
    | [] =>
        if absolute then [] else [component]
  else
    component :: stack

/-- Normalize components while retaining the exact bytes of every name. -/
def normalizeComponents (absolute : Bool) (components : List PathBytes) : List PathBytes :=
  (components.foldl (advanceComponent absolute) []).reverse

/-- A normalized path keeps its root premise separate from its names. -/
structure NormalizedPath where
  absolute : Bool
  components : List PathBytes
  deriving DecidableEq, Repr

/-- Convert byte components to their normalized path representation. -/
def normalizeModel (bytes : PathBytes) : NormalizedPath :=
  let absolute := isAbsolute bytes
  { absolute, components := normalizeComponents absolute (bytes.splitOn slashByte) }

/-- Join components with `/`, preserving each component's native bytes. -/
def joinComponents (components : List PathBytes) : PathBytes :=
  match components with
  | [] => []
  | first :: remaining =>
      remaining.foldl (fun joined component => joined ++ [slashByte] ++ component) first

/-- Render the normalized model, using `.` for an empty relative path. -/
def render (path : NormalizedPath) : PathBytes :=
  let body := joinComponents path.components
  if path.absolute then
    slashByte :: body
  else if path.components.isEmpty then
    [dotByte]
  else
    body

/-- Normalize a native-byte path lexically. -/
def normalizePath (bytes : PathBytes) : PathBytes :=
  render (normalizeModel bytes)

theorem absolute_root_is_retained :
    normalizePath [slashByte] = [slashByte] := by
  decide

theorem empty_relative_path_normalizes_to_dot :
    normalizePath [] = [dotByte] := by
  decide

theorem empty_and_dot_components_are_removed :
    normalizePath (asciiBytes [97, 47, 47, 46, 47, 98, 47]) =
      asciiBytes [97, 47, 98] := by
  decide

theorem ordinary_component_before_parent_is_cancelled :
    normalizePath (asciiBytes [97, 47, 98, 47, 46, 46, 47, 99]) =
      asciiBytes [97, 47, 99] := by
  decide

theorem leading_and_consecutive_relative_parents_are_retained :
    normalizePath (asciiBytes [97, 47, 46, 46, 47, 46, 46, 47, 46, 46, 47, 98]) =
      asciiBytes [46, 46, 47, 46, 46, 47, 98] := by
  decide

theorem absolute_parents_clamp_at_root :
    normalizePath (asciiBytes [47, 97, 47, 46, 46, 47, 46, 46, 47, 46, 46, 47, 98]) =
      asciiBytes [47, 98] := by
  decide

theorem absolute_normalization_renders_root (path : PathBytes)
    (absolute : isAbsolute path = true) :
    isAbsolute (normalizePath path) = true := by
  simp only [normalizePath, normalizeModel]
  rw [absolute]
  simp [render, isAbsolute]

theorem repeated_normalization_matches_single_result_for_example :
    normalizePath
        (normalizePath (asciiBytes [97, 47, 46, 47, 98, 47, 46, 46, 47, 99])) =
      normalizePath (asciiBytes [97, 47, 46, 47, 98, 47, 46, 46, 47, 99]) := by
  decide

theorem normalized_components_remove_empty_and_dot_for_example :
    ∀ component ∈
        normalizeComponents false
          [[], [dotByte], asciiBytes [97], [], [dotByte], asciiBytes [98], []],
      component ≠ [] ∧ component ≠ [dotByte] := by
  decide

theorem ordinary_component_step_preserves_bytes
    (absolute : Bool) (stack : List PathBytes) (component : PathBytes)
    (notEmpty : component ≠ []) (notDot : component ≠ [dotByte])
    (notParent : component ≠ [dotByte, dotByte]) :
    advanceComponent absolute stack component = component :: stack := by
  simp [advanceComponent, notEmpty, notDot, notParent]

private def advanceComponentDropsLeadingRelativeParent
    (absolute : Bool) (stack : List PathBytes) (component : PathBytes) : List PathBytes :=
  if !absolute && stack.isEmpty && component == [dotByte, dotByte] then
    stack
  else
    advanceComponent absolute stack component

/-- The faulty variant drops a leading relative `..` instead of retaining it. -/
private def normalizePathDropsLeadingRelativeParent (bytes : PathBytes) : PathBytes :=
  let absolute := isAbsolute bytes
  render
    { absolute
      components :=
        (bytes.splitOn slashByte).foldl
          (advanceComponentDropsLeadingRelativeParent absolute) [] |>.reverse }

theorem relative_parent_stack_mutation_witness :
    normalizePath (asciiBytes [46, 46, 47, 97]) = asciiBytes [46, 46, 47, 97] ∧
      normalizePathDropsLeadingRelativeParent (asciiBytes [46, 46, 47, 97]) =
        asciiBytes [97] := by
  decide

private def advanceComponentWithoutAbsoluteClamp
    (absolute : Bool) (stack : List PathBytes) (component : PathBytes) : List PathBytes :=
  if component == [dotByte, dotByte] then
    match stack with
    | top :: remaining =>
        if !top.isEmpty && top != [dotByte] && top != [dotByte, dotByte] then
          remaining
        else
          component :: stack
    | [] => [component]
  else
    advanceComponent absolute stack component

/-- The faulty variant retains `..` above an absolute root. -/
private def normalizePathWithoutAbsoluteClamp (bytes : PathBytes) : PathBytes :=
  let absolute := isAbsolute bytes
  render
    { absolute
      components :=
        (bytes.splitOn slashByte).foldl
          (advanceComponentWithoutAbsoluteClamp absolute) [] |>.reverse }

theorem absolute_parent_stack_mutation_witness :
    normalizePath (asciiBytes [47, 46, 46, 47, 97]) = asciiBytes [47, 97] ∧
      normalizePathWithoutAbsoluteClamp (asciiBytes [47, 46, 46, 47, 97]) =
        asciiBytes [47, 46, 46, 47, 97] := by
  decide

end Xsh.Paths.Normalize
