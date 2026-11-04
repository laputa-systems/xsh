import Xsh.Typing.Connected.QualifiedFunction

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

theorem duplicate_phantom_result_row_is_refused :
    checksGroundScheme ⟨[], .function .argumentsEmpty
      (.result (.primitive .integer)
        (.record (.rowField generatedProjectionField (.primitive .integer)
          (.rowField generatedProjectionField (.primitive .boolean) .rowEmpty)))), []⟩ [] = false := by
  decide +kernel

def cycleLocal : NameId := exampleName 42 (by decide)
def cycleBinder : BinderId := ⟨⟨41, by decide⟩⟩
def cycleVariable : Variable := ⟨1, .value⟩
def cycleBody : List Statement :=
  [.binding .var (.name cycleLocal) none (.ident generatedProjectionName),
   .assign (.name cycleLocal) .set (.field (.ident generatedProjectionName) generatedProjectionField),
   .tailName cycleLocal]
def cycleCandidate : List StatementNode :=
  [.binding cycleBinder .mutable (.variable generatedParameterVariable) (.read generatedProjectionParameter.binder []),
   .assignment cycleBinder (.field (.read generatedProjectionParameter.binder []) generatedProjectionField),
   .evaluated (.read cycleBinder [])]
def cycleConstraints : List Constraint :=
  [.equal (.variable generatedParameterVariable) (.variable generatedParameterVariable),
   .field (.variable generatedParameterVariable) generatedProjectionField (.variable cycleVariable),
   .store (.variable generatedParameterVariable) (.variable cycleVariable)]
def cycleScheme : Scheme :=
  ⟨[generatedParameterVariable, cycleVariable],
    .function (.argument (.variable generatedParameterVariable) .argumentsEmpty) (.variable generatedParameterVariable),
    [.field (.variable generatedParameterVariable) generatedProjectionField (.variable cycleVariable),
     .store (.variable generatedParameterVariable) (.variable cycleVariable)]⟩
def intCycleWitness : List (Variable × MonoType) := [(generatedParameterVariable, .primitive .integer), (cycleVariable, .primitive .integer)]
def recordCycleWitness : List (Variable × MonoType) :=
  [(generatedParameterVariable, .record (.rowField generatedProjectionField (.primitive .integer) .rowEmpty)),
   (cycleVariable, .primitive .integer)]

def cycleSchemeWasPrincipallyChecked : Bool :=
  match validateGeneratedFunctionScheme 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedParameterVariable) cycleBody cycleConstraints [.reflexive] 1 cycleScheme with
  | .ok _ => true | .error _ => false

def cycleFunctionRejected : Bool :=
  match validateQualifiedFunction 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedParameterVariable) cycleBody cycleConstraints [.reflexive] 1 cycleScheme
      cycleCandidate (fun _ => some (.variable generatedParameterVariable))
      (fun _ => some (.variable cycleVariable)) intCycleWitness with
  | .error .mismatch => true | _ => false

def cycleRecordFunctionRejected : Bool :=
  match validateQualifiedFunction 24 exampleTypeNames [generatedProjectionParameter] []
      (.variable generatedParameterVariable) cycleBody cycleConstraints [.reflexive] 1 cycleScheme
      cycleCandidate (fun _ => some (.variable generatedParameterVariable))
      (fun _ => some (.variable cycleVariable)) recordCycleWitness with
  | .error .mismatch => true | _ => false

theorem record_cycle_witness_fails_storage_after_field_projection :
    checksConstraint (witnessSubstitution recordCycleWitness)
      (.field (.variable generatedParameterVariable) generatedProjectionField (.variable cycleVariable)) = true ∧
    checksConstraint (witnessSubstitution recordCycleWitness)
      (.store (.variable generatedParameterVariable) (.variable cycleVariable)) = false := by
  decide +kernel

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

def emptyIntScheme : Scheme := ⟨[], .function .argumentsEmpty (.primitive .integer), []⟩
def emptyIntConstraints : List Constraint := [.equal (.primitive .integer) (.primitive .unit)]
def emptyFunctionRejected : Bool :=
  match validateQualifiedFunction 24 exampleTypeNames [] [] (.primitive .integer) [] emptyIntConstraints [.reflexive] 1
      emptyIntScheme [] (fun _ => none) (fun _ => none) [] with
  | .error .mismatch => true | _ => false

def emptyInstanceRejected : Bool :=
  match validateQualifiedFunctionInstance 24 exampleTypeNames [] [] (.primitive .integer) [] emptyIntConstraints [.reflexive] 1
      emptyIntScheme identity [] (fun _ => none) with
  | .error .mismatch => true | _ => false

def partialBranchBody : List Statement :=
  [.ifStmt [(.literal (.bool true), [.returnValue (some (.literal (.integer "1")))])] none]
def partialBranchCandidate : List StatementNode :=
  [.conditional [(.boolean true, [.returned (.integer "1")])] []]
def partialBranchConstraints : List Constraint :=
  [.equal (.primitive .boolean) (.primitive .boolean), .equal (.primitive .integer) (.primitive .integer),
   .equal (.primitive .integer) (.primitive .unit)]
def partialBranchPrincipalRejected : Bool :=
  match validateGeneratedFunctionScheme 24 exampleTypeNames [] [] (.primitive .integer) partialBranchBody
      partialBranchConstraints [.reflexive, .reflexive, .reflexive] 3 emptyIntScheme with
  | .error .mismatch => true | _ => false

def partialBranchRejected : Bool :=
  match validateQualifiedFunction 24 exampleTypeNames [] [] (.primitive .integer) partialBranchBody
      partialBranchConstraints [.reflexive, .reflexive, .reflexive] 3 emptyIntScheme partialBranchCandidate
      (fun _ => none) (fun _ => none) [] with
  | .error .mismatch => true | _ => false

def partialBranchInstanceRejected : Bool :=
  match validateQualifiedFunctionInstance 24 exampleTypeNames [] [] (.primitive .integer) partialBranchBody
      partialBranchConstraints [.reflexive, .reflexive, .reflexive] 3 emptyIntScheme identity partialBranchCandidate
      (fun _ => none) with
  | .error .mismatch => true | _ => false

def completeBranchBody : List Statement :=
  [.ifStmt [(.literal (.bool true), [.returnValue (some (.literal (.integer "1")))])]
    (some [.returnValue (some (.literal (.integer "2")))])]
def completeBranchCandidate : List StatementNode :=
  [.conditional [(.boolean true, [.returned (.integer "1")])] [.returned (.integer "2")]]
def completeBranchConstraints : List Constraint :=
  [.equal (.primitive .boolean) (.primitive .boolean), .equal (.primitive .integer) (.primitive .integer),
   .equal (.primitive .integer) (.primitive .integer)]
def completeBranchAccepted : Bool :=
  match validateQualifiedFunction 24 exampleTypeNames [] [] (.primitive .integer) completeBranchBody
      completeBranchConstraints [.reflexive, .reflexive, .reflexive] 3 emptyIntScheme completeBranchCandidate
      (fun _ => none) (fun _ => none) [] with
  | .ok _ => true | _ => false

theorem function_fallthrough_is_not_accepted :
    emptyFunctionRejected = true ∧ emptyInstanceRejected = true ∧
    partialBranchPrincipalRejected = true ∧ partialBranchRejected = true ∧
    partialBranchInstanceRejected = true ∧ completeBranchAccepted = true := by
  decide +kernel

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

theorem uncalled_functions_refuse_inconsistent_inhabitants :
    cycleSchemeWasPrincipallyChecked = true ∧ cycleFunctionRejected = true ∧ cycleRecordFunctionRejected = true ∧
    intFieldSchemeWasPrincipallyChecked = true ∧ intFieldRejected = true := by
  decide +kernel

mutual
  def storageDepth : MonoType → Nat
    | .record row => storageRowDepth row + 1
    | .result value error => max (storageDepth value) (storageDepth error) + 1
    | .nominal _ base => storageDepth base + 1
    | _ => 0

  def storageRowDepth : MonoType → Nat
    | .rowField _ value tail => max (storageDepth value) (storageRowDepth tail)
    | _ => 0
end

theorem field_occurs_storage_depth {name : Xsh.Foundation.NameId} {value row : MonoType}
    (found : FieldOccurs name value row) : storageDepth value ≤ storageRowDepth row := by
  induction found with
  | here => exact Nat.le_max_left _ _
  | later _ _ ih => exact Nat.le_trans ih (Nat.le_max_right _ _)

theorem store_compatible_depth {expected actual : MonoType}
    (compatible : StoreCompatible expected actual) : storageDepth expected ≤ storageDepth actual := by
  induction compatible using StoreCompatible.rec
    (motive_2 := fun expected actual _ => storageRowDepth expected ≤ storageRowDepth actual) with
  | same => exact Nat.le_refl _
  | result _ _ left right => exact Nat.succ_le_succ (Nat.max_le.mpr ⟨Nat.le_trans left (Nat.le_max_left _ _), Nat.le_trans right (Nat.le_max_right _ _)⟩)
  | record _ row => exact Nat.succ_le_succ row
  | nominal _ base => exact Nat.succ_le_succ base
  | empty => exact Nat.zero_le _
  | field found _ _ value rest =>
    exact Nat.max_le.mpr ⟨Nat.le_trans value (field_occurs_storage_depth found), rest⟩

theorem store_field_cycle_has_no_solution (substitution : Substitution) :
    ¬ (satisfies substitution (.field (.variable generatedParameterVariable) generatedProjectionField (.variable cycleVariable)) ∧
       satisfies substitution (.store (.variable generatedParameterVariable) (.variable cycleVariable))) := by
  intro solved
  obtain ⟨⟨row, same, found⟩, compatible⟩ := solved
  have bigger := store_compatible_depth compatible
  rw [same] at bigger
  have smaller := field_occurs_storage_depth found
  have impossible := Nat.le_trans bigger smaller
  exact (Nat.not_succ_le_self _) impossible

theorem cyclic_store_scheme_has_no_solution (substitution : Substitution) :
    ¬ Solves cycleScheme.requirements substitution := by
  intro solves
  apply store_field_cycle_has_no_solution substitution
  exact ⟨solves _ (by simp [cycleScheme]), solves _ (by simp [cycleScheme])⟩


end Xsh.Typing.Connected
