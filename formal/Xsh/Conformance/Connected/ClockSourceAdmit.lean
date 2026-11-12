import Xsh.Conformance.Connected.ClockTokensDerive

namespace Xsh.Conformance.Connected

open Xsh.Syntax
open Xsh.Foundation

theorem clock_source_admits_unit : admitSource 0 clockSource = .ok clockUnit := by
  unfold admitSource
  rw [clock_source_tokenizes]
  change (do
    let declarations ← parseDeclarations (32 * (clockTokens.length + 1)) (sourceNames clockTokens) clockTokens
    pure (⟨0, sourceNames clockTokens, declarations⟩ : SourceUnit)) = _
  rw [clock_token_names, clock_tokens_parse]
  rfl

end Xsh.Conformance.Connected
