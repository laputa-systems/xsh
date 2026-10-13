import Xsh.Effects.Lexical

/-!
A deliberately closed source/checker fragment for producer creation and
suspended stream provenance. Calls carry the lexical context assigned to their
source site by the checker. The fragment checks producer summaries at that
call, resolves aliases to already checked handles, checks every conditional
arm, and records delegation ancestry. It does not classify arbitrary XSH
syntax or connect these facts to the Rust checker or stream machine.
-/

namespace Xsh.Semantics.Effects

open Xsh.Syntax
open Xsh.Foundation
open Xsh.Semantics.Environments

/-- A declaration identity for the closed producer table. -/
structure ProducerId where
  value : Nat
  deriving DecidableEq, Repr

/-- A cursor identity that remains stable when a checked stream is aliased. -/
structure CursorId where
  value : Nat
  deriving DecidableEq, Repr

/-- A source-classified host operation in a producer body. -/
abbrev HostOp := EffectObligation

/-- The only producer declarations represented by this source fragment. -/
structure ProducerDecl where
  captures : CapturedEnvironment
  body : List HostOp

/-- Materialize a declaration's latent body facts at its lexical call site. -/
def ProducerDecl.workAt (declaration : ProducerDecl) (callSite : Expr) : DelayedWork :=
  { kind := .streamProducer
    creation := callSite
    captures := declaration.captures
    obligations := declaration.body }

/-- Every producer call retains the lexical context of that particular source site. -/
inductive EffectSource where
  | producerCall (context : LexicalContext) (producer : ProducerId)
      (cursor : CursorId) (callSite : Expr)
  | alias (binding : BinderId)
  | conditional (condition : Expr) (whenTrue whenFalse : EffectSource)
  | yieldDelegate (parent child : EffectSource)
  | unknown

/-- Which feasible arm contributed a checked producer alternative. -/
inductive AlternativeArm where
  | whenTrue
  | whenFalse
  deriving DecidableEq, Repr

structure AlternativeChoice where
  condition : Expr
  arm : AlternativeArm

/-- A checked source handle is a static derivation, not a consumer permission. -/
structure CheckedProducer where
  producer : ProducerId
  cursor : CursorId
  creationContext : LexicalContext
  work : DelayedWork
  provenanceSites : List Expr
  alternatives : List AlternativeChoice
  creationPermission : permitsCreation creationContext work

abbrev ProducerTable := ProducerId → Option ProducerDecl
abbrev CheckedAliases := BinderId → Option (List CheckedProducer)

/-- Boolean membership keeps the closed checker executable on finite clauses. -/
def effectMemberBool (required : Effect) : EffectSet → Bool
  | [] => false
  | head :: rest => decide (required = head) || effectMemberBool required rest

theorem effectMemberBool_correct (required : Effect) (effects : EffectSet) :
    effectMemberBool required effects = true ↔ required ∈ effects := by
  induction effects with
  | nil => simp [effectMemberBool]
  | cons head rest ih =>
      simp [effectMemberBool, ih, List.mem_cons]

def ioCoversBool : Effect → Bool
  | .fs | .net | .process | .env => true
  | _ => false

theorem ioCoversBool_correct (required : Effect) :
    ioCoversBool required = true ↔ ioCovers required := by
  cases required <;> simp [ioCoversBool, ioCovers]

def withoutBlocksBool (excluded required : Effect) : Bool :=
  (decide (excluded ≠ .error) && decide (excluded = required)) ||
    (decide (required = .io) &&
      (decide (excluded = .fs) || decide (excluded = .net) ||
        decide (excluded = .process) || decide (excluded = .env)))

theorem withoutBlocksBool_correct (excluded required : Effect) :
    withoutBlocksBool excluded required = true ↔ withoutBlocks excluded required := by
  cases excluded <;> cases required <;>
    simp [withoutBlocksBool, withoutBlocks]

def blockedByBool : EffectSet → Effect → Bool
  | [], _ => false
  | excluded :: rest, required =>
      withoutBlocksBool excluded required || blockedByBool rest required

theorem blockedByBool_correct (excluded : EffectSet) (required : Effect) :
    blockedByBool excluded required = true ↔ blockedBy excluded required := by
  induction excluded with
  | nil => simp [blockedByBool, blockedBy]
  | cons head rest ih =>
      simp [blockedByBool, blockedBy, withoutBlocksBool_correct, ih]

private theorem blockedByBool_false_iff (excluded : EffectSet) (required : Effect) :
    blockedByBool excluded required = false ↔ ¬ blockedBy excluded required := by
  cases result : blockedByBool excluded required with
  | false =>
      constructor
      · intro _ blocked
        have trueResult : blockedByBool excluded required = true :=
          (blockedByBool_correct excluded required).mpr blocked
        rw [result] at trueResult
        cases trueResult
      · intro _
        rfl
  | true =>
      have blocked : blockedBy excluded required :=
        (blockedByBool_correct excluded required).mp (by simp [result])
      constructor
      · intro impossible
        cases impossible
      · intro notBlocked
        exact False.elim (notBlocked blocked)

def clauseCoversBool (clause : EffectSet) (required : Effect) : Bool :=
  effectMemberBool required clause ||
    (effectMemberBool .io clause && ioCoversBool required)

theorem clauseCoversBool_correct (clause : EffectSet) (required : Effect) :
    clauseCoversBool clause required = true ↔ clauseCovers clause required := by
  simp [clauseCoversBool, clauseCovers, effectMemberBool_correct, ioCoversBool_correct]

def permitsBool (context : LexicalContext) (required : Effect) : Bool :=
  clauseCoversBool context.clause required &&
    decide (blockedByBool context.excluded required = false)

theorem permitsBool_correct (context : LexicalContext) (required : Effect) :
    permitsBool context required = true ↔ permits context required := by
  simp [permitsBool, permits, clauseCoversBool_correct, blockedByBool_false_iff]

def permitsObligationsBool (context : LexicalContext) : List EffectObligation → Bool
  | [] => true
  | obligation :: rest =>
      permitsBool context obligation.required && permitsObligationsBool context rest

theorem permitsObligationsBool_correct (context : LexicalContext)
    (obligations : List EffectObligation) :
    permitsObligationsBool context obligations = true ↔
      permitsObligations context obligations := by
  induction obligations with
  | nil => simp [permitsObligationsBool, permitsObligations]
  | cons obligation rest ih =>
      simp [permitsObligationsBool, permitsObligations, permitsBool_correct, ih]

def permitsCreationBool (context : LexicalContext) (work : DelayedWork) : Bool :=
  permitsObligationsBool context work.obligations

theorem permitsCreationBool_correct (context : LexicalContext) (work : DelayedWork) :
    permitsCreationBool context work = true ↔ permitsCreation context work := by
  exact permitsObligationsBool_correct context work.obligations

/-- Check one producer call using only its declared, classified body summary. -/
def checkProducerCall (context : LexicalContext) (declarations : ProducerTable)
    (producer : ProducerId) (cursor : CursorId) (callSite : Expr) :
    Option CheckedProducer :=
  match declarations producer with
  | none => none
  | some declaration =>
      let work := declaration.workAt callSite
      if permitted : permitsCreationBool context work = true then
        some {
          producer := producer
          cursor := cursor
          creationContext := context
          work := work
          provenanceSites := [callSite]
          alternatives := []
          creationPermission := permitsCreationBool_correct context work |>.mp permitted
        }
      else none

/-- Add one explicit branch choice while retaining the producer's own origin. -/
def CheckedProducer.withAlternative (producer : CheckedProducer)
    (condition : Expr) (arm : AlternativeArm) : CheckedProducer :=
  { producer with alternatives := ⟨condition, arm⟩ :: producer.alternatives }

/-- Check only the closed call, alias, conditional, and delegation forms above.
Unknown producers and missing aliases fail closed. -/
def checkEffectSource (declarations : ProducerTable) (aliases : CheckedAliases) :
    EffectSource → Option (List CheckedProducer)
  | .producerCall context producer cursor callSite =>
      (checkProducerCall context declarations producer cursor callSite).map
        (fun handle => [handle])
  | .alias binding => aliases binding
  | .conditional condition whenTrue whenFalse => do
      let trueProducers ← checkEffectSource declarations aliases whenTrue
      let falseProducers ← checkEffectSource declarations aliases whenFalse
      pure
        (trueProducers.map (·.withAlternative condition .whenTrue) ++
          falseProducers.map (·.withAlternative condition .whenFalse))
  | .yieldDelegate parent child => do
      let parents ← checkEffectSource declarations aliases parent
      let children ← checkEffectSource declarations aliases child
      pure <| parents.flatMap fun parent =>
        children.map fun child =>
          { child with
            provenanceSites := parent.provenanceSites ++ child.provenanceSites
            alternatives := parent.alternatives ++ child.alternatives }
  | .unknown => none

/-- Publicly observable checker facts, omitting proof terms. -/
structure ProducerObservation where
  producer : ProducerId
  cursor : CursorId
  creationContext : LexicalContext
  creationSite : Expr
  provenanceSites : List Expr
  causalPaths : List (List Expr)
  alternatives : List AlternativeChoice
  captures : CapturedEnvironment
  body : List HostOp

/-- Project a checked handle to the source and provenance facts it preserves. -/
def CheckedProducer.causalPaths (producer : CheckedProducer) : List (List Expr) :=
  producer.work.obligations.map fun obligation =>
    producer.provenanceSites ++ [obligation.source]

/-- Project a checked handle to the source and provenance facts it preserves. -/
def observeProducer (producer : CheckedProducer) : ProducerObservation :=
  { producer := producer.producer
    cursor := producer.cursor
    creationContext := producer.creationContext
    creationSite := producer.work.creation
    provenanceSites := producer.provenanceSites
    causalPaths := producer.causalPaths
    alternatives := producer.alternatives
    captures := producer.work.captures
    body := producer.work.obligations }

/-- The checked alternatives of a source expression, with no fabricated default. -/
def checkedObservations (declarations : ProducerTable) (aliases : CheckedAliases)
    (source : EffectSource) : List ProducerObservation :=
  ((checkEffectSource declarations aliases source).getD []).map observeProducer

theorem checkProducerCall_success_implies_creation_permission
    (context : LexicalContext) (declarations : ProducerTable)
    (producer : ProducerId) (cursor : CursorId) (callSite : Expr)
    (checked : CheckedProducer)
    (success : checkProducerCall context declarations producer cursor callSite =
      some checked) :
    permitsCreation checked.creationContext checked.work := by
  unfold checkProducerCall at success
  cases declarationResult : declarations producer with
  | none => simp [declarationResult] at success
  | some declaration =>
      simp only [declarationResult] at success
      split at success
      · rename_i permitted
        cases success
        simpa using (permitsCreationBool_correct context
          (declaration.workAt callSite)).mp permitted
      · simp at success

theorem checkProducerCall_success_records_its_lexical_site
    (context : LexicalContext) (declarations : ProducerTable)
    (producer : ProducerId) (cursor : CursorId) (callSite : Expr)
    (checked : CheckedProducer)
    (success : checkProducerCall context declarations producer cursor callSite =
      some checked) :
    checked.creationContext = context ∧ checked.work.creation = callSite := by
  unfold checkProducerCall at success
  cases declarationResult : declarations producer with
  | none => simp [declarationResult] at success
  | some declaration =>
      simp only [declarationResult] at success
      split at success
      · cases success
        exact ⟨rfl, rfl⟩
      · simp at success

/-- Every classified body obligation is permitted at a successful call site. -/
theorem checked_producer_body_obligation_is_permitted_at_creation
    (producer : CheckedProducer) (obligation : EffectObligation)
    (member : obligation ∈ producer.work.obligations) :
    permits producer.creationContext
      (chargeAtCreation producer.work obligation).required := by
  exact creation_permission_pays_body_obligation producer.creationContext
    producer.work obligation member producer.creationPermission

private def bridgeOuterProducer : ProducerId := ⟨21⟩
private def bridgeChildProducer : ProducerId := ⟨22⟩
private def bridgeOuterCursor : CursorId := ⟨31⟩
private def bridgeChildCursor : CursorId := ⟨32⟩
private def bridgeAlias : BinderId := ⟨⟨41, by decide⟩⟩
private def bridgeMissingAlias : BinderId := ⟨⟨42, by decide⟩⟩
private def bridgeConditionName : RawName := ⟨⟨51, by decide⟩⟩
private def bridgeDelegateName : RawName := ⟨⟨52, by decide⟩⟩
private def bridgeCondition : Expr := .ident bridgeConditionName
private def bridgeDelegateSite : Expr := .call (.ident bridgeDelegateName) []
private def bridgeChildCallSite : Expr := bridgeDelegateSite

private def bridgeOuterDeclaration : ProducerDecl := {
  captures := exampleStream.captures
  body := [exampleNetworkObligation]
}

private def bridgeChildDeclaration : ProducerDecl := {
  captures := exampleStream.captures
  body := [exampleLaterNetworkObligation]
}

private def bridgeDeclarations : ProducerTable := fun producer =>
  if producer = bridgeOuterProducer then some bridgeOuterDeclaration
  else if producer = bridgeChildProducer then some bridgeChildDeclaration
  else none

private def bridgeOutsideCall : EffectSource :=
  .producerCall exampleOutsideContext bridgeOuterProducer bridgeOuterCursor
    exampleStreamCreation

private def bridgeOutsideChildCall : EffectSource :=
  .producerCall exampleOutsideContext bridgeChildProducer bridgeChildCursor
    bridgeChildCallSite

private def bridgeRestrictedChildCall : EffectSource :=
  .producerCall exampleInsideWithoutNet bridgeChildProducer bridgeChildCursor
    bridgeChildCallSite

theorem outside_creation_accepts_the_classified_producer_fragment :
    (checkedObservations bridgeDeclarations (fun _ => none) bridgeOutsideCall).length = 1 := by
  simp [checkedObservations, checkEffectSource, bridgeOutsideCall,
    checkProducerCall, bridgeDeclarations, bridgeOuterDeclaration,
    ProducerDecl.workAt, permitsCreationBool, permitsObligationsBool,
    permitsBool, clauseCoversBool, effectMemberBool, ioCoversBool,
    blockedByBool, exampleOutsideContext,
    exampleNetworkObligation]

private def bridgeOutsideHandle : CheckedProducer :=
  (checkProducerCall exampleOutsideContext bridgeDeclarations bridgeOuterProducer
    bridgeOuterCursor exampleStreamCreation).get (by
      simp [checkProducerCall, bridgeDeclarations, bridgeOuterDeclaration,
        ProducerDecl.workAt, permitsCreationBool, permitsObligationsBool,
        permitsBool, clauseCoversBool, effectMemberBool, ioCoversBool,
        blockedByBool,
        exampleOutsideContext, exampleNetworkObligation])

private def bridgeAliases : CheckedAliases := fun binding =>
  if binding = bridgeAlias then some [bridgeOutsideHandle] else none

private def bridgeAliasedInsideWithoutNet : EffectSource := .alias bridgeAlias

theorem alias_preserves_the_creation_handle_inside_without
    : checkedObservations bridgeDeclarations bridgeAliases
        bridgeAliasedInsideWithoutNet = [observeProducer bridgeOutsideHandle] := by
  rfl

theorem restricted_consumption_does_not_recharge_a_checked_alias_fragment :
    (checkedConsumption exampleRestrictedConsumer
        (suspend bridgeOutsideHandle.work)).map
        (fun result => (result.1.lexicalSite, result.1.causalBodySite,
          result.1.required)) =
      some (exampleStreamCreation, exampleNetworkBody, .net) := by
  simp [checkedConsumption, suspend, pull, bridgeOutsideHandle,
    checkProducerCall, bridgeDeclarations, bridgeOuterDeclaration,
    ProducerDecl.workAt, permitsCreationBool, permitsObligationsBool,
    permitsBool, clauseCoversBool, effectMemberBool, ioCoversBool,
    blockedByBool, chargeAtCreation, exampleOutsideContext,
    exampleNetworkObligation]

theorem without_net_rejects_a_network_producer_creation_fragment :
    checkProducerCall exampleInsideWithoutNet bridgeDeclarations
      bridgeOuterProducer bridgeOuterCursor exampleStreamCreation = none := by
  simp [checkProducerCall, bridgeDeclarations, bridgeOuterDeclaration,
    ProducerDecl.workAt, permitsCreationBool, permitsObligationsBool,
    permitsBool, clauseCoversBool, effectMemberBool, ioCoversBool,
    blockedByBool, withoutBlocksBool,
    exampleInsideWithoutNet, exampleNetworkObligation]

private def bridgeDelegatedSource : EffectSource :=
  .yieldDelegate bridgeOutsideCall bridgeOutsideChildCall

private def bridgeDelegatedBlockedChild : EffectSource :=
  .yieldDelegate bridgeOutsideCall bridgeRestrictedChildCall

theorem delegated_child_retains_outer_delegate_child_and_body_sites_fragment :
    checkedObservations bridgeDeclarations (fun _ => none) bridgeDelegatedSource =
      [{ producer := bridgeChildProducer
         cursor := bridgeChildCursor
         creationContext := exampleOutsideContext
         creationSite := bridgeChildCallSite
         provenanceSites := [exampleStreamCreation, bridgeDelegateSite]
         causalPaths := [[exampleStreamCreation, bridgeDelegateSite,
           exampleLaterNetworkBody]]
         alternatives := []
         captures := exampleStream.captures
         body := [exampleLaterNetworkObligation] }] := by
  simp [checkedObservations, observeProducer, CheckedProducer.causalPaths,
    checkEffectSource, bridgeDelegatedSource,
    bridgeOutsideCall, bridgeOutsideChildCall, checkProducerCall,
    bridgeDeclarations, bridgeOuterDeclaration, bridgeChildDeclaration,
    ProducerDecl.workAt, permitsCreationBool, permitsObligationsBool,
    permitsBool, clauseCoversBool, effectMemberBool, ioCoversBool,
    blockedByBool,
    bridgeOuterProducer, bridgeChildProducer, bridgeOuterCursor,
    bridgeChildCursor, bridgeChildCallSite, bridgeDelegateSite,
    exampleOutsideContext, exampleNetworkObligation,
    exampleLaterNetworkObligation]

theorem delegated_child_is_checked_at_its_own_call_context_fragment :
    checkEffectSource bridgeDeclarations (fun _ => none)
      bridgeDelegatedBlockedChild = none := by
  simp [checkEffectSource, bridgeDelegatedBlockedChild, bridgeOutsideCall,
    bridgeRestrictedChildCall, checkProducerCall, bridgeDeclarations,
    bridgeOuterDeclaration, bridgeChildDeclaration, ProducerDecl.workAt,
    permitsCreationBool, permitsObligationsBool, permitsBool,
    clauseCoversBool, effectMemberBool, ioCoversBool, blockedByBool,
    withoutBlocksBool, bridgeOuterProducer, bridgeChildProducer,
    bridgeOuterCursor, bridgeChildCursor, bridgeChildCallSite,
    exampleOutsideContext,
    exampleInsideWithoutNet, exampleNetworkObligation,
    exampleLaterNetworkObligation]

private def bridgeConditional : EffectSource :=
  .conditional bridgeCondition bridgeOutsideCall bridgeOutsideChildCall

private def bridgeConditionalBlockedArm : EffectSource :=
  .conditional bridgeCondition bridgeOutsideCall bridgeRestrictedChildCall

theorem conditional_source_retains_both_tagged_producer_alternatives_fragment :
    checkedObservations bridgeDeclarations (fun _ => none) bridgeConditional =
      [{ producer := bridgeOuterProducer
         cursor := bridgeOuterCursor
         creationContext := exampleOutsideContext
         creationSite := exampleStreamCreation
         provenanceSites := [exampleStreamCreation]
         causalPaths := [[exampleStreamCreation, exampleNetworkBody]]
         alternatives := [⟨bridgeCondition, .whenTrue⟩]
         captures := exampleStream.captures
         body := [exampleNetworkObligation] },
       { producer := bridgeChildProducer
         cursor := bridgeChildCursor
         creationContext := exampleOutsideContext
         creationSite := bridgeChildCallSite
         provenanceSites := [bridgeChildCallSite]
         causalPaths := [[bridgeChildCallSite, exampleLaterNetworkBody]]
         alternatives := [⟨bridgeCondition, .whenFalse⟩]
         captures := exampleStream.captures
         body := [exampleLaterNetworkObligation] }] := by
  simp [checkedObservations, observeProducer, CheckedProducer.causalPaths,
    checkEffectSource, bridgeConditional,
    CheckedProducer.withAlternative, bridgeOutsideCall, bridgeOutsideChildCall,
    checkProducerCall,
    bridgeDeclarations, bridgeOuterDeclaration, bridgeChildDeclaration,
    ProducerDecl.workAt, permitsCreationBool, permitsObligationsBool,
    permitsBool, clauseCoversBool, effectMemberBool, ioCoversBool,
    blockedByBool,
    bridgeOuterProducer, bridgeChildProducer, bridgeOuterCursor,
    bridgeChildCursor, bridgeChildCallSite,
    bridgeCondition, exampleOutsideContext, exampleNetworkObligation,
    exampleLaterNetworkObligation]

theorem conditional_blocked_arm_mutation_is_rejected_fragment :
    checkEffectSource bridgeDeclarations (fun _ => none)
      bridgeConditionalBlockedArm = none := by
  simp [checkEffectSource, bridgeConditionalBlockedArm, bridgeOutsideCall,
    bridgeRestrictedChildCall, checkProducerCall, bridgeDeclarations,
    bridgeOuterDeclaration, bridgeChildDeclaration, ProducerDecl.workAt,
    permitsCreationBool, permitsObligationsBool, permitsBool,
    clauseCoversBool, effectMemberBool, ioCoversBool, blockedByBool,
    withoutBlocksBool, bridgeOuterProducer, bridgeChildProducer,
    bridgeOuterCursor, bridgeChildCursor, bridgeChildCallSite,
    bridgeDelegateSite, bridgeCondition, exampleOutsideContext,
    exampleInsideWithoutNet, exampleNetworkObligation,
    exampleLaterNetworkObligation]

theorem missing_alias_binding_mutation_is_rejected_fragment :
    checkEffectSource bridgeDeclarations bridgeAliases
      (.alias bridgeMissingAlias) = none := by
  rfl

theorem unknown_source_fails_closed_fragment :
    checkEffectSource bridgeDeclarations (fun _ => none) .unknown = none := by
  rfl

end Xsh.Semantics.Effects
