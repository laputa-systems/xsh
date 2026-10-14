import Xsh.Effects.ExpressionBridge
import Xsh.Effects.Solver

/-!
A finite bridge from solved producer facts to lexical source checks. Known
summary effects become body-site obligations before the existing source
checker checks producer calls, aliases, conditional arms, and delegation.
Unresolved or missing summaries fail closed. The model takes classified facts
as input and fixes captures to the empty fixture; it does not establish Rust
checker or source-classifier correspondence, stream execution, or runtime
alias behavior.
-/

namespace Xsh.Semantics.Effects.CheckerBridge

open Xsh.Syntax
open Xsh.Semantics.Effects
open Xsh.Semantics.Effects.Solver

/-- Project solver facts to source-labelled obligations, retaining unresolved. -/
def obligationsAtSummary (bodySite : Expr) (summary : FactSet) :
    Option (List EffectObligation) :=
  if summary none then none else
    some <| allEffects.filterMap fun effect =>
      if summary (some effect) then some ⟨bodySite, effect⟩ else none

/-- Build the closed producer table consumed by the finite source checker. -/
def declarationsFromSummaries
    (summaries : ProducerId → Option FactSet)
    (bodySites : ProducerId → Expr) : ProducerTable := fun producer => do
    let summary ← summaries producer
    let body ← obligationsAtSummary (bodySites producer) summary
    some { captures := exampleStream.captures, body := body }

/-- Check source forms after translating their finite summaries to obligations. -/
def checkSummarySource
    (summaries : ProducerId → Option FactSet)
    (bodySites : ProducerId → Expr) (aliases : CheckedAliases)
    (source : EffectSource) : Option (List CheckedProducer) :=
  checkEffectSource (declarationsFromSummaries summaries bodySites) aliases source

/-- Observe the checked producer alternatives without their proof fields. -/
def observeSummarySource
    (summaries : ProducerId → Option FactSet)
    (bodySites : ProducerId → Expr) (aliases : CheckedAliases)
    (source : EffectSource) : List ProducerObservation :=
  (checkSummarySource summaries bodySites aliases source).getD [] |>.map observeProducer

private def parentProducer : ProducerId := ⟨301⟩
private def childProducer : ProducerId := ⟨302⟩
private def unresolvedProducer : ProducerId := ⟨303⟩
private def absentProducer : ProducerId := ⟨304⟩
private def parentCursor : CursorId := ⟨401⟩
private def childCursor : CursorId := ⟨402⟩
private def aliasBinder : Xsh.Foundation.BinderId := ⟨⟨501, by decide⟩⟩
private def conditionName : RawName := ⟨⟨601, by decide⟩⟩
private def producerName : RawName := ⟨⟨602, by decide⟩⟩
private def parentBodyName : RawName := ⟨⟨603, by decide⟩⟩
private def childBodyName : RawName := ⟨⟨604, by decide⟩⟩

private def conditionSite : Expr := .ident conditionName
private def parentCallSite : Expr := .call (.ident producerName) []
private def childCallSite : Expr := .call (.ident producerName) [.positional conditionSite]
private def parentBodySite : Expr := .call (.ident parentBodyName) []
private def childBodySite : Expr := .call (.ident childBodyName) []

private def parentSummary : FactSet := effectFact .net

private def childSummary : FactSet := effectFact .net

private def unresolvedNetSummary : FactSet := fun fact =>
  decide (fact = none ∨ fact = some .net)

private def knownEmptySummary : FactSet := emptyFacts

private def summaryTable : ProducerId → Option FactSet := fun producer =>
  if producer = parentProducer then some parentSummary
  else if producer = childProducer then some childSummary
  else if producer = unresolvedProducer then some unresolvedNetSummary
  else none

private def bodySiteTable : ProducerId → Expr := fun producer =>
  if producer = parentProducer then parentBodySite else childBodySite

private def outsideParentCall : EffectSource :=
  .producerCall exampleOutsideContext parentProducer parentCursor parentCallSite

private def outsideChildCall : EffectSource :=
  .producerCall exampleOutsideContext childProducer childCursor childCallSite

private def restrictedChildCall : EffectSource :=
  .producerCall exampleInsideWithoutNet childProducer childCursor childCallSite

private def acceptedParent : CheckedProducer :=
  (checkProducerCall exampleOutsideContext
    (declarationsFromSummaries summaryTable bodySiteTable)
    parentProducer parentCursor parentCallSite).get (by
      simp [checkProducerCall, declarationsFromSummaries, summaryTable,
        parentProducer, parentSummary, bodySiteTable, parentBodySite,
        obligationsAtSummary, exampleStream, exampleOutsideContext,
        allEffects, effectFact, ProducerDecl.workAt, permitsCreationBool,
        permitsObligationsBool, permitsBool, clauseCoversBool,
        effectMemberBool, ioCoversBool, blockedByBool])

private def checkedAliases : CheckedAliases := fun binder =>
  if binder = aliasBinder then some [acceptedParent] else none

private def aliasedParent : EffectSource := .alias aliasBinder

private def delegatedParentAndChild : EffectSource :=
  .yieldDelegate outsideParentCall outsideChildCall

private def delegatedRestrictedChild : EffectSource :=
  .yieldDelegate outsideParentCall restrictedChildCall

private def conditionalParentAndChild : EffectSource :=
  .conditional conditionSite outsideParentCall outsideChildCall

private def conditionalRestrictedChild : EffectSource :=
  .conditional conditionSite outsideParentCall restrictedChildCall

/-- A resolved summary contributes exactly its classified body-site obligation. -/
theorem resolved_network_summary_projects_at_body_site :
    obligationsAtSummary parentBodySite parentSummary =
      some [⟨parentBodySite, .net⟩] := by
  simp [obligationsAtSummary, parentSummary, allEffects, effectFact]

/-- An unresolved summary with known effects is not confused with a known-empty one. -/
theorem unresolved_summary_does_not_become_known_empty :
    obligationsAtSummary parentBodySite unresolvedNetSummary = none ∧
      obligationsAtSummary parentBodySite knownEmptySummary = some [] := by
  simp [obligationsAtSummary, unresolvedNetSummary, knownEmptySummary,
    allEffects, emptyFacts]

/-- Unresolved, missing, and opaque source summaries all fail closed. -/
theorem unresolved_missing_and_opaque_sources_fail_closed :
    checkSummarySource summaryTable bodySiteTable (fun _ => none)
      (.producerCall exampleOutsideContext unresolvedProducer parentCursor parentCallSite) = none ∧
    checkSummarySource summaryTable bodySiteTable (fun _ => none)
      (.producerCall exampleOutsideContext absentProducer parentCursor parentCallSite) = none ∧
    checkSummarySource summaryTable bodySiteTable (fun _ => none) .unknown = none := by
  simp [checkSummarySource, checkEffectSource, checkProducerCall,
    declarationsFromSummaries, summaryTable, bodySiteTable,
    unresolvedProducer, absentProducer, parentProducer, childProducer,
    unresolvedNetSummary, obligationsAtSummary, exampleStream]

/-- Successful calls retain both the caller's lexical context and source site. -/
theorem successful_summary_call_records_lexical_creation
    (summaries : ProducerId → Option FactSet)
    (bodySites : ProducerId → Expr) (context : LexicalContext)
    (producer : ProducerId) (cursor : CursorId) (site : Expr)
    (checked : CheckedProducer)
    (success : checkProducerCall context
      (declarationsFromSummaries summaries bodySites) producer cursor site =
        some checked) :
    checked.creationContext = context ∧ checked.work.creation = site := by
  exact checkProducerCall_success_records_its_lexical_site context
    (declarationsFromSummaries summaries bodySites) producer cursor site checked success

/-- Every classified summary effect is permitted where its producer is created. -/
theorem successful_summary_call_pays_each_body_obligation
    (summaries : ProducerId → Option FactSet)
    (bodySites : ProducerId → Expr) (context : LexicalContext)
    (producer : ProducerId) (cursor : CursorId) (site : Expr)
    (checked : CheckedProducer)
    (success : checkProducerCall context
      (declarationsFromSummaries summaries bodySites) producer cursor site =
        some checked)
    (obligation : EffectObligation) (member : obligation ∈ checked.work.obligations) :
    permits context obligation.required := by
  have permission := checkProducerCall_success_implies_creation_permission
    context (declarationsFromSummaries summaries bodySites) producer cursor site
    checked success
  have lexicalSite := checkProducerCall_success_records_its_lexical_site
    context (declarationsFromSummaries summaries bodySites) producer cursor site
    checked success
  rw [lexicalSite.1] at permission
  exact permits_obligation_at_member context checked.work.obligations obligation
    member permission

/-- Alias lookup reuses the checked handle and does not reconstruct its summary. -/
theorem alias_source_returns_the_checked_binding
    (declarations : ProducerTable) (aliases : CheckedAliases)
    (binding : Xsh.Foundation.BinderId) :
    checkEffectSource declarations aliases (.alias binding) = aliases binding := rfl

/-- A previously checked producer alias keeps its origin inside `without net`. -/
theorem restricted_alias_keeps_the_original_creation_observation :
    observeSummarySource summaryTable bodySiteTable checkedAliases aliasedParent =
      [observeProducer acceptedParent] := by
  simp [observeSummarySource, checkSummarySource, aliasedParent,
    checkedAliases, checkEffectSource]

/-- Rechecking that alias at a restricted consumer is a detectable mutation. -/
theorem consumer_site_recheck_mutation_changes_alias_acceptance :
    checkSummarySource summaryTable bodySiteTable checkedAliases aliasedParent ≠
      checkSummarySource summaryTable bodySiteTable checkedAliases
        (.producerCall exampleInsideWithoutNet parentProducer parentCursor parentCallSite) := by
  have aliasResult : checkSummarySource summaryTable bodySiteTable
      checkedAliases aliasedParent = some [acceptedParent] := by
    simp [checkSummarySource, aliasedParent, checkedAliases, checkEffectSource]
  have restrictedResult : checkSummarySource summaryTable bodySiteTable
      checkedAliases
        (.producerCall exampleInsideWithoutNet parentProducer parentCursor parentCallSite) = none := by
    simp [checkSummarySource, checkEffectSource, checkProducerCall,
      declarationsFromSummaries, summaryTable, parentProducer,
      parentSummary, bodySiteTable, parentBodySite, obligationsAtSummary,
      exampleStream, exampleInsideWithoutNet, allEffects, effectFact,
      ProducerDecl.workAt, permitsCreationBool, permitsObligationsBool,
      permitsBool, clauseCoversBool, effectMemberBool, ioCoversBool,
      blockedByBool, withoutBlocksBool]
  rw [aliasResult, restrictedResult]
  decide

/-- Delegation records the ordered parent, child-call, and child-body sites. -/
theorem delegation_preserves_ordered_parent_child_body_provenance :
    ((observeSummarySource summaryTable bodySiteTable (fun _ => none)
      delegatedParentAndChild).map fun observation =>
        (observation.provenanceSites, observation.causalPaths)) =
      [([parentCallSite, childCallSite],
        [[parentCallSite, childCallSite, childBodySite]])] := by
  simp [observeSummarySource, checkSummarySource, delegatedParentAndChild,
    outsideParentCall, outsideChildCall, checkEffectSource, checkProducerCall,
    declarationsFromSummaries, summaryTable, parentProducer, childProducer,
    parentSummary, childSummary, bodySiteTable, parentBodySite, childBodySite,
    obligationsAtSummary, observeProducer, CheckedProducer.causalPaths,
    ProducerDecl.workAt, exampleStream, exampleOutsideContext,
    parentCursor, childCursor, parentCallSite, childCallSite,
    allEffects, effectFact, permitsCreationBool, permitsObligationsBool,
    permitsBool, clauseCoversBool, effectMemberBool, ioCoversBool, blockedByBool]

/-- Delegation checks the child call under that child's lexical context. -/
theorem delegation_rejects_child_blocked_at_its_own_creation :
    checkSummarySource summaryTable bodySiteTable (fun _ => none)
      delegatedRestrictedChild = none := by
  simp [checkSummarySource, delegatedRestrictedChild, outsideParentCall,
    restrictedChildCall, checkEffectSource, checkProducerCall,
    declarationsFromSummaries, summaryTable, parentProducer, childProducer,
    parentSummary, childSummary, bodySiteTable, parentBodySite, childBodySite,
    obligationsAtSummary, ProducerDecl.workAt, exampleStream,
    exampleOutsideContext, exampleInsideWithoutNet, parentCursor, childCursor,
    parentCallSite, childCallSite, allEffects, effectFact,
    permitsCreationBool, permitsObligationsBool, permitsBool, clauseCoversBool,
    effectMemberBool, ioCoversBool, blockedByBool, withoutBlocksBool]

/-- Conditional checking retains one tagged observation for each valid arm. -/
theorem conditional_source_retains_both_tagged_arms :
    ((observeSummarySource summaryTable bodySiteTable (fun _ => none)
      conditionalParentAndChild).map fun observation =>
        (observation.producer, observation.alternatives)) =
      [(parentProducer, [{ condition := conditionSite, arm := .whenTrue }]),
        (childProducer, [{ condition := conditionSite, arm := .whenFalse }])] := by
  simp [observeSummarySource, checkSummarySource, conditionalParentAndChild,
    outsideParentCall, outsideChildCall, checkEffectSource, checkProducerCall,
    declarationsFromSummaries, summaryTable, parentProducer, childProducer,
    parentSummary, childSummary, bodySiteTable, parentBodySite, childBodySite,
    obligationsAtSummary, observeProducer, CheckedProducer.withAlternative,
    ProducerDecl.workAt, exampleStream, exampleOutsideContext,
    parentCursor, childCursor, parentCallSite, childCallSite,
    conditionSite, allEffects, effectFact, permitsCreationBool,
    permitsObligationsBool, permitsBool, clauseCoversBool,
    effectMemberBool, ioCoversBool, blockedByBool]

/-- One rejected conditional arm rejects the whole source expression. -/
theorem conditional_source_rejects_a_blocked_arm :
    checkSummarySource summaryTable bodySiteTable (fun _ => none)
      conditionalRestrictedChild = none := by
  simp [checkSummarySource, conditionalRestrictedChild, outsideParentCall,
    restrictedChildCall, checkEffectSource, checkProducerCall,
    declarationsFromSummaries, summaryTable, parentProducer, childProducer,
    parentSummary, childSummary, bodySiteTable, parentBodySite, childBodySite,
    obligationsAtSummary, ProducerDecl.workAt, exampleStream,
    exampleOutsideContext, exampleInsideWithoutNet, parentCursor, childCursor,
    parentCallSite, childCallSite, conditionSite, allEffects, effectFact,
    permitsCreationBool, permitsObligationsBool, permitsBool, clauseCoversBool,
    effectMemberBool, ioCoversBool, blockedByBool, withoutBlocksBool]

end Xsh.Semantics.Effects.CheckerBridge
