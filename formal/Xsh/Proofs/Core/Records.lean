import Xsh.Proofs.Core.Slices

namespace Xsh.Proofs.Core.Records

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Semantics.CoreControl
open Xsh.Proofs.Core.Control

section Laws

variable {Declaration Identity Source : Type}

theorem setField_same (label : String) (value : Value Declaration Identity Source) :
    ∀ fields, (setField label value fields).lookup label = some value
  | [] => by simp [setField, List.lookup]
  | (other, held) :: rest => by
    unfold setField
    split
    · rename_i same; subst same; simp [List.lookup]
    · rename_i different
      have : (label == other) = false := by simpa [beq_iff_eq] using Ne.symm different
      simp only [List.lookup, this]
      exact setField_same label value rest

theorem setField_other (label other : String) (value : Value Declaration Identity Source) (different : other ≠ label) :
    ∀ fields, (setField label value fields).lookup other = fields.lookup other
  | [] => by
    have : (other == label) = false := by simpa [beq_iff_eq] using different
    simp [setField, List.lookup, this]
  | (first, held) :: rest => by
    unfold setField
    split
    · rename_i same; subst same
      have : (other == first) = false := by simpa [beq_iff_eq] using different
      simp [List.lookup, this]
    · simp only [List.lookup]
      split
      · rfl
      · exact setField_other label other value different rest

/-- A path entry replaces the field it names and leaves every other field of
that record as it was; it never adds a field. -/
theorem updatePath_single (value : Value Declaration Identity Source) (label : String)
    (fields updated : List (String × Value Declaration Identity Source))
    (done : updatePath value [label] fields = some updated) :
    fields.lookup label ≠ none ∧ updated.lookup label = some value ∧
      ∀ other, other ≠ label → updated.lookup other = fields.lookup other := by
  simp only [updatePath, Option.map_eq_some_iff] at done
  obtain ⟨_, present, rfl⟩ := done
  exact ⟨by simp [present], setField_same label value fields, fun other different =>
    setField_other label other value different fields⟩

/-- The `brace_literals` fragment root (SPEC 6.4): a brace literal with a
computed key is a map and otherwise a record; every entry is evaluated once
in source order, a computed key before its value, and a shorthand field reads
its name; a later field or spread replaces an earlier field of that label and
leaves the others; and a path entry replaces the existing field it names and
nothing else. -/
theorem brace_literals [DecidableEq Declaration] {world : World Declaration Identity Source}
    (context : Context Declaration Identity Source) (evaluate : Frame world → Xsh.Syntax.Expr → Valued world) :
    (∀ entries : List (BraceEntry Declaration Identity Source),
      entries.any isKey = true → braceValue entries = (mapOf [] entries).map .map) ∧
    (∀ entries : List (BraceEntry Declaration Identity Source),
      entries.any isKey = false → braceValue entries = (recordOf [] entries).map .record) ∧
    (∀ (frame : Frame world) key operand rest,
      threadEntries context evaluate frame (.computed key operand :: rest) =
        (evaluate frame key).bind fun computed after =>
          (evaluate after operand).bind fun value middle =>
            (threadEntries context evaluate middle rest).bind fun entries last =>
              .value (.key computed value :: entries) last) ∧
    (∀ (frame : Frame world) name rest,
      threadEntries context evaluate frame (.shorthand name :: rest) =
        (evaluate frame (.ident name)).bind fun value after =>
          (threadEntries context evaluate after rest).bind fun entries last =>
            .value (.field (context.spelling name) value :: entries) last) ∧
    (∀ (label : String) (value : Value Declaration Identity Source) fields,
      (setField label value fields).lookup label = some value ∧
        ∀ other, other ≠ label → (setField label value fields).lookup other = fields.lookup other) ∧
    (∀ (value : Value Declaration Identity Source) label fields updated,
      updatePath value [label] fields = some updated →
        fields.lookup label ≠ none ∧ updated.lookup label = some value ∧
          ∀ other, other ≠ label → updated.lookup other = fields.lookup other) := by
  refine ⟨fun entries keyed => by simp [braceValue, keyed], fun entries keyed => by simp [braceValue, keyed],
    fun _ _ _ _ => rfl, fun _ _ _ => rfl,
    fun label value fields => ⟨setField_same label value fields, fun other different =>
      setField_other label other value different fields⟩, updatePath_single⟩

end Laws

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.MatchValues.Witness
open Xsh.Proofs.Core.Slices.Witness

def labelA : Xsh.Syntax.RawName := Xsh.Proofs.Core.EvalStatements.Witness.aName
def labelB : Xsh.Syntax.RawName := Xsh.Proofs.Core.EvalStatements.Witness.bName

def letName (target : Xsh.Syntax.RawName) (value : Xsh.Syntax.Expr) : Xsh.Syntax.Statement :=
  .binding .letBinding (.name target) none value

def intOf : Value Unit Unit Unit → Option Int
  | .scalar (.integer value) => some value.val
  | _ => none

def fieldInt (labels : List String) : Value Unit Unit Unit → Option Int
  | value => match labels with
    | [] => intOf value
    | label :: rest => match value with
      | .record fields => (fields.lookup label).bind (fieldInt rest)
      | _ => none

def keyInt (key : String) : Value Unit Unit Unit → Option Int
  | .map entries => (entries.lookup (.string key)).bind intOf
  | _ => none

def defaults : Xsh.Syntax.Statement := letName iName (.record [.named labelA (int "1"), .named labelB (int "2")])
def overrides : Xsh.Syntax.Statement := letName jName (.record [.named labelA (int "7")])

def result (body : List Xsh.Syntax.Statement) : Option (Value Unit Unit Unit) := readValue (run body)

/-- As the verification binary prints `{"jobs":4,"verbose":true}` for
`{...defaults, ...overrides}` and `9` for `{...defaults, jobs: 9}.jobs`: a
later spread or field replaces an earlier field and keeps the others. -/
theorem spreads_replace :
    (result [defaults, overrides, letValue (.record [.spread (ident iName), .spread (ident jName)])]).bind
      (fieldInt ["a"]) = some 7 ∧
    (result [defaults, overrides, letValue (.record [.spread (ident iName), .spread (ident jName)])]).bind
      (fieldInt ["b"]) = some 2 ∧
    (result [defaults, letValue (.record [.spread (ident iName), .named labelA (int "8")])]).bind
      (fieldInt ["a"]) = some 8 := by
  decide +kernel

/-- `{name}` is `{name: name}`. -/
theorem shorthand_reads_name :
    (result [letName nName (int "7"), letValue (.record [.shorthand nName])]).bind (fieldInt ["m"]) = some 7 := by
  decide +kernel

def nested : Xsh.Syntax.Statement :=
  letName iName (.record [.named labelA (.record [.named labelB (int "1")]), .named nName (int "2")])

/-- As the verification binary prints `{"build":{"flags":{"debug":true},"jobs":8},"mode":"x"}`
for a functional update: a path entry replaces the nested field and keeps the
rest. -/
theorem path_updates :
    (result [nested, letValue (.record [.spread (ident iName), .path [labelA, labelB] (int "8")])]).bind
      (fieldInt ["a", "b"]) = some 8 ∧
    (result [nested, letValue (.record [.spread (ident iName), .path [labelA, labelB] (int "8")])]).bind
      (fieldInt ["m"]) = some 2 := by
  decide +kernel

def rs : Xsh.Syntax.Expr := .literal (.string "rs")

/-- As the verification binary prints `{"other":2,"rs":1}` and `{"rs":5}`: a
computed key makes a map whose constant labels are `Str` keys, and a later
entry replaces an earlier one. -/
theorem computed_keys_make_maps :
    (result [letValue (.record [.computed rs (int "1"), .named labelB (int "2")])]).bind (keyInt "rs") = some 1 ∧
    (result [letValue (.record [.computed rs (int "1"), .named labelB (int "2")])]).bind (keyInt "b") = some 2 ∧
    (result [letValue (.record [.computed rs (int "1"), .computed rs (int "7")])]).bind (keyInt "rs") = some 7 := by
  decide +kernel

def outOfRange : Xsh.Syntax.Expr := .index (.list []) (int "0") false

/-- A computed key is evaluated before its value: the key's division by zero
is the failure, not the value's index out of range. -/
theorem key_before_value :
    runtimeMessage (run [letValue (.record [.computed divisionByZero outOfRange])]) = some "division-by-zero" := by
  decide +kernel

/-- Evaluating the value first fails with `index-out-of-range` instead. -/
theorem value_first_rejected :
    runtimeMessage (run [letValue outOfRange]) = some "index-out-of-range" ∧
      runtimeMessage (run [letValue (.record [.computed divisionByZero outOfRange])]) = some "division-by-zero" := by
  decide +kernel

/-- A spread that keeps earlier fields gives `a = 1` for
`{...defaults, ...overrides}`; the model gives `7`. -/
theorem first_wins_rejected :
    ((([("a", 1), ("b", 2)] : List (String × Int)).foldl
        (fun built (label, item) => if (built.lookup label).isSome then built else built ++ [(label, item)])
        [("a", 1), ("b", 2)]).lookup "a" = some 1) ∧
      (result [defaults, overrides, letValue (.record [.spread (ident iName), .spread (ident jName)])]).bind
        (fieldInt ["a"]) = some 7 := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Records
