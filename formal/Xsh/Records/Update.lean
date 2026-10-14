import Xsh.Values.Composite

/-!
This fragment models checked functional updates to existing record paths. It
uses immutable composite values, evaluates the base once, evaluates replacement
expressions in source order, and exposes a candidate only after every update
succeeds. A small external state parameter lets a later replacement observe
effects from an earlier replacement while the candidate remains rooted in the
base snapshot. Replacement values are supplied after checking; this fragment
does not model the checker's existing-field or same-type decisions. It does not
establish parser, checker, diagnostic, Rust, or whole-language correspondence.
-/

namespace Xsh.Records.Update

open Xsh.Values

abbrev FieldPath := List String

/-- Compare paths by prefix so parent and child updates are not disjoint. -/
def pathPrefix : FieldPath → FieldPath → Prop
  | [], _ => True
  | _ :: _, [] => False
  | left :: leftRest, right :: rightRest =>
      left = right ∧ pathPrefix leftRest rightRest

/-- Paths are disjoint only when neither target contains the other. -/
def pathsDisjoint (left right : FieldPath) : Prop :=
  ¬ pathPrefix left right ∧ ¬ pathPrefix right left

/-- Executable prefix comparison used by the checked update gate. -/
def pathPrefixBool : FieldPath → FieldPath → Bool
  | [], _ => true
  | _ :: _, [] => false
  | left :: leftRest, right :: rightRest =>
      (left == right) && pathPrefixBool leftRest rightRest

/-- Executable disjointness check corresponding to `pathsDisjoint`. -/
def pathsDisjointBool (left right : FieldPath) : Bool :=
  !(pathPrefixBool left right) && !(pathPrefixBool right left)

/-- Test whether a path is disjoint from every target in the remaining list. -/
def disjointFromAll (path : FieldPath) : List FieldPath → Bool
  | [] => true
  | target :: rest => pathsDisjointBool path target && disjointFromAll path rest

/-- Checked updates accept only lists whose targets are pairwise disjoint. -/
def pairwiseDisjointPaths : List FieldPath → Bool
  | [] => true
  | path :: rest => disjointFromAll path rest && pairwiseDisjointPaths rest

/-- Pairwise path disjointness expressed as a proposition. -/
def PairwiseDisjointPaths : List FieldPath → Prop
  | [] => True
  | path :: rest =>
      (∀ target, target ∈ rest → pathsDisjoint path target) ∧
        PairwiseDisjointPaths rest

theorem pathPrefixBool_eq_true_iff (left right : FieldPath) :
    pathPrefixBool left right = true ↔ pathPrefix left right := by
  induction left generalizing right with
  | nil => cases right <;> simp [pathPrefixBool, pathPrefix]
  | cons leftKey leftRest inductionHypothesis =>
      cases right with
      | nil => simp [pathPrefixBool, pathPrefix]
      | cons rightKey rightRest =>
          simp [pathPrefixBool, pathPrefix, inductionHypothesis]

theorem pathPrefixBool_eq_false_iff (left right : FieldPath) :
    pathPrefixBool left right = false ↔ ¬ pathPrefix left right := by
  constructor
  · intro isFalse hasPrefix
    have isTrue := (pathPrefixBool_eq_true_iff left right).2 hasPrefix
    rw [isFalse] at isTrue
    contradiction
  · intro noPrefix
    cases isTrueOrFalse : pathPrefixBool left right with
    | false => rfl
    | true => exact False.elim (noPrefix
        ((pathPrefixBool_eq_true_iff left right).1 isTrueOrFalse))

theorem pathsDisjointBool_eq_true_iff (left right : FieldPath) :
    pathsDisjointBool left right = true ↔ pathsDisjoint left right := by
  simp [pathsDisjointBool, Bool.and_eq_true, pathPrefixBool_eq_false_iff,
    pathsDisjoint]

theorem disjointFromAll_eq_true_iff (path : FieldPath) (targets : List FieldPath) :
    disjointFromAll path targets = true ↔
      ∀ target, target ∈ targets → pathsDisjoint path target := by
  induction targets with
  | nil => simp [disjointFromAll]
  | cons target rest inductionHypothesis =>
      simp [disjointFromAll, pathsDisjointBool_eq_true_iff, inductionHypothesis]

theorem pairwiseDisjointPaths_eq_true_iff (paths : List FieldPath) :
    pairwiseDisjointPaths paths = true ↔ PairwiseDisjointPaths paths := by
  induction paths with
  | nil => simp [pairwiseDisjointPaths, PairwiseDisjointPaths]
  | cons path rest inductionHypothesis =>
      simp [pairwiseDisjointPaths, disjointFromAll_eq_true_iff, inductionHypothesis,
        PairwiseDisjointPaths]

/-- Replace an existing field, preserving every other field position. -/
def replaceExistingField? : List (String × CompositeValue) → String → CompositeValue →
    Option (List (String × CompositeValue))
  | [], _, _ => none
  | (candidate, oldValue) :: rest, key, value =>
      if candidate == key then some ((candidate, value) :: rest)
      else (replaceExistingField? rest key value).map
        (fun updated => (candidate, oldValue) :: updated)

/-- Update an existing nonempty path of record fields without extending shape. -/
def updateAtPath (base : CompositeValue) : FieldPath → CompositeValue →
    Option CompositeValue
  | [], _ => none
  | key :: rest, replacement =>
      match base with
      | .record fields =>
          match findRecordField? key fields with
          | none => none
          | some child =>
              match rest with
              | [] => (replaceExistingField? fields key replacement).map
                  CompositeValue.record
              | nextKey :: tail =>
                  match updateAtPath child (nextKey :: tail) replacement with
                  | none => none
                  | some updatedChild =>
                      (replaceExistingField? fields key updatedChild).map
                        CompositeValue.record
      | _ => none
termination_by path => path.length
decreasing_by simp_wf <;> omega

inductive UpdateEvent where
  | base (tag : Nat)
  | replacement (tag : Nat)
  deriving DecidableEq, Repr

/-- Expression callbacks report their own observable event and resulting world. -/
structure ExpressionResult (Error State Value : Type) where
  outcome : Except Error Value
  state : State
  events : List UpdateEvent

/-- One checked target and its source expression. The expression can read the
base snapshot and the current external state, but receives neither a mutable
candidate nor a partially published result.
-/
structure Replacement (Error State : Type) where
  path : FieldPath
  evaluate : CompositeValue → State → ExpressionResult Error State CompositeValue

/-- An update expression can fail, and an invalid model path is reported too. -/
inductive UpdateFailure (Error : Type) where
  | expression (error : Error)
  | invalidTarget
  | overlappingTargets

/-- The only public value is the complete result; failed runs contain no value. -/
structure UpdateResult (Error State : Type) where
  outcome : Except (UpdateFailure Error) CompositeValue
  state : State
  trace : List UpdateEvent

/-- Evaluate replacement expressions once, from left to right, against the
original snapshot. Successful values update a private candidate. A failure
returns without exposing that candidate or evaluating a later expression.
-/
def evaluateReplacements {Error State : Type} (snapshot : CompositeValue)
    (state : State) (candidate : CompositeValue)
    (replacements : List (Replacement Error State)) :
    UpdateResult Error State :=
  match replacements with
  | [] => ⟨.ok candidate, state, []⟩
  | replacement :: rest =>
      let evaluated := replacement.evaluate snapshot state
      match evaluated.outcome with
      | .error error =>
          ⟨.error (.expression error), evaluated.state, evaluated.events⟩
      | .ok value =>
          match updateAtPath candidate replacement.path value with
          | none => ⟨.error .invalidTarget, evaluated.state, evaluated.events⟩
          | some nextCandidate =>
              let later := evaluateReplacements snapshot evaluated.state
                nextCandidate rest
              { later with trace := evaluated.events ++ later.trace }

/-- Evaluate a record base once, then run its ordered replacements privately. -/
def evaluateRecordUpdate {Error State : Type}
    (base : State → ExpressionResult Error State CompositeValue)
    (initialState : State) (replacements : List (Replacement Error State)) :
    UpdateResult Error State :=
  if pairwiseDisjointPaths (replacements.map Replacement.path) then
    let evaluated := base initialState
    match evaluated.outcome with
    | .error error =>
        ⟨.error (.expression error), evaluated.state, evaluated.events⟩
    | .ok snapshot =>
        let result := evaluateReplacements snapshot evaluated.state snapshot
          replacements
        { result with trace := evaluated.events ++ result.trace }
  else
    ⟨.error .overlappingTargets, initialState, []⟩

/-- Read an existing path for expression examples in the finite model. -/
def valueAtPath? (base : CompositeValue) : FieldPath → Option CompositeValue
  | [] => some base
  | key :: rest =>
      match base with
      | .record fields => do
          let child ← findRecordField? key fields
          valueAtPath? child rest
      | _ => none

/-- A faulty shallow update replaces an entire nested record with one field. -/
def shallowUpdateMutation (base : CompositeValue) (parent child : String)
    (replacement : CompositeValue) : Option CompositeValue :=
  match base with
  | .record fields =>
      (replaceExistingField? fields parent (.record [(child, replacement)])).map
        CompositeValue.record
  | _ => none

def intZeroValue : CompositeValue := .integer intZero
def intOneValue : CompositeValue := .integer intOne
def intNegativeOneValue : CompositeValue := .integer intNegativeOne

def flatBase : CompositeValue := .record [
  ("left", intZeroValue),
  ("right", intZeroValue),
  ("label", .string "kept")
]

def nestedBase : CompositeValue := .record [
  ("build", .record [
    ("jobs", intZeroValue),
    ("flags", .record [
      ("debug", .boolean false),
      ("optimize", .boolean true)
    ])
  ]),
  ("name", .string "original")
]

def expectedNestedUpdate : CompositeValue := .record [
  ("build", .record [
    ("jobs", intOneValue),
    ("flags", .record [
      ("debug", .boolean false),
      ("optimize", .boolean true)
    ])
  ]),
  ("name", .string "original")
]

/-- A counted base expression increments state and identifies its evaluation. -/
def countedBase (value : CompositeValue) : Nat → ExpressionResult Unit Nat CompositeValue :=
  fun state => ⟨.ok value, state + 1, [.base 90]⟩

/-- A counted replacement increments state and emits its distinct callback tag. -/
def countedReplacement {Error : Type} (path : FieldPath) (value : CompositeValue)
    (tag : Nat) :
    Replacement Error Nat :=
  ⟨path, fun _ state => ⟨.ok value, state + 1, [.replacement tag]⟩⟩

/-- The base expression is called once before any replacement expression. -/
theorem record_update_base_evaluates_once :
    let result := evaluateRecordUpdate (countedBase flatBase) 0 []
    result.outcome = .ok flatBase ∧ result.state = 1 ∧
      result.trace = [.base 90] := by
  simp [evaluateRecordUpdate, evaluateReplacements, pairwiseDisjointPaths,
    countedBase]

/-- Successful replacements are evaluated once in their written order. -/
theorem record_update_rhs_evaluates_in_source_order :
    let first : Replacement Unit Nat :=
      ⟨["left"], fun _ state =>
        ⟨.ok intOneValue, state + 1, [.replacement 41]⟩⟩
    let second : Replacement Unit Nat :=
      ⟨["right"], fun _ state =>
        ⟨.ok (if state == 2 then intNegativeOneValue else intZeroValue),
          state + 1, [.replacement 73]⟩⟩
    let result := evaluateRecordUpdate (countedBase flatBase) 0 [
      first, second
    ]
    result.outcome = .ok (.record [
      ("left", intOneValue),
      ("right", intNegativeOneValue),
      ("label", .string "kept")
    ]) ∧ result.state = 3 ∧
      result.trace = [.base 90, .replacement 41, .replacement 73] := by
  simp [evaluateRecordUpdate, evaluateReplacements, pairwiseDisjointPaths,
    disjointFromAll, pathsDisjointBool, pathPrefixBool, countedBase,
    updateAtPath.eq_1, updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?, flatBase, intOneValue,
    intZeroValue, intNegativeOneValue]

/-- A failing RHS prevents all later source expressions from running. -/
theorem record_update_failure_stops_later_rhs :
    let first := countedReplacement ["left"] intOneValue 41
    let failure : Replacement Nat Nat :=
      ⟨["right"], fun _ state =>
        ⟨.error 7, state + 1, [.replacement 73]⟩⟩
    let later : Replacement Nat Nat :=
      ⟨["label"], fun _ state =>
        ⟨.ok (.string "later"), state + 100, [.replacement 99]⟩⟩
    let result := evaluateRecordUpdate (fun state =>
      ⟨.ok flatBase, state + 1, [.base 90]⟩) 0 [first, failure, later]
    result.outcome = .error (.expression 7) ∧ result.state = 3 ∧
      result.trace = [.base 90, .replacement 41, .replacement 73] := by
  simp [evaluateRecordUpdate, evaluateReplacements, pairwiseDisjointPaths,
    disjointFromAll, pathsDisjointBool, pathPrefixBool, countedReplacement,
    updateAtPath.eq_1, updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?, flatBase, intOneValue,
    intZeroValue]

/-- A failure exposes no candidate containing earlier successful replacements. -/
theorem record_update_failure_publishes_no_partial_value :
    let first := countedReplacement ["left"] intOneValue 41
    let failure : Replacement Nat Nat :=
      ⟨["right"], fun _ state =>
        ⟨.error 7, state + 1, [.replacement 73]⟩⟩
    let result := evaluateRecordUpdate (fun state =>
      ⟨.ok flatBase, state + 1, [.base 90]⟩) 0 [first, failure]
    result.outcome = .error (.expression 7) := by
  simp [evaluateRecordUpdate, evaluateReplacements, pairwiseDisjointPaths,
    disjointFromAll, pathsDisjointBool, pathPrefixBool, countedReplacement,
    updateAtPath.eq_1, updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?, flatBase, intOneValue,
    intZeroValue]

def changedExternalSource : CompositeValue := .record [
  ("build", .record [
    ("jobs", intZeroValue),
    ("flags", .record [
      ("debug", .boolean true),
      ("optimize", .boolean false)
    ])
  ]),
  ("name", .string "changed")
]

def expectedSnapshotUpdate : CompositeValue := .record [
  ("build", .record [
    ("jobs", intZeroValue),
    ("flags", .record [
      ("debug", .boolean false),
      ("optimize", .boolean true)
    ])
  ]),
  ("name", .string "renamed")
]

/-- The candidate retains the base snapshot while later RHSs can observe a
changed external source value.
-/
theorem record_update_keeps_base_snapshot :
    let rename : Replacement Unit CompositeValue :=
      ⟨["name"], fun _ _ =>
        ⟨.ok (.string "renamed"), changedExternalSource, [.replacement 11]⟩⟩
    let copyLiveDebug : Replacement Unit CompositeValue :=
      ⟨["build", "flags", "optimize"], fun _ world =>
        match valueAtPath? world ["build", "flags", "debug"] with
        | some value => ⟨.ok value, world, [.replacement 22]⟩
        | none => ⟨.error (), world, [.replacement 22]⟩⟩
    let result := evaluateRecordUpdate
      (fun source => ⟨.ok source, source, [.base 90]⟩)
      nestedBase [rename, copyLiveDebug]
    result.outcome = .ok expectedSnapshotUpdate ∧
      result.state = changedExternalSource ∧
      result.trace = [.base 90, .replacement 11, .replacement 22] := by
  simp [evaluateRecordUpdate, evaluateReplacements, pairwiseDisjointPaths,
    disjointFromAll, pathsDisjointBool, pathPrefixBool, updateAtPath.eq_1,
    updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?, valueAtPath?,
    nestedBase, changedExternalSource, expectedSnapshotUpdate, intZeroValue]

/-- A checked replacement changes the selected existing leaf. -/
theorem record_update_replaces_existing_leaf :
    updateAtPath flatBase ["left"] intOneValue = some (.record [
      ("left", intOneValue),
      ("right", intZeroValue),
      ("label", .string "kept")
    ]) := by
  simp [updateAtPath.eq_1, updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?, flatBase, intOneValue, intZeroValue]

/-- Updating a nested leaf retains the sibling leaf and outer sibling. -/
theorem nested_update_preserves_sibling_fields :
    updateAtPath nestedBase ["build", "jobs"] intOneValue =
      some expectedNestedUpdate := by
  simp [updateAtPath.eq_1, updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?, nestedBase, expectedNestedUpdate,
    intOneValue, intZeroValue]

/-- Two disjoint root updates both remain present in the final value. -/
theorem disjoint_updates_preserve_both_targets :
    updateAtPath (match updateAtPath flatBase ["left"] intOneValue with
      | some updated => updated
      | none => flatBase) ["right"] intNegativeOneValue = some (.record [
        ("left", intOneValue),
        ("right", intNegativeOneValue),
        ("label", .string "kept")
      ]) := by
  simp [updateAtPath.eq_1, updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?, flatBase, intOneValue, intZeroValue,
    intNegativeOneValue]

/-- Disjoint updates commute for this concrete record and distinct targets. -/
theorem disjoint_updates_commute_in_value :
    (do
      let first ← updateAtPath flatBase ["left"] intOneValue
      updateAtPath first ["right"] intNegativeOneValue) =
    (do
      let first ← updateAtPath flatBase ["right"] intNegativeOneValue
      updateAtPath first ["left"] intOneValue) := by
  simp [updateAtPath.eq_1, updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?, flatBase, intOneValue, intZeroValue,
    intNegativeOneValue]

/-- A successful nonempty path update preserves the top-level field sequence. -/
theorem update_preserves_record_shape (fields : List (String × CompositeValue))
    (path : FieldPath) (replacement : CompositeValue) (updated : CompositeValue)
    (nonempty : path ≠ [])
    (applied : updateAtPath (.record fields) path replacement = some updated) :
    ∃ updatedFields, updated = .record updatedFields ∧
      updatedFields.map Prod.fst = fields.map Prod.fst := by
  have replaceExistingField_preserves_names
      (source : List (String × CompositeValue)) (key : String)
      (value : CompositeValue) (updatedFields : List (String × CompositeValue))
      (replaced : replaceExistingField? source key value = some updatedFields) :
      updatedFields.map Prod.fst = source.map Prod.fst := by
    have preserves : ∀ (source : List (String × CompositeValue))
        (key : String) (value : CompositeValue)
        (updatedFields : List (String × CompositeValue)),
        replaceExistingField? source key value = some updatedFields →
          updatedFields.map Prod.fst = source.map Prod.fst := by
      intro source
      induction source with
      | nil =>
          intro key value updatedFields replaced
          simp [replaceExistingField?] at replaced
      | cons head rest inductionHypothesis =>
          intro key value updatedFields replaced
          rcases head with ⟨candidate, oldValue⟩
          by_cases equalKey : candidate == key
          · simp [replaceExistingField?, equalKey] at replaced
            cases replaced
            simp
          · cases replacedRest : replaceExistingField? rest key value with
            | none => simp [replaceExistingField?, equalKey, replacedRest] at replaced
            | some updatedRest =>
                simp [replaceExistingField?, equalKey, replacedRest] at replaced
                cases replaced
                have restNames := inductionHypothesis key value updatedRest replacedRest
                simp [restNames]
    exact preserves source key value updatedFields replaced
  cases path with
  | nil => simp at nonempty
  | cons key rest =>
      rw [updateAtPath.eq_2] at applied
      cases found : findRecordField? key fields with
      | none => simp [found] at applied
      | some child =>
          cases rest with
          | nil =>
              cases replaced : replaceExistingField? fields key replacement with
              | none => simp [found, replaced] at applied
              | some updatedFields =>
                  simp only [found, replaced, Option.map_some] at applied
                  cases applied
                  exact ⟨_, rfl, replaceExistingField_preserves_names
                    fields key replacement updatedFields replaced⟩
          | cons nextKey tail =>
              cases inner : updateAtPath child (nextKey :: tail) replacement with
              | none => simp [found, inner] at applied
              | some updatedChild =>
                  cases replaced : replaceExistingField? fields key updatedChild with
                  | none => simp [found, inner, replaced] at applied
                  | some updatedFields =>
                      simp only [found, inner, replaced, Option.map_some] at applied
                      cases applied
                      exact ⟨_, rfl, replaceExistingField_preserves_names
                        fields key updatedChild updatedFields replaced⟩

/-- Parent and child targets overlap, so they are not disjoint. -/
theorem ancestor_and_descendant_targets_are_not_disjoint :
    ¬ pathsDisjoint ["build"] ["build", "jobs"] ∧
      let parent : Replacement Unit Nat :=
        ⟨["build"], fun _ state =>
          ⟨.ok intOneValue, state + 1, [.replacement 41]⟩⟩
      let child : Replacement Unit Nat :=
        ⟨["build", "jobs"], fun _ state =>
          ⟨.ok intNegativeOneValue, state + 1, [.replacement 73]⟩⟩
      let result := evaluateRecordUpdate
        (fun state => ⟨.ok nestedBase, state + 1, [.base 90]⟩)
        0 [parent, child]
      result.outcome = .error .overlappingTargets ∧
        result.state = 0 ∧ result.trace = [] := by
  simp [evaluateRecordUpdate, pairwiseDisjointPaths, disjointFromAll,
    pathsDisjointBool, pathPrefixBool, pathsDisjoint, pathPrefix]

/-- A shallow nested replacement drops an existing sibling, unlike a path update. -/
theorem shallow_update_mutation_loses_sibling :
    updateAtPath nestedBase ["build", "jobs"] intOneValue =
      some (.record [
        ("build", .record [
          ("jobs", intOneValue),
          ("flags", .record [
            ("debug", .boolean false),
            ("optimize", .boolean true)
          ])
        ]),
        ("name", .string "original")
      ]) ∧
    shallowUpdateMutation nestedBase "build" "jobs" intOneValue =
      some (.record [
        ("build", .record [("jobs", intOneValue)]),
        ("name", .string "original")
      ]) ∧
    ((shallowUpdateMutation nestedBase "build" "jobs" intOneValue).bind
      (fun value => valueAtPath? value ["build", "flags"])) = none := by
  simp [updateAtPath.eq_1, updateAtPath.eq_2, updateAtPath.eq_3,
    replaceExistingField?, findRecordField?,
    valueAtPath?, nestedBase, intZeroValue, intOneValue,
    shallowUpdateMutation]

end Xsh.Records.Update
