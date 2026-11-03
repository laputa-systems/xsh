import Xsh.Typing.Connected.Derivation

namespace Xsh.Typing.Connected

open Xsh.Syntax

def validateDerivedBlock (fuel : Nat) (names : TypeNames) (scope : Scope)
    (result : MonoType) (source : List Statement) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (steps : List DerivationStep) : Except ValidationError Unit := do
  let (_, state) ← ((block fuel names scope result source).run (initialState scope)).mapError generationFailure
  let solution ← checkDerivation steps state.constraints
  validateBlock fuel names scope solution binderTypes (substitute solution result) source candidate

theorem derived_block_certificate_sound (fuel : Nat) (names : TypeNames) (scope : Scope)
    (result : MonoType) (source : List Statement) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (steps : List DerivationStep)
    (accepted : validateDerivedBlock fuel names scope result source candidate binderTypes steps = .ok ()) :
    ∃ generated state solution,
      (block fuel names scope result source).run (initialState scope) = .ok (generated, state) ∧
      checkDerivation steps state.constraints = .ok solution ∧
      KindedPrincipal state.constraints solution ∧
      BlockHasType names solution scope (substitute solution result) source := by
  unfold validateDerivedBlock at accepted
  cases found : (block fuel names scope result source).run (initialState scope) with
  | error error =>
    simp only [found, Except.mapError] at accepted
    cases accepted
  | ok output =>
    obtain ⟨generated, state⟩ := output
    simp [found, Except.mapError, except_bind_ok] at accepted
    obtain ⟨solution, derived, checked⟩ := accepted
    exact ⟨generated, state, solution, rfl, derived, check_derivation_principal steps _ _ derived,
      accepted_block_has_type fuel names scope solution binderTypes _ source candidate checked⟩


def integerLocalName : Xsh.Foundation.NameId := exampleName 20 (by decide)
def integerLocalBinder : Xsh.Foundation.BinderId := ⟨⟨0, by decide⟩⟩

def integerLocalSource : List Statement :=
  [.binding .var (.name integerLocalName) none (.literal (.integer "1")),
   .assign (.name integerLocalName) .set (.literal (.integer "2")),
   .returnValue (some (.ident integerLocalName))]

def integerLocalCandidate : List StatementNode :=
  [.binding integerLocalBinder .mutable (.integer "1"),
   .assignment integerLocalBinder (.integer "2"),
   .returned (.read integerLocalBinder [])]

def integerLocalTypes (binder : Xsh.Foundation.BinderId) : Option MonoType :=
  if binder = integerLocalBinder then some (.primitive .integer) else none

def integerLocalCertificateAccepted : Bool :=
  match validateDerivedBlock 16 exampleTypeNames [] (.primitive .integer) integerLocalSource
      integerLocalCandidate integerLocalTypes [.reflexive, .reflexive] with
  | .ok () => true
  | _ => false

theorem integer_local_derived_certificate_is_inhabited : integerLocalCertificateAccepted = true := by
  decide +kernel


def changedIntegerLocalCandidate : List StatementNode :=
  [.binding integerLocalBinder .mutable (.integer "1"),
   .assignment ⟨⟨1, by decide⟩⟩ (.integer "2"),
   .returned (.read integerLocalBinder [])]

def integerLocalRejected (candidate : List StatementNode) (types : BinderTypes) : Bool :=
  match validateDerivedBlock 16 exampleTypeNames [] (.primitive .integer) integerLocalSource
      candidate types [.reflexive, .reflexive] with
  | .error .mismatch => true
  | _ => false

def integerLocalExhausted : Bool :=
  match validateDerivedBlock 16 exampleTypeNames [] (.primitive .integer) integerLocalSource
      integerLocalCandidate integerLocalTypes [.reflexive] with
  | .error .exhausted => true
  | _ => false

theorem integer_local_rejects_wrong_binder_and_type_and_distinguishes_exhaustion :
    integerLocalRejected changedIntegerLocalCandidate integerLocalTypes = true ∧
    integerLocalRejected integerLocalCandidate (fun _ => some (.primitive .boolean)) = true ∧
    integerLocalExhausted = true := by
  constructor
  · decide +kernel
  constructor <;> decide +kernel

end Xsh.Typing.Connected
