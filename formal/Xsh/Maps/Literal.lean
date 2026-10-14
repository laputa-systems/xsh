import Xsh.Values.MapPresence
import Xsh.Collections.KeyOrder

/-!
Checked evaluation of finite Map literals over one finite, already validated
key domain. Each key space selects one primitive domain and supplies an
injective table already listed in key order; a literal and all its spreads use
that same table. Entry evaluators record the key occurrence before the value
occurrence; spread maps are merged at their source position. Expression
outcomes are abstract inputs whose event trace records evaluation order and
failure short-circuiting. The model does not execute arbitrary effects, parse
literals, validate or coerce keys, convert between key domains, or establish
runtime correspondence.
-/

namespace Xsh.Maps.Literal

open Xsh.Values.MapPresence
open Xsh.Collections.KeyOrder

universe u

/-- A finite key universe in the selected primitive domain, listed in key order. -/
structure CheckedKeySpace where
  domain : KeyDomain
  size : Nat
  keyAt : Fin size → Key domain
  keyAt_injective : Function.Injective keyAt
  keyAt_ordered : ∀ {left right : Fin size}, left.val < right.val →
    Xsh.Collections.KeyOrder.keyLE domain (keyAt left) (keyAt right)

/-- One written entry or spread, identified by its expression occurrence. -/
inductive Item (domainSize : Nat) where
  | entry (occurrence : Nat)
  | spread (occurrence : Nat)

/-- The occurrence whose checked expression is evaluated at one step. -/
inductive EvalEvent where
  | key (occurrence : Nat)
  | value (occurrence : Nat)
  | spread (occurrence : Nat)
  deriving DecidableEq, Repr

/-- Failures retain the expression occurrence that stopped the literal. -/
inductive EvalError where
  | keyFailure (occurrence : Nat)
  | valueFailure (occurrence : Nat)
  | spreadFailure (occurrence : Nat)
  deriving DecidableEq, Repr

/-- Evaluation returns either the completed map or an error, plus its trace. -/
structure EvalResult (α : Type u) where
  outcome : Except EvalError α
  trace : List EvalEvent

/-- Checked expression outcomes for one literal's finite key universe. -/
structure Evaluators (domainSize : Nat) (Payload : Type u) where
  key : Nat → Option (Fin domainSize)
  value : Nat → Option (EntryValue Payload)
  spread : Nat → Option (FiniteMap domainSize Payload)

/-- Replace every present left entry with a present right entry. -/
def overlay {domainSize : Nat} {Payload : Type u}
    (left right : FiniteMap domainSize Payload) : FiniteMap domainSize Payload :=
  ⟨fun key =>
    match lookup right key with
    | some value => some value
    | none => lookup left key⟩

/-- Add an evaluation prefix while retaining the final outcome. -/
private def prefixTrace {α : Type u} (events : List EvalEvent)
    (result : EvalResult α) : EvalResult α :=
  ⟨result.outcome, events ++ result.trace⟩

/-- Events contributed by one written item, in source evaluation order. -/
def itemEvents {domainSize : Nat} : Item domainSize → List EvalEvent
  | .entry occurrence => [.key occurrence, .value occurrence]
  | .spread occurrence => [.spread occurrence]

/-- The trace prescribed by the complete written item sequence. -/
def writtenTrace {domainSize : Nat} (items : List (Item domainSize)) :
    List EvalEvent :=
  items.flatMap itemEvents

/-- Evaluate a literal left to right, keeping intermediate maps private. -/
def evaluateFrom {Payload : Type u} (space : CheckedKeySpace)
    (evaluators : Evaluators space.size Payload) :
    List (Item space.size) → FiniteMap space.size Payload →
      EvalResult (FiniteMap space.size Payload)
  | [], current => ⟨.ok current, []⟩
  | .entry occurrence :: rest, current =>
      match evaluators.key occurrence with
      | none => ⟨.error (.keyFailure occurrence), [.key occurrence]⟩
      | some key =>
          match evaluators.value occurrence with
          | none =>
              ⟨.error (.valueFailure occurrence),
                [.key occurrence, .value occurrence]⟩
          | some value =>
              prefixTrace [.key occurrence, .value occurrence]
                (evaluateFrom space evaluators rest
                  (Xsh.Values.MapPresence.set current key value))
  | .spread occurrence :: rest, current =>
      match evaluators.spread occurrence with
      | none => ⟨.error (.spreadFailure occurrence), [.spread occurrence]⟩
      | some spreadMap =>
          prefixTrace [.spread occurrence]
            (evaluateFrom space evaluators rest (overlay current spreadMap))

/-- Evaluate from the empty map and publish only a completed result. -/
def evaluate {Payload : Type u} (space : CheckedKeySpace)
    (evaluators : Evaluators space.size Payload)
    (items : List (Item space.size)) :
    EvalResult (FiniteMap space.size Payload) :=
  evaluateFrom space evaluators items
    (Xsh.Values.MapPresence.empty space.size Payload)

/-- Iterate present entries by scanning the checked domain in key order. -/
def orderedPresentKeys {Payload : Type u} (space : CheckedKeySpace)
    (map : FiniteMap space.size Payload) : List (Fin space.size) :=
  (List.finRange space.size).filter (Xsh.Values.MapPresence.contains map)

/-- Successful evaluators visit each written expression once and in order. -/
theorem successful_trace_is_written_trace {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (items : List (Item space.size)) (initial : FiniteMap space.size Payload)
    (keysSucceed : ∀ occurrence, ∃ key,
      evaluators.key occurrence = some key)
    (valuesSucceed : ∀ occurrence, ∃ value,
      evaluators.value occurrence = some value)
    (spreadsSucceed : ∀ occurrence, ∃ map,
      evaluators.spread occurrence = some map) :
    (evaluateFrom space evaluators items initial).trace = writtenTrace items := by
  induction items generalizing initial with
  | nil => rfl
  | cons item rest inductionHypothesis =>
      cases item with
      | entry occurrence =>
          obtain ⟨key, keySuccess⟩ := keysSucceed occurrence
          obtain ⟨value, valueSuccess⟩ := valuesSucceed occurrence
          simp [evaluateFrom, keySuccess, valueSuccess, prefixTrace,
            inductionHypothesis, writtenTrace, itemEvents]
      | spread occurrence =>
          obtain ⟨map, spreadSuccess⟩ := spreadsSucceed occurrence
          simp [evaluateFrom, spreadSuccess, prefixTrace,
            inductionHypothesis, writtenTrace, itemEvents]

/-- An entry's key occurrence precedes its value occurrence. -/
theorem entry_evaluates_key_before_value {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (initial : FiniteMap space.size Payload) (occurrence : Nat)
    (key : Fin space.size) (value : EntryValue Payload)
    (keySuccess : evaluators.key occurrence = some key)
    (valueSuccess : evaluators.value occurrence = some value) :
    (evaluateFrom space evaluators [.entry occurrence] initial).trace =
        [.key occurrence, .value occurrence] := by
  simp [evaluateFrom, keySuccess, valueSuccess, prefixTrace]

/-- A failed key skips its value and every later written item. -/
theorem key_failure_short_circuits {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (initial : FiniteMap space.size Payload) (occurrence : Nat)
    (rest : List (Item space.size))
    (keyFailure : evaluators.key occurrence = none) :
    (evaluateFrom space evaluators (.entry occurrence :: rest) initial).outcome =
        .error (.keyFailure occurrence) ∧
      (evaluateFrom space evaluators (.entry occurrence :: rest) initial).trace =
        [.key occurrence] := by
  simp [evaluateFrom, keyFailure]

/-- A failed value stops the suffix after recording the key and value attempts. -/
theorem value_failure_short_circuits {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (initial : FiniteMap space.size Payload) (occurrence : Nat)
    (rest : List (Item space.size)) (key : Fin space.size)
    (keySuccess : evaluators.key occurrence = some key)
    (valueFailure : evaluators.value occurrence = none) :
    (evaluateFrom space evaluators (.entry occurrence :: rest) initial).outcome =
        .error (.valueFailure occurrence) ∧
      (evaluateFrom space evaluators (.entry occurrence :: rest) initial).trace =
        [.key occurrence, .value occurrence] := by
  simp [evaluateFrom, keySuccess, valueFailure]

/-- A failed spread stops the suffix after its single spread attempt. -/
theorem spread_failure_short_circuits {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (initial : FiniteMap space.size Payload) (occurrence : Nat)
    (rest : List (Item space.size))
    (spreadFailure : evaluators.spread occurrence = none) :
    (evaluateFrom space evaluators (.spread occurrence :: rest) initial).outcome =
        .error (.spreadFailure occurrence) ∧
      (evaluateFrom space evaluators (.spread occurrence :: rest) initial).trace =
        [.spread occurrence] := by
  simp [evaluateFrom, spreadFailure]

/-- A later failure exposes only its error, even after a local entry update. -/
theorem failure_does_not_publish_partial_map {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (initial : FiniteMap space.size Payload) (first failed : Nat)
    (key : Fin space.size) (value : EntryValue Payload)
    (keySuccess : evaluators.key first = some key)
    (valueSuccess : evaluators.value first = some value)
    (keyFailure : evaluators.key failed = none) :
    (evaluateFrom space evaluators
      [.entry first, .entry failed] initial).outcome =
        .error (.keyFailure failed) ∧
      (evaluateFrom space evaluators
        [.entry first, .entry failed] initial).trace =
        [.key first, .value first, .key failed] := by
  simp [evaluateFrom, keySuccess, valueSuccess, keyFailure, prefixTrace]

/-- A later written entry replaces an earlier value at the same key. -/
theorem later_entry_replaces_earlier {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (initial : FiniteMap space.size Payload) (earlier later : Nat)
    (key : Fin space.size) (oldValue newValue : EntryValue Payload)
    (earlierKey : evaluators.key earlier = some key)
    (earlierValue : evaluators.value earlier = some oldValue)
    (laterKey : evaluators.key later = some key)
    (laterValue : evaluators.value later = some newValue) :
    (evaluateFrom space evaluators [.entry earlier, .entry later] initial).outcome =
        .ok (Xsh.Values.MapPresence.set
          (Xsh.Values.MapPresence.set initial key oldValue) key newValue) := by
  simp [evaluateFrom, earlierKey, earlierValue, laterKey, laterValue, prefixTrace]

/-- A spread replaces prior values only at keys present in the spread map. -/
theorem spread_replaces_prior_entry {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (initial spreadMap : FiniteMap space.size Payload)
    (entryId spreadId : Nat) (key : Fin space.size)
    (entryValue spreadValue : EntryValue Payload)
    (entryKey : evaluators.key entryId = some key)
    (entryValueSuccess : evaluators.value entryId = some entryValue)
    (spreadSuccess : evaluators.spread spreadId = some spreadMap)
    (spreadPresent : Xsh.Values.MapPresence.lookup spreadMap key = some spreadValue) :
    Xsh.Values.MapPresence.lookup
      (match (evaluateFrom space evaluators
        [.entry entryId, .spread spreadId] initial).outcome with
       | .ok map => map
       | .error _ => Xsh.Values.MapPresence.empty space.size Payload) key =
      some spreadValue := by
  change spreadMap.entries key = some spreadValue at spreadPresent
  simp [evaluateFrom, entryKey, entryValueSuccess, spreadSuccess,
    prefixTrace, overlay, Xsh.Values.MapPresence.lookup,
    Xsh.Values.MapPresence.set, spreadPresent]

/-- A later entry replaces the value supplied by an earlier spread. -/
theorem later_entry_replaces_spread {Payload : Type u}
    (space : CheckedKeySpace) (evaluators : Evaluators space.size Payload)
    (initial spreadMap : FiniteMap space.size Payload)
    (spreadId entryId : Nat) (key : Fin space.size)
    (spreadValue entryValue : EntryValue Payload)
    (spreadSuccess : evaluators.spread spreadId = some spreadMap)
    (spreadPresent : Xsh.Values.MapPresence.lookup spreadMap key = some spreadValue)
    (entryKey : evaluators.key entryId = some key)
    (entryValueSuccess : evaluators.value entryId = some entryValue) :
    Xsh.Values.MapPresence.lookup (overlay initial spreadMap) key =
        some spreadValue ∧
      Xsh.Values.MapPresence.lookup
        (match (evaluateFrom space evaluators
          [.spread spreadId, .entry entryId] initial).outcome with
         | .ok map => map
         | .error _ => Xsh.Values.MapPresence.empty space.size Payload) key =
        some entryValue := by
  change spreadMap.entries key = some spreadValue at spreadPresent
  simp [evaluateFrom, spreadSuccess, entryKey, entryValueSuccess,
    prefixTrace, overlay, Xsh.Values.MapPresence.lookup,
    Xsh.Values.MapPresence.set, spreadPresent]

/-- Iteration contains exactly the present keys from the finite universe. -/
theorem ordered_iteration_exactly_present {Payload : Type u}
    (space : CheckedKeySpace) (map : FiniteMap space.size Payload)
    (key : Fin space.size) :
    key ∈ orderedPresentKeys space map ↔
      Xsh.Values.MapPresence.contains map key = true := by
  simp [orderedPresentKeys]

/-- Iteration order agrees with the selected primitive key comparator. -/
theorem ordered_iteration_respects_key_order {Payload : Type u}
    (space : CheckedKeySpace) (map : FiniteMap space.size Payload) :
    (orderedPresentKeys space map).Pairwise
      (fun left right =>
        Xsh.Collections.KeyOrder.keyLE space.domain
          (space.keyAt left) (space.keyAt right) ∧
        space.keyAt left ≠ space.keyAt right) := by
  unfold orderedPresentKeys
  apply ((List.pairwise_lt_finRange space.size).filter
    (Xsh.Values.MapPresence.contains map)).imp
  intro left right before
  constructor
  · exact space.keyAt_ordered before
  · intro sameKey
    exact (Fin.ne_of_lt before) (space.keyAt_injective sameKey)

/-- Faulty duplicate elimination drops an earlier overwritten expression. -/
def skipEarlierOverwrittenEntry {domainSize : Nat}
    (items : List (Item domainSize)) : List (Item domainSize) :=
  items.drop 1

private def overwrittenWitnessSpace : CheckedKeySpace where
  domain := .uint
  size := 1
  keyAt := fun _ => ⟨0, by decide⟩
  keyAt_injective := by
    intro left right same
    apply Fin.ext
    omega
  keyAt_ordered := by
    intro left right before
    omega

private def overwrittenWitnessEvaluators : Evaluators 1 Nat where
  key := fun _ => some ⟨0, by decide⟩
  value := fun occurrence => some (.payload occurrence)
  spread := fun _ => some (Xsh.Values.MapPresence.empty 1 Nat)

/-- A concrete repeated key detects the mutation that skips an overwritten entry. -/
theorem skipped_overwritten_entry_mutation_detected :
    (evaluateFrom overwrittenWitnessSpace overwrittenWitnessEvaluators
      [.entry 20, .entry 21] (Xsh.Values.MapPresence.empty 1 Nat)).trace =
        [.key 20, .value 20, .key 21, .value 21] ∧
      (evaluateFrom overwrittenWitnessSpace overwrittenWitnessEvaluators
        (skipEarlierOverwrittenEntry [.entry 20, .entry 21])
          (Xsh.Values.MapPresence.empty 1 Nat)).trace =
        [.key 21, .value 21] ∧
      (evaluateFrom overwrittenWitnessSpace overwrittenWitnessEvaluators
        [.entry 20, .entry 21] (Xsh.Values.MapPresence.empty 1 Nat)).trace ≠
        (evaluateFrom overwrittenWitnessSpace overwrittenWitnessEvaluators
          (skipEarlierOverwrittenEntry [.entry 20, .entry 21])
            (Xsh.Values.MapPresence.empty 1 Nat)).trace := by
  decide

end Xsh.Maps.Literal
