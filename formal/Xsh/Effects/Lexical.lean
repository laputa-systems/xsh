import Xsh.Syntax.Core
import Xsh.Semantics.Environments

/-!
An abstract static model for lexical effect bounds and delayed obligations.

The source expression fields identify already-classified lexical sites in
`Xsh.Syntax`; this module does not classify Rust operations or prove parser,
checker, runtime, or lowering conformance. A delayed item retains its origin
and captured cells while suspended, but stores no effect permission. This is
not a runtime sandbox or a consumer-side effect capability.
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

def exampleCallback : DelayedWork := {
  kind := .callback
  creation := exampleCallbackCreation
  captures := fun _ => none
  obligations := [exampleNetworkObligation]
}

def exampleOutsideContext : LexicalContext := ⟨[.net], []⟩
def exampleInsideWithoutNet : LexicalContext := ⟨[.net], [.net]⟩

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
