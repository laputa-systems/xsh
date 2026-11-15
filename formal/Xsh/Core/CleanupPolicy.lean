import Init.Data.List.Perm

namespace Xsh.Core.CleanupPolicy

structure Error where
  id : Nat
  forced : Bool
  deriving DecidableEq, Repr

inductive Disposition where
  | raise | reportSecondary | surface
  deriving DecidableEq, Repr

def disposition (forced primaryFails : Bool) : Disposition :=
  if forced then .raise else if primaryFails then .reportSecondary else .surface

def failureIsRecorded (managed forced : Bool) : Bool := !managed || forced

def record (current : Option Error) (error : Error) : Option Error × Option Error :=
  match current with
  | none => (some error, none)
  | some previous =>
    if error.forced && !previous.forced then (some error, some previous)
    else (some previous, some error)

def fold (current : Option Error) : List Error → Option Error × List Error
  | [] => (current, [])
  | error :: rest =>
    let (next, emitted) := record current error
    let (last, reports) := fold next rest
    (last, emitted.toList ++ reports)

def selected (current : Option Error) (errors : List Error) : Option Error :=
  match current with
  | some e => if e.forced then some e else (errors.find? (·.forced)).orElse (fun _ => some e)
  | none => (errors.find? (·.forced)).orElse (fun _ => errors.head?)

theorem disposition_contract (forced primaryFails : Bool) :
    (forced = true → disposition forced primaryFails = .raise) ∧
    (disposition forced primaryFails = .reportSecondary ↔ primaryFails = true ∧ forced = false) := by
  cases forced <;> cases primaryFails <;> simp [disposition]

theorem failure_recorded_contract (managed forced : Bool) :
    failureIsRecorded managed forced = false ↔ managed = true ∧ forced = false := by
  cases managed <;> cases forced <;> simp [failureIsRecorded]

theorem fold_conserves (current : Option Error) (errors : List Error) :
    List.Perm ((fold current errors).1.toList ++ (fold current errors).2) (current.toList ++ errors) := by
  induction errors generalizing current with
  | nil => simp [fold]
  | cons e es ih =>
    cases current with
    | none => simpa [fold, record] using ih (some e)
    | some prev =>
      cases hf : (e.forced && !prev.forced) with
      | false =>
        simp [fold, record, hf]
        apply List.Perm.trans (List.perm_append_comm_assoc _ [e] _)
        simpa using (List.Perm.cons e (ih (some prev))).trans (List.Perm.swap _ _ _)
      | true =>
        simp [fold, record, hf]
        apply List.Perm.trans (List.perm_append_comm_assoc _ [prev] _)
        simpa using List.Perm.cons prev (ih (some e))

theorem fold_selects (current : Option Error) (errors : List Error) :
    (fold current errors).1 = selected current errors := by
  induction errors generalizing current with
  | nil => cases current <;> simp [fold, selected]
  | cons e es ih =>
    cases current with
    | none =>
      cases h : e.forced <;> simp [fold, record, ih, selected, h, List.find?]
    | some prev =>
      cases hp : prev.forced <;> cases he : e.forced <;> simp [fold, record, ih, selected, hp, he, List.find?]

end Xsh.Core.CleanupPolicy
