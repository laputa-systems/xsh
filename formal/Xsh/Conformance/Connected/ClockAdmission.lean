import Xsh.Conformance.Connected.ClockSourceAdmit

namespace Xsh.Conformance.Connected

open Xsh.Syntax
open Xsh.Foundation

def resultOption {ε α : Type} (value : Except ε α) : Option α :=
  match value with | .ok item => some item | .error _ => none

def derivedClockSource : Option DerivedDeclarationEffect :=
  resultOption (deriveSourceEffect 32 0 clockSource 0)

theorem clock_source_derives_time : derivedClockSource = some clockDerivedEffect := by
  unfold derivedClockSource resultOption deriveSourceEffect
  rw [clock_source_admits_unit]
  change derivedClockUnit = _
  exact clock_unit_derives_time

theorem except_some_is_ok {ε α : Type} (value : Except ε α) (expected : α)
    (found : resultOption value = some expected) :
    value = .ok expected := by
  cases value with
  | error failure => cases found
  | ok item => cases found; rfl

theorem clock_source_derivation : deriveSourceEffect 32 0 clockSource 0 = .ok clockDerivedEffect := by
  apply except_some_is_ok
  exact clock_source_derives_time

end Xsh.Conformance.Connected
