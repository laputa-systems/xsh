import Xsh.Conformance.Vertical.FactsDocument
import Xsh.Conformance.Vertical.CallsCertificate

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport

def letRead : NodeFact := .read (.letBinding 0)

def arithmetic (op : FragmentOp) : NodeFact := .operate op .int .int

/-- The facts of the failures document, written out. `the_failures_emitted_facts_are_the_decoded_document`
proves they are what the exchanged bytes decode to; they are not a second candidate. -/
def failuresEmitted : List FunctionFacts :=
  [⟨"divided", [], [.int], .int, [], [.literal 1, letRead, arithmetic .div]⟩,
    ⟨"overflowed", [], [.int], .int, [], [letRead, .literal 1, arithmetic .add]⟩]

/-- The facts of the calls document, written out in the same way. -/
def callsEmitted : List FunctionFacts :=
  [⟨"excess", [.int, .int], [], .int, [],
      [.read (.parameter 0), .read (.parameter 1), .operate .gt .int .bool, .read (.parameter 0),
        .read (.parameter 1), arithmetic .sub, .literal 0, .branch]⟩,
    ⟨"surplus", [], [.int], .int, [], [letRead, .literal 9, .call 0 2]⟩]

set_option maxRecDepth 10000 in
theorem the_failures_emitted_facts_are_the_decoded_document :
    decodeFactsDocument failuresFactsDocumentCodes = some ⟨bridgeId! 1, failuresEmitted⟩ := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem the_calls_emitted_facts_are_the_decoded_document :
    decodeFactsDocument callsFactsDocumentCodes = some ⟨bridgeId! 1, callsEmitted⟩ := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- ARTIFACT_CERTIFIED. Artifact kind: candidate fact set. The two facts documents are the checked
facts the Rust checker published for the failures and calls fixtures, exported by the test-only
bridge into the closed v1 facts exchange, exactly the bytes of `failuresFactsDocumentCodes` and
`callsFactsDocumentCodes`; each is paired with its source document, `sourceDocumentCodes` and
`callsSourceDocumentCodes`.
Proposition: for each pair, both documents decode, the facts document names the source document,
the source text parses in the admitted productions, and the validator accepts the decoded facts,
which therefore are a valid derivation of the declarative rules for the parsed functions
(`certifies_facts_sound`): every resolution, selected callee, argument count, operand and result
type, scheme, let type, effect set and evaluation order the Rust checker published equals what
the rules determine. The validator reads nothing from the candidate except the facts it lists.
Rust's parser and checker are the producers of the bytes and are not shown correct here: that the
bytes are what Rust exports is the `RUST_CONFORMANCE` case
`exported_facts_are_the_lean_certified_bytes`; nothing covers other programs, effects beyond the
empty set of a pure function, or facts the exporter does not publish. -/
theorem emitted_facts_are_certified :
    certifiesFacts sourceDocumentCodes failuresFactsDocumentCodes = true ∧
      certifiesFacts callsSourceDocumentCodes callsFactsDocumentCodes = true := by
  constructor <;> decide +kernel

theorem the_emitted_facts_are_a_valid_derivation :
    ProgramHolds fixtureFunctions failuresEmitted ∧ ProgramHolds callsFunctions callsEmitted := by
  refine ⟨validate_facts_iff.mp ?_, validate_facts_iff.mp ?_⟩ <;> decide +kernel

theorem the_emitted_facts_are_accepted_in_any_order :
    validateFacts callsFunctions callsEmitted.reverse = true := by
  decide +kernel

def withFact (facts : List FunctionFacts) (index : Nat) (change : FunctionFacts → FunctionFacts) :
    List FunctionFacts :=
  match facts[index]? with
  | some one => facts.set index (change one)
  | none => facts

def callsVerdict (candidate : List FunctionFacts) : Bool :=
  validateFacts callsFunctions candidate

theorem the_unmodified_emitted_calls_facts_are_accepted : callsVerdict callsEmitted = true := by
  decide +kernel

theorem a_forged_binding_is_rejected :
    callsVerdict (withFact callsEmitted 0 fun facts =>
      { facts with nodes := facts.nodes.set 1 (.read (.parameter 0)) }) = false ∧
    callsVerdict (withFact callsEmitted 1 fun facts =>
      { facts with nodes := facts.nodes.set 0 (.read (.parameter 0)) }) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

theorem a_wrong_selected_operation_is_rejected :
    callsVerdict (withFact callsEmitted 0 fun facts =>
      { facts with nodes := facts.nodes.set 5 (arithmetic .add) }) = false ∧
    callsVerdict (withFact callsEmitted 0 fun facts =>
      { facts with nodes := facts.nodes.set 2 (.operate .gt .int .int) }) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

theorem a_forged_effect_fact_is_rejected :
    callsVerdict (withFact callsEmitted 1 fun facts => { facts with effects := ["io"] }) =
      false := by
  decide +kernel

theorem a_forged_scheme_is_rejected :
    callsVerdict (withFact callsEmitted 0 fun facts =>
      { facts with parameterTypes := [.int, .bool] }) = false ∧
    callsVerdict (withFact callsEmitted 1 fun facts => { facts with resultType := .bool }) =
      false ∧
    callsVerdict (withFact callsEmitted 0 fun facts =>
      { facts with parameterTypes := [.int] }) = false ∧
    callsVerdict (withFact callsEmitted 1 fun facts => { facts with localTypes := [.bool] }) =
      false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem a_changed_evaluation_order_is_rejected :
    callsVerdict (withFact callsEmitted 1 fun facts =>
      { facts with nodes := [.literal 9, letRead, .call 0 2] }) = false ∧
    callsVerdict (withFact callsEmitted 0 fun facts =>
      { facts with nodes := facts.nodes.reverse }) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

theorem a_wrong_declaration_is_rejected :
    callsVerdict (withFact callsEmitted 1 fun facts =>
      { facts with nodes := [letRead, .literal 9, .call 1 2] }) = false ∧
    callsVerdict (withFact callsEmitted 1 fun facts =>
      { facts with nodes := [letRead, .literal 9, .call 0 1] }) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

theorem a_missing_or_extra_fact_is_rejected :
    callsVerdict (callsEmitted.drop 1) = false ∧
    callsVerdict (callsEmitted ++ callsEmitted.take 1) = false ∧
    callsVerdict (withFact callsEmitted 0 fun facts => { facts with nodes := facts.nodes.drop 1 }) =
      false ∧
    callsVerdict (withFact callsEmitted 1 fun facts => { facts with name := "excess" }) =
      false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem a_forged_literal_is_rejected :
    callsVerdict (withFact callsEmitted 0 fun facts =>
      { facts with nodes := facts.nodes.set 6 (.literal 1) }) = false := by
  decide +kernel

set_option maxRecDepth 10000 in
/-- A pair is refused when its bytes are changed: the byte of a literal, a callee, an argument
count and a parameter index in the facts document each decode to a different fact, and the
operation, the literal argument and the document identifier of the source document each stop
matching the facts. -/
theorem changed_bytes_are_not_certified :
    certifiesFacts callsSourceDocumentCodes (callsFactsDocumentCodes.set 893 56) = false ∧
      certifiesFacts callsSourceDocumentCodes (callsFactsDocumentCodes.set 929 49) = false ∧
      certifiesFacts callsSourceDocumentCodes (callsFactsDocumentCodes.set 952 49) = false ∧
      certifiesFacts callsSourceDocumentCodes (callsFactsDocumentCodes.set 334 48) = false ∧
      certifiesFacts (callsSourceDocumentCodes.set 130 43) callsFactsDocumentCodes = false ∧
      certifiesFacts (callsSourceDocumentCodes.set 214 56) callsFactsDocumentCodes = false ∧
      certifiesFacts (callsSourceDocumentCodes.set 39 50) callsFactsDocumentCodes = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
/-- The changed facts bytes above decode, so the refusal is the validator's and not the
decoder's: the literal, callee, argument count and parameter index each decode to a
different fact. -/
theorem the_changed_fact_bytes_still_decode :
    (decodeFactsDocument (callsFactsDocumentCodes.set 893 56)).isSome = true ∧
      (decodeFactsDocument (callsFactsDocumentCodes.set 929 49)).isSome = true ∧
      (decodeFactsDocument (callsFactsDocumentCodes.set 952 49)).isSome = true ∧
      (decodeFactsDocument (callsFactsDocumentCodes.set 334 48)).isSome = true := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

/-- A validator that takes the candidate's word for its facts. -/
def trustingValidator (_ : List SourceFunction) (_ : List FunctionFacts) : Bool := true

theorem a_validator_that_trusts_the_candidate_accepts_a_forgery_that_the_validator_rejects :
    trustingValidator callsFunctions (withFact callsEmitted 1 fun facts =>
      { facts with nodes := [.literal 9, letRead, .call 0 2] }) = true ∧
    callsVerdict (withFact callsEmitted 1 fun facts =>
      { facts with nodes := [.literal 9, letRead, .call 0 2] }) = false := by
  refine ⟨rfl, ?_⟩
  decide +kernel

theorem a_source_that_resolves_nothing_has_no_valid_facts :
    validateFacts [⟨"f", [], [], .value (.atom (.var "missing"))⟩]
        [⟨"f", [], [], .int, [], [.read (.parameter 0)]⟩] = false ∧
    validateFacts [⟨"f", [], [], .value (.atom (.int 9223372036854775808))⟩]
        [⟨"f", [], [], .int, [], [.literal 9223372036854775808]⟩] = false ∧
    validateFacts [⟨"f", [], [], .call "g" []⟩, ⟨"g", [], [], .value (.atom (.int 1))⟩]
        [⟨"f", [], [], .int, [], [.call 1 0]⟩, ⟨"g", [], [], .int, [], [.literal 1]⟩] = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
