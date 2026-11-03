import Xsh.Semantics.Connected.Machine
import Xsh.Typing.Connected.BlockCertificate

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Syntax.Foundation
open Xsh.Values
open Xsh.Typing.Connected


theorem clock_request_survives_result_propagation (state : Machine)
    (clock ok : DeclarationId) (errorType : RuntimeType) (response : IntValue)
    (clockFound : state.functions clock = some .timeNow)
    (okFound : state.functions ok = some .resultOk) :
    let target : Value := .function ok (.argument .integer .argumentsEmpty) (.result .integer errorType)
    let initial := { state with control := .invoke (.function clock .argumentsEmpty .integer) [], stack := .callArguments target [] [] :: .propagate :: state.stack }
    let final := { state with control := .value (.scalar (.integer response)) }
    Execution initial [.request .timeNow response, .internal, .internal, .internal] final := by
  dsimp
  let target : Value := .function ok (.argument .integer .argumentsEmpty) (.result .integer errorType)
  let outer := { state with stack := .propagate :: state.stack }
  let inner := { outer with stack := .callArguments target [] [] :: outer.stack }
  refine .next (Step.callTime inner clock response clockFound) ?_
  refine .next (Step.callArgumentsComplete outer target (.scalar (.integer response)) []) ?_
  refine .next (Step.callOk outer ok (.scalar (.integer response)) .integer errorType okFound rfl) ?_
  exact .next (Step.propagateOk state (.scalar (.integer response)) errorType) (.done _)


theorem assignment_then_return_reads_new_value (state : Machine) (binder : BinderId)
    (cell : Xsh.Semantics.Environments.CellId) (text : String) (value : IntValue) (next : Store)
    (parsed : parseInteger text = some value)
    (active : state.cells binder = some cell)
    (written : write state.store cell (.scalar (.integer value)) = .accepted next) :
    let source := [.assignment binder (.integer text), .returned (.read binder [])]
    let initial := { state with control := .statements source }
    let final := { state with control := .returned (.scalar (.integer value)), store := next }
    Execution initial [.internal, .internal, .internal, .internal, .internal, .internal] final := by
  dsimp
  let rest := [StatementNode.returned (.read binder [])]
  let stored := { state with store := next }
  refine .next (Step.assignmentStart state binder (.integer text) rest) ?_
  refine .next (Step.integer { state with stack := .assign binder rest :: state.stack } text value parsed) ?_
  refine .next (Step.assignmentCommit state binder (.scalar (.integer value)) rest cell next active written) ?_
  refine .next (Step.returnStart stored (.read binder []) []) ?_
  have visible := read_after_assignment state.store next state.cells binder cell (.scalar (.integer value)) active written
  refine .next (Step.read { stored with stack := .returnValue :: state.stack } binder [] _ visible) ?_
  exact .next (Step.returnValue stored _) (.done _)


theorem identity_function_accepts_integer_and_boolean_activations :
    SignatureInstance identityScheme (.argument .integer .argumentsEmpty) .integer ∧
    SignatureInstance identityScheme (.argument .boolean .argumentsEmpty) .boolean := by
  constructor
  · refine ⟨_, _, fun _ => .primitive .integer, [], rfl, rfl, ?_, ?_⟩
    · simp [Instantiates, schemeSubstitution, identityScheme, identityVariable, substitute]
    · simp [Solves]
  · refine ⟨_, _, fun _ => .primitive .boolean, [], rfl, rfl, ?_, ?_⟩
    · simp [Instantiates, schemeSubstitution, identityScheme, identityVariable, substitute]
    · simp [Solves]


theorem mutable_integer_assignment_executes (state : Machine) (binder : BinderId)
    (cell : Xsh.Semantics.Environments.CellId) (text : String) (value : IntValue)
    (current : Xsh.Semantics.Environments.StoreCell RuntimeType Value HasType)
    (parsed : parseInteger text = some value)
    (active : state.cells binder = some cell) (present : state.store cell = some current)
    (mutable : current.mutability = .mutable)
    (typed : HasType current.contents.typeTag (.scalar (.integer value))) :
    ∃ next, Execution
      { state with control := .statements [.assignment binder (.integer text), .returned (.read binder [])] }
      [.internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .returned (.scalar (.integer value)), store := next } := by
  obtain ⟨next, written⟩ :=
    (Xsh.Semantics.Environments.write_accepted_iff (fun _ _ => inferInstance)
      state.store cell (.scalar (.integer value))).mpr ⟨current, present, mutable, typed⟩
  exact ⟨next, assignment_then_return_reads_new_value state binder cell text value next parsed active written⟩


def integerTwo : IntValue := ⟨2, by decide⟩

theorem certified_integer_local_executes (state : Machine)
    (fresh : state.store ⟨state.activation, integerLocalBinder⟩ = none) :
    integerLocalCertificateAccepted = true ∧
    ∃ next, Execution { state with control := .statements integerLocalCandidate }
      [.internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .returned (.scalar (.integer integerTwo)), cells := bindActive state.cells integerLocalBinder ⟨state.activation, integerLocalBinder⟩, store := next } := by
  constructor
  · exact integer_local_derived_certificate_is_inhabited
  · let cell : Xsh.Semantics.Environments.CellId := ⟨state.activation, integerLocalBinder⟩
    obtain ⟨allocated, current, allocatedProof, present, mutable, tag, _⟩ :=
      allocate_fresh_typed_cell state.store cell .mutable (.scalar (.integer intOne)) .integer fresh rfl
    let afterBinding := { state with cells := bindActive state.cells integerLocalBinder cell, store := allocated }
    have active : afterBinding.cells integerLocalBinder = some cell := by simp [afterBinding, bindActive]
    have member : HasType current.contents.typeTag (.scalar (.integer integerTwo)) := by
      rw [tag]
      rfl
    obtain ⟨next, run⟩ := mutable_integer_assignment_executes afterBinding integerLocalBinder cell "2"
      integerTwo current (by decide +kernel) active present mutable member
    refine ⟨next, ?_⟩
    let rest : List StatementNode := [.assignment integerLocalBinder (.integer "2"),
      .returned (.read integerLocalBinder [])]
    refine .next (Step.bindingStart state integerLocalBinder .mutable (.integer "1") rest) ?_
    refine .next (Step.integer { state with stack := .bind integerLocalBinder .mutable rest :: state.stack }
      "1" intOne (by decide +kernel)) ?_
    refine .next (Step.bindingCommit state integerLocalBinder .mutable (.scalar (.integer intOne))
      rest allocated allocatedProof) ?_
    exact run

end Xsh.Semantics.Connected
