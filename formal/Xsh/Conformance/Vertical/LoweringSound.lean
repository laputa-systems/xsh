import Xsh.Conformance.Vertical.LoweringSoundBase

namespace Xsh.Conformance.Vertical

open Xsh.Foundation
open Xsh.Conformance.Transport
open Xsh.IR.Raw
open Xsh.Semantics.BaseMachine
open Xsh.Values

set_option maxHeartbeats 800000 in
theorem function_lowers_record_one {src : List (String × Option Callee)} {s s' : LoweringState}
    {name parameter target : String} {fields : List (String × SourceValue)} {result : RecordValue}
    (wf : BlocksIndexed s)
    (h : Lower.function ⟨name, [parameter], [], none, .recordLocal target fields result⟩ s = some ((), s')) :
    ∃ program row, (⟨name, [parameter], [], none, .recordLocal target fields result⟩ : SourceFunction).program? src = some program ∧
      program.callee?.isSome = (⟨name, [parameter], [], none, .recordLocal target fields result⟩ : SourceFunction).callable ∧
      program.parameters = 1 ∧ s'.functions = s.functions ++ [row] ∧
      s'.callable = s.callable ++ [(⟨name, [parameter], [], none, .recordLocal target fields result⟩ : SourceFunction).callable] ∧
      StateLE s s' ∧ BlocksIndexed s' ∧ row.name = name ∧ row.paramsLen.val = 1 ∧
      ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId) (observations : List Observation),
        functionProgram? (F.toArtifact sourceId observations) (src.map (·.2)) row = some program := by
  obtain ⟨scope, owner, binding, s6, hscope, howner, admitted, hrecord, tail⟩ :=
    record_function_run h
  have initLe : StateLE s { s with strings := (addSourceStrings s.strings (⟨name, [parameter], [], none, .recordLocal target fields result⟩ : SourceFunction)) } := by
    exact ⟨List.prefix_refl _, List.prefix_refl _, List.prefix_refl _, List.prefix_refl _,
      fun _ => add_source_strings_prefix _ _, fun _ => List.prefix_refl _⟩
  have wf5 : BlocksIndexed { s with strings := (addSourceStrings s.strings (⟨name, [parameter], [], none, .recordLocal target fields result⟩ : SourceFunction)) } := wf
  obtain ⟨program, row, source, callable, parameters, fa, fb, le, finalWf,
    hname, hparams, reads⟩ := record_function_after_init (src := src) wf5 hscope admitted hrecord tail
  exact ⟨program, row, source, callable, parameters, by simpa using fa,
    by simpa using fb, initLe.trans le, finalWf, hname, hparams, reads⟩


theorem function_lowers_record {src : List (String × Option Callee)} {f : SourceFunction}
    {s s' : LoweringState} {target : String} {fields : List (String × SourceValue)}
    {result : RecordValue} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (record : f.result = .recordLocal target fields result)
    (h : Lower.function f s = some ((), s')) :
    ∃ program row, f.program? src = some program ∧
      program.callee?.isSome = (f.callable) ∧
      program.parameters = f.parameters.length ∧ s'.functions = s.functions ++ [row] ∧
      s'.callable = s.callable ++ [f.callable] ∧ StateLE s s' ∧ BlocksIndexed s' ∧
      row.name = f.name ∧ row.paramsLen.val = f.parameters.length ∧
      ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId) (observations : List Observation),
        functionProgram? (F.toArtifact sourceId observations) (src.map (·.2)) row = some program := by
  obtain ⟨name, parameters, lets, propagation, sourceResult⟩ := f
  dsimp only at record
  subst sourceResult
  cases propagation with
  | some propagation =>
    simp [Lower.function, SourceExpression.isRecordLocal, Lower.fail, run_bind,
      run_lift, run_get, run_require] at h
  | none =>
    cases lets with
    | cons first rest =>
      cases rest with
      | nil =>
        simp [Lower.function, Lower.computedLetStatements, Lower.fail, run_bind,
          run_lift, run_get] at h
      | cons second more =>
        simp [Lower.function, Lower.fail, run_bind, run_lift, run_get] at h
    | nil =>
      cases parameters with
      | nil =>
        simp [Lower.function, SourceExpression.isRecordLocal, Lower.fail, run_bind,
          run_lift, run_get] at h
      | cons parameter rest =>
        cases rest with
        | cons second more =>
          simp [Lower.function, SourceExpression.isRecordLocal, Lower.fail, run_bind,
            run_lift, run_get] at h
        | nil =>
          simpa using (function_lowers_record_one (src := src) wf h)


theorem function_lowers {src : List (String × Option Callee)} {f : SourceFunction}
    {s s' : LoweringState} (wf : BlocksIndexed s) (aligned : Aligned s src)
    (h : Lower.function f s = some ((), s')) :
    ∃ program row, f.program? src = some program ∧
      program.callee?.isSome = (f.callable) ∧
      program.parameters = f.parameters.length ∧ s'.functions = s.functions ++ [row] ∧
      s'.callable = s.callable ++ [f.callable] ∧
      StateLE s s' ∧ BlocksIndexed s' ∧
      row.name = f.name ∧ row.paramsLen.val = f.parameters.length ∧
      ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId) (observations : List Observation),
        functionProgram? (F.toArtifact sourceId observations) (src.map (·.2)) row = some program := by
  by_cases isRecord : f.result.isRecordLocal = true
  · cases form : f.result <;> simp only [form, SourceExpression.isRecordLocal, Bool.false_eq_true] at isRecord
    case recordLocal target fields result =>
      exact function_lowers_record wf aligned form h
  · have noRecord : f.result.isRecordLocal = false := by
      cases flag : f.result.isRecordLocal <;> simp_all
    by_cases computed : ∃ (n : String) (v r : SourceValue), f.result = .computedLet n v r
    · obtain ⟨n, v, r, same⟩ := computed
      cases propagationCase : f.propagation with
      | some propagation =>
          simpa [Option.isNone, propagationCase] using
            function_lowers_propagated wf aligned propagationCase h
      | none =>
          simpa [Option.isNone, propagationCase] using
            function_lowers_computed wf aligned propagationCase same h
    · by_cases sequenced : ∃ (bindings : List (String × SourceValue)) (r : SourceValue),
          f.result = .computedLets bindings r
      · obtain ⟨bindings, r, same⟩ := sequenced
        cases propagationCase : f.propagation with
        | some propagation =>
            simpa [Option.isNone, propagationCase] using
              function_lowers_propagated wf aligned propagationCase h
        | none =>
            simpa [Option.isNone, propagationCase] using
              function_lowers_computedLets wf aligned propagationCase same h
      · cases propagationCase : f.propagation with
        | some propagation =>
            simpa [Option.isNone, propagationCase] using
              function_lowers_propagated wf aligned propagationCase h
        | none =>
            cases hassigned : f.result.isAssigned with
            | false =>
                simpa [Option.isNone, propagationCase] using
                  function_lowers_plain wf aligned propagationCase
                    (fun n v r same => computed ⟨n, v, r, same⟩)
                    (fun bindings r same => sequenced ⟨bindings, r, same⟩) noRecord hassigned h
            | true =>
                simpa [Option.isNone, propagationCase] using
                  function_lowers_assigned wf aligned propagationCase hassigned h


theorem callee_parameters {program : FunctionProgram} {callee : Callee}
    (found : program.callee? = some callee) : callee.parameters = program.parameters := by
  unfold FunctionProgram.callee? at found
  split at found
  · cases assignment : program.assignment with
    | none =>
      simp only [assignment] at found
      split at found <;> (simp only [Option.some.injEq] at found; subst found; rfl)
    | some assignment =>
      simp only [assignment] at found
      unfold assignedCallee? at found
      split at found
      · split at found
        · split at found
          · obtain ⟨initial, _, equality⟩ := Option.map_eq_some_iff.mp found
            subst equality
            rfl
          · simp at found
        · simp at found
      · simp at found
  · simp at found


theorem assigned_literal_call_begins_with_the_assignment (slot : Word32) (initial : IntValue)
    (assigned returned : BaseExpr)
    (valid : (⟨slot, .integerLiteral initial.val⟩ : LetBinding).literalInt? = some initial) :
    let callee : Callee :=
      ⟨0, returned, [⟨slot, substitute [(NameId.mk slot, initial)] assigned⟩]⟩
    (⟨0, [⟨slot, .integerLiteral initial.val⟩], some ⟨slot, assigned⟩, none,
      returned, none⟩ : FunctionProgram).callee? = some callee ∧
    step (machineOf [] (callee.apply [])) =
      some { machineOf [] (substituteExpressions [] (substitute [(NameId.mk slot, initial)] assigned))
        with continuation := [.callArgument [NameId.mk slot] returned [] []] } := by
  constructor
  · exact assigned_literal_function_is_callable slot initial assigned returned valid
  · rfl


theorem lower_all :
    ∀ (functions : List SourceFunction) {s s' : LoweringState}
      {src : List (String × Option Callee)} {programs : List (Option FunctionProgram)},
      BlocksIndexed s → Aligned s src →
      src.map (·.2) = programs.map (fun program => program.bind (·.callee?)) →
      (functions.forM Lower.function) s = some ((), s') →
      ∃ rows, s'.functions = s.functions ++ rows ∧ StateLE s s' ∧ BlocksIndexed s' ∧
        ∀ (F : LoweringState), StateLE s' F → ∀ (sourceId : BridgeId)
          (observations : List Observation),
          rows.foldl (programStep (F.toArtifact sourceId observations)) programs =
            (functions.foldl sourceStep (src, programs)).2 := by
  intro functions
  induction functions with
  | nil =>
    intro s s' src programs wf aligned link h
    unfold List.forM at h
    simp only [run_pure] at h
    obtain ⟨_, rfl⟩ := h
    exact ⟨[], by simp, StateLE.refl _, wf, fun _ _ _ _ => rfl⟩
  | cons f rest ih =>
    intro s s' src programs wf aligned link h
    unfold List.forM at h
    simp only [run_bind] at h
    obtain ⟨_, s1, hf, hrest⟩ := h
    obtain ⟨program, row, hprogram, hbindings, hparameters, fa, fb, le1, wf1, hname, hwidth,
      reads⟩ := function_lowers wf aligned hf
    have aligned1 : Aligned s1 (src ++ [(f.name, program.callee?)]) := by
      obtain ⟨names, flags, widths⟩ := aligned
      have sameLength : s.functions.length = src.length := by
        simpa using congrArg List.length names
      refine ⟨?_, ?_, ?_⟩
      · rw [fa]; simp [names, hname]
      · rw [fb]; simp [flags, hbindings]
      · intro index found callee located callee_at
        rw [fa] at located
        by_cases old : index < s.functions.length
        · rw [List.getElem?_append_left old] at located
          rw [List.getElem?_append_left (by omega)] at callee_at
          exact widths index found callee located callee_at
        · have atEnd : index = s.functions.length := by
            have := (List.getElem?_eq_some_iff.mp located).1
            simp at this
            omega
          subst atEnd
          rw [List.getElem?_append_right (by omega)] at located callee_at
          simp only [Nat.sub_self, List.getElem?_cons_zero, Option.some.injEq] at located
          simp only [sameLength, Nat.sub_self, List.getElem?_cons_zero, Option.bind_some] at callee_at
          subst located
          rw [hwidth, callee_parameters callee_at, hparameters]
    have link1 : (src ++ [(f.name, program.callee?)]).map (·.2) =
        (programs ++ [some program]).map (fun program => program.bind (·.callee?)) := by
      simp [link]
    obtain ⟨rows, ra, le2, wf2, folded⟩ := ih wf1 aligned1 link1 hrest
    refine ⟨row :: rows, by rw [ra, fa]; simp, le1.trans le2, wf2, ?_⟩
    intro F leF sourceId observations
    have step := reads F (le2.trans leF) sourceId observations
    have e1 : programStep (F.toArtifact sourceId observations) programs row =
        programs ++ [some program] := by
      unfold programStep
      rw [← link, step]
    have e2 : sourceStep (src, programs) f =
        (src ++ [(f.name, program.callee?)], programs ++ [some program]) := by
      unfold sourceStep
      simp only [hprogram]
      rfl
    rw [List.foldl_cons, List.foldl_cons, e1, e2]
    exact folded F leF sourceId observations


theorem lower_functions_run {spans : List Word32} {functions : List SourceFunction}
    {state : LoweringState} (h : lowerFunctions spans functions = some state) :
    (functions.forM Lower.function) { spans } = some ((), state) := by
  unfold lowerFunctions at h
  simp only [Option.bind_eq_bind, check_bind] at h
  split at h
  · generalize hrun : StateT.run (forM functions Lower.function) { spans := spans } = result at h
    cases result with
    | none => simp at h
    | some pair =>
      obtain ⟨u, ran⟩ := pair
      change (if ran.spans.isEmpty = true then some ran else none) = some state at h
      split at h
      · simp only [Option.some.injEq] at h
        subst h
        cases u
        exact hrun
      · simp at h
  · simp at h


/-- Lowering is correct for the admitted shape: whenever the Lean lowerer succeeds on parsed
source functions, the base-machine program the raw-IR rows it emits denote, function by function,
is the program the source tree denotes. The statement is universal over the parsed functions and
the span words; it concerns the Lean lowerer, not the Rust compiler. -/
theorem lowering_means_the_source {spans : List Word32} {functions : List SourceFunction}
    {state : LoweringState} (lowered : lowerFunctions spans functions = some state)
    (sourceId : BridgeId) (observations : List Observation) :
    functionPrograms (state.toArtifact sourceId observations) = sourcePrograms functions := by
  have run := lower_functions_run lowered
  obtain ⟨rows, rowsEq, _, _, folded⟩ := lower_all functions (s := { spans := spans })
    (src := []) (programs := []) (fun index block located => by simp at located)
    ⟨rfl, rfl, fun index row callee located => by simp at located⟩ rfl run
  have := folded state (StateLE.refl _) sourceId observations
  unfold functionPrograms sourcePrograms
  show state.functions.foldl (programStep (state.toArtifact sourceId observations)) [] = _
  rw [rowsEq]
  simpa using this

theorem lowers_to_means_the_source {artifact : RawArtifact} {functions : List SourceFunction}
    (lowers : lowersTo artifact functions = true) :
    functionPrograms artifact = sourcePrograms functions := by
  unfold lowersTo at lowers
  cases lowered : lowerFunctions (spanWords artifact) functions with
  | none => simp [lowered] at lowers
  | some state =>
    simp only [lowered, beq_iff_eq] at lowers
    rw [← lowers]
    exact lowering_means_the_source lowered _ _


/-- A certified pair means the same under its source tree and its IR: the source text parses to
functions, the artifact is the lowerer's output on them, and their base-machine programs are the
programs of the artifact's functions, function by function. -/
theorem certified_pair_means_the_same {source : SourceDocument} {raw : RawArtifact}
    {results : List ObservedResult} (certified : certifyDecoded source raw = some results) :
    ∃ functions, parseSource source.text = some functions ∧
      functionPrograms raw = sourcePrograms functions ∧ validateArtifact raw = true ∧
      namedResults raw = some results := by
  obtain ⟨functions, _, parsed, lowers, valid, named⟩ := certify_decoded_sound certified
  exact ⟨functions, parsed, lowers_to_means_the_source lowers, valid, named⟩

theorem source_atom_has_pure_evaluation (binding : String) (slot : Word32)
    {atom : SourceAtom} {expression : BaseExpr}
    (admitted : atom.expression? [(binding, slot.val)] = some expression) (value : IntValue) :
    ∃ result, PureIntegerEvaluation ([(NameId.mk slot, value)].lookup ·) expression result ∧
      pureIntegerCost expression = 1 := by
  cases atom with
  | int literal =>
      simp only [SourceAtom.expression?] at admitted
      split at admitted
      · cases admitted
        exact ⟨_, .literal _, rfl⟩
      · simp at admitted
  | var name =>
      simp only [SourceAtom.expression?, Option.bind_eq_bind, Option.bind_eq_some_iff, Option.some.injEq] at admitted
      obtain ⟨foundSlot, located, word, decoded, rfl⟩ := admitted
      have slotIs : foundSlot = slot.val := by
        simp only [List.lookup] at located
        split at located
        · exact (Option.some.inj located).symm
        · simp at located
      subst foundSlot
      have wordIs : word = slot := Fin.ext (decode_val decoded)
      subst word
      refine ⟨.value value, .read ?_, rfl⟩
      simp [List.lookup]

theorem source_value_has_pure_evaluation (binding : String) (slot : Word32)
    {source : SourceValue} {expression : BaseExpr}
    (admitted : source.expression? [(binding, slot.val)] = some expression) (value : IntValue) :
    ∃ result, PureIntegerEvaluation ([(NameId.mk slot, value)].lookup ·) expression result ∧
      pureIntegerCost expression ≤ 5 := by
  cases source with
  | atom atom =>
      obtain ⟨result, evaluated, cost⟩ := source_atom_has_pure_evaluation binding slot admitted value
      exact ⟨result, evaluated, by omega⟩
  | binary operator left right =>
      simp only [SourceValue.expression?, Option.bind_eq_bind] at admitted
      split at admitted
      · simp at admitted
      · simp only [Option.bind_eq_some_iff, Option.some.injEq] at admitted
        obtain ⟨leftExpr, leftRead, rightExpr, rightRead, rfl⟩ := admitted
        obtain ⟨leftResult, leftRuns, leftCost⟩ := source_atom_has_pure_evaluation binding slot leftRead value
        obtain ⟨rightResult, rightRuns, rightCost⟩ := source_atom_has_pure_evaluation binding slot rightRead value
        rename_i arithmetic
        cases operator <;> simp [FragmentOp.isComparison] at arithmetic
        all_goals refine ⟨_, .binary _ leftRuns rightRuns, ?_⟩
        all_goals simp [pureIntegerCost, FragmentOp.expression, leftCost, rightCost]

theorem assigned_source_program_read (name binding : String) (initial : Nat)
    (assigned returned : SourceValue) (callees : List (String × Option Callee)) :
    (⟨name, [], [(binding, initial)], none,
      .assigned binding assigned returned⟩ : SourceFunction).program? callees =
      (do
        let bounded ← validateInt? (initial : Int)
        let assignedExpr ← assigned.expression? [(binding, letSlot)]
        let returnedExpr ← returned.expression? [(binding, letSlot)]
        some (⟨0, [⟨⟨letSlot, by decide⟩, .integerLiteral bounded.val⟩],
          some ⟨⟨letSlot, by decide⟩, assignedExpr⟩, none, returnedExpr, none⟩ : FunctionProgram)) := by
  have sourceScope : (⟨name, [], [(binding, initial)], none,
      .assigned binding assigned returned⟩ : SourceFunction).scope? = some [(binding, letSlot)] := rfl
  have lookup : [(binding, letSlot)].lookup binding = some letSlot := by simp [List.lookup]
  have decoded : Word32.decode? letSlot = some (⟨letSlot, by decide⟩ : Word32) := by decide

  cases checked : validateInt? (initial : Int) <;>
    cases assignedRead : assigned.expression? [(binding, letSlot)] <;>
    cases returnedRead : returned.expression? [(binding, letSlot)] <;>
    simp [SourceFunction.program?, SourceFunction.recordProgram?, sourceScope, lookup, decoded, SourceFunction.assignment?,
      SourceExpression.expression?, checked, assignedRead, returnedRead, Option.bind_eq_bind]


/-- Success of the existing source-program projection for a nullary literal local, assignment
and integer return supplies the pure-evaluation premises. Twelve transitions suffice for the
normalized call, which has the direct settlement's observation. -/
theorem source_assignment_normalization_is_correct (name binding : String) (initial : Nat)
    (assigned returned : SourceValue) (callees : List (String × Option Callee))
    {program : FunctionProgram}
    (admitted : (⟨name, [], [(binding, initial)], none,
      .assigned binding assigned returned⟩ : SourceFunction).program? callees = some program) :
    ∃ callee result, program.callee? = some callee ∧
      program.outcome? [] = some (integerFunctionOutcome result) ∧
      ∀ fuel, 12 ≤ fuel → runFuel fuel (machineOf [] (callee.apply [])) =
        .completed (integerBaseOutcome result) := by
  rw [assigned_source_program_read name binding initial assigned returned callees] at admitted
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.some.injEq] at admitted
  obtain ⟨bounded, checked, assignedExpr, assignedRead, returnedExpr, returnedRead, programIs⟩ := admitted
  subst program
  obtain ⟨assignedResult, assignmentRuns, assignmentCost⟩ :=
    source_value_has_pure_evaluation binding ⟨letSlot, by decide⟩ assignedRead bounded
  have bodyRuns : ∀ value, ∃ result,
      PureIntegerEvaluation ([(NameId.mk ⟨letSlot, by decide⟩, value)].lookup ·) returnedExpr result := by
    intro value
    obtain ⟨result, runs, _⟩ :=
      source_value_has_pure_evaluation binding ⟨letSlot, by decide⟩ returnedRead value
    exact ⟨result, runs⟩
  obtain ⟨_, _, returnedCost⟩ :=
    source_value_has_pure_evaluation binding ⟨letSlot, by decide⟩ returnedRead bounded
  obtain ⟨callable, result, outcome, runs⟩ :=
    assigned_literal_normalization_operationally_correct ⟨letSlot, by decide⟩ bounded assignmentRuns bodyRuns
      (by unfold runBudget; omega) (by unfold runBudget; omega)
  refine ⟨_, result, callable, outcome, ?_⟩
  intro fuel enough
  exact runs fuel (by omega)


end Xsh.Conformance.Vertical
