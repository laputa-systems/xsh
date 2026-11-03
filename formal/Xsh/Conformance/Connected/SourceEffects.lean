import Xsh.Conformance.Connected.ClockAdmission

namespace Xsh.Conformance.Connected

open Xsh.Foundation
open Xsh.Syntax
open Xsh.Conformance.Transport

def clockFactValidation (fact : DeclarationEffectFact) : Except AdmissionFailure Unit := do
  let expected ← deriveSourceEffect 32 0 clockSource 0
  validateEffectAgainstDerived expected fact

def checkClockMutation (fact : DeclarationEffectFact) : Bool :=
  match clockFactValidation fact with | .ok _ => true | .error _ => false

theorem clock_source_validates_emitted_effects : checkClockMutation clockEffectFact = true := by
  unfold checkClockMutation clockFactValidation
  rw [clock_source_derivation]
  decide +kernel

def clockEffectMutationsRefused : Bool :=
  match deriveSourceEffect 32 0 clockSource 0 with
  | .error _ => false
  | .ok expected =>
    [{ clockEffectFact with required := .known [] [] },
     { clockEffectFact with effective := .known [] [] },
     { clockEffectFact with owner := none },
     { clockEffectFact with inferred := true },
     { clockEffectFact with redundantClause := false },
     { clockEffectFact with unknownChain := ["unknown"] }].all fun fact =>
       match validateEffectAgainstDerived expected fact with | .ok _ => false | .error _ => true

theorem clock_effect_mutations_are_refused : clockEffectMutationsRefused = true := by
  unfold clockEffectMutationsRefused
  rw [clock_source_derivation]
  decide +kernel

theorem emitted_clock_effects_match_source :
    (match validateSourceEffectFacts 32 0 clockSource effectFactsCodes 0 0 with
      | .ok _ => true | .error _ => false) = true := by
  unfold validateSourceEffectFacts
  rw [clock_source_derivation, effect_facts_parse_to_wire]
  simp only [Option.bind_some, emitted_clock_effects_decode]
  change (match validateEffectAgainstDerived clockDerivedEffect clockEffectFact with
    | .ok _ => true | .error _ => false) = true
  decide +kernel

end Xsh.Conformance.Connected
