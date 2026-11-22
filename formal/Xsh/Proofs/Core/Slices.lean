import Xsh.Proofs.Core.MatchValues

namespace Xsh.Proofs.Core.Slices

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

section Laws

variable {Declaration Identity Source : Type}

theorem clampBound_le (length : Nat) (bound : Int) : clampBound length bound ≤ length := by
  dsimp only [clampBound]
  split <;> omega

/-- A negative bound no further back than the start counts from the end. -/
theorem clampBound_negative (length : Nat) (back : Nat) (within : back ≤ length) (positive : 0 < back) :
    clampBound length (-(back : Int)) = length - back := by
  dsimp only [clampBound]
  split <;> omega

/-- A slice is a contiguous run of the items, as long as its clamped end is
past its clamped start. -/
theorem sliceItems_spec {α : Type} (items : List α) (low high : Option Int) :
    (sliceItems items low high).IsInfix items ∧
      (sliceItems items low high).length =
        clampBound items.length (high.getD items.length) - clampBound items.length (low.getD 0) := by
  have startBound := clampBound_le items.length (low.getD 0)
  have finishBound := clampBound_le items.length (high.getD items.length)
  refine ⟨?_, ?_⟩
  · unfold sliceItems
    exact (List.take_prefix _ _).isInfix.trans (List.drop_suffix _ _).isInfix
  · simp only [sliceItems, List.length_take, List.length_drop]
    omega

theorem sliceItems_whole {α : Type} (items : List α) : sliceItems items none none = items := by
  have whole : clampBound items.length (items.length : Int) = items.length := by
    dsimp only [clampBound]
    split <;> omega
  have zero : clampBound items.length 0 = 0 := by
    dsimp only [clampBound]
    split <;> omega
  simp only [sliceItems, Option.getD_none, whole, zero, List.drop_zero, Nat.sub_zero, List.take_length]

/-- A guarded hop (SPEC 6.7): `null` makes the operation `null`, an `Err`
raises its error, an `Ok` gives its payload, and any other receiver is
itself. -/
theorem hopOf_spec (receiver : Value Declaration Identity Source) :
    (receiver.eraseNominal = .scalar .null → hopOf true receiver = .value none) ∧
    (∀ error, receiver.eraseNominal = .err error → hopOf true receiver = .failure (.raised error)) ∧
    (∀ payload, receiver.eraseNominal = .ok payload → hopOf true receiver = .value (some payload)) ∧
    hopOf false receiver = .value (some receiver) := by
  refine ⟨fun shape => ?_, fun error shape => ?_, fun payload shape => ?_, rfl⟩ <;>
    simp [hopOf, propagate, shape, Result.bind]

/-- The `slices_and_guarded_hops` fragment root (SPEC 6.6, 6.7): a slice of a
`List`, `Str` or `Bytes` is the contiguous run between its bounds, each
counted from the end when negative and clamped into the value, and an
omitted bound is the start or the end; a guarded field, index or slice on a
`null` receiver is `null` without evaluating its index or bounds, propagates
a `Result` receiver as `?` does, and is the ordinary operation otherwise. -/
theorem slices_and_guarded_hops [DecidableEq Declaration] {world : World Declaration Identity Source}
    (machine : Machine world) (fuel : Nat) (frame next : Frame world) (base : Xsh.Syntax.Expr)
    (null : evalExpr machine fuel frame base = .value (.scalar .null) next) :
    (∀ {α : Type} (items : List α) low high,
      (sliceItems items low high).IsInfix items ∧
        (sliceItems items low high).length =
          clampBound items.length (high.getD items.length) - clampBound items.length (low.getD 0)) ∧
    (∀ {α : Type} (items : List α), sliceItems items none none = items) ∧
    (∀ length back, back ≤ length → 0 < back → clampBound length (-(back : Int)) = length - back) ∧
    (∀ receiver : Value Declaration Identity Source,
      (receiver.eraseNominal = .scalar .null → hopOf true receiver = .value none) ∧
      (∀ error, receiver.eraseNominal = .err error → hopOf true receiver = .failure (.raised error)) ∧
      (∀ payload, receiver.eraseNominal = .ok payload → hopOf true receiver = .value (some payload)) ∧
      hopOf false receiver = .value (some receiver)) ∧
    (∀ key, evalExpr machine (fuel + 1) frame (.index base key true) = .value (.scalar .null) next) ∧
    (∀ start finish, evalExpr machine (fuel + 1) frame (.slice base start finish true) = .value (.scalar .null) next) ∧
    (∀ name, evalExpr machine (fuel + 1) frame (.nullSafeField base name) = .value (.scalar .null) next) := by
  refine ⟨fun items low high => sliceItems_spec items low high, fun items => sliceItems_whole items,
    clampBound_negative, hopOf_spec, fun key => ?_, fun start finish => ?_, fun name => ?_⟩ <;>
    simp [evalExpr, null, Outcome.bind, hopOf, fromEval, resultThen, Value.eraseNominal]

end Laws

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.Control.Witness
open Xsh.Proofs.Core.MatchValues.Witness

def letValue (value : Xsh.Syntax.Expr) : Xsh.Syntax.Statement := .binding .letBinding (.name xName) none value

def readValue (outcome : Exec witnessWorld) : Option (Value Unit Unit Unit) :=
  match outcome with
  | .normal frame =>
    match frame.environment.cellOf xName with
    | .ok cell => (frame.store cell).map fun found => found.contents.value
    | .error _ => none
  | _ => none

def ints : Value Unit Unit Unit → Option (List Int)
  | .list items => items.mapM fun item => match item with
    | .scalar (.integer value) => some value.val
    | _ => none
  | _ => none

def text : Value Unit Unit Unit → Option String
  | .scalar (.string value) => some value
  | _ => none

def bytes : Value Unit Unit Unit → Option (List UInt8)
  | .scalar (.bytes value) => some value
  | _ => none

def isNullValue : Value Unit Unit Unit → Bool
  | .scalar .null => true
  | _ => false

def sliced (value : Xsh.Syntax.Expr) (start finish : Option Xsh.Syntax.Expr) : Option (Value Unit Unit Unit) :=
  readValue (run [letValue (.slice value start finish false)])

def negative (lexeme : String) : Xsh.Syntax.Expr := .unary .neg (int lexeme)

def numbers : Xsh.Syntax.Expr := items ["1", "2", "7", "8"]

/-- As the verification binary prints `[2,3,4] [] [1,2,3,4] [1,2,3]` for
`xs[-3..]`, `xs[2..1]`, `xs[-10..10]` and `xs[..-1]` over `[1, 2, 3, 4]`. -/
theorem list_slices :
    (sliced numbers (some (negative "7")) none).bind ints = some [1, 2, 7, 8] ∧
    (sliced numbers (some (negative "2")) none).bind ints = some [7, 8] ∧
    (sliced numbers (some (int "2")) (some (int "1"))).bind ints = some [] ∧
    (sliced numbers (some (negative "8")) (some (int "8"))).bind ints = some [1, 2, 7, 8] ∧
    (sliced numbers none (some (negative "1"))).bind ints = some [1, 2, 7] := by
  decide +kernel

/-- As the verification binary prints `é🦀`, `hell` and `true` for
`"aé🦀"[1..3]`, `"hello"[..-1]` and `b"abcdef"[2..5] == b"cde"`: a `Str` slice
counts Unicode scalar values and a `Bytes` slice bytes. -/
theorem text_slices :
    (sliced (.literal (.string "aé🦀")) (some (int "1")) none).bind text = some "é🦀" ∧
    (sliced (.literal (.string "hello")) none (some (negative "1"))).bind text = some "hell" ∧
    (sliced (.literal (.bytes [97, 98, 99, 100, 101, 102])) (some (int "2")) (some (int "7"))).bind bytes =
      some [99, 100, 101, 102] := by
  decide +kernel

def divisionByZero : Xsh.Syntax.Expr := .binary .div (int "1") (int "0")

def nullThen (value : Xsh.Syntax.Expr) : List Xsh.Syntax.Statement :=
  [.binding .letBinding (.name nName) none (.literal .null), letValue value]

/-- As the verification binary prints `true` for `none?[0] == null`,
`none?[0..1] == null` and `r?.a == null` over `null` receivers and for
`some?[1] == 8` and `s?.a == 5` over present ones: a `null` receiver makes the
operation `null` without evaluating its index or bounds, here a division by
zero, and a present one runs the ordinary operation. -/
theorem guarded_hops :
    (readValue (run (nullThen (.index (ident nName) divisionByZero true)))).map isNullValue = some true ∧
    (readValue (run (nullThen (.slice (ident nName) (some divisionByZero) none true)))).map isNullValue = some true ∧
    (readValue (run (nullThen (.nullSafeField (ident nName) aName)))).map isNullValue = some true ∧
    (readValue (run [letValue (.index (items ["7", "8"]) (int "1") true)])).bind
      (fun value => match value with | .scalar (.integer found) => some found.val | _ => none) = some 8 := by
  decide +kernel

/-- Evaluating the index before testing the receiver fails with
`division-by-zero`; the guarded index is `null`. -/
theorem eager_index_rejected :
    runtimeMessage (run [letValue divisionByZero]) = some "division-by-zero" ∧
      (readValue (run (nullThen (.index (ident nName) divisionByZero true)))).map isNullValue = some true := by
  decide +kernel

/-- A slice that drops a negative start as zero instead of counting it from
the end keeps all four items; the model keeps the last two. -/
theorem unshifted_bound_rejected :
    (([1, 2, 7, 8] : List Int).drop (-2 : Int).toNat).length = 4 ∧
      (sliced numbers (some (negative "2")) none).bind ints = some [7, 8] := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Slices
