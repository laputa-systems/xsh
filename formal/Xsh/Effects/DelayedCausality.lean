import Xsh.Effects.Lexical

namespace Xsh.Semantics.Effects.DelayedCausality

open Xsh.Syntax
open Xsh.Semantics.Environments

inductive HostAction where
  | filesystemRead (resource : Nat)
  | networkRequest (request : Nat)
  | processStart (command : Nat)
  | environmentRead (key : Nat)
  deriving DecidableEq, Repr

def HostAction.requiredEffect : HostAction → Effect
  | .filesystemRead _ => .fs
  | .networkRequest _ => .net
  | .processStart _ => .process
  | .environmentRead _ => .env

structure ProducerInstruction where
  source : Expr
  value : Nat
  action : HostAction

def ProducerInstruction.obligation (instruction : ProducerInstruction) :
    EffectObligation :=
  ⟨instruction.source, instruction.action.requiredEffect⟩

def producerWork (creationSite : Expr) (captures : CapturedEnvironment)
    (body : List ProducerInstruction) : DelayedWork :=
  { kind := .streamProducer
    creation := creationSite
    captures := captures
    obligations := body.map ProducerInstruction.obligation }

structure CheckedProducer where
  context : LexicalContext
  creationSite : Expr
  captures : CapturedEnvironment
  body : List ProducerInstruction
  accepted : permitsCreation context (producerWork creationSite captures body)

def CheckedProducer.work (producer : CheckedProducer) : DelayedWork :=
  producerWork producer.creationSite producer.captures producer.body

structure CallbackOperation where
  source : Expr
  action : HostAction

def CallbackOperation.obligation (operation : CallbackOperation) :
    EffectObligation :=
  ⟨operation.source, operation.action.requiredEffect⟩

structure CheckedConsumer where
  context : LexicalContext
  invocationSite : Expr
  callback : List CallbackOperation
  accepted : permitsObligations context
    (callback.map CallbackOperation.obligation)

structure EffectObservation where
  event : HostAction × Expr
  charge : EffectCharge

def observeProducerInstruction (producer : CheckedProducer)
    (instruction : ProducerInstruction) : EffectObservation :=
  { event := (instruction.action, instruction.source)
    charge := chargeAtCreation producer.work instruction.obligation }

def observeCallbackOperation (operation : CallbackOperation) : EffectObservation :=
  { event := (operation.action, operation.source)
    charge := chargeAtUse operation.obligation }

structure PullObservation where
  value : Nat
  operations : List EffectObservation

def PullObservation.actions (observation : PullObservation) : List HostAction :=
  observation.operations.map fun operation => operation.event.1

def observePull (consumer : CheckedConsumer) (producer : CheckedProducer)
    (instruction : ProducerInstruction) : PullObservation :=
  { value := instruction.value
    operations := observeProducerInstruction producer instruction ::
      consumer.callback.map observeCallbackOperation }

structure SuspendedProducer where
  producer : CheckedProducer
  consumed : List ProducerInstruction
  pending : List ProducerInstruction
  decomposition : producer.body = consumed ++ pending

def suspend (producer : CheckedProducer) : SuspendedProducer :=
  { producer := producer
    consumed := []
    pending := producer.body
    decomposition := by simp }

def pull (consumer : CheckedConsumer) (suspended : SuspendedProducer) :
    Option (PullObservation × SuspendedProducer) :=
  match suspended with
  | ⟨producer, consumed, [], _decomposition⟩ => none
  | ⟨producer, consumed, instruction :: remaining, decomposition⟩ =>
      some (observePull consumer producer instruction,
        { producer := producer
          consumed := consumed ++ [instruction]
          pending := remaining
          decomposition := by
            calc
              producer.body = consumed ++ instruction :: remaining := decomposition
              _ = (consumed ++ [instruction]) ++ remaining := by
                simp [List.append_assoc] })

def runPulls : Nat → CheckedConsumer → SuspendedProducer →
    List PullObservation × SuspendedProducer
  | 0, _, suspended => ([], suspended)
  | fuel + 1, consumer, suspended =>
      match pull consumer suspended with
      | none => ([], suspended)
      | some (observation, next) =>
          let (observations, final) := runPulls fuel consumer next
          (observation :: observations, final)

/-- Finite execution preserves the exact order of the pending instruction prefix. -/
theorem runPulls_observes_pending_prefix (fuel : Nat) (consumer : CheckedConsumer)
    (suspended : SuspendedProducer) :
    (runPulls fuel consumer suspended).1 =
      (suspended.pending.take fuel).map (observePull consumer suspended.producer) := by
  induction fuel generalizing suspended with
  | zero => rfl
  | succ fuel ih =>
      cases suspended with
      | mk producer consumed pending decomposition =>
          cases pending with
          | nil => simp [runPulls, pull]
          | cons instruction remaining =>
              simp [runPulls, pull, ih]

/-- Every finite execution keeps producer events ahead of callback events per item. -/
theorem runPulls_preserves_observation_order (fuel : Nat)
    (consumer : CheckedConsumer) (suspended : SuspendedProducer) :
    ((runPulls fuel consumer suspended).1.map PullObservation.actions) =
      (suspended.pending.take fuel).map
        (fun instruction => (observePull consumer suspended.producer instruction).actions) := by
  rw [runPulls_observes_pending_prefix]
  simp [List.map_map, Function.comp_def]

/-- A producer instruction's host event names its body site and its creation charge. -/
theorem producer_observation_keeps_creation_and_body_sites
    (producer : CheckedProducer) (instruction : ProducerInstruction) :
    let observation := observeProducerInstruction producer instruction
    observation.event = (instruction.action, instruction.source) ∧
      observation.charge.lexicalSite = producer.creationSite ∧
      observation.charge.causalBodySite = instruction.source ∧
      observation.charge.required = instruction.action.requiredEffect := by
  simp [observeProducerInstruction, CheckedProducer.work, producerWork,
    ProducerInstruction.obligation, HostAction.requiredEffect, chargeAtCreation]

/-- Each callback host event keeps its own lexical operation site as its charge. -/
theorem callback_observation_keeps_its_lexical_site (operation : CallbackOperation) :
    let observation := observeCallbackOperation operation
    observation.event = (operation.action, operation.source) ∧
      observation.charge.lexicalSite = operation.source ∧
      observation.charge.required = operation.action.requiredEffect := by
  simp [observeCallbackOperation, CallbackOperation.obligation,
    HostAction.requiredEffect, chargeAtUse]

/-- Every instruction in an accepted producer body is permitted at creation. -/
theorem checked_producer_pays_each_body_instruction
    (producer : CheckedProducer) (instruction : ProducerInstruction)
    (member : instruction ∈ producer.body) :
    permits producer.context instruction.action.requiredEffect := by
  have obligationMember : instruction.obligation ∈ producer.work.obligations := by
    change instruction.obligation ∈
      producer.body.map ProducerInstruction.obligation
    exact List.mem_map.mpr ⟨instruction, member, rfl⟩
  exact permits_obligation_at_member producer.context producer.work.obligations
    instruction.obligation obligationMember producer.accepted

/-- Each checked callback operation is permitted in the consumer's lexical context. -/
theorem checked_consumer_pays_each_callback_operation
    (consumer : CheckedConsumer) (operation : CallbackOperation)
    (member : operation ∈ consumer.callback) :
    permits consumer.context operation.action.requiredEffect := by
  have obligationMember : operation.obligation ∈
      consumer.callback.map CallbackOperation.obligation :=
    List.mem_map.mpr ⟨operation, member, rfl⟩
  exact permits_obligation_at_member consumer.context
    (consumer.callback.map CallbackOperation.obligation) operation.obligation
    obligationMember consumer.accepted

/-- A pending operation remains one of the creation-checked body instructions. -/
theorem suspended_pending_instruction_belongs_to_body
    (suspended : SuspendedProducer) (instruction : ProducerInstruction)
    (member : instruction ∈ suspended.pending) :
    instruction ∈ suspended.producer.body := by
  rw [suspended.decomposition]
  exact List.mem_append_right suspended.consumed member

/-- Suspension preserves the creation-context permission for every pending action. -/
theorem suspended_pending_instruction_is_permitted_at_creation
    (suspended : SuspendedProducer) (instruction : ProducerInstruction)
    (member : instruction ∈ suspended.pending) :
    permits suspended.producer.context instruction.action.requiredEffect := by
  exact checked_producer_pays_each_body_instruction suspended.producer instruction
    (suspended_pending_instruction_belongs_to_body suspended instruction member)

private def name (value : Nat) (bound : value < 4294967296) : RawName :=
  ⟨⟨value, bound⟩⟩

def creationSite : Expr := .call (.ident (name 1 (by decide))) []
def producerBodySite : Expr := .call (.ident (name 2 (by decide))) []
def consumerInvocationSite : Expr := .call (.ident (name 3 (by decide))) []
def callbackBodySite : Expr := .call (.ident (name 4 (by decide))) []

def outsideContext : LexicalContext := ⟨[.io], []⟩
def withoutNetContext : LexicalContext := ⟨[.io], [.net]⟩

def networkInstruction : ProducerInstruction :=
  { source := producerBodySite
    value := 17
    action := .networkRequest 29 }

def filesystemCallback : CallbackOperation :=
  { source := callbackBodySite
    action := .filesystemRead 31 }

def producer : CheckedProducer :=
  { context := outsideContext
    creationSite := creationSite
    captures := fun _ => none
    body := [networkInstruction]
    accepted := by
      simp [permitsCreation, producerWork, networkInstruction,
        ProducerInstruction.obligation, HostAction.requiredEffect,
        outsideContext, permitsObligations, permits,
        clauseCovers, ioCovers, blockedBy] }

def consumer : CheckedConsumer :=
  { context := withoutNetContext
    invocationSite := consumerInvocationSite
    callback := [filesystemCallback]
    accepted := by
      simp [filesystemCallback, CallbackOperation.obligation,
        HostAction.requiredEffect, permitsObligations, permits,
        withoutNetContext, clauseCovers, ioCovers, blockedBy, withoutBlocks] }

theorem accepted_outside_creation_and_restricted_callback :
    permitsCreation producer.context producer.work ∧
      permitsObligations consumer.context
        (consumer.callback.map CallbackOperation.obligation) :=
  ⟨producer.accepted, consumer.accepted⟩

theorem pull_emits_both_causal_host_observations :
    (pull consumer (suspend producer)).map
      (fun result => result.1.actions) =
      some [.networkRequest 29, .filesystemRead 31] := by
  simp [pull, suspend, observePull, observeProducerInstruction,
    observeCallbackOperation, networkInstruction, filesystemCallback,
    producer, consumer, producerWork, CheckedProducer.work,
    ProducerInstruction.obligation, CallbackOperation.obligation,
    PullObservation.actions, producerBodySite, creationSite,
    callbackBodySite, consumerInvocationSite, outsideContext, withoutNetContext,
    HostAction.requiredEffect, chargeAtCreation, chargeAtUse]

theorem pull_keeps_distinct_lexical_charges :
    (pull consumer (suspend producer)).map
      (fun result => result.1.operations.map fun operation =>
        (operation.charge.lexicalSite, operation.charge.required)) =
      some [(creationSite, .net), (callbackBodySite, .fs)] := by
  simp [pull, suspend, observePull, observeProducerInstruction,
    observeCallbackOperation, networkInstruction, filesystemCallback,
    producer, consumer, producerWork, CheckedProducer.work,
    ProducerInstruction.obligation, CallbackOperation.obligation,
    producerBodySite, creationSite,
    callbackBodySite, consumerInvocationSite, outsideContext, withoutNetContext,
    HostAction.requiredEffect, chargeAtCreation, chargeAtUse]

def producerChargeAtConsumer (consumer : CheckedConsumer)
    (producer : CheckedProducer) (instruction : ProducerInstruction) : EffectCharge :=
  { chargeAtCreation producer.work instruction.obligation with
    lexicalSite := consumer.invocationSite }

theorem consumer_recharge_mutation_is_rejected :
    (chargeAtCreation producer.work networkInstruction.obligation).lexicalSite =
        creationSite ∧
      (producerChargeAtConsumer consumer producer networkInstruction).lexicalSite =
        consumerInvocationSite ∧
      permits producer.context .net ∧
      ¬ permits consumer.context .net := by
  simp [producerChargeAtConsumer, producer, consumer, producerWork,
    CheckedProducer.work, ProducerInstruction.obligation,
    networkInstruction, outsideContext, withoutNetContext, creationSite,
    consumerInvocationSite, permits, clauseCovers, ioCovers, blockedBy,
    withoutBlocks, chargeAtCreation, HostAction.requiredEffect]

def dropProducerObservation (observation : PullObservation) : PullObservation :=
  { observation with operations := observation.operations.drop 1 }

theorem dropped_producer_observation_changes_the_pull_trace :
    (observePull consumer producer networkInstruction).actions =
        [.networkRequest 29, .filesystemRead 31] ∧
      (dropProducerObservation
        (observePull consumer producer networkInstruction)).actions =
        [.filesystemRead 31] := by
  simp [dropProducerObservation, observePull, observeProducerInstruction,
    observeCallbackOperation, producer, consumer, producerWork,
    CheckedProducer.work, ProducerInstruction.obligation,
    CallbackOperation.obligation, PullObservation.actions,
    networkInstruction, filesystemCallback, producerBodySite, creationSite,
    callbackBodySite, consumerInvocationSite, outsideContext, withoutNetContext,
    HostAction.requiredEffect, chargeAtCreation, chargeAtUse]

end Xsh.Semantics.Effects.DelayedCausality
