import Xsh.Semantics.Connected.SafetyReadGrounding
import Xsh.Typing.Connected.QualifiedFunction
import Xsh.Typing.Connected.QualifiedNodeBlockSound
import Xsh.Conformance.Connected.ScopedOccurrences

namespace Xsh.Semantics.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Typing.Connected
open Xsh.Conformance.Connected

theorem qualified_function_candidate_types_every_solution (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat) (scheme : Scheme)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (principal later : Substitution) (inhabitant : List (Variable × MonoType))
    (accepted : validateQualifiedFunction fuel names parameters globals result body ordered steps solvedCount
      scheme candidate binderTypes fields inhabitant = .ok principal)
    (sorted : WellSorted later) (solves : Solves scheme.requirements later) :
    NodeBlockHasType (compose later principal) (parameters ++ globals)
      (substitute (compose later principal) result) candidate := by
  cases complete : guaranteesFunctionResult candidate with
  | false =>
    cases supported : supportedStatementTails true candidate <;>
      simp [validateQualifiedFunction, except_bind_ok, except_map_ok, exists_require_ok, complete, supported] at accepted
  | true =>
    cases supported : supportedStatementTails true candidate with
    | false =>
      simp [validateQualifiedFunction, except_bind_ok, except_map_ok, exists_require_ok, supported, complete] at accepted
    | true =>
      simp [validateQualifiedFunction, except_bind_ok, except_map_ok, exists_require_ok, supported, complete,
        Bool.and_eq_true, checks_constraints_iff] at accepted
      obtain ⟨⟨⟨finite, ground⟩, inhabited⟩, chosen, found, unique, propagation, checked, same⟩ := accepted
      subst chosen
      obtain ⟨checkedUnit, checked⟩ := checked
      cases checkedUnit
      rw [substitute_compose]
      exact validated_qualified_node_block_instantiates fuel names (parameters ++ globals) principal later
        scheme.requirements fields binderTypes (substitute principal result) body candidate checked sorted solves

theorem qualified_function_inhabitant_solves (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat) (scheme : Scheme)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (principal : Substitution) (inhabitant : List (Variable × MonoType))
    (accepted : validateQualifiedFunction fuel names parameters globals result body ordered steps solvedCount
      scheme candidate binderTypes fields inhabitant = .ok principal) :
    WellSorted (witnessSubstitution inhabitant) ∧ Solves scheme.requirements (witnessSubstitution inhabitant) := by
  cases complete : guaranteesFunctionResult candidate with
  | false =>
    cases supported : supportedStatementTails true candidate <;>
      simp [validateQualifiedFunction, except_bind_ok, except_map_ok, exists_require_ok, complete, supported] at accepted
  | true =>
    cases supported : supportedStatementTails true candidate with
    | false =>
      simp [validateQualifiedFunction, except_bind_ok, except_map_ok, exists_require_ok, supported, complete] at accepted
    | true =>
      simp [validateQualifiedFunction, except_bind_ok, except_map_ok, exists_require_ok, supported, complete,
        Bool.and_eq_true, checks_constraints_iff] at accepted
      obtain ⟨⟨⟨finite, ground⟩, inhabited⟩, remaining⟩ := accepted
      exact ⟨finite_substitution_is_well_sorted inhabitant finite, inhabited⟩

theorem validated_scoped_candidate_types_every_solution (fuel : Nat) (unit : SourceUnit)
    (source : SourceFunction) (variables : VariableResolver) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType))
    (document : ScopedBodyDocument) (principal : Substitution)
    (accepted : validateScopedBodyFacts fuel unit source variables names parameters globals result ordered steps
      solvedCount candidate binderTypes fields inhabitant document = .ok principal) :
    ∃ scheme, schemeFromScopedBody variables document = some scheme ∧
      ∀ later, WellSorted later → Solves scheme.requirements later →
        NodeBlockHasType (compose later principal) (parameters ++ globals)
          (substitute (compose later principal) result) candidate := by
  obtain ⟨scheme, decoded, certified⟩ := validated_scoped_body_has_function_certificate fuel unit source variables
    names parameters globals result ordered steps solvedCount candidate binderTypes fields inhabitant document principal accepted
  refine ⟨scheme, decoded, ?_⟩
  intro later sorted solves
  exact qualified_function_candidate_types_every_solution fuel names parameters globals result source.body ordered steps
    solvedCount scheme candidate binderTypes fields principal later inhabitant certified sorted solves

theorem validated_scoped_document_has_body (fuel : Nat) (unit : SourceUnit)
    (facts : ScopedFactsDocument) (scopes : List (SourceSite × List Nat))
    (declaration bodyIndex : Nat) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (principal : Substitution)
    (accepted : validateSourceScopedDecoded fuel unit facts scopes declaration bodyIndex names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant = .ok principal) :
    ∃ source document, unit.declarations[declaration]? = some (.function source) ∧
      facts.bodies[bodyIndex]? = some document ∧
      validateScopedBodyFacts fuel unit source (scopedVariableResolver scopes) names parameters globals result
        ordered steps solvedCount candidate binderTypes fields inhabitant document = .ok principal := by
  cases sourceAt : unit.declarations[declaration]? with
  | none => simp [validateSourceScopedDecoded, sourceAt] at accepted
  | some declarationValue =>
    cases declarationValue with
    | statement statement expressions statements => simp [validateSourceScopedDecoded, sourceAt] at accepted
    | function source =>
      cases documentAt : facts.bodies[bodyIndex]? with
      | none => simp [validateSourceScopedDecoded, sourceAt, documentAt, except_bind_ok] at accepted
      | some document =>
        cases bodyCheck : validateScopedBodyFacts fuel unit source (scopedVariableResolver scopes) names parameters
            globals result ordered steps solvedCount candidate binderTypes fields inhabitant document with
        | error error =>
          simp [validateSourceScopedDecoded, sourceAt, documentAt, bodyCheck, except_bind_ok] at accepted
        | ok chosen =>
          simp [validateSourceScopedDecoded, sourceAt, documentAt, bodyCheck, except_bind_ok,
            exists_require_ok] at accepted
          cases schemeAt : schemeFromScopedBody (scopedVariableResolver scopes) document with
          | none => simp [schemeAt] at accepted
          | some scheme =>
            cases supported : supportedScopedCallSites unit with
            | false => simp [schemeAt, supported, Bind.bind, Except.bind] at accepted
            | true =>
              simp [schemeAt, supported, except_map_ok, exists_require_ok] at accepted
              rw [accepted.2.2] at bodyCheck
              exact ⟨source, document, rfl, rfl, bodyCheck⟩

theorem validated_scoped_document_candidate_types (fuel : Nat) (unit : SourceUnit)
    (facts : ScopedFactsDocument) (scopes : List (SourceSite × List Nat))
    (declaration bodyIndex : Nat) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (principal : Substitution)
    (accepted : validateSourceScopedDecoded fuel unit facts scopes declaration bodyIndex names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant = .ok principal) :
    ∃ source document scheme, unit.declarations[declaration]? = some (.function source) ∧
      facts.bodies[bodyIndex]? = some document ∧
      schemeFromScopedBody (scopedVariableResolver scopes) document = some scheme ∧
      ∀ later, WellSorted later → Solves scheme.requirements later →
        NodeBlockHasType (compose later principal) (parameters ++ globals)
          (substitute (compose later principal) result) candidate := by
  obtain ⟨source, document, sourceAt, documentAt, bodyAccepted⟩ :=
    validated_scoped_document_has_body fuel unit facts scopes declaration bodyIndex names parameters globals result
      ordered steps solvedCount candidate binderTypes fields inhabitant principal accepted
  obtain ⟨scheme, decoded, candidateTyped⟩ := validated_scoped_candidate_types_every_solution fuel unit source
    (scopedVariableResolver scopes) names parameters globals result ordered steps solvedCount candidate binderTypes fields
    inhabitant document principal bodyAccepted
  exact ⟨source, document, scheme, sourceAt, documentAt, decoded, candidateTyped⟩

theorem validated_scoped_document_inhabitant_types (fuel : Nat) (unit : SourceUnit)
    (facts : ScopedFactsDocument) (scopes : List (SourceSite × List Nat))
    (declaration bodyIndex : Nat) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (inhabitant : List (Variable × MonoType)) (principal : Substitution)
    (accepted : validateSourceScopedDecoded fuel unit facts scopes declaration bodyIndex names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant = .ok principal) :
    ∃ source document scheme, unit.declarations[declaration]? = some (.function source) ∧
      facts.bodies[bodyIndex]? = some document ∧
      schemeFromScopedBody (scopedVariableResolver scopes) document = some scheme ∧
      WellSorted (witnessSubstitution inhabitant) ∧ Solves scheme.requirements (witnessSubstitution inhabitant) ∧
      NodeBlockHasType (compose (witnessSubstitution inhabitant) principal) (parameters ++ globals)
        (substitute (compose (witnessSubstitution inhabitant) principal) result) candidate := by
  obtain ⟨source, document, sourceAt, documentAt, bodyAccepted⟩ :=
    validated_scoped_document_has_body fuel unit facts scopes declaration bodyIndex names parameters globals result
      ordered steps solvedCount candidate binderTypes fields inhabitant principal accepted
  obtain ⟨scheme, decoded, certified⟩ := validated_scoped_body_has_function_certificate fuel unit source
    (scopedVariableResolver scopes) names parameters globals result ordered steps solvedCount candidate binderTypes fields
    inhabitant document principal bodyAccepted
  obtain ⟨sorted, solves⟩ := qualified_function_inhabitant_solves fuel names parameters globals result source.body
    ordered steps solvedCount scheme candidate binderTypes fields principal inhabitant certified
  have candidateTyped := qualified_function_candidate_types_every_solution fuel names parameters globals result source.body
    ordered steps solvedCount scheme candidate binderTypes fields principal (witnessSubstitution inhabitant) inhabitant
    certified sorted solves
  exact ⟨source, document, scheme, sourceAt, documentAt, decoded, sorted, solves, candidateTyped⟩

end Xsh.Semantics.Connected
