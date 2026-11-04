import Xsh.Semantics.Connected.Machine

namespace Xsh.Semantics.Connected

open Xsh.Syntax.Foundation

theorem missing_callable_cannot_invoke (state : Machine) (declaration : DeclarationId)
    (missing : state.functions declaration = none) :
    ¬ ∃ event next, Step { state with control := .invoke (.function declaration .argumentsEmpty .integer) [] } event next := by
  intro ⟨event, next, step⟩
  cases state
  cases step <;> simp_all

end Xsh.Semantics.Connected
