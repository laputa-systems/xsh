import Xsh.Typing.Connected.QualifiedFunction

namespace Xsh.Typing.Connected

open Xsh.Foundation
open Xsh.Syntax

def scopedExampleName : NameId := exampleName 57 (by decide)
def scopedExampleBinder : BinderId := ⟨⟨0, by decide⟩⟩
def scopedExampleSource : List Statement :=
  [.binding .var (.name scopedExampleName) none (.literal (.integer "0")),
   .expression (.valueBlock [.assign (.name scopedExampleName) .set (.literal (.integer "1"))]),
   .returnValue (some (.ident scopedExampleName))]
def scopedExampleNodes : List StatementNode :=
  [.binding scopedExampleBinder .mutable (.integer "0"),
   .evaluated (.valueBlock (.primitive .unit) [.assignment scopedExampleBinder (.integer "1")]),
   .returned (.read scopedExampleBinder [])]

def scopedExampleAccepted : Bool :=
  match validateBlock 32 exampleTypeNames [] identity (fun _ => some (.primitive .integer))
      (.primitive .integer) scopedExampleSource scopedExampleNodes with
  | .ok _ => true
  | .error _ => false

theorem scoped_example_is_typed : scopedExampleAccepted = true := by
  decide +kernel

def scopedGenerationAgrees : Bool :=
  match (block 32 exampleTypeNames [] (.primitive .integer) scopedExampleSource).run
      (initialBlockState [] (.primitive .integer)) with
  | .ok ([.binding binder .mutable (.integer "0"), .evaluated (.valueBlock _ [.assignment target (.integer "1")]),
      .returned (.read read [])], _) => decide (binder = scopedExampleBinder ∧ target = binder ∧ read = binder)
  | _ => false

theorem scoped_body_is_checked_at_discarded_completion : scopedGenerationAgrees = true := by
  decide +kernel

def scopedQualifiedAccepted : Bool :=
  match validateQualifiedFunction 32 exampleTypeNames [] [] (.primitive .integer) scopedExampleSource
      [.equal (.primitive .integer) (.primitive .integer), .equal (.variable ⟨0, .value⟩) (.primitive .unit),
       .equal (.primitive .integer) (.primitive .integer)]
      [.reflexive, .bindLeft, .reflexive] 3 ⟨[], .function .argumentsEmpty (.primitive .integer), []⟩ scopedExampleNodes
      (fun _ => some (.primitive .integer)) (fun _ => none) [] with
  | .ok _ => true
  | .error _ => false

theorem scoped_qualified_function_is_accepted : scopedQualifiedAccepted = true := by
  decide +kernel

def integerFallthroughRejected : Bool :=
  match validateBlock 16 exampleTypeNames [] identity (fun _ => none) (.primitive .integer) [] [] with
  | .error .mismatch => true
  | _ => false

theorem unit_fallthrough_cannot_satisfy_integer_result : integerFallthroughRejected = true := by
  decide +kernel

def duplicateNestedBinder : List StatementNode :=
  [.evaluated (.record [
    (exampleName 58 (by decide), .valueBlock (.primitive .unit) [.binding scopedExampleBinder .mutable (.integer "1")]),
    (exampleName 59 (by decide), .valueBlock (.primitive .unit) [.binding scopedExampleBinder .mutable (.integer "2")])])]

theorem sibling_value_blocks_cannot_reuse_a_binder :
    decide (declaredBinders duplicateNestedBinder).Nodup = false := by
  decide +kernel

end Xsh.Typing.Connected
