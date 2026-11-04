import Xsh.Semantics.Connected.SafetyValueBlockInvariant

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

def ExpressionTarget : Control → Prop
  | .expression _ | .value _ | .statements _ => True
  | _ => False

theorem ground_expression_progress (state : Machine) (scope : Scope)
    (solution : Substitution) (storage : BindingStorageMap) (functionResult : MonoType) (node : Node) (ty : MonoType)
    (ground : GroundNode solution state.solution storage scope functionResult node ty)
    (values : ScopeValuesAgree scope solution state.cells state.store)
    (schemas : SchemaMatches scope solution state.schemas)
    (storageAgrees : StorageAgreement state.cells state.store storage) :
    ∃ next, Step { state with control := .expression node } .internal next ∧ ExpressionTarget next.control := by
  cases ground with
  | integer parsed => exact ⟨_, .integer state _ _ parsed, trivial⟩
  | boolean => exact ⟨_, .boolean state _, trivial⟩
  | string => exact ⟨_, .string state _, trivial⟩
  | read nodeTyped closed contract =>
    cases nodeTyped with
    | read found notSchema keys sorts instantiated solved wellFormed sort =>
      rename_i name binding raw requirements
      obtain ⟨value, readValue, agrees⟩ := values name binding found notSchema
      rcases agrees with ⟨declaration, parameters, result, schemeValue, schemeBody⟩ | ordinary
      · subst value
        have schemeStorage := storageAgrees.1 binding.binder declaration binding.scheme readValue
        obtain ⟨runtimeParameters, runtimeResult, normalized, instantiated⟩ :=
          contract name binding found rfl schemeStorage
        exact ⟨_, .readFunction state binding.binder _ declaration binding.scheme runtimeParameters runtimeResult normalized
          readValue instantiated.1, trivial⟩
      · exact ⟨_, .read state binding.binder _ value readValue ordinary.1, trivial⟩
  | binary => exact ⟨_, .binary state _ _ _, trivial⟩
  | call => exact ⟨_, .callStart state _ _, trivial⟩
  | record typed unique closed =>
    rename_i fields types runtimeType
    cases fields with
    | nil => exact ⟨_, .recordEmpty state, trivial⟩
    | cons field rest =>
      obtain ⟨name, value⟩ := field
      exact ⟨_, .recordStart state name value rest, trivial⟩
  | field => exact ⟨_, .fieldStart state _ _, trivial⟩
  | conditional branches fallback closed =>
    rename_i branchList otherwise runtimeType
    cases branchList with
    | nil => exact ⟨_, .conditionalEmpty state otherwise, trivial⟩
    | cons branch rest =>
      obtain ⟨condition, value⟩ := branch
      exact ⟨_, .conditionalStart state condition value rest otherwise, trivial⟩
  | requireValue nodeTyped valueTyped closed =>
    cases nodeTyped with
    | requireValue found kind quantified requirements body wellFormed input =>
      obtain ⟨guard, guardFound, base⟩ := schemas _ _ _ _ found kind body
      exact ⟨_, .requireStart state _ _ _ guard guardFound, trivial⟩
  | propagate => exact ⟨_, .propagateStart state _ _, trivial⟩
  | valueBlock => exact ⟨_, .valueBlockStart state _ _, trivial⟩

theorem typed_machine_expression_progress (state : Machine) (globalScope : Scope)
    (globalStorage : BindingStorageMap) (globalTypes : CellTypes) (node : Node)
    (invariant : TypedMachine globalScope globalStorage globalTypes state)
    (current : state.control = .expression node) :
    ∃ next, Step state .internal next ∧ ExpressionTarget next.control := by
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active, inputClosed, outputClosed, control, stack⟩ := invariant.focus
  rw [current] at control
  cases control with
  | expression ground closed fresh =>
    rename_i context ty runtimeType
    have activeScope : ScopeStateAgrees context.scope { state with solution := context.solution } ∧
        ScopeWritableContract context.scope context.solution state.cells state.store ∧
        SchemaMatches context.scope context.solution state.schemas ∧
        SchemeCellsImmutable state.cells state.store ∧ StorageAgreement state.cells state.store context.storage := by
      simpa [ActiveScopeTyped, current, TypedDemand.context] using active
    have groundActual : GroundNode context.solution state.solution context.storage context.scope context.result node ty := by
      simpa using runtimeSolution ▸ ground
    obtain ⟨next, step, safe⟩ := ground_expression_progress state context.scope context.solution context.storage context.result
      node ty groundActual activeScope.1.2.1 activeScope.2.2.1 activeScope.2.2.2.2
    have sameState : { state with control := .expression node } = state := by
      cases state
      simp_all
    rw [sameState] at step
    exact ⟨next, step, safe⟩

def StatementTarget : Control → Prop
  | .expression _ | .statements _ | .completed _ => True
  | _ => False

theorem ground_statements_progress (state : Machine) (solution runtimeSolution : Substitution)
    (storage : BindingStorageMap) (scope : Scope) (result : MonoType) (completion : Option MonoType)
    (body : List StatementNode) (ground : GroundBlock solution runtimeSolution storage scope result completion body)
    (runtimeMatch : runtimeSolution = state.solution) :
    ∃ next, Step { state with control := .statements body } .internal next ∧ StatementTarget next.control := by
  cases ground with
  | nil accepts => exact ⟨_, .blockComplete state, trivial⟩
  | binding initializer compatible closed runtimeClosed kind storage fresh rest =>
    exact ⟨_, .bindingStart state _ _ _ _ _ _ (by simpa [runtimeMatch] using runtimeClosed), trivial⟩
  | assignment => exact ⟨_, .assignmentStart state _ _ _, trivial⟩
  | returned => exact ⟨_, .returnStart state _ _, trivial⟩
  | evaluatedTail => exact ⟨_, .evaluateTail state _, trivial⟩
  | evaluatedNext => exact ⟨_, .evaluateAndContinue state _ _ _, trivial⟩
  | conditionalTail branches fallback =>
    rename_i branchList otherwise
    cases branchList with
    | nil => exact ⟨_, .statementConditionalEmpty state otherwise [], trivial⟩
    | cons branch rest =>
      obtain ⟨condition, body⟩ := branch
      exact ⟨_, .statementConditionalStart state condition body rest otherwise [], trivial⟩
  | conditionalNext branches fallback following =>
    rename_i branchList otherwise next tail
    cases branchList with
    | nil => exact ⟨_, .statementConditionalEmpty state otherwise (next :: tail), trivial⟩
    | cons branch rest =>
      obtain ⟨condition, body⟩ := branch
      exact ⟨_, .statementConditionalStart state condition body rest otherwise (next :: tail), trivial⟩

theorem typed_machine_statements_progress (state : Machine) (globalScope : Scope)
    (globalStorage : BindingStorageMap) (globalTypes : CellTypes) (body : List StatementNode)
    (invariant : TypedMachine globalScope globalStorage globalTypes state)
    (current : state.control = .statements body) :
    ∃ next, Step state .internal next ∧ StatementTarget next.control := by
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active, inputClosed, outputClosed, control, stack⟩ := invariant.focus
  rw [current] at control
  cases control with
  | statements ground fresh =>
    rename_i context
    obtain ⟨next, step, safe⟩ := ground_statements_progress state context.solution context.runtimeSolution
      context.storage context.scope context.result context.completion body ground runtimeSolution
    have sameState : { state with control := .statements body } = state := by
      cases state
      simp_all
    rw [sameState] at step
    exact ⟨next, step, safe⟩

end Xsh.Semantics.Connected
