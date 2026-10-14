import Xsh.Paths.Components

/-!
This fragment models lexical path fields over native path bytes with an
explicit separator. Its public examples specialize to `/`, the separator of
the POSIX byte spelling represented here. The byte list is already the native
spelling; the model performs no text decoding. It covers final-name,
POSIX-basename and dirname, lexical-parent, and extension queries only. It
does not resolve paths through the filesystem, normalize components, convert
invalid UTF-8 lossily, model `with_ext`, or establish Rust correspondence.
-/

namespace Xsh.Paths.Fields

open Xsh.Validation.RelPath

/-- Trim a run of trailing separators without interpreting path components. -/
def dropTrailingSeparators (separator : UInt8) (bytes : PathBytes) : PathBytes :=
  (bytes.reverse.dropWhile (fun byte => byte == separator)).reverse

/-- Read the final nonempty component as bytes, leaving an all-separator root
without a name. -/
def name (separator : UInt8) (bytes : PathBytes) : PathBytes :=
  if bytes.all (fun byte => byte == separator) then
    []
  else
    let trimmed := dropTrailingSeparators separator bytes
    (trimmed.reverse.takeWhile (fun byte => byte != separator)).reverse

/-- POSIX basename keeps one separator for every nonempty root spelling. -/
def posixBasename (separator : UInt8) (bytes : PathBytes) : PathBytes :=
  if !bytes.isEmpty && bytes.all (fun byte => byte == separator) then
    [separator]
  else
    name separator bytes

/-- Mutated basename that substitutes the native `name` field for POSIX
basename. -/
def basenameUsingNameMutation (separator : UInt8) (bytes : PathBytes) : PathBytes :=
  name separator bytes

/-- Return the lexical directory prefix, using POSIX dirname's `.` and `/`
results for a bare name and a root. -/
def posixDirname (separator : UInt8) (bytes : PathBytes) : PathBytes :=
  if bytes.isEmpty then
    asciiBytes [46]
  else if bytes.all (fun byte => byte == separator) then
    [separator]
  else
    let trimmed := dropTrailingSeparators separator bytes
    let reversed := trimmed.reverse
    let finalName := reversed.takeWhile (fun byte => byte != separator)
    let reversedPrefix := reversed.drop finalName.length
    if reversedPrefix.isEmpty then
      asciiBytes [46]
    else
      let parentPrefix := reversedPrefix.reverse
      let parent := dropTrailingSeparators separator parentPrefix
      if parent.isEmpty then [separator] else parent

/-- Read the lexical parent with `.` for a path without a directory prefix. -/
def parent (separator : UInt8) (bytes : PathBytes) : PathBytes :=
  posixDirname separator bytes

/-- Scan a final component from left to right and retain its last dot after
the leading byte. -/
private def scanLastDot? : PathBytes → Nat → Option Nat
  | [], _ => none
  | byte :: remaining, index =>
      match scanLastDot? remaining (index + 1) with
      | some last => some last
      | none => if byte == dotByte then some index else none

/-- Find a dot that is not the first byte of a name. -/
private def lastNonleadingDot? (bytes : PathBytes) : Option Nat :=
  match bytes with
  | [] => none
  | _ :: remaining => scanLastDot? remaining 1

/-- Return bytes following the final nonleading dot in the final component. -/
def extension? (separator : UInt8) (bytes : PathBytes) : Option PathBytes :=
  let finalName := name separator bytes
  (lastNonleadingDot? finalName).map fun index => finalName.drop (index + 1)

/-- A trailing separator does not change the final name bytes. -/
theorem name_reads_final_component_after_trailing_separators :
    name slashByte (asciiBytes [47, 117, 115, 114, 47, 108, 105, 98, 47, 47]) =
      asciiBytes [108, 105, 98] := by
  decide

/-- The root has no `name`, while POSIX basename returns `/`; replacing the
POSIX operation with `name` therefore changes the root result. -/
theorem root_name_posix_basename_and_mutation_witness :
    name slashByte [slashByte] = [] ∧
      posixBasename slashByte [slashByte] = [slashByte] ∧
      basenameUsingNameMutation slashByte [slashByte] ≠
        posixBasename slashByte [slashByte] := by
  decide

/-- POSIX basename ignores a trailing separator after a named component. -/
theorem posix_basename_ignores_trailing_separators :
    posixBasename slashByte (asciiBytes [47, 117, 115, 114, 47, 108, 105, 98, 47]) =
      asciiBytes [108, 105, 98] := by
  decide

/-- POSIX dirname ignores trailing separators before selecting the parent. -/
theorem posix_dirname_ignores_trailing_separators :
    posixDirname slashByte (asciiBytes [47, 117, 115, 114, 47, 108, 105, 98, 47, 47]) =
      asciiBytes [47, 117, 115, 114] := by
  decide

/-- POSIX dirname maps a bare name to the current-directory spelling. -/
theorem posix_dirname_of_bare_name_is_dot :
    posixDirname slashByte (asciiBytes [115, 99, 114, 105, 112, 116]) = asciiBytes [46] := by
  decide

/-- POSIX dirname keeps the root spelling when given a root. -/
theorem posix_dirname_of_root_is_root :
    posixDirname slashByte [slashByte] = [slashByte] := by
  decide

/-- A lexical parent preserves a written `..` component instead of
normalizing the path. -/
theorem parent_preserves_written_components :
    parent slashByte (asciiBytes [97, 47, 46, 46, 47, 98]) =
      asciiBytes [97, 47, 46, 46] := by
  decide

/-- A single relative component has the current-directory lexical parent. -/
theorem parent_of_single_relative_name_is_dot :
    parent slashByte (asciiBytes [102, 105, 108, 101]) = asciiBytes [46] := by
  decide

/-- The root's lexical parent remains the root. -/
theorem parent_of_root_is_root :
    parent slashByte [slashByte] = [slashByte] := by
  decide

/-- A leading dot alone does not introduce an extension. -/
theorem leading_dot_name_has_no_extension :
    extension? slashByte (asciiBytes [47, 46, 101, 110, 118]) = none := by
  decide

/-- A final dot introduces a present but empty extension. -/
theorem trailing_dot_has_empty_extension :
    extension? slashByte (asciiBytes [102, 105, 108, 101, 46]) = some [] := by
  decide

/-- The extension follows the final nonleading dot in the final component. -/
theorem extension_uses_final_nonleading_dot :
    extension? slashByte (asciiBytes [47, 97, 114, 99, 104, 105, 118, 101, 46,
      116, 97, 114, 46, 103, 122, 47]) = some (asciiBytes [103, 122]) := by
  decide

end Xsh.Paths.Fields
