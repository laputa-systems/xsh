import Xsh.Conformance.Connected.ClockParse

namespace Xsh.Conformance.Connected

open Xsh.Syntax
open Xsh.Foundation

def derivedClockTokens : Option DerivedDeclarationEffect :=
  match parseDeclarations (32 * (clockTokens.length + 1)) (sourceNames clockTokens) clockTokens with
  | .error _ => none
  | .ok declarations =>
    match deriveDeclarationEffect 32 ⟨0, sourceNames clockTokens, declarations⟩ 0 with
    | .ok effect => some effect | .error _ => none

theorem clock_tokens_derive_time : derivedClockTokens = some clockDerivedEffect := by
  unfold derivedClockTokens
  rw [clock_token_names, clock_tokens_parse]
  change derivedClockUnit = _
  exact clock_unit_derives_time

end Xsh.Conformance.Connected
