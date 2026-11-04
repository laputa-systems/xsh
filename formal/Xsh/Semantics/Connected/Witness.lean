import Xsh.Semantics.Connected.Machine
import Xsh.Typing.Connected.BlockCertificate
import Xsh.Typing.Connected.ContextCertificate

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
  refine .next (Step.read { stored with stack := .returnValue :: state.stack } binder [] _ visible rfl) ?_
  exact .next (Step.returnValue stored _) (.done _)


theorem scoped_assignment_continues_with_outer_write (state : Machine) (binder : BinderId)
    (cell : Xsh.Semantics.Environments.CellId) (text : String) (value : IntValue) (next : Store)
    (parsed : parseInteger text = some value)
    (active : state.cells binder = some cell)
    (written : write state.store cell (.scalar (.integer value)) = .accepted next) :
    Execution
      { state with control := .statements [.evaluated (.valueBlock (.primitive .unit) [.assignment binder (.integer text)]), .returned (.read binder [])] }
      [.internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .returned (.scalar (.integer value)), store := next } := by
  let returnNode := StatementNode.returned (.read binder [])
  let outer := { state with stack := .discard [returnNode] :: state.stack }
  let inner := { outer with stack := .valueBlock state.cells :: outer.stack }
  let stored := { state with store := next }
  let storedOuter := { outer with store := next }
  let storedInner := { inner with store := next }
  refine .next (Step.evaluateAndContinue state (.valueBlock (.primitive .unit) [.assignment binder (.integer text)]) returnNode []) ?_
  refine .next (Step.valueBlockStart outer (.primitive .unit) [.assignment binder (.integer text)]) ?_
  refine .next (Step.assignmentStart inner binder (.integer text) []) ?_
  refine .next (Step.integer { inner with stack := .assign binder [] :: inner.stack } text value parsed) ?_
  refine .next (Step.assignmentCommit inner binder (.scalar (.integer value)) [] cell next active written) ?_
  refine .next (Step.blockComplete storedInner) ?_
  refine .next (Step.valueBlockComplete storedOuter .unit state.cells) ?_
  refine .next (Step.discarded stored .unit [returnNode]) ?_
  refine .next (Step.returnStart stored (.read binder []) []) ?_
  have visible := read_after_assignment state.store next state.cells binder cell (.scalar (.integer value)) active written
  refine .next (Step.read { stored with stack := .returnValue :: state.stack } binder [] _ visible rfl) ?_
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




def recordNominal : Xsh.Validation.Nominal.NominalId := ⟨"module.example", "Item"⟩
def schemaName : NameId := exampleName 21 (by decide)
def fieldName : NameId := exampleName 22 (by decide)
def schemaBinder : BinderId := ⟨⟨21, by decide⟩⟩
def schemaRow : MonoType := .rowField fieldName (.primitive .integer) .rowEmpty
def schemaScope : Scope :=
  [⟨schemaName, schemaBinder, .schema, ⟨[], .nominal recordNominal (.record schemaRow), []⟩⟩]
def recordSource : Xsh.Syntax.Expr :=
  .require (.record [.named fieldName (.literal (.integer "7"))]) (some (.named schemaName))
def recordCandidate : Node := .requireValue schemaBinder recordNominal (.record [(fieldName, .integer "7")])
def integerSeven : IntValue := ⟨7, by decide⟩
def recordGuard : Guard := .recordShape (.rowField fieldName .integer .rowEmpty)

def recordCertificateAccepted : Bool :=
  match validateDerivedExpression 16 schemaScope recordSource recordCandidate [] with
  | .ok (.result (.nominal identity (.record _)) (.primitive .error)) => decide (identity = recordNominal)
  | _ => false

def wrongNominalRejected : Bool :=
  let wrong : Xsh.Validation.Nominal.NominalId := ⟨"other.module", "Item"⟩
  match validateDerivedExpression 16 schemaScope recordSource
      (.requireValue schemaBinder wrong (.record [(fieldName, .integer "7")])) [] with
  | .error .mismatch => true
  | _ => false

theorem record_nominal_certificate_and_identity_control :
    recordCertificateAccepted = true ∧ wrongNominalRejected = true := by
  constructor <;> decide +kernel

theorem record_nominal_guard_executes (state : Machine)
    (found : state.schemas schemaBinder = some (recordNominal, recordGuard)) :
    recordCertificateAccepted = true ∧
    Execution { state with control := .expression recordCandidate }
      [.internal, .internal, .internal, .internal, .internal]
      { state with control := .value (.ok (.nominal recordNominal (.record (.rowField fieldName .integer .rowEmpty))
        (.record [(fieldName, .scalar (.integer integerSeven))])) .error) } := by
  constructor
  · exact record_nominal_certificate_and_identity_control.1
  · let outer := { state with stack := .requireGuard recordNominal recordGuard :: state.stack }
    refine .next (Step.requireStart state schemaBinder recordNominal (.record [(fieldName, .integer "7")]) recordGuard found) ?_
    refine .next (Step.recordStart outer fieldName (.integer "7") []) ?_
    refine .next (Step.integer { outer with stack := .recordFields [] fieldName [] :: outer.stack }
      "7" integerSeven (by decide +kernel)) ?_
    refine .next (Step.recordFinish outer [] fieldName (.scalar (.integer integerSeven))) ?_
    exact .next (Step.requireSuccess state recordNominal recordGuard
      (.record [(fieldName, .scalar (.integer integerSeven))]) (by decide +kernel)) (.done _)

def invalidRecordSource : Xsh.Syntax.Expr :=
  .require (.record [.named fieldName (.literal (.bool true))]) (some (.named schemaName))
def invalidRecordCandidate : Node := .requireValue schemaBinder recordNominal (.record [(fieldName, .boolean true)])

def invalidRecordCertificateAccepted : Bool :=
  match validateDerivedExpression 16 schemaScope invalidRecordSource invalidRecordCandidate [] with
  | .ok (.result (.nominal identity (.record _)) (.primitive .error)) => decide (identity = recordNominal)
  | _ => false

def omittedGuardRejected : Bool :=
  match validateDerivedExpression 16 schemaScope recordSource (.record [(fieldName, .integer "7")]) [] with
  | .error .mismatch => true
  | _ => false

theorem record_guard_refusal_remains_a_result (state : Machine)
    (found : state.schemas schemaBinder = some (recordNominal, recordGuard)) :
    invalidRecordCertificateAccepted = true ∧ omittedGuardRejected = true ∧
    Execution { state with control := .expression invalidRecordCandidate }
      [.internal, .internal, .internal, .internal, .internal]
      { state with control := .value (.error .validationError
        (.nominal recordNominal (.record (.rowField fieldName .integer .rowEmpty)))) } := by
  refine ⟨by decide +kernel, by decide +kernel, ?_⟩
  let outer := { state with stack := .requireGuard recordNominal recordGuard :: state.stack }
  refine .next (Step.requireStart state schemaBinder recordNominal (.record [(fieldName, .boolean true)]) recordGuard found) ?_
  refine .next (Step.recordStart outer fieldName (.boolean true) []) ?_
  refine .next (Step.boolean { outer with stack := .recordFields [] fieldName [] :: outer.stack } true) ?_
  refine .next (Step.recordFinish outer [] fieldName (.scalar (.boolean true))) ?_
  exact .next (Step.requireFailure state recordNominal recordGuard
    (.record [(fieldName, .scalar (.boolean true))]) (by decide +kernel)) (.done _)


theorem branch_tail_value_continues_non_tail_block (state : Machine) (value : Value)
    (cells : ActiveCells) (next : StatementNode) (rest : List StatementNode) :
    Execution { state with control := .value value, stack := .tailValue :: .continueBlock cells (next :: rest) :: state.stack }
      [.internal, .internal]
      { state with control := .statements (next :: rest), cells := cells } := by
  let outer := { state with stack := .continueBlock cells (next :: rest) :: state.stack }
  exact .next (Step.tailValue outer value) (.next (Step.nonTailBranchComplete state value cells next rest) (.done _))


def integerIdentityArguments : List (Variable × MonoType) := [(identityVariable, .primitive .integer)]
def booleanIdentityArguments : List (Variable × MonoType) := [(identityVariable, .primitive .boolean)]

theorem stored_polymorphic_function_reads_at_two_types (state : Machine) (declaration : DeclarationId)
    (found : Connected.read state.store state.cells identityBinding.binder = some (.schemeFunction declaration identityScheme)) :
    Step { state with control := .expression (.read identityBinding.binder integerIdentityArguments) } .internal
      { state with control := .value (.instantiatedFunction declaration (.argument .integer .argumentsEmpty)
        .integer state.solution integerIdentityArguments) } ∧
    Step { state with control := .expression (.read identityBinding.binder booleanIdentityArguments) } .internal
      { state with control := .value (.instantiatedFunction declaration (.argument .boolean .argumentsEmpty)
        .boolean state.solution booleanIdentityArguments) } := by
  constructor
  · apply Step.readFunction _ _ _ _ _ _ _ integerIdentityArguments found
    simp [instantiateFunction, integerIdentityArguments, instantiationKeysValid, instantiationSortsValid,
      witnessSubstitution, schemeSubstitution, identityScheme, identityVariable, substitute,
      checksConstraints, checksWellFormed, typeSort, VariableKind.sort, monoRuntime]
  · apply Step.readFunction _ _ _ _ _ _ _ booleanIdentityArguments found
    simp [instantiateFunction, booleanIdentityArguments, instantiationKeysValid, instantiationSortsValid,
      witnessSubstitution, schemeSubstitution, identityScheme, identityVariable, substitute,
      checksConstraints, checksWellFormed, typeSort, VariableKind.sort, monoRuntime]

theorem global_binding_visible_under_fresh_locals (locals globals : ActiveCells)
    (binder : BinderId) (cell : Xsh.Semantics.Environments.CellId)
    (unshadowed : locals binder = none) (global : globals binder = some cell) :
    withGlobals locals globals binder = some cell := by
  simp [withGlobals, unshadowed, global]


def invalidPropagationLocal : NameId := exampleName 23 (by decide)
def invalidPropagationBinder : BinderId := ⟨⟨22, by decide⟩⟩
def invalidPropagationSource : List Xsh.Syntax.Statement :=
  [.binding .letBinding (.name invalidPropagationLocal) none (.tryExpr invalidRecordSource),
   .returnValue (some (.literal (.integer "7")))]
def invalidPropagationCandidate : List StatementNode :=
  [.binding invalidPropagationBinder .immutable (.propagate invalidRecordCandidate (.primitive .error)),
   .returned (.integer "7")]

theorem propagation_outside_result_boundary_is_never_certified (types : BinderTypes)
    (steps : List DerivationStep) :
    validateDerivedBlock 32 exampleTypeNames schemaScope (.primitive .integer) invalidPropagationSource
      invalidPropagationCandidate types steps ≠ .ok () := by
  intro accepted
  obtain ⟨_, _, solution, _, _, _, _, matching, _⟩ :=
    derived_block_certificate_sound 32 exampleTypeNames schemaScope (.primitive .integer)
      invalidPropagationSource invalidPropagationCandidate types steps accepted
  have member : MonoType.primitive .error ∈ blockPropagationErrors invalidPropagationCandidate := by
    simp [invalidPropagationCandidate, invalidRecordCandidate, blockPropagationErrors, statementPropagationErrors, propagationErrors]
  obtain ⟨expected, impossible, _⟩ := matching (.primitive .error) member
  cases impossible


def guardedTrySource : Xsh.Syntax.Expr := .tryExpr recordSource
def guardedTryCandidate : Node := .propagate recordCandidate (.primitive .error)
def guardedTryDerivation : List DerivationStep := [.result, .bindRight, .bindRight, .reflexive]

def guardedTryAccepted : Bool :=
  match validateContextualExpression 32 schemaScope (some (.primitive .error)) guardedTrySource
      guardedTryCandidate guardedTryDerivation with
  | .ok (.nominal identity (.record _)) => decide (identity = recordNominal)
  | _ => false

def guardedTryWithoutContext : Bool :=
  match validateContextualExpression 32 schemaScope none guardedTrySource guardedTryCandidate [] with
  | .error .invalidContext => true
  | _ => false

theorem guarded_try_accepts_matching_context_and_reports_missing_context :
    guardedTryAccepted = true ∧ guardedTryWithoutContext = true := by
  constructor <;> decide +kernel

theorem guarded_try_with_wrong_error_context_is_never_certified (steps : List DerivationStep)
    (ty : MonoType) :
    validateContextualExpression 32 schemaScope (some (.primitive .boolean)) guardedTrySource
      guardedTryCandidate steps ≠ .ok ty := by
  intro accepted
  obtain ⟨_, _, _, solution, _, _, _, _, matching, _, _⟩ :=
    contextual_expression_certificate_sound 32 schemaScope (some (.primitive .boolean))
      guardedTrySource guardedTryCandidate steps ty accepted
  have member : MonoType.primitive .error ∈ propagationErrors guardedTryCandidate := by
    simp [guardedTryCandidate, recordCandidate, propagationErrors, recordErrors]
  obtain ⟨expected, same, matchingError⟩ := matching (.primitive .error) member
  simp only [Option.map, substitute, Option.some.injEq] at same
  simp only [substitute] at matchingError
  have impossible : MonoType.primitive .boolean = .primitive .error := same.trans matchingError.symm
  cases impossible


def branchContinuationSource : List Xsh.Syntax.Statement :=
  [.ifStmt [(.literal (.bool true), [.expression (.literal (.integer "1"))])]
      (some [.expression (.literal (.integer "2"))]),
   .returnValue (some (.literal (.integer "3")))]

def branchContinuationCandidate : List StatementNode :=
  [.conditional [(.boolean true, [.evaluated (.integer "1")])] [.evaluated (.integer "2")],
   .returned (.integer "3")]

def branchContinuationUnsupported : Bool :=
  match validateDerivedBlock 32 exampleTypeNames [] (.primitive .integer) branchContinuationSource
      branchContinuationCandidate (fun _ => none) [.reflexive, .reflexive] with
  | .error .unsupported => true
  | _ => false

def integerThree : IntValue := ⟨3, by decide⟩

theorem unsupported_non_tail_value_does_not_abort_machine (state : Machine) :
    branchContinuationUnsupported = true ∧
    Execution { state with control := .statements branchContinuationCandidate }
      [.internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .returned (.scalar (.integer integerThree)) } := by
  constructor
  · decide +kernel
  · let body : List StatementNode := [.evaluated (.integer "1")]
    let fallback : List StatementNode := [.evaluated (.integer "2")]
    let rest : List StatementNode := [.returned (.integer "3")]
    let branches : List (Node × List StatementNode) := [(.boolean true, body)]
    let branchFrame := Frame.branch branches fallback rest
    let blockFrame := Frame.continueBlock state.cells rest
    let withBranch := { state with stack := branchFrame :: state.stack }
    let withBlock := { state with stack := blockFrame :: state.stack }
    refine .next (Step.statementConditionalStart state (.boolean true) body [] fallback rest) ?_
    refine .next (Step.boolean withBranch true) ?_
    refine .next (Step.statementConditionalTrue state (.boolean true) body [] fallback rest) ?_
    refine .next (Step.evaluateTail withBlock (.integer "1")) ?_
    refine .next (Step.integer { withBlock with stack := .tailValue :: withBlock.stack } "1" intOne (by decide +kernel)) ?_
    refine .next (Step.tailValue withBlock (.scalar (.integer intOne))) ?_
    refine .next (Step.nonTailBranchComplete state (.scalar (.integer intOne)) state.cells _ []) ?_
    refine .next (Step.returnStart state (.integer "3") []) ?_
    refine .next (Step.integer { state with stack := .returnValue :: state.stack } "3" integerThree (by decide +kernel)) ?_
    exact .next (Step.returnValue state _) (.done _)


def okArguments : List (Variable × MonoType) :=
  [(identityVariable, .primitive .integer), (resultValueVariable, .primitive .string)]

theorem scheme_valued_ok_dispatches_after_read (state : Machine) (binder : BinderId)
    (declaration : DeclarationId)
    (foundValue : Connected.read state.store state.cells binder = some (.schemeFunction declaration okScheme))
    (foundFunction : state.functions declaration = some .resultOk) :
    Execution { state with control := .expression (.call (.read binder okArguments) [.integer "1"]) }
      [.internal, .internal, .internal, .internal, .internal, .internal]
      { state with control := .value (.ok (.scalar (.integer intOne)) .string) } := by
  let target : Value := .instantiatedFunction declaration (.argument .integer .argumentsEmpty)
    (.result .integer .string) state.solution okArguments
  let withTarget := { state with stack := .callTarget [.integer "1"] :: state.stack }
  let withArgument := { state with stack := .callArguments target [] [] :: state.stack }
  have instantiates : instantiateFunction state.solution okScheme okArguments =
      some (.argument .integer .argumentsEmpty, .result .integer .string, okArguments) := by
    have different : ((⟨1, .value⟩ : Variable) == ⟨0, .value⟩) = false := by decide
    simp [instantiateFunction, okArguments, instantiationKeysValid, instantiationSortsValid,
      witnessSubstitution, schemeSubstitution, okScheme, identityVariable, resultValueVariable,
      substitute, checksConstraints, checksWellFormed, typeSort, VariableKind.sort, monoRuntime, List.lookup, different]
  refine .next (Step.callStart state (.read binder okArguments) [.integer "1"]) ?_
  refine .next (Step.readFunction withTarget binder okArguments declaration okScheme _ _ okArguments foundValue instantiates) ?_
  refine .next (Step.callFirstArgument state target (.integer "1") []) ?_
  refine .next (Step.integer withArgument "1" intOne (by decide +kernel)) ?_
  refine .next (Step.callArgumentsComplete state target (.scalar (.integer intOne)) []) ?_
  exact .next (Step.callOkInstantiated state declaration (.scalar (.integer intOne)) .integer .string
    state.solution okArguments foundFunction rfl) (.done _)

end Xsh.Semantics.Connected
