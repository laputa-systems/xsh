import Xsh.Semantics.Connected.SafetyResultCall
import Xsh.Semantics.Connected.SafetyActivation

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Syntax.Foundation
open Xsh.Typing.Connected
open Xsh.Conformance.Connected

def resultFunctionScheme (signature : ResultSignature) : Scheme := ⟨[], resultFunctionType signature, []⟩

def resultParameterBinders (signature : ResultSignature) : List BinderId :=
  (List.range signature.parameters.length).filterMap BinderId.decode?

def certifiedFunction (signature : ResultSignature) (body : List StatementNode) : Function :=
  ⟨resultParameterBinders signature, body, resultFunctionScheme signature, identity⟩

structure CertifiedResultProgram (state : Machine) (signatures : List ResultSignature)
    (cases : List (List StatementNode × List DerivationStep)) : Prop where
  functions : ∀ index signature candidate steps binder, signatures[index]? = some signature →
    cases[index]? = some (candidate, steps) → BinderId.decode? (resultFunctionBinderBase + index) = some binder →
    ∃ declaration, Connected.read state.store state.globals binder =
        some (.schemeFunction declaration (resultFunctionScheme signature)) ∧
      state.functions declaration = some (.function (certifiedFunction signature candidate))
  ok : ∃ declaration, Connected.read state.store state.globals resultOkBinder =
      some (.schemeFunction declaration okScheme) ∧ state.functions declaration = some .resultOk

theorem the_callee_scheme_is_the_published_signature :
    resultCallSignatures[0]?.map resultFunctionScheme = some wrapScheme := rfl

theorem the_certified_callee_takes_one_parameter :
    resultCallSignatures[0]?.map resultParameterBinders = some [resultFirstBinder] := by
  decide

theorem fresh_activation_cell_is_empty (state : Machine) (activation : ActivationInvariant state) (binder : BinderId) :
    state.store ⟨state.nextActivation, binder⟩ = none := by
  cases occupied : state.store ⟨state.nextActivation, binder⟩ with
  | none => rfl
  | some entry => exact absurd (activation.store _ entry occupied) (Nat.lt_irrefl _)

theorem certified_call_program_gives_the_call_world (state : Machine)
    (program : CertifiedResultProgram state resultCallSignatures resultCallCandidates)
    (activation : ActivationInvariant state) (topLevel : state.cells = state.globals) :
    Nonempty (ResultCallWorld state wrapCandidate) := by
  obtain ⟨declaration, found, table⟩ :=
    program.functions 0 _ wrapCandidate wrapSteps wrapBinder rfl rfl rfl
  obtain ⟨okDeclaration, okFound, okFunction⟩ := program.ok
  rw [← topLevel] at found okFound
  exact ⟨{
    ok := ⟨okDeclaration, okFound, okFunction⟩
    wrapDeclaration := declaration
    wrapFunction := certifiedFunction _ wrapCandidate
    wrapFound := found
    wrapTable := table
    wrapSchemeIs := rfl
    wrapParameters := by decide
    wrapBody := rfl
    okVisible := by rw [topLevel]
    fresh := fresh_activation_cell_is_empty state activation resultFirstBinder }⟩

theorem a_certified_call_program_runs_lift_through_wrap (state : Machine)
    (program : CertifiedResultProgram state resultCallSignatures resultCallCandidates)
    (activation : ActivationInvariant state) (topLevel : state.cells = state.globals) :
    resultCallCertified = true ∧ returnedNode liftCandidate = some liftNode ∧
      Execution { state with control := .expression liftNode } (List.replicate 22 .internal)
        { callResumed state with control := .value (.ok callOne .string) } := by
  obtain ⟨world⟩ := certified_call_program_gives_the_call_world state program activation topLevel
  exact ⟨result_call_is_certified, rfl, lift_runs_the_wrap_call_frame_and_propagates_its_success state world⟩

def integerStringResult : MonoType := .result (.primitive .integer) (.primitive .string)

def liftBinder : BinderId := ⟨⟨21, by decide⟩⟩
def programWrapDeclaration : DeclarationId := ⟨wrapBinder⟩
def programLiftDeclaration : DeclarationId := ⟨liftBinder⟩

def liftSignature : ResultSignature :=
  ⟨"lift", ⟨0, 87, 23⟩, [], integerStringResult,
    [(⟨0, 96, 12⟩, integerStringResult), (⟨0, 99, 8⟩, .primitive .integer), (⟨0, 99, 7⟩, integerStringResult),
      (⟨0, 104, 1⟩, .primitive .integer)], [(⟨0, 99, 4⟩, "wrap")]⟩

def wrapSignature : ResultSignature :=
  ⟨"wrap", ⟨0, 38, 16⟩, [("n", .primitive .integer)], integerStringResult,
    [(⟨0, 47, 5⟩, integerStringResult), (⟨0, 50, 1⟩, .primitive .integer)], []⟩

theorem the_program_signatures_are_the_published_ones : resultCallSignatures = [wrapSignature, liftSignature] := by
  decide +kernel

def programFunctions : FunctionTable := fun declaration =>
  if declaration = callOkDeclaration then some .resultOk
  else if declaration = programWrapDeclaration then some (.function (certifiedFunction wrapSignature wrapCandidate))
  else if declaration = programLiftDeclaration then some (.function (certifiedFunction liftSignature liftCandidate))
  else none

def programGlobals : ActiveCells := fun binder =>
  if binder = resultOkBinder ∨ binder = wrapBinder ∨ binder = liftBinder then some ⟨0, binder⟩ else none

def programStore : Store := fun cell =>
  if cell = ⟨0, resultOkBinder⟩ then
    some ⟨⟨.schemeFunction, .schemeFunction callOkDeclaration okScheme, by decide⟩, .immutable⟩
  else if cell = ⟨0, wrapBinder⟩ then
    some ⟨⟨.schemeFunction, .schemeFunction programWrapDeclaration (resultFunctionScheme wrapSignature), by decide⟩,
      .immutable⟩
  else if cell = ⟨0, liftBinder⟩ then
    some ⟨⟨.schemeFunction, .schemeFunction programLiftDeclaration (resultFunctionScheme liftSignature), by decide⟩,
      .immutable⟩
  else none

def programMachine (base : Machine) : Machine :=
  { base with
    functions := programFunctions, globals := programGlobals, cells := programGlobals, store := programStore,
    activation := 0, nextActivation := 1, stack := [] }

theorem program_identities_are_distinct :
    wrapBinder ≠ resultOkBinder ∧ liftBinder ≠ resultOkBinder ∧ liftBinder ≠ wrapBinder ∧
      programWrapDeclaration ≠ callOkDeclaration ∧ programLiftDeclaration ≠ callOkDeclaration ∧
      programLiftDeclaration ≠ programWrapDeclaration := by
  decide

theorem the_program_machine_loads_the_certified_program (base : Machine) :
    CertifiedResultProgram (programMachine base) resultCallSignatures resultCallCandidates where
  functions := by
    intro index signature candidate steps binder signatureFound caseFound decoded
    rw [the_program_signatures_are_the_published_ones] at signatureFound
    match index, signatureFound, caseFound, decoded with
    | 0, signatureFound, caseFound, decoded =>
      cases signatureFound
      simp [resultCallCandidates] at caseFound
      obtain ⟨rfl, rfl⟩ := caseFound
      have : binder = wrapBinder := by
        simp [BinderId.decode?, Word32.decode?, resultFunctionBinderBase] at decoded
        rw [← decoded]
        rfl
      subst this
      exact ⟨programWrapDeclaration,
        by simp [Connected.read, programMachine, programGlobals, programStore, program_identities_are_distinct],
        by simp [programMachine, programFunctions, program_identities_are_distinct]⟩
    | 1, signatureFound, caseFound, decoded =>
      cases signatureFound
      simp [resultCallCandidates] at caseFound
      obtain ⟨rfl, rfl⟩ := caseFound
      have : binder = liftBinder := by
        simp [BinderId.decode?, Word32.decode?, resultFunctionBinderBase] at decoded
        rw [← decoded]
        rfl
      subst this
      exact ⟨programLiftDeclaration,
        by simp [Connected.read, programMachine, programGlobals, programStore, program_identities_are_distinct],
        by simp [programMachine, programFunctions, program_identities_are_distinct]⟩
    | _ + 2, signatureFound, _, _ => simp at signatureFound
  ok := ⟨callOkDeclaration, by simp [Connected.read, programMachine, programGlobals, programStore],
    by simp [programMachine, programFunctions]⟩

theorem the_program_machine_satisfies_the_activation_invariant (base : Machine) :
    ActivationInvariant (programMachine base) where
  active := by simp [programMachine]
  saved := by simp [programMachine, StackActivationsBelow]
  store := by
    intro cell entry found
    simp only [programMachine, programStore] at found
    split at found
    · rename_i same; subst same; simp [programMachine]
    · split at found
      · rename_i same; subst same; simp [programMachine]
      · split at found
        · rename_i same; subst same; simp [programMachine]
        · cases found

theorem the_loaded_certified_program_runs_lift_through_wrap (base : Machine) :
    Execution { programMachine base with control := .expression liftNode } (List.replicate 22 .internal)
      { callResumed (programMachine base) with control := .value (.ok callOne .string) } :=
  (a_certified_call_program_runs_lift_through_wrap (programMachine base)
    (the_program_machine_loads_the_certified_program base)
    (the_program_machine_satisfies_the_activation_invariant base) rfl).2.2

def forgedCalleeScheme : Scheme :=
  ⟨[], .function (.argument (.primitive .string) .argumentsEmpty) integerStringResult, []⟩

def forgedCalleeMachine (base : Machine) : Machine :=
  { programMachine base with
    store := fun cell =>
      if cell = ⟨0, wrapBinder⟩ then
        some ⟨⟨.schemeFunction, .schemeFunction programWrapDeclaration forgedCalleeScheme, by decide⟩, .immutable⟩
      else programStore cell }

theorem a_callee_loaded_at_a_forged_scheme_is_not_the_certified_program (base : Machine) :
    ¬ CertifiedResultProgram (forgedCalleeMachine base) resultCallSignatures resultCallCandidates := by
  intro program
  obtain ⟨declaration, found, _⟩ := program.functions 0 _ wrapCandidate wrapSteps wrapBinder rfl rfl rfl
  simp [Connected.read, forgedCalleeMachine, programMachine, programGlobals] at found
  have bodies := congrArg Scheme.body found.2
  simp [forgedCalleeScheme, resultFunctionScheme, resultFunctionType] at bodies

end Xsh.Semantics.Connected
