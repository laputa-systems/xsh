import Xsh.Semantics.Connected.SafetyDerivedInvocation

namespace Xsh.Semantics.Connected

open Xsh.Foundation Xsh.Syntax Xsh.Syntax.Foundation Xsh.Typing.Connected Xsh.Conformance.Connected

def derivedScopedInvocation : SelectedInvocation :=
  ⟨⟨scopedFixtureInput⟩, .argument .integer .argumentsEmpty, .integer, scopedFixtureInhabitant,
    [LiteralScalar.integer scopedFixtureArgument]⟩

theorem actual_scoped_call_is_derived :
    deriveSelectedInvocation scopedUnit scopedDocument scopedScopes 0 0 = some derivedScopedInvocation := by
  decide +kernel

theorem derived_scoped_machine_is_fixture_machine (base : Machine) (principal : Substitution) :
    invocationMachine base derivedScopedInvocation (scopedFixtureFunction principal) =
      scopedInvocationMachine base principal scopedFixtureArgument := by
  rfl

theorem scoped_document_publishes_fixture_scheme :
    ∀ body, scopedDocument.bodies[0]? = some body →
      schemeFromScopedBody (scopedVariableResolver scopedScopes) body = some (scopedFixtureFunction identity).scheme := by
  intro body found
  have same : body = ⟨scopedBodyDocument, [.store scopedStore]⟩ := by
    simpa [scopedDocument] using found.symm
  subst body
  exact scoped_fixture_scheme_decodes

theorem actual_derived_scoped_invocation_execution_safe (base : Machine) (response : Xsh.Values.IntValue) :
    ∃ principal selected,
      admitSource 0 actualScopedSource = .ok scopedUnit ∧
      decodeScopedFactsDocument 128 (sourceNameResolver scopedUnit) scopedFactsCodes = some (scopedDocument, scopedScopes) ∧
      deriveSelectedInvocation scopedUnit scopedDocument scopedScopes 0 0 = some selected ∧
      selected.arguments = [LiteralScalar.integer scopedFixtureArgument] ∧
      validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
        scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
        scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal ∧
      TypedMachine [] (fun _ => .ordinary) (fun _ => none)
        (invocationMachine base selected (scopedFixtureFunction principal)) ∧
      (∀ final events fault, Execution (invocationMachine base selected (scopedFixtureFunction principal)) events final →
        final.control = .fault fault → ∃ issue, fault = .integer issue) ∧
      (∀ final events, Execution (invocationMachine base selected (scopedFixtureFunction principal)) events final →
        MachineTerminal final ∨ ∃ event next, Step final event next ∧ PermittedControl next.control) := by
  obtain ⟨principal, accepted⟩ := scoped_fixture_accepted
  have derived := actual_scoped_call_is_derived
  have published := scoped_document_publishes_fixture_scheme
  have certified := scoped_fixture_function_certified (invocationMachine base derivedScopedInvocation
    (scopedFixtureFunction principal)) principal accepted
  have typed := derived_invocation_machine_typed base scopedUnit scopedDocument scopedScopes 0 0 derivedScopedInvocation
    (scopedFixtureFunction principal) derived published certified
  obtain ⟨faults, progress⟩ := derived_invocation_execution_safe base scopedUnit scopedDocument scopedScopes 0 0
    derivedScopedInvocation (scopedFixtureFunction principal) response derived published certified
  exact ⟨principal, derivedScopedInvocation, actual_scoped_source_is_admitted, scoped_document_decodes, derived, rfl,
    accepted, typed, faults, progress⟩

def scopedUnitCalling (argument : CallArgument) : SourceUnit :=
  { scopedUnit with declarations := [.function scopedFunction, .statement (.assert scopedAssert none)
    [(⟨0, 93, 9⟩, .call (.ident scopedFunctionName) [argument])] []] }

def scopedDocumentWith (change : ScopedCallDocument → ScopedCallDocument) : ScopedFactsDocument :=
  { scopedDocument with calls := scopedDocument.calls.map change }

def scopedDocumentTargeting (target : FunctionTarget) : ScopedFactsDocument :=
  { scopedDocument with functions := [target], calls := scopedDocument.calls.map fun call => { call with target := target } }

def literalArgument (text : String) : CallArgument := .positional (.literal (.integer text))

theorem derived_invocation_follows_the_source_literal :
    deriveSelectedInvocation (scopedUnitCalling (literalArgument "9")) scopedDocument scopedScopes 0 0 =
      some { derivedScopedInvocation with arguments := [LiteralScalar.integer ⟨9, by decide⟩] } := by
  decide +kernel

theorem derived_invocation_rejects_source_and_call_forgeries :
    deriveSelectedInvocation (scopedUnitCalling (.positional (.literal (.bool true)))) scopedDocument scopedScopes 0 0 = none ∧
    deriveSelectedInvocation (scopedUnitCalling (.positional (.ident scopedInput))) scopedDocument scopedScopes 0 0 = none ∧
    deriveSelectedInvocation (scopedUnitCalling (.positional (.literal (.string "7")))) scopedDocument scopedScopes 0 0 = none ∧
    deriveSelectedInvocation (scopedUnitCalling (literalArgument "99999999999999999999")) scopedDocument scopedScopes 0 0 = none ∧
    deriveSelectedInvocation (scopedUnitCalling (.named scopedInput (.literal (.integer "7")))) scopedDocument scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit (scopedDocumentWith fun call => { call with typeArguments := [(0, .primitive .boolean)] })
      scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit (scopedDocumentWith fun call => { call with typeArguments := [] })
      scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit (scopedDocumentWith fun call => { call with role := .value })
      scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit (scopedDocumentWith fun call => { call with site := ⟨0, 93, 14⟩ })
      scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit (scopedDocumentTargeting { scopedTarget with body := ⟨0, 19, 67⟩ })
      scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit (scopedDocumentTargeting { scopedTarget with name := "other" })
      scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit (scopedDocumentTargeting { scopedTarget with definition := 4294967296 })
      scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit (scopedDocumentWith fun call =>
      { call with target := { scopedTarget with definition := 3 } }) scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit { scopedDocument with functions := [] } scopedScopes 0 0 = none ∧
    deriveSelectedInvocation scopedUnit scopedDocument scopedScopes 0 1 = none ∧
    deriveSelectedInvocation scopedUnit scopedDocument scopedScopes 1 0 = none := by
  decide +kernel

theorem derived_invocation_follows_the_selected_target :
    deriveSelectedInvocation scopedUnit (scopedDocumentTargeting { scopedTarget with definition := 3 }) scopedScopes 0 0 =
      some { derivedScopedInvocation with declaration := ⟨⟨⟨3, by decide⟩⟩⟩ } := by
  decide +kernel

def boolInhabitant : List (Variable × MonoType) := [(scopedFixtureTypeVar, .primitive .boolean)]

theorem selection_checks_arity_and_literal_types :
    selectInvocation scopedFixtureScheme ⟨scopedFixtureInput⟩ scopedFixtureInhabitant [literalArgument "7"] =
      some derivedScopedInvocation ∧
    (selectInvocation scopedFixtureScheme ⟨scopedFixtureInput⟩ boolInhabitant [.positional (.literal (.bool true))]).map
      (fun selected => (selected.parameters, selected.result, selected.arguments)) =
      some (.argument .boolean .argumentsEmpty, .boolean, [LiteralScalar.boolean true]) ∧
    selectInvocation scopedFixtureScheme ⟨scopedFixtureInput⟩ boolInhabitant [literalArgument "7"] = none ∧
    selectInvocation scopedFixtureScheme ⟨scopedFixtureInput⟩ scopedFixtureInhabitant
      [.positional (.literal (.bool true))] = none ∧
    selectInvocation scopedFixtureScheme ⟨scopedFixtureInput⟩ scopedFixtureInhabitant [] = none ∧
    selectInvocation scopedFixtureScheme ⟨scopedFixtureInput⟩ scopedFixtureInhabitant
      [literalArgument "7", literalArgument "7"] = none ∧
    selectInvocation scopedFixtureScheme ⟨scopedFixtureInput⟩ [] [literalArgument "7"] = none := by
  decide +kernel

end Xsh.Semantics.Connected
