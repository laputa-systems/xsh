import Xsh.Typing.Connected.QualifiedFunction

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def cycleLocal : NameId := exampleName 42 (by decide)
def cycleBinder : BinderId := ⟨⟨41, by decide⟩⟩
def cycleVariable : Variable := ⟨1, .value⟩
def cycleBody : List Statement :=
  [.binding .var (.name cycleLocal) none (.ident generatedProjectionName),
   .assign (.name cycleLocal) .set (.field (.ident generatedProjectionName) generatedProjectionField),
   .tailName cycleLocal]
def cycleCandidate : List StatementNode :=
  [.binding cycleBinder .mutable (.read generatedProjectionParameter.binder []),
   .assignment cycleBinder (.field (.read generatedProjectionParameter.binder []) generatedProjectionField),
   .evaluated (.read cycleBinder [])]
def cycleConstraints : List Constraint :=
  [.equal (.variable cycleVariable) (.variable generatedParameterVariable),
   .equal (.variable generatedParameterVariable) (.variable generatedParameterVariable),
   .field (.variable generatedParameterVariable) generatedProjectionField (.variable cycleVariable)]
def cycleScheme : Scheme :=
  ⟨[generatedParameterVariable],
    .function (.argument (.variable generatedParameterVariable) .argumentsEmpty) (.variable generatedParameterVariable),
    [.field (.variable generatedParameterVariable) generatedProjectionField (.variable generatedParameterVariable)]⟩
def intCycleWitness : List (Variable × MonoType) := [(generatedParameterVariable, .primitive .integer)]

def cycleSchemeWasPrincipallyChecked : Bool :=
  match validateGeneratedFunctionScheme 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedParameterVariable) cycleBody cycleConstraints [.bindLeft, .reflexive] 2 cycleScheme with
  | .ok _ => true | .error _ => false

def cycleFunctionRejected : Bool :=
  match validateQualifiedFunction 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedParameterVariable) cycleBody cycleConstraints [.bindLeft, .reflexive] 2 cycleScheme
      cycleCandidate (fun _ => some (.variable generatedParameterVariable))
      (fun _ => some (.variable generatedParameterVariable)) intCycleWitness with
  | .error .mismatch => true | _ => false

def intFieldRequirement : Scheme :=
  ⟨[], .function (.argument (.primitive .integer) .argumentsEmpty) (.primitive .integer),
    [.field (.primitive .integer) generatedProjectionField (.primitive .integer)]⟩
def intParameter : Binding :=
  ⟨generatedProjectionName, generatedProjectionParameter.binder, .immutable, ⟨[], .primitive .integer, []⟩⟩
def intFieldConstraints : List Constraint :=
  [.equal (.variable generatedParameterVariable) (.primitive .integer),
   .field (.primitive .integer) generatedProjectionField (.variable generatedParameterVariable)]
def intFieldSchemeWasPrincipallyChecked : Bool :=
  match validateGeneratedFunctionScheme 24 exampleTypeNames [intParameter] [] (.primitive .integer)
      generatedProjectionBody intFieldConstraints [.bindLeft] 1 intFieldRequirement with
  | .ok _ => true | .error _ => false

def intFieldRejected : Bool :=
  match validateQualifiedFunction 24 exampleTypeNames [intParameter] []
      (.primitive .integer) generatedProjectionBody intFieldConstraints [.bindLeft] 1 intFieldRequirement generatedProjectionCandidate
      (fun _ => none) (fun _ => some (.primitive .integer)) [] with
  | .error .mismatch => true | _ => false

theorem field_occurs_smaller {name : NameId} {value row : MonoType}
    (found : FieldOccurs name value row) : sizeOf value < sizeOf row := by
  induction found with
  | here => simp_wf; omega
  | later different found ih => simp_wf; omega

theorem cyclic_field_requirement_has_no_solution (substitution : Substitution) :
    ¬ satisfies substitution
      (.field (.variable generatedParameterVariable) generatedProjectionField (.variable generatedParameterVariable)) := by
  intro solved
  obtain ⟨row, same, found⟩ := solved
  have smaller := field_occurs_smaller found
  rw [same] at smaller
  have larger : sizeOf row < sizeOf (MonoType.record row) := by simp_wf
  exact Nat.lt_asymm smaller larger

theorem concrete_integer_field_has_no_solution (substitution : Substitution) :
    ¬ satisfies substitution (.field (.primitive .integer) generatedProjectionField (.primitive .integer)) := by
  simp [satisfies, substitute]

theorem uncalled_unsatisfiable_functions_are_refused :
    cycleSchemeWasPrincipallyChecked = true ∧ cycleFunctionRejected = true ∧
    intFieldSchemeWasPrincipallyChecked = true ∧ intFieldRejected = true := by
  decide +kernel

end Xsh.Typing.Connected
