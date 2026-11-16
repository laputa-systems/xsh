import Xsh.Typing.Connected.BlockCertificate

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def constructorName : NameId := exampleName 8 (by decide)

def constructorScope : Scope :=
  [⟨constructorName, ⟨⟨8, by decide⟩⟩, .immutable, okScheme⟩]

def okConstruction (argument : Expr) : Expr :=
  .call (.ident constructorName) [.positional argument]

def nestedOkBody : List Statement :=
  [.returnValue (some (okConstruction
    (.binary .add (.tryExpr (okConstruction (.literal (.integer "1")))) (.literal (.integer "1")))))]

def plainOkBody : List Statement :=
  [.returnValue (some (okConstruction (.literal (.integer "1"))))]

def stringErrorResult : MonoType := .result (.primitive .integer) (.primitive .string)
def broadErrorResult : MonoType := .result (.primitive .integer) (.primitive .error)

def generatedConstraints (result : MonoType) (body : List Statement) : Option (List Constraint) :=
  match (block 16 exampleTypeNames constructorScope result body).run
      (initialBlockState constructorScope result) with
  | .ok (_, state) => some state.constraints
  | .error _ => none

def derivationAccepts (steps : List DerivationStep) (result : MonoType) (body : List Statement) : Bool :=
  match generatedConstraints result body with
  | some constraints => match checkDerivation steps constraints with | .ok _ => true | .error _ => false
  | none => false

def nestedOkSteps : List DerivationStep :=
  [.function, .argument, .bindLeft, .reflexive, .bindRight, .result, .bindRight, .bindLeft, .result, .bindRight,
    .bindRight, .reflexive, .reflexive, .function, .argument, .bindLeft, .reflexive, .bindRight, .result, .bindRight,
    .reflexive, .result, .reflexive, .bindLeft]

def plainOkSteps : List DerivationStep :=
  [.function, .argument, .bindLeft, .reflexive, .bindRight, .result, .reflexive, .bindLeft]

def operandDefault : Constraint :=
  .equal (.variable ⟨6, .value⟩) (.result (.variable ⟨7, .value⟩) (.primitive .error))
def operandResult : Constraint :=
  .equal (.variable ⟨6, .value⟩) (.result (.variable ⟨8, .value⟩) (.variable ⟨9, .value⟩))
def declaredResult : Constraint :=
  .equal stringErrorResult (.result (.variable ⟨11, .value⟩) (.variable ⟨9, .value⟩))

theorem generated_constraints_contain_the_error_conflict :
    (match generatedConstraints stringErrorResult nestedOkBody with
      | some constraints => decide (operandDefault ∈ constraints ∧ operandResult ∈ constraints ∧
          declaredResult ∈ constraints)
      | none => false) = true := by
  decide +kernel

theorem the_error_conflict_has_no_solution (solution : Substitution)
    (solves : Solves [operandDefault, operandResult, declaredResult] solution) : False := by
  have first := solves operandDefault (by simp)
  have second := solves operandResult (by simp)
  have third := solves declaredResult (by simp)
  simp only [operandDefault, operandResult, declaredResult, stringErrorResult, satisfies, substitute] at first second third
  rw [first] at second
  simp only [MonoType.result.injEq] at second third
  have same := second.2.trans third.2.symm
  simp at same

/-- A bare `Ok` under `?` takes the error type `Error` (SPEC 8.5), so the nested construction
has no solution in a function whose error type is `Str`. -/
theorem nested_ok_under_propagation_is_rejected_at_a_string_error :
    ∃ constraints, generatedConstraints stringErrorResult nestedOkBody = some constraints ∧
      ∀ solution, ¬ Solves constraints solution := by
  have conflict := generated_constraints_contain_the_error_conflict
  cases found : generatedConstraints stringErrorResult nestedOkBody with
  | none => simp [found] at conflict
  | some constraints =>
    refine ⟨constraints, rfl, fun solution solves => ?_⟩
    simp only [found, decide_eq_true_eq] at conflict
    obtain ⟨first, second, third⟩ := conflict
    exact the_error_conflict_has_no_solution solution (by
      intro constraint member
      apply solves
      simp only [List.mem_cons, List.not_mem_nil, or_false] at member
      rcases member with rfl | rfl | rfl <;> assumption)

theorem nested_ok_under_propagation_is_accepted_at_the_broad_error :
    derivationAccepts nestedOkSteps broadErrorResult nestedOkBody = true := by
  decide +kernel

theorem an_expected_ok_keeps_the_declared_error_type :
    derivationAccepts plainOkSteps stringErrorResult plainOkBody = true ∧
    derivationAccepts plainOkSteps broadErrorResult plainOkBody = true := by
  decide +kernel

end Xsh.Typing.Connected
