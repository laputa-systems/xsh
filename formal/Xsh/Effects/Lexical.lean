import Xsh.Syntax.Core
import Xsh.Semantics.Environments

/-!
An abstract static model for lexical effect bounds and delayed obligations.

The source expression fields identify already-classified lexical sites in
`Xsh.Syntax`; this module does not classify Rust operations or prove parser,
checker, runtime, or lowering conformance. A delayed item retains its origin
and captured cells while suspended, but stores no effect permission. This is
not a runtime sandbox or a consumer-side effect capability.

Finite observations below pair supplied emission prefixes with creation-side
charges. They do not prove that a producer executes, emits a complete prefix,
or has complete latent-effect classification or semantics.
Lexical sites in this fragment are structural expression labels, so distinct
source occurrences with identical expression syntax are not distinguished.
-/

namespace Xsh.Semantics.Effects

open Xsh.Syntax
open Xsh.Semantics.Environments

/-- The finite effect list attached to a checked clause or lexical region. -/
abbrev EffectSet := List Effect

/-- `io` covers the four host effects named by the language contract. -/
def ioCovers (required : Effect) : Prop :=
  required = .fs ∨ required = .net ∨ required = .process ∨ required = .env

/-- A checked clause containing `io` covers its component host effects too. -/
def clauseCovers (clause : EffectSet) (required : Effect) : Prop :=
  required ∈ clause ∨ (.io ∈ clause ∧ ioCovers required)

/-- A valid `without` entry blocks itself; excluding a component also blocks `io`. -/
def withoutBlocks (excluded required : Effect) : Prop :=
  (excluded ≠ .error ∧ excluded = required) ∨
    (required = .io ∧
      (excluded = .fs ∨ excluded = .net ∨ excluded = .process ∨ excluded = .env))

/-- At least one enclosing entry blocks a requirement. -/
def blockedBy (excluded : EffectSet) (required : Effect) : Prop :=
  match excluded with
  | [] => False
  | bound :: rest => withoutBlocks bound required ∨ blockedBy rest required

/-- A lexical context contains the enclosing clause and accumulated `without` entries. -/
structure LexicalContext where
  clause : EffectSet
  excluded : EffectSet

/-- Whether the checker can justify one classified requirement at this lexical site. -/
def permits (context : LexicalContext) (required : Effect) : Prop :=
  clauseCovers context.clause required ∧
    ¬ blockedBy context.excluded required

/-- A nested `without` adds exclusions without changing the enclosing clause. -/
def enterWithout (context : LexicalContext) (excluded : EffectSet) : LexicalContext :=
  { context with excluded := context.excluded ++ excluded }

theorem blockedBy_append_left (leftSide rightSide : EffectSet) (required : Effect)
    (blocked : blockedBy leftSide required) :
    blockedBy (leftSide ++ rightSide) required := by
  induction leftSide with
  | nil => simp [blockedBy] at blocked
  | cons head tail ih =>
      simp only [blockedBy] at blocked ⊢
      rcases blocked with direct | rest
      · exact Or.inl direct
      · exact Or.inr (ih rest)

theorem blockedBy_append_right (leftSide rightSide : EffectSet) (required : Effect)
    (blocked : blockedBy rightSide required) :
    blockedBy (leftSide ++ rightSide) required := by
  induction leftSide generalizing rightSide blocked with
  | nil => simpa [blockedBy] using blocked
  | cons head tail ih =>
      simp only [List.cons_append, blockedBy]
      exact Or.inr (ih rightSide blocked)

theorem blockedBy_of_member (bound : Effect) :
    ∀ (excluded : EffectSet) (required : Effect),
      bound ∈ excluded → withoutBlocks bound required → blockedBy excluded required := by
  intro excluded
  induction excluded with
  | nil => intro required member; cases member
  | cons head tail ih =>
      intro required member blocks
      simp only [List.mem_cons] at member
      rcases member with same | member
      · subst head
        exact Or.inl blocks
      · exact Or.inr (ih required member blocks)

theorem nested_without_only_restricts (context : LexicalContext)
    (excluded : EffectSet) (required : Effect)
    (allowed : permits (enterWithout context excluded) required) :
    permits context required := by
  constructor
  · exact allowed.1
  · intro denied
    apply allowed.2
    exact blockedBy_append_left context.excluded excluded required denied

theorem without_rejects_a_blocked_requirement (context : LexicalContext)
    (excluded : EffectSet) (bound required : Effect)
    (member : bound ∈ excluded) (blocks : withoutBlocks bound required) :
    ¬ permits (enterWithout context excluded) required := by
  intro allowed
  apply allowed.2
  exact blockedBy_append_right context.excluded excluded required
    (blockedBy_of_member bound excluded required member blocks)

theorem without_does_not_block_error (excluded : Effect) :
    ¬ withoutBlocks excluded .error := by
  simp [withoutBlocks]

/-- A checked call or callback obligation names its source expression. -/
structure EffectObligation where
  source : Expr
  required : Effect

/-- A finite sequence of requirements is accepted only if each fits its site. -/
def permitsObligations (context : LexicalContext) : List EffectObligation → Prop
  | [] => True
  | obligation :: rest =>
      permits context obligation.required ∧ permitsObligations context rest

theorem permits_obligation_at_member (context : LexicalContext)
    (obligations : List EffectObligation) (obligation : EffectObligation)
    (member : obligation ∈ obligations)
    (permitted : permitsObligations context obligations) :
    permits context obligation.required := by
  induction obligations with
  | nil => simp at member
  | cons head tail ih =>
      simp only [List.mem_cons] at member
      rcases member with same | member
      · cases same
        exact permitted.1
      · exact ih member permitted.2

/-- The kind of delayed body whose classified obligations are checked at origin. -/
inductive DelayedKind where
  | streamProducer
  | callback
  deriving DecidableEq, Repr

/-- A delayed body retains its creation expression, captures, and classified effects. -/
structure DelayedWork where
  kind : DelayedKind
  creation : Expr
  captures : CapturedEnvironment
  obligations : List EffectObligation

/-- All delayed body requirements must fit the lexical context at creation. -/
def permitsCreation (context : LexicalContext) (work : DelayedWork) : Prop :=
  permitsObligations context work.obligations

/-- One body requirement attributed to the expression that created the delayed work. -/
structure EffectCharge where
  lexicalSite : Expr
  causalBodySite : Expr
  required : Effect

def chargeAtUse (obligation : EffectObligation) : EffectCharge :=
  ⟨obligation.source, obligation.source, obligation.required⟩

def chargeAtCreation (work : DelayedWork) (obligation : EffectObligation) : EffectCharge :=
  ⟨work.creation, obligation.source, obligation.required⟩

theorem immediate_charge_names_invocation (obligation : EffectObligation) :
    (chargeAtUse obligation).lexicalSite = obligation.source := by
  rfl

theorem charge_lexical_site_is_creation (work : DelayedWork)
    (obligation : EffectObligation) :
    (chargeAtCreation work obligation).lexicalSite = work.creation := by
  rfl

theorem creation_permission_pays_body_obligation (context : LexicalContext)
    (work : DelayedWork) (obligation : EffectObligation)
    (member : obligation ∈ work.obligations)
    (permitted : permitsCreation context work) :
    permits context (chargeAtCreation work obligation).required := by
  exact permits_obligation_at_member context work.obligations obligation member permitted

/-- Consumer-site obligations are checked independently of the delayed body. -/
structure CheckedConsumer where
  context : LexicalContext
  obligations : List EffectObligation
  permitted : permitsObligations context obligations

/-- Suspended work carries causality and captures, not a permitted-effect set. -/
structure SuspendedWork where
  work : DelayedWork
  remaining : List EffectObligation

def suspend (work : DelayedWork) : SuspendedWork :=
  ⟨work, work.obligations⟩

/-- Select one pending obligation when an abstract consumer pulls the work. -/
def pull (suspended : SuspendedWork) : Option (EffectCharge × SuspendedWork) :=
  match suspended.remaining with
  | [] => none
  | obligation :: remaining =>
      some (chargeAtCreation suspended.work obligation,
        { suspended with remaining := remaining })

/-- Consumer effects are checked locally; a selected producer obligation keeps its origin. -/
def checkedConsumption (_consumer : CheckedConsumer) (suspended : SuspendedWork) :=
  pull suspended

theorem pull_charges_at_creation (work : DelayedWork)
    (obligation : EffectObligation) (remaining : List EffectObligation) :
    pull ⟨work, obligation :: remaining⟩ =
      some (chargeAtCreation work obligation, ⟨work, remaining⟩) := by
  rfl

theorem pull_preserves_captured_cell (work : DelayedWork)
    (obligation : EffectObligation) (remaining : List EffectObligation)
    (binder : Xsh.Foundation.BinderId) :
    (pull ⟨work, obligation :: remaining⟩).map
      (fun result => result.2.work.captures binder) =
      some (work.captures binder) := by
  rfl

theorem checked_consumption_keeps_creation_charge (consumer : CheckedConsumer)
    (work : DelayedWork)
    (obligation : EffectObligation) (remaining : List EffectObligation)
    : checkedConsumption consumer ⟨work, obligation :: remaining⟩ =
      some (chargeAtCreation work obligation, ⟨work, remaining⟩) := by
  rfl

/-- Pull a finite prefix, recording each charge in the order it is observed. -/
def pullSequence : Nat → SuspendedWork → List EffectCharge × SuspendedWork
  | 0, suspended => ([], suspended)
  | Nat.succ steps, suspended =>
      match pull suspended with
      | none => ([], suspended)
      | some (charge, next) =>
          let (charges, final) := pullSequence steps next
          (charge :: charges, final)

/-- Every finite pull prefix is the corresponding ordered creation charge prefix. -/
theorem pull_sequence_preserves_creation_and_body_order (steps : Nat)
    (work : DelayedWork) (remaining : List EffectObligation) :
    pullSequence steps ⟨work, remaining⟩ =
      ((remaining.take steps).map (chargeAtCreation work),
        ⟨work, remaining.drop steps⟩) := by
  induction steps generalizing remaining with
  | zero => simp [pullSequence]
  | succ steps ih =>
      cases remaining with
      | nil => simp [pullSequence, pull]
      | cons obligation remaining =>
          simp [pullSequence, pull, ih]

/-- Each reported charge keeps the delayed work's origin and its body's source site. -/
theorem pull_sequence_charge_keeps_origin_and_body_site (steps : Nat)
    (work : DelayedWork) (remaining : List EffectObligation)
    (charge : EffectCharge)
    (member : charge ∈ (pullSequence steps ⟨work, remaining⟩).1) :
    ∃ obligation ∈ remaining.take steps,
      charge = chargeAtCreation work obligation ∧
        charge.lexicalSite = work.creation ∧
        charge.causalBodySite = obligation.source ∧
        charge.required = obligation.required := by
  rw [pull_sequence_preserves_creation_and_body_order] at member
  change charge ∈ (remaining.take steps).map (chargeAtCreation work) at member
  rcases List.mem_map.mp member with ⟨obligation, ⟨inPrefix, sameCharge⟩⟩
  subst charge
  exact ⟨obligation, inPrefix,
    ⟨rfl, ⟨charge_lexical_site_is_creation work obligation, ⟨rfl, rfl⟩⟩⟩⟩

/-- If the queued obligations fit the creation context, every pulled one still does. -/
theorem pull_sequence_charges_are_permitted (context : LexicalContext)
    (work : DelayedWork) :
    ∀ (remaining : List EffectObligation) (steps : Nat),
      permitsObligations context remaining →
      ∀ charge ∈ (pullSequence steps ⟨work, remaining⟩).1,
        permits context charge.required := by
  intro remaining steps
  induction steps generalizing remaining with
  | zero =>
      intro permitted charge member
      simp [pullSequence] at member
  | succ steps ih =>
      cases remaining with
      | nil =>
          intro permitted charge member
          simp [pullSequence, pull] at member
      | cons obligation remaining =>
          intro permitted charge member
          simp only [pullSequence, pull, List.mem_cons] at member
          rcases member with sameCharge | member
          · subst charge
            exact permitted.1
          · exact ih remaining permitted.2 charge member

/-- One delayed output retains the classified body obligation that supports it. -/
structure DelayedEmission (α : Type) where
  value : α
  obligation : EffectObligation

/-- A finite observation records output order beside its positional effect charges. -/
structure PullObservation (α : Type) where
  emissions : List α
  charges : List EffectCharge

/-- Pair each output with the charge at the same observation position. -/
def PullObservation.pairs {α : Type} (observation : PullObservation α) :
    List (α × EffectCharge) :=
  observation.emissions.zip observation.charges

/-- Observe a finite emission prefix, charging each supporting obligation at creation. -/
def observePullPrefix {α : Type} (work : DelayedWork) (steps : Nat)
    (emissions : List (DelayedEmission α)) : PullObservation α :=
  let taken := emissions.take steps
  { emissions := taken.map DelayedEmission.value
    charges := taken.map fun emission => chargeAtCreation work emission.obligation }

private theorem zip_emission_values_and_creation_charges {α : Type}
    (work : DelayedWork) (emissions : List (DelayedEmission α)) :
    (emissions.map DelayedEmission.value).zip
        (emissions.map fun emission => chargeAtCreation work emission.obligation) =
      emissions.map fun emission =>
        (emission.value, chargeAtCreation work emission.obligation) := by
  induction emissions with
  | nil => rfl
  | cons emission remaining ih => simp [ih]

/-- The observation's positional pairs are exactly its ordered emission prefix. -/
theorem pull_observation_pairs_preserve_emission_order {α : Type}
    (work : DelayedWork) (steps : Nat) (emissions : List (DelayedEmission α)) :
    (observePullPrefix work steps emissions).pairs =
      (emissions.take steps).map fun emission =>
        (emission.value, chargeAtCreation work emission.obligation) := by
  let taken := emissions.take steps
  change (taken.map DelayedEmission.value).zip
      (taken.map fun emission => chargeAtCreation work emission.obligation) =
    taken.map fun emission =>
      (emission.value, chargeAtCreation work emission.obligation)
  exact zip_emission_values_and_creation_charges work taken

/-- An observed pair keeps the producer origin, body site, and required effect. -/
theorem pull_observation_pair_keeps_origin_body_site_and_effect {α : Type}
    (work : DelayedWork) (steps : Nat) (emissions : List (DelayedEmission α))
    (pair : α × EffectCharge)
    (member : pair ∈ (observePullPrefix work steps emissions).pairs) :
    ∃ emission ∈ emissions.take steps,
      pair = (emission.value, chargeAtCreation work emission.obligation) ∧
        pair.2.lexicalSite = work.creation ∧
        pair.2.causalBodySite = emission.obligation.source ∧
        pair.2.required = emission.obligation.required := by
  rw [pull_observation_pairs_preserve_emission_order] at member
  rcases List.mem_map.mp member with ⟨emission, ⟨inPrefix, samePair⟩⟩
  subst pair
  exact ⟨emission, inPrefix, rfl,
    charge_lexical_site_is_creation work emission.obligation, rfl, rfl⟩

/-- A charge is permitted at creation only when its body obligation is explicitly aligned. -/
theorem pull_observation_pair_is_permitted_when_aligned {α : Type}
    (context : LexicalContext) (work : DelayedWork) (steps : Nat)
    (emissions : List (DelayedEmission α)) (pair : α × EffectCharge)
    (creationPermitted : permitsCreation context work)
    (obligationsAligned : ∀ emission, emission ∈ emissions.take steps →
      emission.obligation ∈ work.obligations)
    (member : pair ∈ (observePullPrefix work steps emissions).pairs) :
    permits context pair.2.required := by
  obtain ⟨emission, inPrefix, samePair, _, _, _⟩ :=
    pull_observation_pair_keeps_origin_body_site_and_effect
      work steps emissions pair member
  rw [samePair]
  exact creation_permission_pays_body_obligation context work emission.obligation
    (obligationsAligned emission inPrefix) creationPermitted

/-- With explicit obligation alignment, observation charges match finite pulls. -/
theorem pull_observation_charges_match_pull_sequence {α : Type}
    (work : DelayedWork) (steps : Nat) (remaining : List EffectObligation)
    (emissions : List (DelayedEmission α))
    (obligationsAligned : (emissions.take steps).map DelayedEmission.obligation =
      remaining.take steps) :
    (observePullPrefix work steps emissions).charges =
      (pullSequence steps ⟨work, remaining⟩).1 := by
  change (emissions.take steps).map
      (fun emission => chargeAtCreation work emission.obligation) =
    (pullSequence steps ⟨work, remaining⟩).1
  rw [pull_sequence_preserves_creation_and_body_order]
  calc
    (emissions.take steps).map
        (fun emission => chargeAtCreation work emission.obligation) =
      ((emissions.take steps).map DelayedEmission.obligation).map
        (chargeAtCreation work) := by
      rw [List.map_map]
      rw [Function.comp_def]
    _ = (remaining.take steps).map (chargeAtCreation work) := by
      rw [obligationsAligned]

/-- A consumer requirement is discharged by its own checked context. -/
theorem checked_consumer_obligation_is_permitted_at_its_site
    (consumer : CheckedConsumer) (obligation : EffectObligation)
    (member : obligation ∈ consumer.obligations) :
    permits consumer.context obligation.required :=
  permits_obligation_at_member consumer.context consumer.obligations
    obligation member consumer.permitted

private def streamName : RawName := ⟨⟨1, by decide⟩⟩
private def requestName : RawName := ⟨⟨2, by decide⟩⟩
private def callbackName : RawName := ⟨⟨3, by decide⟩⟩

def exampleStreamCreation : Expr := .call (.ident streamName) []
def exampleNetworkBody : Expr := .call (.ident requestName) []
def exampleCallbackCreation : Expr :=
  .builderCall (.call (.ident callbackName) []) [.expression exampleNetworkBody]

def exampleNetworkObligation : EffectObligation :=
  ⟨exampleNetworkBody, .net⟩

def exampleLaterNetworkBody : Expr := .call (.ident callbackName) []

def exampleLaterNetworkObligation : EffectObligation :=
  ⟨exampleLaterNetworkBody, .net⟩

def exampleStream : DelayedWork := {
  kind := .streamProducer
  creation := exampleStreamCreation
  captures := fun _ => none
  obligations := [exampleNetworkObligation]
}

def exampleTwoSiteStream : DelayedWork := {
  exampleStream with
  obligations := [exampleNetworkObligation, exampleLaterNetworkObligation]
}

def exampleNetworkEmission : DelayedEmission String :=
  ⟨"first", exampleNetworkObligation⟩

def exampleLaterNetworkEmission : DelayedEmission String :=
  ⟨"second", exampleLaterNetworkObligation⟩

def exampleTwoEmissionObservation : PullObservation String :=
  observePullPrefix exampleTwoSiteStream 2
    [exampleNetworkEmission, exampleLaterNetworkEmission]

def exampleCallback : DelayedWork := {
  kind := .callback
  creation := exampleCallbackCreation
  captures := fun _ => none
  obligations := [exampleNetworkObligation]
}

def exampleOutsideContext : LexicalContext := ⟨[.net], []⟩
def exampleInsideWithoutNet : LexicalContext := ⟨[.net], [.net]⟩

def exampleConsumerContext : LexicalContext := ⟨[.fs, .net], [.net]⟩
def exampleConsumerOperation : Expr := .call (.ident callbackName) []
def exampleConsumerObligation : EffectObligation := ⟨exampleConsumerOperation, .fs⟩
def exampleConsumerNetworkObligation : EffectObligation :=
  ⟨exampleConsumerOperation, .net⟩

def exampleConsumerWithLocalObligation : CheckedConsumer := {
  context := exampleConsumerContext
  obligations := [exampleConsumerObligation]
  permitted := by
    simp [permitsObligations, permits, clauseCovers, ioCovers, blockedBy,
      withoutBlocks, exampleConsumerContext, exampleConsumerObligation,
      exampleConsumerOperation]
}

def exampleStructurallyDistinctConsumerChargeObservation : PullObservation String := {
  exampleTwoEmissionObservation with
  charges := [chargeAtUse exampleConsumerNetworkObligation,
    chargeAtUse exampleConsumerNetworkObligation]
}

def exampleOmittedChargeObservation : PullObservation String := {
  exampleTwoEmissionObservation with
  charges := [chargeAtCreation exampleTwoSiteStream exampleNetworkObligation]
}

def exampleReversedEmissionObservation : PullObservation String := {
  exampleTwoEmissionObservation with
  emissions := ["second", "first"]
}

theorem example_finite_pull_pairs_ordered_emissions_with_creation_charges :
    exampleTwoEmissionObservation = {
      emissions := ["first", "second"]
      charges := [chargeAtCreation exampleTwoSiteStream exampleNetworkObligation,
        chargeAtCreation exampleTwoSiteStream exampleLaterNetworkObligation]
    } := by
  rfl

theorem example_observation_charges_match_the_finite_pull :
    exampleTwoEmissionObservation.charges =
      (pullSequence 2 ⟨exampleTwoSiteStream, exampleTwoSiteStream.obligations⟩).1 := by
  exact pull_observation_charges_match_pull_sequence exampleTwoSiteStream 2
    exampleTwoSiteStream.obligations
    [exampleNetworkEmission, exampleLaterNetworkEmission] rfl

theorem example_structurally_distinct_consumer_charge_is_detected :
    exampleStructurallyDistinctConsumerChargeObservation ≠ exampleTwoEmissionObservation := by
  intro same
  have sites := congrArg
    (fun observation : PullObservation String =>
      (observation.charges.map EffectCharge.lexicalSite)) same
  simp [exampleStructurallyDistinctConsumerChargeObservation,
    exampleTwoEmissionObservation,
    observePullPrefix, chargeAtUse, chargeAtCreation, exampleTwoSiteStream,
    exampleStream, exampleNetworkObligation, exampleLaterNetworkObligation,
    exampleNetworkEmission, exampleLaterNetworkEmission, exampleStreamCreation,
    exampleNetworkBody, exampleLaterNetworkBody, exampleConsumerNetworkObligation,
    exampleConsumerOperation, streamName, requestName, callbackName] at sites

theorem example_omitted_charge_is_detected :
    exampleOmittedChargeObservation ≠ exampleTwoEmissionObservation := by
  intro same
  have counts := congrArg
    (fun observation : PullObservation String => observation.charges.length) same
  simp [exampleOmittedChargeObservation, exampleTwoEmissionObservation,
    observePullPrefix, chargeAtCreation, exampleTwoSiteStream, exampleStream,
    exampleNetworkObligation, exampleLaterNetworkObligation,
    exampleNetworkEmission, exampleLaterNetworkEmission] at counts

theorem example_reversed_distinct_emissions_are_detected :
    exampleReversedEmissionObservation ≠ exampleTwoEmissionObservation := by
  intro same
  have outputs := congrArg PullObservation.emissions same
  simp [exampleReversedEmissionObservation, exampleTwoEmissionObservation,
    observePullPrefix, exampleNetworkEmission, exampleLaterNetworkEmission] at outputs

theorem example_consumer_site_effects_are_independent_of_the_pull_charge :
    permitsCreation exampleOutsideContext exampleTwoSiteStream ∧
      permitsObligations exampleConsumerWithLocalObligation.context
        exampleConsumerWithLocalObligation.obligations ∧
      ¬ permits exampleConsumerWithLocalObligation.context .net ∧
      exampleTwoEmissionObservation.charges.map EffectCharge.required = [.net, .net] := by
  constructor
  · simp [permitsCreation, exampleTwoSiteStream, exampleStream,
      exampleOutsideContext, permitsObligations, permits, clauseCovers,
      ioCovers, blockedBy, exampleNetworkObligation,
      exampleLaterNetworkObligation]
  constructor
  · exact exampleConsumerWithLocalObligation.permitted
  constructor
  · simp [permits, clauseCovers, ioCovers, blockedBy, withoutBlocks,
      exampleConsumerWithLocalObligation, exampleConsumerContext]
  · rfl

def exampleRestrictedConsumer : CheckedConsumer := {
  context := exampleInsideWithoutNet
  obligations := []
  permitted := by simp [permitsObligations]
}

theorem stream_created_outside_can_be_consumed_inside_without_net :
    permitsCreation exampleOutsideContext exampleStream ∧
      permitsObligations exampleInsideWithoutNet [] ∧
      ¬ permitsCreation exampleInsideWithoutNet exampleStream := by
  constructor
  · simp [permitsCreation, exampleStream, exampleOutsideContext,
      permitsObligations, permits, clauseCovers, ioCovers, blockedBy,
      exampleNetworkObligation]
  constructor
  · simp [permitsObligations]
  · simp [permitsCreation, exampleStream, exampleInsideWithoutNet,
      permitsObligations, permits, clauseCovers, ioCovers, blockedBy, withoutBlocks,
      exampleNetworkObligation]

theorem example_stream_charge_names_creation_and_body :
    chargeAtCreation exampleStream exampleNetworkObligation =
      ⟨exampleStreamCreation, exampleNetworkBody, .net⟩ := by
  rfl

example : pullSequence 1 ⟨exampleTwoSiteStream, exampleTwoSiteStream.obligations⟩ =
    ([chargeAtCreation exampleTwoSiteStream exampleNetworkObligation],
      ⟨exampleTwoSiteStream, [exampleLaterNetworkObligation]⟩) := by
  rfl

example : pullSequence 2 ⟨exampleTwoSiteStream, exampleTwoSiteStream.obligations⟩ =
    ([chargeAtCreation exampleTwoSiteStream exampleNetworkObligation,
        chargeAtCreation exampleTwoSiteStream exampleLaterNetworkObligation],
      ⟨exampleTwoSiteStream, []⟩) := by
  rfl

theorem callback_created_inside_without_net_is_rejected :
    ¬ permitsCreation exampleInsideWithoutNet exampleCallback := by
  simp [permitsCreation, exampleCallback, exampleInsideWithoutNet,
    permitsObligations, permits, clauseCovers, ioCovers, blockedBy, withoutBlocks,
    exampleNetworkObligation]

example : checkedConsumption exampleRestrictedConsumer (suspend exampleStream) =
      some (chargeAtCreation exampleStream exampleNetworkObligation,
        ⟨exampleStream, []⟩) := by
  rfl

theorem io_clause_covers_filesystem_requirement :
    clauseCovers [.io] .fs := by
  simp [clauseCovers, ioCovers]

theorem without_filesystem_blocks_io_requirement :
    ¬ permits (enterWithout ⟨[.io], []⟩ [.fs]) .io := by
  apply without_rejects_a_blocked_requirement
    (context := ⟨[.io], []⟩) (excluded := [.fs]) (bound := .fs) (required := .io)
  · simp
  · simp [withoutBlocks]

theorem without_io_does_not_block_filesystem_requirement :
    permits (enterWithout ⟨[.io], []⟩ [.io]) .fs := by
  simp [permits, enterWithout, clauseCovers, ioCovers, blockedBy, withoutBlocks]

end Xsh.Semantics.Effects
