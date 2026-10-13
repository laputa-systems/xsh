/-!
This fragment expands already-classified command words into argv strings.
An ordinary word contributes one item, and a splice contributes its members
in order. It does not model parsing, interpolation, path conversion, globbing,
NUL validation, process creation, or host behavior.
-/

namespace Xsh.Semantics.Argv

/-- The formal text type used for one XSH `Str` argv item. -/
abbrev Str := String

/-- A command word contributes one string; a splice contributes each list item. -/
inductive CommandWord where
  | word (value : Str)
  | splice (values : List Str)
  deriving DecidableEq, Repr

/-- Expand one classified word without splitting or otherwise rewriting text. -/
def expandWord : CommandWord → List Str
  | .word value => [value]
  | .splice values => values

/-- Flatten command words from left to right, preserving each word's expansion. -/
def expandArgv (words : List CommandWord) : List Str :=
  words.flatMap expandWord

theorem ordinary_word_is_one_argument (value : Str) :
    expandWord (.word value) = [value] := rfl

theorem word_with_embedded_space_remains_one_argument :
    expandArgv [.word "quarterly report.pdf"] = ["quarterly report.pdf"] := rfl

theorem wildcard_word_remains_literal :
    expandArgv [.word "*.log"] = ["*.log"] := rfl

theorem splice_preserves_items_and_order (values : List Str) :
    expandWord (.splice values) = values := rfl

theorem splice_items_keep_boundaries_in_source_order :
    expandArgv [.word "printf", .splice ["-l", "-a"], .word "*.log"] =
      ["printf", "-l", "-a", "*.log"] := rfl

theorem empty_splice_is_identity (before after : List CommandWord) :
    expandArgv (before ++ [.splice []] ++ after) =
      expandArgv (before ++ after) := by
  simp [expandArgv, expandWord, List.flatMap_append]

theorem expand_argv_append (before after : List CommandWord) :
    expandArgv (before ++ after) = expandArgv before ++ expandArgv after := by
  simp [expandArgv, List.flatMap_append]

theorem mixed_words_preserve_source_order :
    expandArgv
      [.word "run", .word "quarterly report.pdf", .splice ["--force", "--quiet"],
        .word "*.log"] =
      ["run", "quarterly report.pdf", "--force", "--quiet", "*.log"] := rfl

/-- Split character text at ASCII spaces and omit empty pieces. -/
def splitOnAsciiSpaces : List Char → List Char → List (List Char)
  | [], current => if current.isEmpty then [] else [current.reverse]
  | char :: rest, current =>
      if char = ' ' then
        if current.isEmpty then splitOnAsciiSpaces rest []
        else current.reverse :: splitOnAsciiSpaces rest []
      else
        splitOnAsciiSpaces rest (char :: current)

/-- A faulty expansion that splits each word at ASCII spaces before argv insertion. -/
def asciiSpaceSplitMutation (words : List CommandWord) : List Str :=
  (expandArgv words).flatMap (fun value =>
    (splitOnAsciiSpaces value.toList []).map String.ofList)

theorem whitespace_split_mutation_changes_argv :
    expandArgv [.word "quarterly report.pdf"] ≠
      asciiSpaceSplitMutation [.word "quarterly report.pdf"] := by
  decide

end Xsh.Semantics.Argv
