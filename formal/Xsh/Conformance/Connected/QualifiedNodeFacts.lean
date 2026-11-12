import Xsh.Conformance.Connected.QualifiedFacts
import Xsh.Typing.Connected.QualifiedNodeBlockSound

namespace Xsh.Conformance.Connected

open Xsh.Typing.Connected
open Xsh.Conformance.Transport
open Xsh.Syntax

theorem qualified_function_nodes_every_solution (fuel : Nat) (names : TypeNames)
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
      simp [validateQualifiedFunction, except_bind_ok, except_map_ok, exists_require_ok, supported, complete] at accepted
      obtain ⟨_, chosen, found, unique, propagation, checked, same⟩ := accepted
      subst chosen
      obtain ⟨resultUnit, checked⟩ := checked
      cases resultUnit
      rw [substitute_compose]
      exact validated_qualified_node_block_instantiates fuel names (parameters ++ globals) principal later
        scheme.requirements fields binderTypes (substitute principal result) body candidate checked sorted solves

theorem validated_source_facts_nodes_every_solution (fuel : Nat) (sourceId : Nat) (text : String)
    (factsCodes : ExchangeCodes) (declaration bodyIndex : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (ordered : List Constraint)
    (steps : List DerivationStep) (solvedCount : Nat) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (fields : FieldResults) (inhabitant : List (Variable × MonoType))
    (principal : Substitution)
    (accepted : validateSourceFactsBody fuel sourceId text factsCodes declaration bodyIndex names parameters globals
      result ordered steps solvedCount candidate binderTypes fields inhabitant = .ok principal) :
    ∃ unit facts scopes source body scheme, admitSource sourceId text = .ok unit ∧
      decodeSelfScopedFactsDocument fuel (sourceNameResolver unit) factsCodes = some (facts, scopes) ∧
      facts.bodies[bodyIndex]? = some body ∧ unit.declarations[declaration]? = some (.function source) ∧
      schemeFromBody (scopedVariableResolver scopes) body = some scheme ∧
      ∀ later, WellSorted later → Solves scheme.requirements later →
        (∃ generated state,
          (block fuel names (parameters ++ globals) result source.body).run
            (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
          QualifiedScheme .generalize (globalEnvironment globals)
            (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
          BlockHasType names (compose later principal) (parameters ++ globals)
            (substitute (compose later principal) result) source.body) ∧
        NodeBlockHasType (compose later principal) (parameters ++ globals)
          (substitute (compose later principal) result) candidate := by
  unfold validateSourceFactsBody at accepted
  cases admitted : admitSource sourceId text with
  | error failure =>
    rw [admitted] at accepted
    cases failure <;> cases accepted
  | ok unit =>
    simp only [admitted] at accepted
    cases decoded : decodeSelfScopedFactsDocument fuel (sourceNameResolver unit) factsCodes with
    | none => simp [decoded] at accepted
    | some value =>
      obtain ⟨facts, scopes⟩ := value
      cases selected : facts.bodies[bodyIndex]? with
      | none => simp [decoded, selected] at accepted
      | some body =>
        simp [decoded, selected] at accepted
        obtain ⟨source, member, bodyChecked⟩ := validated_unit_body_belongs_to_unit fuel unit declaration
          (scopedVariableResolver scopes) names parameters globals result ordered steps solvedCount candidate binderTypes
          fields inhabitant body principal accepted
        obtain ⟨scheme, schemeDecoded, functionChecked⟩ := validated_qualified_body_has_function_certificate
          fuel unit source (scopedVariableResolver scopes) names parameters globals result ordered steps solvedCount
          candidate binderTypes fields inhabitant body principal bodyChecked
        refine ⟨unit, facts, scopes, source, body, scheme, rfl, decoded, selected, member, schemeDecoded, ?_⟩
        intro later sorted solves
        obtain ⟨generated, state, generatedBody, qualified, _, _, _, _, _, _, sourceTyped⟩ :=
          qualified_function_types_every_solution fuel names parameters globals result source.body ordered steps
            solvedCount scheme candidate binderTypes fields principal later inhabitant functionChecked sorted solves
        exact ⟨⟨generated, state, generatedBody, qualified, sourceTyped⟩,
          qualified_function_nodes_every_solution fuel names parameters globals result source.body ordered steps
            solvedCount scheme candidate binderTypes fields principal later inhabitant functionChecked sorted solves⟩

end Xsh.Conformance.Connected
