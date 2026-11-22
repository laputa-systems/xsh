import Xsh.Proofs.Core.Requires

namespace Xsh.Proofs.Core.Contexts

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreStatements
open Xsh.Semantics.CoreEval
open Xsh.Semantics.CoreControl
open Xsh.Proofs.Core.Control

section Root

variable {Declaration Identity Source : Type}

/-- The `error_contexts` fragment root (SPEC 8.8): `ctx description { body }`
evaluates its description once and runs its body as a value block; a raised
or runtime failure leaving the body gains exactly one context frame with the
description after the body's cleanup, its family, variant, message, payload
and earlier contexts unchanged; and every other outcome, a value returned as
data included, is left as it is. -/
theorem error_contexts [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame : Frame world) :
    (∀ message body, runValue machine (fuel + 1) frame (.errorContext message body) =
      (evalExpr machine fuel frame message).bind fun described next =>
        match described.eraseNominal with
        | .scalar (.string text) => annotate text (valueScope machine fuel next body)
        | _ => .exec .unsupported) ∧
    (∀ text (data : ErrorData Declaration Identity Source) (last : Frame world),
      annotate text (.exec (.failed (.raised (.error data)) last)) =
        .exec (.failed (.raised (.error { data with contexts := data.contexts ++ [⟨"ctx", some text, none⟩] })) last)) ∧
    (∀ text (data : ErrorData Declaration Identity Source) (last : Frame world),
      annotate text (.exec (.failed (.runtime (.error data)) last)) =
        .exec (.failed (.runtime (.error { data with contexts := data.contexts ++ [⟨"ctx", some text, none⟩] })) last)) ∧
    (∀ text (value : Value Declaration Identity Source) (last : Frame world),
      annotate text (.value value last) = .value value last) := by
  refine ⟨fun _ _ => rfl, fun _ _ _ => rfl, fun _ _ _ => rfl, fun _ _ _ => rfl⟩

end Root

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.Slices.Witness

def fetchError : ErrorData Unit Unit Unit where
  family := ()
  variant := "Missing"
  kind := "fetch"
  message := "missing"
  payload := []
  facets := []
  span := none
  contexts := []
  cause := none
  abort := none

def holding : Frame witnessWorld := { start with locals := [(gName, .err (.error fetchError))] }

def runHolding (body : List Xsh.Syntax.Statement) : Exec witnessWorld := runStatements machine 40 holding body

def described (body : List Xsh.Syntax.Statement) : Xsh.Syntax.Expr :=
  .errorContext (.literal (.string "installing core")) body

def raisedContexts : Exec witnessWorld → Option (String × List (Option String))
  | .failed (.raised (.error data)) _ => some (data.variant, data.contexts.map ErrorContext.message)
  | _ => none

def returnedContexts : Option (Value Unit Unit Unit) → Option (List (Option String))
  | some (.err (.error data)) => some (data.contexts.map ErrorContext.message)
  | _ => none

/-- As the verification binary reports `(ctx: outer step)` on a schema error
that `?` propagates out of a `ctx` body: the error keeps its variant and gains
the description. -/
theorem propagated_failure_gains_context :
    raisedContexts (runHolding [.expression (described [.expression (.tryExpr (ident gName))])]) =
      some ("Missing", [some "installing core"]) := by
  decide +kernel

/-- An `Err` a `ctx` block gives as its value is data and is not annotated. -/
theorem returned_err_unannotated :
    returnedContexts (readValue (runHolding [letValue (described [.expression (ident gName)])])) = some [] := by
  decide +kernel

/-- A `ctx` that annotates every `Err` it sees marks the returned data; the
model leaves it unannotated. -/
theorem annotating_data_rejected :
    (withContext "installing core" (.err (.error fetchError) : Value Unit Unit Unit) matches .err _) ∧
      returnedContexts (some (.err (withContext "installing core" (.error fetchError)))) = some [some "installing core"] ∧
      returnedContexts (readValue (runHolding [letValue (described [.expression (ident gName)])])) = some [] := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Contexts
