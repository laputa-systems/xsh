import Xsh.Semantics.Connected.SafetyValueBlockBranchComplete

namespace Xsh.Semantics.Connected

open Xsh.Typing.Connected

theorem ground_empty_branch (solution runtimeSolution : Substitution) (storage : BindingStorageMap)
    (scope : Scope) (result : MonoType) (completion : Option MonoType) (otherwise rest : List StatementNode)
    (ground : GroundBlock solution runtimeSolution storage scope result completion (.conditional [] otherwise :: rest)) :
    GroundBlock solution runtimeSolution storage scope result (if rest.isEmpty then completion else none) otherwise ∧
      (rest ≠ [] → GroundBlock solution runtimeSolution storage scope result completion rest) := by
  cases ground with
  | conditionalTail branches fallback => exact ⟨by simpa using fallback, by simp⟩
  | conditionalNext branches fallback rest => exact ⟨by simpa using fallback, fun _ => rest⟩

theorem typed_machine_empty_statement_branch (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (otherwise rest : List StatementNode)
    (typed : TypedMachine globals storage types { base with control := .statements (.conditional [] otherwise :: rest) }) :
    TypedMachine globals storage types
      { base with control := .statements otherwise, stack := .continueBlock base.cells rest :: base.stack } := by
  have step := Step.statementConditionalEmpty base otherwise rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | statements ground fresh =>
    rename_i context
    simp only [TypedDemand.context] at runtimeSolution activation storageEqual proofSorted
    obtain ⟨fallback, following⟩ := ground_empty_branch context.solution context.runtimeSolution context.storage
      context.scope context.result context.completion otherwise rest ground
    let inner : TypingContext := { context with completion := if rest.isEmpty then context.completion else none }
    have newPending : PendingBindingsSafe
        { base with control := .statements otherwise, stack := .continueBlock base.cells rest :: base.stack } := by
      constructor
      · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
          frameCallerActivation, declaredBinders, branchBinders, List.map_append, List.append_assoc] using pending.unique
      · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
          frameCallerActivation, declaredBinders, branchBinders, List.map_append, List.append_assoc] using pending.fresh
    have fallbackFresh : FutureBindingsFresh base.store context.activation otherwise := by
      intro binder member
      apply fresh binder
      simp [declaredBinders, branchBinders, member]
    have restFresh : FutureBindingsFresh base.store context.activation rest := by
      intro binder member
      apply fresh binder
      simp [declaredBinders, branchBinders, member]
    obtain ⟨scope, writable, schemas, schemes, storageAgrees⟩ := active
    have oldStack : StackTyped
        { base with control := .statements otherwise, stack := .continueBlock base.cells rest :: base.stack }
        (.block context) output base.stack := by
      apply stack_typing_survives_pending step stack
      intro binding member
      apply newPending.fresh binding
      simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
      right
      right
      simpa [TypedDemand.context, activation, frameCallerActivation] using member
    have frame : FrameTyped
        { base with control := .statements otherwise, stack := .continueBlock base.cells rest :: base.stack }
        (.block inner) (.block context) (.continueBlock base.cells rest) := by
      exact .continueBlock rfl rfl rfl rfl rfl rfl (by simpa [ScopeStateAgrees, TypedDemand.context] using scope)
        writable schemes storageAgrees schemas following restFresh
    have innerClosed : DemandClosed (.block inner) := by
      refine ⟨context.resultClosed, ?_⟩
      intro ty equal
      by_cases empty : rest.isEmpty
      · have outerEqual : context.completion = some ty := by simpa [inner, empty] using equal
        exact inputClosed.2 ty outerEqual
      · simp [inner, empty] at equal
    refine ⟨.block inner, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
      ?_, innerClosed, outputClosed, .statements fallback fallbackFresh, .cons frame inputClosed oldStack, newPending⟩
    exact ⟨by simpa [inner, ScopeStateAgrees, TypedDemand.context] using scope, writable, schemas, schemes, storageAgrees⟩

theorem ground_statement_branch (solution runtimeSolution : Substitution) (storage : BindingStorageMap)
    (scope : Scope) (result : MonoType) (completion : Option MonoType)
    (branches : List (Node × List StatementNode)) (otherwise rest : List StatementNode)
    (ground : GroundBlock solution runtimeSolution storage scope result completion (.conditional branches otherwise :: rest)) :
    GroundStatementBranches solution runtimeSolution storage scope result (if rest.isEmpty then completion else none) branches ∧
      GroundBlock solution runtimeSolution storage scope result (if rest.isEmpty then completion else none) otherwise ∧
      (rest ≠ [] → GroundBlock solution runtimeSolution storage scope result completion rest) := by
  cases ground with
  | conditionalTail branches fallback => exact ⟨by simpa using branches, by simpa using fallback, by simp⟩
  | conditionalNext branches fallback rest => exact ⟨by simpa using branches, by simpa using fallback, fun _ => rest⟩

theorem typed_machine_statement_branch_start (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (condition : Node) (body : List StatementNode) (branches : List (Node × List StatementNode))
    (otherwise rest : List StatementNode)
    (typed : TypedMachine globals storage types
      { base with control := .statements (.conditional ((condition, body) :: branches) otherwise :: rest) }) :
    TypedMachine globals storage types
      { base with control := .expression condition, stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack } := by
  have step := Step.statementConditionalStart base condition body branches otherwise rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | statements ground fresh =>
    rename_i context
    simp only [TypedDemand.context] at runtimeSolution activation storageEqual proofSorted
    obtain ⟨allBranches, fallback, following⟩ := ground_statement_branch context.solution context.runtimeSolution context.storage
      context.scope context.result context.completion ((condition, body) :: branches) otherwise rest ground
    have newPending : PendingBindingsSafe
        { base with control := .expression condition, stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack } := by
      constructor
      · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
          frameCallerActivation, declaredBinders, branchBinders, List.map_append, List.append_assoc] using pending.unique
      · simpa [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
          frameCallerActivation, declaredBinders, branchBinders, List.map_append, List.append_assoc] using pending.fresh
    have conditionFresh : FutureNodeBindingsFresh base.store context.activation condition := by
      intro binder member
      apply fresh binder
      simp [declaredBinders, branchBinders, member]
    have frameFresh := pending_head_frame_fresh
      { base with control := .expression condition, stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }
      (.expression context (.primitive .boolean)) (.branch ((condition, body) :: branches) otherwise rest) base.stack
      newPending (by simpa [TypedDemand.context] using activation.symm) rfl
    have frame : FrameTyped
        { base with control := .expression condition, stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }
        (.expression context (.primitive .boolean)) (.block context) (.branch ((condition, body) :: branches) otherwise rest) := by
      exact .branch (by simp) allBranches fallback following frameFresh.1 frameFresh.2.1 frameFresh.2.2
    have oldStack : StackTyped
        { base with control := .expression condition, stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }
        (.block context) output base.stack := by
      apply stack_typing_survives_pending step stack
      intro binding member
      apply newPending.fresh binding
      simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
      right
      right
      simpa [TypedDemand.context, activation, frameCallerActivation] using member
    cases allBranches with
    | cons conditionTyped bodyTyped remaining =>
      refine ⟨.expression context (.primitive .boolean), output, runtimeSolution, activation, storageEqual,
        proofSorted, runtimeSorted, ?_, ⟨.boolean, rfl⟩, outputClosed,
        .expression conditionTyped rfl conditionFresh, .cons frame inputClosed oldStack, newPending⟩
      simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

theorem typed_machine_statement_branch_true (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (condition : Node) (body : List StatementNode) (branches : List (Node × List StatementNode))
    (otherwise rest : List StatementNode)
    (typed : TypedMachine globals storage types
      { base with control := .value (.scalar (.boolean true)), stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }) :
    TypedMachine globals storage types
      { base with control := .statements body, stack := .continueBlock base.cells rest :: base.stack } := by
  have step := Step.statementConditionalTrue base condition body branches otherwise rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed callable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | branch nonempty allBranches fallback following branchFresh fallbackFresh restFresh =>
        rename_i context runtimeType
        simp only [TypedDemand.context] at runtimeSolution activation storageEqual proofSorted
        cases allBranches with
        | cons conditionTyped bodyTyped branchesTyped =>
          let inner : TypingContext := { context with completion := if rest.isEmpty then context.completion else none }
          have subset : List.Sublist
              (pendingMachineBinders { base with control := .statements body, stack := .continueBlock base.cells rest :: base.stack })
              (pendingMachineBinders { base with control := .value (.scalar (.boolean true)), stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }) := by
            simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation, List.map_nil, List.nil_append, List.map_append, List.append_assoc]
            apply List.Sublist.append_left
            simp
          have newPending := pending_safe_of_sublist
            { base with control := .value (.scalar (.boolean true)), stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }
            { base with control := .statements body, stack := .continueBlock base.cells rest :: base.stack } rfl subset pending
          have bodyFresh : FutureBindingsFresh base.store context.activation body := by
            intro binder member
            apply newPending.fresh (context.activation, binder)
            simp [pendingMachineBinders, pendingControlBinders, activation, member]
          obtain ⟨scope, writable, schemas, schemes, storageAgrees⟩ := active
          have oldStack : StackTyped
              { base with control := .statements body, stack := .continueBlock base.cells rest :: base.stack }
              (.block context) output base.stack := by
            apply stack_typing_survives_pending step remaining
            intro binding member
            apply newPending.fresh binding
            simp only [pendingMachineBinders, pendingStackBinders, List.mem_append]
            right
            right
            simpa [TypedDemand.context, activation, frameCallerActivation] using member
          have frame : FrameTyped
              { base with control := .statements body, stack := .continueBlock base.cells rest :: base.stack }
              (.block inner) (.block context) (.continueBlock base.cells rest) := by
            exact .continueBlock rfl rfl rfl rfl rfl rfl (by simpa [ScopeStateAgrees, TypedDemand.context] using scope)
              writable schemes storageAgrees schemas following restFresh
          have innerClosed : DemandClosed (.block inner) := by
            refine ⟨context.resultClosed, ?_⟩
            intro ty equal
            by_cases empty : rest.isEmpty
            · have outerEqual : context.completion = some ty := by simpa [inner, empty] using equal
              exact middleClosed.2 ty outerEqual
            · simp [inner, empty] at equal
          refine ⟨.block inner, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, innerClosed, outputClosed, .statements bodyTyped bodyFresh, .cons frame middleClosed oldStack, newPending⟩
          exact ⟨by simpa [inner, ScopeStateAgrees, TypedDemand.context] using scope, writable, schemas, schemes, storageAgrees⟩

theorem typed_machine_statement_branch_false (globals : Scope) (storage : BindingStorageMap) (types : CellTypes)
    (base : Machine) (condition : Node) (body : List StatementNode) (branches : List (Node × List StatementNode))
    (otherwise rest : List StatementNode)
    (typed : TypedMachine globals storage types
      { base with control := .value (.scalar (.boolean false)), stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }) :
    TypedMachine globals storage types { base with control := .statements (.conditional branches otherwise :: rest) } := by
  have step := Step.statementConditionalFalse base condition body branches otherwise rest
  obtain ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues⟩ :=
    typed_machine_world_preserved globals storage types typed step
  refine ⟨memory, functions, globalDistinct, globalSchemes, globalStorage, storedValues, ?_⟩
  obtain ⟨input, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted, active,
    inputClosed, outputClosed, control, stack, pending⟩ := typed.focus
  cases control with
  | value valueTyped valueClosed callable =>
    cases stack with
    | cons frameTyped middleClosed remaining =>
      cases frameTyped with
      | branch nonempty allBranches fallback following branchFresh fallbackFresh restFresh =>
        rename_i context runtimeType
        simp only [TypedDemand.context] at runtimeSolution activation storageEqual proofSorted
        cases allBranches with
        | cons conditionTyped bodyTyped branchesTyped =>
          have subset : List.Sublist
              (pendingMachineBinders { base with control := .statements (.conditional branches otherwise :: rest) })
              (pendingMachineBinders { base with control := .value (.scalar (.boolean false)), stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }) := by
            simp only [pendingMachineBinders, pendingControlBinders, pendingStackBinders, pendingFrameBinders,
              frameCallerActivation, declaredBinders, List.map_nil, List.nil_append, List.map_append, List.append_assoc]
            exact List.sublist_append_right _ _
          have newPending := pending_safe_of_sublist
            { base with control := .value (.scalar (.boolean false)), stack := .branch ((condition, body) :: branches) otherwise rest :: base.stack }
            { base with control := .statements (.conditional branches otherwise :: rest) } rfl subset pending
          have fresh : FutureBindingsFresh base.store context.activation (.conditional branches otherwise :: rest) := by
            intro binder member
            apply newPending.fresh (context.activation, binder)
            simp [pendingMachineBinders, pendingControlBinders, activation, member]
          have block : GroundBlock context.solution context.runtimeSolution context.storage context.scope context.result
              context.completion (.conditional branches otherwise :: rest) := by
            cases rest with
            | nil => exact .conditionalTail (by simpa using branchesTyped) (by simpa using fallback)
            | cons next rest => exact .conditionalNext (by simpa using branchesTyped) (by simpa using fallback) (following (by simp))
          have oldStack : StackTyped { base with control := .statements (.conditional branches otherwise :: rest) }
              (.block context) output base.stack := by
            apply stack_typing_survives_with_pending step remaining newPending
            · simpa [TypedDemand.context] using activation.symm
            · rfl
          refine ⟨.block context, output, runtimeSolution, activation, storageEqual, proofSorted, runtimeSorted,
            ?_, middleClosed, outputClosed, .statements block fresh, oldStack, newPending⟩
          simpa [ActiveScopeTyped, ScopeStateAgrees, TypedDemand.context] using active

end Xsh.Semantics.Connected
