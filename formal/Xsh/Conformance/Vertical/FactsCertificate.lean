import Xsh.Conformance.Vertical.Facts
import Xsh.Conformance.Vertical.CallsCertificate

namespace Xsh.Conformance.Vertical

def letRead : NodeFact := .read (.letBinding 0)

def arithmetic (op : FragmentOp) : NodeFact := .operate op .int .int

/-- The six functions of the combined Rust bridge fixture, in source order: the two success
functions are Lean data here, the failure and call functions are the certified fixtures'. -/
def factsFunctions : List SourceFunction :=
  [⟨"shifted", [], [("x", 2)], .value (.binary .add (.var "x") (.int 3))⟩,
    ⟨"scaled", [], [("y", 7)], .value (.binary .mul (.var "y") (.int 6))⟩] ++
    fixtureFunctions ++ callsFunctions

/-- A candidate fact set for the six fixture functions, written in Lean. It is not emitted by
Rust. -/
def fixtureFacts : List FunctionFacts :=
  [⟨"shifted", [], .int, [], [letRead, .literal 3, arithmetic .add]⟩,
    ⟨"scaled", [], .int, [], [letRead, .literal 6, arithmetic .mul]⟩,
    ⟨"divided", [], .int, [], [.literal 1, letRead, arithmetic .div]⟩,
    ⟨"overflowed", [], .int, [], [letRead, .literal 1, arithmetic .add]⟩,
    ⟨"excess", [.int, .int], .int, [],
      [.read (.parameter 0), .read (.parameter 1), .operate .gt .int .bool, .read (.parameter 0),
        .read (.parameter 1), arithmetic .sub, .literal 0, .branch]⟩,
    ⟨"surplus", [], .int, [], [letRead, .literal 9, .call 4 2]⟩]

theorem the_fixture_facts_are_accepted : validateFacts factsFunctions fixtureFacts = true := by
  decide +kernel

theorem the_fixture_facts_are_a_valid_derivation :
    ProgramHolds factsFunctions fixtureFacts :=
  validate_facts_iff.mp the_fixture_facts_are_accepted

theorem the_fixture_facts_are_accepted_in_any_order :
    validateFacts factsFunctions fixtureFacts.reverse = true := by
  decide +kernel

def withFact (facts : List FunctionFacts) (index : Nat) (change : FunctionFacts → FunctionFacts) :
    List FunctionFacts :=
  match facts[index]? with
  | some one => facts.set index (change one)
  | none => facts

def fixtureVerdict (candidate : List FunctionFacts) : Bool :=
  validateFacts factsFunctions candidate

theorem a_forged_binding_is_rejected :
    fixtureVerdict (withFact fixtureFacts 4 fun facts =>
      { facts with nodes := facts.nodes.set 1 (.read (.parameter 0)) }) = false ∧
    fixtureVerdict (withFact fixtureFacts 0 fun facts =>
      { facts with nodes := facts.nodes.set 0 (.read (.parameter 0)) }) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

theorem a_wrong_selected_operation_is_rejected :
    fixtureVerdict (withFact fixtureFacts 0 fun facts =>
      { facts with nodes := facts.nodes.set 2 (arithmetic .sub) }) = false ∧
    fixtureVerdict (withFact fixtureFacts 4 fun facts =>
      { facts with nodes := facts.nodes.set 2 (.operate .gt .int .int) }) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

theorem a_forged_effect_fact_is_rejected :
    fixtureVerdict (withFact fixtureFacts 1 fun facts => { facts with effects := ["io"] }) =
      false := by
  decide +kernel

theorem a_forged_scheme_is_rejected :
    fixtureVerdict (withFact fixtureFacts 4 fun facts =>
      { facts with parameterTypes := [.int, .bool] }) = false ∧
    fixtureVerdict (withFact fixtureFacts 0 fun facts => { facts with resultType := .bool }) =
      false ∧
    fixtureVerdict (withFact fixtureFacts 0 fun facts =>
      { facts with parameterTypes := [.int] }) = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

theorem a_changed_evaluation_order_is_rejected :
    fixtureVerdict (withFact fixtureFacts 5 fun facts =>
      { facts with nodes := [.literal 9, letRead, .call 4 2] }) = false ∧
    fixtureVerdict (withFact fixtureFacts 2 fun facts =>
      { facts with nodes := [letRead, .literal 1, arithmetic .div] }) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

theorem a_wrong_declaration_is_rejected :
    fixtureVerdict (withFact fixtureFacts 5 fun facts =>
      { facts with nodes := [letRead, .literal 9, .call 3 2] }) = false ∧
    fixtureVerdict (withFact fixtureFacts 5 fun facts =>
      { facts with nodes := [letRead, .literal 9, .call 4 1] }) = false := by
  refine ⟨?_, ?_⟩ <;> decide +kernel

theorem a_missing_or_extra_fact_is_rejected :
    fixtureVerdict (fixtureFacts.drop 1) = false ∧
    fixtureVerdict (fixtureFacts ++ fixtureFacts.take 1) = false ∧
    fixtureVerdict (withFact fixtureFacts 0 fun facts => { facts with nodes := facts.nodes.drop 1 }) =
      false ∧
    fixtureVerdict (withFact fixtureFacts 3 fun facts => { facts with name := "shifted" }) =
      false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem a_forged_literal_is_rejected :
    fixtureVerdict (withFact fixtureFacts 4 fun facts =>
      { facts with nodes := facts.nodes.set 6 (.literal 1) }) = false := by
  decide +kernel

/-- A validator that takes the candidate's word for its facts. -/
def trustingValidator (_ : List SourceFunction) (_ : List FunctionFacts) : Bool := true

theorem a_validator_that_trusts_the_candidate_accepts_a_forgery_that_the_validator_rejects :
    trustingValidator factsFunctions (withFact fixtureFacts 5 fun facts =>
      { facts with nodes := [.literal 9, letRead, .call 4 2] }) = true ∧
    fixtureVerdict (withFact fixtureFacts 5 fun facts =>
      { facts with nodes := [.literal 9, letRead, .call 4 2] }) = false := by
  refine ⟨rfl, ?_⟩
  decide +kernel

theorem a_source_that_resolves_nothing_has_no_valid_facts :
    validateFacts [⟨"f", [], [], .value (.atom (.var "missing"))⟩]
        [⟨"f", [], .int, [], [.read (.parameter 0)]⟩] = false ∧
    validateFacts [⟨"f", [], [], .value (.atom (.int 9223372036854775808))⟩]
        [⟨"f", [], .int, [], [.literal 9223372036854775808]⟩] = false ∧
    validateFacts [⟨"f", [], [], .call "g" []⟩, ⟨"g", [], [], .value (.atom (.int 1))⟩]
        [⟨"f", [], .int, [], [.call 1 0]⟩, ⟨"g", [], .int, [], [.literal 1]⟩] = false := by
  refine ⟨?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
