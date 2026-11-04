import Xsh.Semantics.Connected.SafetyValueBlockLocalProgress
namespace Xsh.Semantics.Connected

theorem incompatible_bad_step {state state' bad good : Machine} {event event' : Label} {fault : Fault}
    (badStep : Step state event bad) (goodStep : Step state' event' good) (same : state = state')
    (badControl : bad.control = .fault fault) (notInteger : ∀ issue, fault ≠ .integer issue)
    (safe : PermittedControl good.control) : False := by
  have controlSame := congrArg Machine.control same
  have stackSame := congrArg Machine.stack same
  have storeSame := congrArg Machine.store same
  have cellsSame := congrArg Machine.cells same
  have schemasSame := congrArg Machine.schemas same
  have solutionSame := congrArg Machine.solution same
  have activationSame := congrArg Machine.activation same
  cases badStep <;> simp_all [PermittedControl]
  all_goals cases goodStep <;> simp_all [ordinaryValue, List.lookup_eq_some_iff]
  all_goals grind
theorem step_source_is_not_terminal {state next : Machine} {event : Label} (step : Step state event next) :
    ¬ MachineTerminal state := by
  intro terminal
  cases step <;> simp_all [MachineTerminal]

theorem typed_machine_excludes_internal_fault (state next : Machine) (event : Label)
    (globals : Xsh.Typing.Connected.Scope) (storage : BindingStorageMap) (types : CellTypes)
    (invariant : TypedMachine globals storage types state) (step : Step state event next)
    (response : Xsh.Values.IntValue) (fault : Fault) (bad : next.control = .fault fault) :
    ∃ issue, fault = .integer issue := by
  by_cases existsInteger : ∃ issue, fault = .integer issue
  · exact existsInteger
  · exfalso
    have impossible : ∀ issue, fault ≠ .integer issue := by
      intro issue equal
      exact existsInteger ⟨issue, equal⟩
    rcases typed_machine_safe_progress state globals storage types response invariant with terminal | ⟨event', good, goodStep, safe⟩
    · exact step_source_is_not_terminal step terminal
    · exact incompatible_bad_step step goodStep rfl bad impossible safe

end Xsh.Semantics.Connected
