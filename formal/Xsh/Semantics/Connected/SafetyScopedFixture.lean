import Xsh.Semantics.Connected.SafetySourceGrounding
import Xsh.Conformance.Connected.ScopedCertificate

namespace Xsh.Semantics.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Connected

def scopedFixtureTypeVar : Variable := ⟨0, .value⟩
def scopedFixtureCompletion : Variable := ⟨1, .value⟩
def scopedFixtureInput : BinderId := ⟨⟨0, by decide⟩⟩
def scopedFixtureLocal : BinderId := ⟨⟨1, by decide⟩⟩
def scopedFixtureNames : TypeNames := ⟨scopedName 0 (by decide), scopedName 1 (by decide),
  scopedName 2 (by decide), scopedName 3 (by decide)⟩
def scopedFixtureParameters : List Binding := [⟨scopedInput, scopedFixtureInput, .immutable, ⟨[], .variable scopedFixtureTypeVar, []⟩⟩]
def scopedFixtureCandidate : List StatementNode := [
  .binding scopedFixtureLocal .mutable (.variable scopedFixtureTypeVar) (.read scopedFixtureInput []),
  .evaluated (.valueBlock (.primitive .unit) [.assignment scopedFixtureLocal (.read scopedFixtureInput [])]),
  .evaluated (.read scopedFixtureLocal [])]
def scopedFixtureOrdered : List Constraint := [.equal (.variable scopedFixtureCompletion) (.primitive .unit),
  .equal (.variable scopedFixtureTypeVar) (.variable scopedFixtureTypeVar),
  .store (.variable scopedFixtureTypeVar) (.variable scopedFixtureTypeVar)]
def scopedFixtureBinders : BinderTypes := fun binder => if binder = scopedFixtureLocal then some (.variable scopedFixtureTypeVar) else none
def scopedFixtureInhabitant : List (Variable × MonoType) := [(scopedFixtureTypeVar, .primitive .integer)]

theorem scoped_fixture_accepted : ∃ principal,
    validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal := by
  have checked := actual_scoped_facts_are_checked
  unfold actualScopedCheck at checked
  simp only [source_scoped_validation_reduces] at checked
  change (match validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
    scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
    scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant with
    | .ok _ => true | .error _ => false) = true at checked
  cases result : validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant with
  | error failure => simp [result] at checked
  | ok principal => exact ⟨principal, rfl⟩

theorem scoped_fixture_candidate_types : ∃ principal source document scheme,
    validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal ∧
    scopedUnit.declarations[0]? = some (.function source) ∧ scopedDocument.bodies[0]? = some document ∧
    schemeFromScopedBody (scopedVariableResolver scopedScopes) document = some scheme ∧
    ∀ later, WellSorted later → Solves scheme.requirements later →
      NodeBlockHasType (compose later principal) scopedFixtureParameters
        (substitute (compose later principal) (.variable scopedFixtureTypeVar)) scopedFixtureCandidate := by
  obtain ⟨principal, accepted⟩ := scoped_fixture_accepted
  obtain ⟨source, document, scheme, sourceAt, bodyAt, schemeAt, typed⟩ :=
    validated_scoped_document_candidate_types 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant principal accepted
  exact ⟨principal, source, document, scheme, accepted, sourceAt, bodyAt, schemeAt, by simpa using typed⟩

theorem scoped_fixture_selected_arguments : ∃ call,
    scopedDocument.calls[0]? = some call ∧
    argumentsFromCall (scopedVariableResolver scopedScopes) scopedBodyDocument
      ⟨call.expression, call.site, call.role, call.target, call.typeArguments,
        call.operationBindings.filterMap (fun binding => match binding with | .field => some .field | .store => none)⟩ =
      some scopedFixtureInhabitant := by
  refine ⟨_, rfl, ?_⟩
  decide

def scopedFixtureScheme : Scheme := ⟨[scopedFixtureTypeVar],
  .function (.argument (.variable scopedFixtureTypeVar) .argumentsEmpty) (.variable scopedFixtureTypeVar),
  [.store (.variable scopedFixtureTypeVar) (.variable scopedFixtureTypeVar)]⟩

theorem scoped_fixture_scheme_decodes :
    schemeFromScopedBody (scopedVariableResolver scopedScopes) ⟨scopedBodyDocument, [.store scopedStore]⟩ =
      some scopedFixtureScheme := by rfl

theorem scoped_fixture_selected_substitution :
    schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant) =
      witnessSubstitution scopedFixtureInhabitant := by
  funext id
  by_cases same : id = scopedFixtureTypeVar
  · subst id
    simp [schemeSubstitution, scopedFixtureScheme]
  · have unequal : (id == scopedFixtureTypeVar) = false := by simpa using same
    simp [schemeSubstitution, scopedFixtureScheme, witnessSubstitution, scopedFixtureInhabitant, List.lookup_cons, same, unequal]

theorem scoped_selected_candidate_typed : ∃ principal,
    validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal ∧
    NodeBlockHasType
      (compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal)
      scopedFixtureParameters
      (substitute (compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal)
        (.variable scopedFixtureTypeVar)) scopedFixtureCandidate := by
  obtain ⟨principal, accepted⟩ := scoped_fixture_accepted
  obtain ⟨source, document, scheme, sourceAt, bodyAt, schemeAt, sorted, solves, candidateTyped⟩ :=
    validated_scoped_document_inhabitant_types 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant principal accepted
  have bodySame : document = ⟨scopedBodyDocument, [.store scopedStore]⟩ := by
    simpa [scopedDocument] using bodyAt.symm
  subst document
  rw [scoped_fixture_scheme_decodes] at schemeAt
  cases schemeAt
  refine ⟨principal, accepted, ?_⟩
  simpa [scoped_fixture_selected_substitution] using candidateTyped

end Xsh.Semantics.Connected
theorem Xsh.Semantics.Connected.scoped_fixture_principal_preserves_parameter :
    (match Xsh.Conformance.Connected.actualScopedCheckDecoded Xsh.Conformance.Connected.scopedUnit
        Xsh.Conformance.Connected.scopedDocument Xsh.Conformance.Connected.scopedScopes with
      | .ok principal => decide (principal Xsh.Semantics.Connected.scopedFixtureTypeVar =
          Xsh.Typing.Connected.MonoType.variable Xsh.Semantics.Connected.scopedFixtureTypeVar)
      | .error _ => false) = true := by
  decide +kernel

namespace Xsh.Semantics.Connected
open Xsh.Typing.Connected Xsh.Conformance.Connected

theorem scoped_selected_storage_and_candidate : ∃ principal,
    validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal ∧
    substitute (compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal)
      (.variable scopedFixtureTypeVar) = .primitive .integer ∧
    monoRuntime (substitute
      (compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal)
      (.variable scopedFixtureTypeVar)) = some .integer ∧
    NodeBlockHasType
      (compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal)
      scopedFixtureParameters
      (substitute (compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal)
        (.variable scopedFixtureTypeVar)) scopedFixtureCandidate := by
  obtain ⟨principal, accepted, candidateTyped⟩ := scoped_selected_candidate_typed
  have property := scoped_fixture_principal_preserves_parameter
  change (match validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
    scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
    scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant with
    | .ok principal => decide (principal scopedFixtureTypeVar = .variable scopedFixtureTypeVar)
    | .error _ => false) = true at property
  rw [accepted] at property
  have parameter : principal scopedFixtureTypeVar = .variable scopedFixtureTypeVar := of_decide_eq_true property
  have selected : substitute
      (compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal)
      (.variable scopedFixtureTypeVar) = .primitive .integer := by
    rw [scoped_fixture_selected_substitution]
    simp [substitute, compose, parameter, witnessSubstitution, scopedFixtureInhabitant]
  exact ⟨principal, accepted, selected, by simp [selected, monoRuntime], candidateTyped⟩

end Xsh.Semantics.Connected

namespace Xsh.Semantics.Connected
open Xsh.Foundation Xsh.Typing.Connected Xsh.Conformance.Connected

theorem scoped_fixture_candidate_grounded (solution : Substitution)
    (selected : substitute solution (.variable scopedFixtureTypeVar) = .primitive .integer)
    (typed : NodeBlockHasType solution scopedFixtureParameters (.primitive .integer) scopedFixtureCandidate) :
    GroundBlock solution solution (fun _ => .ordinary) scopedFixtureParameters (.primitive .integer)
      (some (.primitive .integer)) scopedFixtureCandidate := by
  have parametersUnique : (scopedFixtureParameters.map Binding.binder).Nodup := by decide
  have storageClosed : monoRuntime (substitute solution (.variable scopedFixtureTypeVar)) = some .integer := by
    simp [selected, monoRuntime]
  unfold scopedFixtureCandidate at typed ⊢
  cases typed with
  | binding initializerTyped compatible fresh tail =>
    rename_i name initializerType
    have initializerGround := ground_monomorphic_read scopedFixtureParameters solution solution (fun _ => .ordinary)
      (.primitive .integer) scopedInput scopedFixtureInput .immutable (.variable scopedFixtureTypeVar) initializerType .integer
      initializerTyped parametersUnique (by simp [scopedFixtureParameters]) storageClosed rfl
    let localBinding : Binding := ⟨name, scopedFixtureLocal, .mutable, ⟨[], .variable scopedFixtureTypeVar, []⟩⟩
    have scopeUnique : ((localBinding :: scopedFixtureParameters).map Binding.binder).Nodup := by
      simp [localBinding, scopedFixtureParameters, scopedFixtureInput, scopedFixtureLocal]
    cases tail with
    | evaluatedNext blockTyped resultTyped =>
      cases blockTyped with
      | valueBlock bodyTyped =>
        generalize bodyEq : [StatementNode.assignment scopedFixtureLocal (.read scopedFixtureInput [])] = nodes at bodyTyped
        cases bodyTyped with
        | nil => simp at bodyEq
        | binding => simp at bodyEq
        | returned => simp at bodyEq
        | evaluatedTail => simp at bodyEq
        | evaluatedNext => simp at bodyEq
        | conditionalTail => simp at bodyEq
        | conditionalNext => simp at bodyEq
        | assignment found mutable assignedTyped assignmentCompatible emptyTyped =>
          simp only [List.cons.injEq, StatementNode.assignment.injEq] at bodyEq
          obtain ⟨⟨binderEq, valueEq⟩, tailEq⟩ := bodyEq
          cases tailEq
          cases valueEq
          cases emptyTyped with
          | nil emptyAccepts =>
            have assignedGround := ground_monomorphic_read (localBinding :: scopedFixtureParameters) solution solution
              (fun _ => .ordinary) (.primitive .integer) scopedInput scopedFixtureInput .immutable
              (.variable scopedFixtureTypeVar) _ .integer assignedTyped scopeUnique
              (by simp [scopedFixtureParameters]) storageClosed rfl
            cases resultTyped with
            | evaluatedTail resultRead resultAccepts =>
              have resultGround := ground_monomorphic_read (localBinding :: scopedFixtureParameters) solution solution
                (fun _ => .ordinary) (.primitive .integer) name scopedFixtureLocal .mutable
                (.variable scopedFixtureTypeVar) _ .integer resultRead scopeUnique (by simp [localBinding]) storageClosed rfl
              exact .binding initializerGround compatible storageClosed storageClosed (Or.inr rfl) rfl fresh
                (.evaluatedNext (.valueBlock (.assignment found mutable assignedGround assignmentCompatible (.nil emptyAccepts)) rfl)
                  (.evaluatedTail resultGround resultAccepts))

end Xsh.Semantics.Connected

namespace Xsh.Semantics.Connected
open Xsh.Typing.Connected Xsh.Conformance.Connected

theorem actual_scoped_selected_grounded : ∃ principal call,
    admitSource 0 actualScopedSource = .ok scopedUnit ∧
    decodeScopedFactsDocument 128 (sourceNameResolver scopedUnit) scopedFactsCodes = some (scopedDocument, scopedScopes) ∧
    scopedDocument.calls[0]? = some call ∧
    argumentsFromCall (scopedVariableResolver scopedScopes) scopedBodyDocument
      ⟨call.expression, call.site, call.role, call.target, call.typeArguments,
        call.operationBindings.filterMap (fun binding => match binding with | .field => some .field | .store => none)⟩ =
      some scopedFixtureInhabitant ∧
    validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal ∧
    let runtime := compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal
    monoRuntime (substitute runtime (.variable scopedFixtureTypeVar)) = some .integer ∧
    GroundBlock runtime runtime (fun _ => .ordinary) scopedFixtureParameters (.primitive .integer)
      (some (.primitive .integer)) scopedFixtureCandidate := by
  obtain ⟨principal, accepted, selected, storageClosed, candidateTyped⟩ := scoped_selected_storage_and_candidate
  obtain ⟨call, callAt, callArguments⟩ := scoped_fixture_selected_arguments
  have grounded := scoped_fixture_candidate_grounded _ selected (by simpa [selected] using candidateTyped)
  exact ⟨principal, call, actual_scoped_source_is_admitted, scoped_document_decodes, callAt, callArguments,
    accepted, storageClosed, grounded⟩

end Xsh.Semantics.Connected

namespace Xsh.Semantics.Connected
open Xsh.Typing.Connected

theorem scoped_selected_instantiates :
    instantiateFunction identity scopedFixtureScheme scopedFixtureInhabitant =
      some (.argument .integer .argumentsEmpty, .integer, scopedFixtureInhabitant) := by
  decide

end Xsh.Semantics.Connected

namespace Xsh.Semantics.Connected
open Xsh.Typing.Connected Xsh.Conformance.Connected

theorem actual_scoped_call_grounded : ∃ principal call,
    admitSource 0 actualScopedSource = .ok scopedUnit ∧
    decodeScopedFactsDocument 128 (sourceNameResolver scopedUnit) scopedFactsCodes = some (scopedDocument, scopedScopes) ∧
    scopedDocument.calls[0]? = some call ∧
    argumentsFromCall (scopedVariableResolver scopedScopes) scopedBodyDocument
      ⟨call.expression, call.site, call.role, call.target, call.typeArguments,
        call.operationBindings.filterMap (fun binding => match binding with | .field => some .field | .store => none)⟩ =
      some scopedFixtureInhabitant ∧
    instantiateFunction identity scopedFixtureScheme scopedFixtureInhabitant =
      some (.argument .integer .argumentsEmpty, .integer, scopedFixtureInhabitant) ∧
    validateSourceScopedDecoded 128 scopedUnit scopedDocument scopedScopes 0 0 scopedFixtureNames
      scopedFixtureParameters [] (.variable scopedFixtureTypeVar) scopedFixtureOrdered [.bindLeft, .reflexive] 2
      scopedFixtureCandidate scopedFixtureBinders (fun _ => none) scopedFixtureInhabitant = .ok principal ∧
    let runtime := compose identity (compose (schemeSubstitution scopedFixtureScheme
      (witnessSubstitution scopedFixtureInhabitant)) principal)
    monoRuntime (substitute runtime (.variable scopedFixtureTypeVar)) = some .integer ∧
    GroundBlock runtime runtime (fun _ => .ordinary) scopedFixtureParameters (.primitive .integer)
      (some (.primitive .integer)) scopedFixtureCandidate := by
  obtain ⟨principal, call, admitted, decoded, callAt, arguments, accepted, closed, grounded⟩ :=
    actual_scoped_selected_grounded
  have identityCompose : compose identity (compose (schemeSubstitution scopedFixtureScheme
      (witnessSubstitution scopedFixtureInhabitant)) principal) =
      compose (schemeSubstitution scopedFixtureScheme (witnessSubstitution scopedFixtureInhabitant)) principal := by
    funext id
    simp [compose, substitute_identity]
  refine ⟨principal, call, admitted, decoded, callAt, arguments, scoped_selected_instantiates, accepted, ?_, ?_⟩
  · simpa [identityCompose] using closed
  · simpa [identityCompose] using grounded

end Xsh.Semantics.Connected
