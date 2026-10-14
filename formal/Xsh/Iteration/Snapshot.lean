import Xsh.Maps.Literal

/-!
Finite `for` loops evaluate their source once and iterate the resulting
collection snapshot. Lists retain element order, maps use the selected
primitive key order, strings yield Unicode scalars, and bytes yield unsigned
byte values. A Result error propagates before the body runs. This model omits
streams and pipelines, loop control, typechecking, and runtime correspondence.
-/

namespace Xsh.Iteration.Snapshot

open Xsh.Values.MapPresence
open Xsh.Maps.Literal
open Xsh.Collections.KeyOrder

universe u v

/-- The finite source forms consumed by a non-pipeline `for` loop. -/
inductive Source (space : CheckedKeySpace) (Payload : Type u) where
  | list (values : List Payload)
  | map (entries : FiniteMap space.size Payload)
  | text (value : String)
  | bytes (value : List UInt8)

/-- Values bound by one iteration, including map entries and unsigned bytes. -/
inductive Item (space : CheckedKeySpace) (Payload : Type u) where
  | listValue (value : Payload)
  | mapEntry (key : Key space.domain) (value : EntryValue Payload)
  | scalar (value : Char)
  | byte (value : Int)

/-- An item passed to the body is a value, separate from mutable loop state. -/
structure Frame (space : CheckedKeySpace) (Payload : Type u) where
  item : Item space Payload
  source : Source space Payload
  observed : List (Item space Payload)

/-- The body may change the source variable and record observations, but it
cannot replace the immutable item binding in its frame. -/
abbrev Body (space : CheckedKeySpace) (Payload : Type u) :=
  Frame space Payload → Source space Payload × List (Item space Payload)

/-- The body updates loop state while the frame retains its item value. -/
structure BodyReturn (space : CheckedKeySpace) (Payload : Type u) where
  item : Item space Payload
  source : Source space Payload
  observed : List (Item space Payload)

/-- Invoke a body and carry its immutable item binding into the result. -/
def invokeBody (space : CheckedKeySpace) {Payload : Type u}
    (body : Body space Payload) (frame : Frame space Payload) :
    BodyReturn space Payload :=
  let (source, observed) := body frame
  ⟨frame.item, source, observed⟩

/-- The final mutable state and the values visited by the loop engine. -/
structure LoopResult (space : CheckedKeySpace) (Payload : Type u) where
  source : Source space Payload
  observed : List (Item space Payload)
  visited : List (Item space Payload)

/-- A source expression denotes its successive values if evaluated repeatedly. -/
structure SourceExpression (space : CheckedKeySpace) (Payload : Type u) where
  valueAt : Nat → Source space Payload

/-- One source evaluation records the ordinal and value read. -/
structure SourceEvaluation (space : CheckedKeySpace) (Payload : Type u) where
  ordinal : Nat
  source : Source space Payload

/-- Evaluate a source expression and extend the trace with that operation. -/
def evaluateSource (space : CheckedKeySpace) {Payload : Type u}
    (expression : SourceExpression space Payload)
    (evaluations : List (SourceEvaluation space Payload)) :
    Source space Payload × List (SourceEvaluation space Payload) :=
  let ordinal := evaluations.length
  let source := expression.valueAt ordinal
  (source, evaluations ++ [{ ordinal := ordinal, source := source }])

/-- Capture all present map entries in the checked primitive-key order. -/
private def mapSnapshot (space : CheckedKeySpace) {Payload : Type u}
    (entries : FiniteMap space.size Payload) : List (Item space Payload) :=
  (orderedPresentKeys space entries).filterMap fun key =>
    (lookup entries key).map fun value => .mapEntry (space.keyAt key) value

private theorem filterMap_map_some {α : Type} {β : Type v}
    (project : α → Option β) (keys : List α)
    (present : ∀ key, key ∈ keys → ∃ value, project key = some value) :
    (keys.filterMap project).map some = keys.map project := by
  induction keys with
  | nil => rfl
  | cons key rest inductionHypothesis =>
      obtain ⟨value, keyPresent⟩ := present key (by simp)
      have tailPresent : ∀ other, other ∈ rest →
          ∃ value, project other = some value := by
        intro other member
        exact present other (by simp [member])
      simp [List.filterMap, keyPresent, inductionHypothesis tailPresent]

private theorem mapSnapshot_matches_keys (space : CheckedKeySpace)
    {Payload : Type u} (entries : FiniteMap space.size Payload) :
    (mapSnapshot space entries).map some =
      (orderedPresentKeys space entries).map fun key =>
        (lookup entries key).map
          (fun value => Item.mapEntry (space.keyAt key) value) := by
  unfold mapSnapshot
  apply filterMap_map_some
  intro key member
  have isPresent := (ordered_iteration_exactly_present space entries key).mp member
  obtain ⟨value, lookupValue⟩ :=
    (Xsh.Values.MapPresence.contains_iff_lookup_exists entries key).mp isPresent
  exact ⟨Item.mapEntry (space.keyAt key) value, by simp [lookupValue]⟩

/-- Read the finite source once into the values that the loop will consume. -/
def snapshot (space : CheckedKeySpace) {Payload : Type u} :
    Source space Payload → List (Item space Payload)
  | .list values => values.map Item.listValue
  | .map entries => mapSnapshot space entries
  | .text value => value.toList.map Item.scalar
  | .bytes values => values.map fun value => .byte (Int.ofNat value.toNat)

/-- Invoke the body once for each captured item, without consulting the live
source again. -/
def execute (space : CheckedKeySpace) {Payload : Type u}
    (body : Body space Payload) :
    List (Item space Payload) → Source space Payload →
      List (Item space Payload) → LoopResult space Payload
  | [], source, observed => ⟨source, observed, []⟩
  | item :: rest, source, observed =>
      let afterBody := invokeBody space body ⟨item, source, observed⟩
      let suffix := execute space body rest afterBody.source afterBody.observed
      { suffix with visited := item :: suffix.visited }

private theorem execute_visits (space : CheckedKeySpace) {Payload : Type u}
    (body : Body space Payload) (items : List (Item space Payload))
    (source : Source space Payload)
    (observed : List (Item space Payload)) :
    (execute space body items source observed).visited = items := by
  induction items generalizing source observed with
  | nil => rfl
  | cons item rest inductionHypothesis =>
      simp [execute, invokeBody, inductionHypothesis]

private theorem execute_constant_body_source (space : CheckedKeySpace)
    {Payload : Type u} (items : List (Item space Payload))
    (source replacement : Source space Payload)
    (observed : List (Item space Payload)) :
    (execute space (fun frame => (replacement, frame.observed)) items source observed).source =
      if items.isEmpty then source else replacement := by
  induction items generalizing source observed with
  | nil => rfl
  | cons item rest inductionHypothesis =>
      simp [execute, invokeBody, inductionHypothesis]

/-- Result of interpreting a source expression and its finite loop. -/
structure ExpressionResult (space : CheckedKeySpace) (Payload : Type u) where
  loop : LoopResult space Payload
  sourceEvaluations : List (SourceEvaluation space Payload)

/-- The number of source evaluations recorded by the interpreter. -/
def ExpressionResult.sourceEvaluationCount
    (result : ExpressionResult space Payload) : Nat :=
  result.sourceEvaluations.length

/-- Evaluate a source expression once and execute the captured iteration. -/
def executeExpression (space : CheckedKeySpace) {Payload : Type u}
    (expression : SourceExpression space Payload) (body : Body space Payload)
    (observed : List (Item space Payload)) : ExpressionResult space Payload :=
  let (source, evaluations) := evaluateSource space expression []
  ⟨execute space body (snapshot space source) source observed, evaluations⟩

/-- Propagated source errors bypass the loop body. -/
inductive Exit (space : CheckedKeySpace) (Payload Error : Type u) where
  | completed (result : LoopResult space Payload)
  | propagated (error : Error) (observed : List (Item space Payload))

/-- Interpret a Result-wrapped collection source at the loop boundary. -/
def executeResult (space : CheckedKeySpace) {Payload Error : Type u}
    (source : Except Error (Source space Payload)) (body : Body space Payload)
    (observed : List (Item space Payload)) : Exit space Payload Error :=
  match source with
  | .error error => .propagated error observed
  | .ok value => .completed (execute space body (snapshot space value) value observed)

/-- One recorded source evaluation supplies the immutable iteration snapshot. -/
theorem source_evaluates_once (space : CheckedKeySpace) {Payload : Type u}
    (expression : SourceExpression space Payload) (body : Body space Payload)
    (observed : List (Item space Payload)) :
    let result := executeExpression space expression body observed
    result.sourceEvaluations =
        [{ ordinal := 0, source := expression.valueAt 0 }] ∧
      result.sourceEvaluationCount = 1 ∧
      result.loop.visited = snapshot space (expression.valueAt 0) := by
  simp [executeExpression, evaluateSource, ExpressionResult.sourceEvaluationCount,
    execute_visits]

/-- List iteration preserves the captured element order. -/
theorem list_iterates_in_order (space : CheckedKeySpace) {Payload : Type u}
    (values : List Payload) (source : Source space Payload)
    (observed : List (Item space Payload)) :
    (execute space (fun frame => (source, frame.observed))
      (snapshot space (.list values)) source observed).visited =
        values.map Item.listValue := by
  simpa [snapshot] using
    (execute_visits space (fun frame => (source, frame.observed))
      (snapshot space (.list values)) source observed)

/-- Map items match ordered present-key lookups and cover the present keys. -/
theorem map_iterates_in_key_order (space : CheckedKeySpace)
    {Payload : Type u} (entries : FiniteMap space.size Payload) :
    (mapSnapshot space entries).map some =
        ((orderedPresentKeys space entries).map fun key =>
          (lookup entries key).map
            (fun value => Item.mapEntry (space.keyAt key) value)) ∧
      (orderedPresentKeys space entries).Pairwise
        (fun left right =>
          keyLE space.domain (space.keyAt left) (space.keyAt right) ∧
            space.keyAt left ≠ space.keyAt right) ∧
      (∀ key, key ∈ orderedPresentKeys space entries ↔
        contains entries key = true) := by
  exact ⟨mapSnapshot_matches_keys space entries,
    ordered_iteration_respects_key_order space entries,
    fun key => ordered_iteration_exactly_present space entries key⟩

/-- String iteration yields Unicode scalar values in string order. -/
theorem str_iterates_unicode_scalars (space : CheckedKeySpace)
    (Payload : Type u)
    (value : String) :
    snapshot space (Payload := Payload) (.text value) =
      value.toList.map (Item.scalar (Payload := Payload)) := by
  rfl

/-- Byte iteration converts each octet to its unsigned natural value. -/
theorem bytes_iterate_unsigned_values (space : CheckedKeySpace)
    (Payload : Type u)
    (values : List UInt8) :
    snapshot space (Payload := Payload) (.bytes values) =
      values.map fun value => Item.byte (Payload := Payload) (Int.ofNat value.toNat) := by
  rfl

/-- A Result error returns immediately with the observation trace intact. -/
theorem result_error_skips_body (space : CheckedKeySpace)
    {Payload Error : Type u} (error : Error) (body : Body space Payload)
    (observed : List (Item space Payload)) :
    executeResult space (.error error) body observed =
      (.propagated error observed : Exit space Payload Error) := by
  rfl

/-- A successful Result source is unwrapped and its finite snapshot is iterated. -/
theorem result_ok_iterates_wrapped_snapshot (space : CheckedKeySpace)
    {Payload Error : Type u} (source : Source space Payload)
    (body : Body space Payload) (observed : List (Item space Payload)) :
    ∃ result, executeResult space (.ok source) body observed =
      (.completed result : Exit space Payload Error) ∧
      result.visited = snapshot space source := by
  refine ⟨execute space body (snapshot space source) source observed, rfl, ?_⟩
  exact execute_visits space body (snapshot space source) source observed

/-- Rebinding the source variable in the body does not change pending items. -/
theorem source_rebinding_does_not_change_snapshot
    (space : CheckedKeySpace) {Payload : Type u} (first : Payload)
    (rest : List Payload) (replacement : Source space Payload) :
    let source : Source space Payload := .list (first :: rest)
    let body : Body space Payload := fun frame =>
      (replacement, frame.observed)
    let result := execute space body (snapshot space source) source []
    result.visited = (first :: rest).map Item.listValue ∧
      result.source = replacement := by
  change
    (execute space (fun frame => (replacement, frame.observed))
      (snapshot space (.list (first :: rest))) (.list (first :: rest)) []).visited =
        (first :: rest).map Item.listValue ∧
      (execute space (fun frame => (replacement, frame.observed))
        (snapshot space (.list (first :: rest))) (.list (first :: rest)) []).source =
        replacement
  constructor
  · simpa [snapshot] using
      (execute_visits space (fun frame => (replacement, frame.observed))
        (snapshot space (.list (first :: rest))) (.list (first :: rest)) [])
  · change
      (execute space (fun frame => (replacement, frame.observed))
        (snapshot space (.list (first :: rest))) (.list (first :: rest)) []).source = replacement
    rw [execute_constant_body_source]
    simp [snapshot]

/-- The body receives the captured item value, even when it records it. -/
theorem captured_body_observes_item_value (space : CheckedKeySpace)
    {Payload : Type u} (item : Item space Payload)
    (source : Source space Payload) :
    let body : Body space Payload := fun frame =>
      (frame.source, frame.observed ++ [frame.item])
    let result := execute space body [item] source []
    result.observed = [item] := by
  simp [execute, invokeBody]

/-- A body cannot replace the item stored in its immutable iteration frame. -/
theorem item_binding_is_immutable (space : CheckedKeySpace)
    {Payload : Type u} (item : Item space Payload)
    (source : Source space Payload)
    (observed : List (Item space Payload)) (body : Body space Payload) :
    let frame : Frame space Payload := ⟨item, source, observed⟩
    (invokeBody space body frame).item = item := by
  rfl

/-- A live map scan can see a key inserted after it starts. -/
private abbrev boolKeySpace : CheckedKeySpace := {
  domain := .bool
  size := 2
  keyAt := fun index => decide (index.val = 1)
  keyAt_injective := by
    intro left right same
    apply Fin.ext
    by_cases leftOne : left.val = 1 <;> by_cases rightOne : right.val = 1
    · omega
    · simp [leftOne, rightOne] at same
    · simp [leftOne, rightOne] at same
    · omega
  keyAt_ordered := by
    intro left right before
    have leftZero : left.val = 0 := by omega
    have rightOne : right.val = 1 := by omega
    simp [keyLE, leftZero, rightOne]
}

private def firstKey : Fin 2 := ⟨0, by decide⟩
private def insertedKey : Fin 2 := ⟨1, by decide⟩

private theorem finRangeTwo : List.finRange 2 = [firstKey, insertedKey] := by
  rfl

private def initialMap : FiniteMap 2 Nat :=
  set (empty 2 Nat) firstKey (.payload 10)

private def mapAfterInsertion : FiniteMap 2 Nat :=
  set initialMap insertedKey (.payload 20)

private theorem initialMapKeys :
    orderedPresentKeys boolKeySpace initialMap = [firstKey] := by
  unfold orderedPresentKeys initialMap
  rw [finRangeTwo]
  simp [boolKeySpace, Xsh.Values.MapPresence.set,
    Xsh.Values.MapPresence.empty, Xsh.Values.MapPresence.contains,
    Xsh.Values.MapPresence.lookup, firstKey, insertedKey]

private theorem initialMapSnapshot :
    snapshot boolKeySpace (.map initialMap) =
      [.mapEntry false (.payload 10)] := by
  unfold snapshot mapSnapshot orderedPresentKeys initialMap
  rw [finRangeTwo]
  simp [boolKeySpace, Xsh.Values.MapPresence.set,
    Xsh.Values.MapPresence.empty, Xsh.Values.MapPresence.contains,
    Xsh.Values.MapPresence.lookup, firstKey, insertedKey]

/-- Re-scan the still-unvisited suffix of a live finite map. -/
private def liveSuffix (entries : FiniteMap 2 Nat) (nextIndex : Nat) :
    List (Fin 2) :=
  (orderedPresentKeys boolKeySpace entries).filter fun key =>
    decide (nextIndex ≤ key.val)

private theorem insertedMapSuffix :
    liveSuffix mapAfterInsertion (firstKey.val + 1) = [insertedKey] := by
  unfold liveSuffix orderedPresentKeys mapAfterInsertion initialMap
  rw [finRangeTwo]
  simp [boolKeySpace, Xsh.Values.MapPresence.set,
    Xsh.Values.MapPresence.empty, Xsh.Values.MapPresence.contains,
    Xsh.Values.MapPresence.lookup, firstKey, insertedKey]

/-- A two-step live scan sees the first original key and then rescans. -/
private def liveVisitsAfterInsertion : List (Fin 2) :=
  [firstKey] ++ liveSuffix mapAfterInsertion (firstKey.val + 1)

/-- Inserting a later map key changes what a live scan would visit. -/
theorem live_mutation_changes_iteration :
    orderedPresentKeys boolKeySpace initialMap = [firstKey] ∧
      liveVisitsAfterInsertion = [firstKey, insertedKey] ∧
      liveVisitsAfterInsertion ≠ orderedPresentKeys boolKeySpace initialMap := by
  rw [initialMapKeys]
  rw [liveVisitsAfterInsertion]
  rw [insertedMapSuffix]
  simp

private def mapMutationBody (frame : Frame boolKeySpace Nat) :
    Source boolKeySpace Nat × List (Item boolKeySpace Nat) :=
  (.map mapAfterInsertion, frame.observed)

/-- The real snapshot loop ignores the inserted key while retaining the live mutation. -/
theorem snapshot_rejects_live_mutation :
    let source : Source boolKeySpace Nat := .map initialMap
    let result := execute boolKeySpace mapMutationBody
      (snapshot boolKeySpace source) source []
    result.visited = snapshot boolKeySpace source ∧
      result.visited.length = 1 ∧
      match result.source with
      | .map entries => contains entries insertedKey = true
      | _ => False := by
  change
    let result := execute boolKeySpace mapMutationBody
      [.mapEntry false (.payload 10)] (.map initialMap) []
    result.visited = snapshot boolKeySpace (.map initialMap) ∧
      result.visited.length = 1 ∧
      match result.source with
      | .map entries => contains entries insertedKey = true
      | _ => False
  rw [initialMapSnapshot]
  simp [execute, invokeBody, mapMutationBody, mapAfterInsertion, initialMap,
    Xsh.Values.MapPresence.set, Xsh.Values.MapPresence.empty,
    Xsh.Values.MapPresence.contains, Xsh.Values.MapPresence.lookup,
    firstKey, insertedKey]

end Xsh.Iteration.Snapshot
