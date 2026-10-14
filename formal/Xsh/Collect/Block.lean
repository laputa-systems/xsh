/-!
This fragment models a finite collect block as an ordered event trace followed
by an already classified block exit. An escaping transfer records only its
broad kind, not its target or return/error payload; this model does not decide
whether a break or continue leaves a particular block. It covers list
accumulation and publication at normal exit. It omits effects, cleanup,
streams, retry, checker and type-inference behavior, AST evaluation, and
correspondence with the XSH runtime or Rust code.
-/

namespace Xsh.Collect.Block

universe u

/-- One finite collect event appends a value or delegates an ordered list. -/
inductive Event (α : Type u) where
  | yield (value : α)
  | delegate (values : List α)
  deriving DecidableEq, Repr

/-- A control transfer already classified as leaving this collect block. -/
inductive EscapingTransfer where
  | breaking
  | continuing
  | returning
  | failing
  deriving DecidableEq, Repr

/-- The supplied exit is normal or an already classified transfer out of the
collect block. -/
inductive Exit where
  | normal
  | escaping (kind : EscapingTransfer)
  deriving DecidableEq, Repr

/-- Normal completion publishes a list; control transfer preserves only its
kind, without target identity or return/error payload, and discards the
in-progress list. -/
inductive Completion (α : Type u) where
  | published (values : List α)
  | escaped (kind : EscapingTransfer)
  deriving DecidableEq, Repr

/-- Accumulate finite events in their execution order. -/
def collectEvents {α : Type u} : List (Event α) → List α
  | [] => []
  | .yield value :: rest => value :: collectEvents rest
  | .delegate values :: rest => values ++ collectEvents rest

/-- Reverse each delegated list, modeling a delegate-order regression. -/
def reverseDelegatedItems {α : Type u} (events : List (Event α)) : List (Event α) :=
  events.map fun event =>
    match event with
    | .yield value => .yield value
    | .delegate values => .delegate values.reverse

/-- Run the finite trace using an exit classification supplied by the caller;
only a normal exit publishes the list. -/
def evaluate {α : Type u} (events : List (Event α)) (exit : Exit) : Completion α :=
  match exit with
  | .normal => .published (collectEvents events)
  | .escaping kind => .escaped kind

/-- One yield contributes exactly its value. -/
theorem single_yield_appends_one_item {α : Type u} (value : α) :
    collectEvents [.yield value] = [value] := by
  rfl

/-- Delegating a list retains the order of every delegated item. -/
theorem delegated_list_keeps_its_order {α : Type u} (values : List α) :
    collectEvents [.delegate values] = values := by
  simp [collectEvents]

/-- Individual and delegated yields appear in the order their events execute. -/
theorem yields_follow_execution_order :
    collectEvents
      ([.yield 1, .delegate [2, 3], .yield 4] : List (Event Nat)) =
        [1, 2, 3, 4] := by
  rfl

/-- Normal completion publishes the full accumulator. -/
theorem normal_exit_publishes_the_accumulator {α : Type u}
    (events : List (Event α)) :
    evaluate events .normal = .published (collectEvents events) := by
  rfl

/-- A normally completed empty block publishes an empty list. -/
theorem empty_normal_exit_publishes_an_empty_list {α : Type u} :
    evaluate ([] : List (Event α)) .normal = .published [] := by
  rfl

/-- An inner collect owns a separate accumulator, and its list is one outer
item rather than a continuation of the outer yields. -/
theorem nested_collect_uses_an_independent_accumulator :
    let inner :=
      collectEvents ([.yield 1, .yield 11] : List (Event Nat))
    let outer :=
      collectEvents ([.yield [0], .yield inner, .yield [2]] : List (Event (List Nat)))
    inner = [1, 11] ∧ outer = [[0], [1, 11], [2]] := by
  simp [collectEvents]

/-- An Err value is appended as list data; this model does not propagate it. -/
theorem err_is_an_ordinary_collected_item (message : String) :
    evaluate
      ([.yield (Except.error message)] : List (Event (Except String Nat)))
      .normal =
        .published [Except.error message] := by
  rfl

/-- A preclassified escaping break publishes no partial list; loop-local break
classification and its target are outside this model. -/
theorem escaping_break_discards_the_partial_list :
    collectEvents
        ([.yield 1, .delegate [2, 3]] : List (Event Nat)) = [1, 2, 3] ∧
      evaluate
        ([.yield 1, .delegate [2, 3]] : List (Event Nat))
        (.escaping .breaking) =
          .escaped .breaking := by
  constructor <;> rfl

/-- A preclassified escaping continue publishes no partial list; loop-local
continue classification and its target are outside this model. -/
theorem escaping_continue_discards_the_partial_list :
    collectEvents
        ([.yield 4, .delegate [5, 6]] : List (Event Nat)) = [4, 5, 6] ∧
      evaluate
        ([.yield 4, .delegate [5, 6]] : List (Event Nat))
        (.escaping .continuing) =
          .escaped .continuing := by
  constructor <;> rfl

/-- A preclassified escaping return publishes no partial list; its return value
is not represented. -/
theorem escaping_return_discards_the_partial_list :
    collectEvents
        ([.yield 7, .delegate [8, 9]] : List (Event Nat)) = [7, 8, 9] ∧
      evaluate
        ([.yield 7, .delegate [8, 9]] : List (Event Nat))
        (.escaping .returning) =
          .escaped .returning := by
  constructor <;> rfl

/-- A preclassified escaping failure publishes no partial list; its error
payload is not represented. -/
theorem escaping_failure_discards_the_partial_list :
    collectEvents
        ([.yield 10, .delegate [11, 12]] : List (Event Nat)) = [10, 11, 12] ∧
      evaluate
        ([.yield 10, .delegate [11, 12]] : List (Event Nat))
        (.escaping .failing) =
          .escaped .failing := by
  constructor <;> rfl

/-- Reversing a delegated list changes the observable result for a
non-palindromic delegated sequence. -/
theorem reversing_delegated_items_changes_the_list :
    let events : List (Event Nat) := [.yield 1, .delegate [2, 3], .yield 4]
    collectEvents events = [1, 2, 3, 4] ∧
      collectEvents (reverseDelegatedItems events) = [1, 3, 2, 4] ∧
      collectEvents events ≠ collectEvents (reverseDelegatedItems events) := by
  decide

end Xsh.Collect.Block
