import Xsh.Typing.Connected.QualifiedBlockSound

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def checksGroundScheme (scheme : Scheme) (arguments : List (Variable × MonoType)) : Bool :=
  ((freeVariables scheme.body ++ scheme.requirements.flatMap constraintVariables).all fun id =>
    (freeVariables (witnessSubstitution arguments id)).isEmpty) &&
    checksUniqueRows (substitute (witnessSubstitution arguments) scheme.body) &&
    scheme.requirements.all (checksConstraintUniqueRows (witnessSubstitution arguments))

theorem checked_ground_scheme_has_unique_rows (scheme : Scheme) (arguments : List (Variable × MonoType))
    (checked : checksGroundScheme scheme arguments = true) :
    checksUniqueRows (substitute (witnessSubstitution arguments) scheme.body) = true ∧
      ∀ requirement ∈ scheme.requirements,
        checksConstraintUniqueRows (witnessSubstitution arguments) requirement = true := by
  simp [checksGroundScheme, Bool.and_eq_true, List.all_eq_true] at checked
  exact ⟨checked.1.2, checked.2⟩

def validateQualifiedFunction (fuel : Nat) (names : TypeNames) (parameters globals : List Binding)
    (result : MonoType) (body : List Statement) (ordered : List Constraint) (steps : List DerivationStep)
    (solvedCount : Nat) (scheme : Scheme) (candidate : List StatementNode) (binderTypes : BinderTypes)
    (fields : FieldResults) (inhabitant : List (Variable × MonoType)) : Except ValidationError Substitution := do
  requireCheck (checksFiniteSubstitution inhabitant && checksGroundScheme scheme inhabitant &&
    checksConstraints (witnessSubstitution inhabitant) scheme.requirements)
  let principal ← validateGeneratedFunctionScheme fuel names parameters globals result body ordered steps solvedCount scheme
  requireCheck (decide (declaredBinders candidate).Nodup)
  if supportedStatementTails true candidate then pure () else throw .unsupported
  if guaranteesFunctionResult candidate then pure () else throw .unsupported
  requireCheck (checksPropagationErrors (propagationContext (substitute principal result)) principal
    (blockPropagationErrors candidate))
  validateQualifiedBlock fuel names (parameters ++ globals) principal scheme.requirements fields binderTypes
    (substitute principal result) body candidate
  pure principal

theorem qualified_function_types_every_solution (fuel : Nat) (names : TypeNames)
    (parameters globals : List Binding) (result : MonoType) (body : List Statement)
    (ordered : List Constraint) (steps : List DerivationStep) (solvedCount : Nat) (scheme : Scheme)
    (candidate : List StatementNode) (binderTypes : BinderTypes) (fields : FieldResults)
    (principal later : Substitution) (inhabitant : List (Variable × MonoType))
    (accepted : validateQualifiedFunction fuel names parameters globals result body ordered steps solvedCount
      scheme candidate binderTypes fields inhabitant = .ok principal)
    (sorted : WellSorted later) (solves : Solves scheme.requirements later) :
    ∃ generated state,
      (block fuel names (parameters ++ globals) result body).run
        (initialBlockState (parameters ++ globals) result) = .ok (generated, state) ∧
      QualifiedScheme .generalize (globalEnvironment globals)
        (.function (parameterTypes parameters) result) state.constraints principal scheme ∧
      Solves state.constraints (compose later principal) ∧
      (declaredBinders candidate).Nodup ∧
      guaranteesFunctionResult candidate = true ∧
      WellSorted (witnessSubstitution inhabitant) ∧
      checksGroundScheme scheme inhabitant = true ∧
      Solves scheme.requirements (witnessSubstitution inhabitant) ∧
      BlockHasType names (compose later principal) (parameters ++ globals)
        (substitute (compose later principal) result) body := by
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
    obtain ⟨generated, state, generatedBody, qualified⟩ :=
      generated_function_scheme_is_principal fuel names parameters globals result body ordered
        steps solvedCount scheme principal found
    refine ⟨generated, state, generatedBody, qualified,
      (qualified.1.2.1 later sorted).mpr solves, unique, rfl,
      finite_substitution_is_well_sorted inhabitant finite, ground, inhabited, ?_⟩
    rw [substitute_compose]
    exact validated_qualified_block_instantiates fuel names (parameters ++ globals) principal later
      scheme.requirements fields binderTypes (substitute principal result) body candidate checked sorted solves

end Xsh.Typing.Connected
