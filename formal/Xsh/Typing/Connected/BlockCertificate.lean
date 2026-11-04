import Xsh.Typing.Connected.Derivation

namespace Xsh.Typing.Connected

open Xsh.Syntax


mutual
  def declaredBinders : List StatementNode → List Xsh.Foundation.BinderId
    | [] => []
    | .binding binder _ _ value :: rest => binder :: nodeDeclaredBinders value ++ declaredBinders rest
    | .assignment _ value :: rest | .evaluated value :: rest | .returned value :: rest =>
        nodeDeclaredBinders value ++ declaredBinders rest
    | .conditional branches fallback :: rest =>
        branchBinders branches ++ declaredBinders fallback ++ declaredBinders rest

  def branchBinders : List (Node × List StatementNode) → List Xsh.Foundation.BinderId
    | [] => []
    | (condition, body) :: rest => nodeDeclaredBinders condition ++ declaredBinders body ++ branchBinders rest

  def nodeDeclaredBinders : Node → List Xsh.Foundation.BinderId
    | .integer _ | .boolean _ | .string _ | .read _ _ => []
    | .binary _ left right => nodeDeclaredBinders left ++ nodeDeclaredBinders right
    | .call target arguments => nodeDeclaredBinders target ++ argumentBinders arguments
    | .record fields => fieldBinders fields
    | .field base _ | .propagate base _ | .requireValue _ _ base => nodeDeclaredBinders base
    | .conditional branches otherwise => expressionBranchBinders branches ++ nodeDeclaredBinders otherwise
    | .valueBlock _ body => declaredBinders body

  def argumentBinders : List Node → List Xsh.Foundation.BinderId
    | [] => []
    | node :: rest => nodeDeclaredBinders node ++ argumentBinders rest

  def fieldBinders : List (Xsh.Foundation.NameId × Node) → List Xsh.Foundation.BinderId
    | [] => []
    | (_, node) :: rest => nodeDeclaredBinders node ++ fieldBinders rest

  def expressionBranchBinders : List (Node × Node) → List Xsh.Foundation.BinderId
    | [] => []
    | (condition, value) :: rest =>
        nodeDeclaredBinders condition ++ nodeDeclaredBinders value ++ expressionBranchBinders rest
end


def blockPropagationErrors := statementPropagationErrors
def statementBranchErrors := statementBranchPropagationErrors


mutual
  def supportedStatementTails (tail : Bool) : List StatementNode → Bool
    | [] => true
    | .evaluated (.valueBlock _ body) :: rest =>
        supportedStatementTails true body && supportedStatementTails tail rest
    | .evaluated _ :: rest => tail && rest.isEmpty
    | .conditional branches fallback :: rest =>
        supportedBranchTails (tail && rest.isEmpty) branches &&
          supportedStatementTails (tail && rest.isEmpty) fallback && supportedStatementTails tail rest
    | _ :: rest => supportedStatementTails tail rest

  def supportedBranchTails (tail : Bool) : List (Node × List StatementNode) → Bool
    | [] => true
    | (_, body) :: rest => supportedStatementTails tail body && supportedBranchTails tail rest
end

mutual
  def guaranteesFunctionResult : List StatementNode → Bool
    | [] => true
    | .returned _ :: _ => true
    | .evaluated _ :: rest => rest.isEmpty || guaranteesFunctionResult rest
    | .conditional branches fallback :: rest =>
        if rest.isEmpty then guaranteesBranchResults branches && guaranteesFunctionResult fallback
        else guaranteesFunctionResult rest
    | _ :: rest => guaranteesFunctionResult rest

  def guaranteesBranchResults : List (Node × List StatementNode) → Bool
    | [] => true
    | (_, body) :: rest => guaranteesFunctionResult body && guaranteesBranchResults rest
end

def validateDerivedBlock (fuel : Nat) (names : TypeNames) (scope : Scope)
    (result : MonoType) (source : List Statement) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (steps : List DerivationStep) : Except ValidationError Unit := do
  let (_, state) ← ((block fuel names scope result source).run (initialBlockState scope result)).mapError generationFailure
  let solution ← checkDerivation steps state.constraints
  requireCheck (decide (declaredBinders candidate).Nodup)
  if supportedStatementTails true candidate then pure () else throw .unsupported
  requireCheck (checksPropagationErrors (propagationContext (substitute solution result)) solution
    (blockPropagationErrors candidate))
  validateBlock fuel names scope solution binderTypes (substitute solution result) source candidate

theorem derived_block_certificate_sound (fuel : Nat) (names : TypeNames) (scope : Scope)
    (result : MonoType) (source : List Statement) (candidate : List StatementNode)
    (binderTypes : BinderTypes) (steps : List DerivationStep)
    (accepted : validateDerivedBlock fuel names scope result source candidate binderTypes steps = .ok ()) :
    ∃ generated state solution,
      (block fuel names scope result source).run (initialBlockState scope result) = .ok (generated, state) ∧
      checkDerivation steps state.constraints = .ok solution ∧
      KindedPrincipal state.constraints solution ∧ (declaredBinders candidate).Nodup ∧
      PropagationMatches (propagationContext (substitute solution result)) solution (blockPropagationErrors candidate) ∧
      BlockHasType names solution scope (substitute solution result) source := by
  unfold validateDerivedBlock at accepted
  cases found : (block fuel names scope result source).run (initialBlockState scope result) with
  | error error =>
    simp only [found, Except.mapError] at accepted
    cases accepted
  | ok output =>
    obtain ⟨generated, state⟩ := output
    cases supported : supportedStatementTails true candidate with
    | false =>
      simp only [found, Except.mapError, except_bind_ok, supported, Bool.false_eq_true, ↓reduceIte] at accepted
      simp [exists_require_ok] at accepted
    | true =>
      simp [found, Except.mapError, except_bind_ok, supported] at accepted
      simp [exists_require_ok, checks_propagation_errors_iff] at accepted
      obtain ⟨solution, derived, unique, propagation, checked⟩ := accepted
      exact ⟨generated, state, solution, rfl, derived, check_derivation_principal steps _ _ derived, unique, propagation,
        accepted_block_has_type fuel names scope solution binderTypes _ source candidate checked⟩


def integerLocalName : Xsh.Foundation.NameId := exampleName 20 (by decide)
def integerLocalBinder : Xsh.Foundation.BinderId := ⟨⟨0, by decide⟩⟩

def integerLocalSource : List Statement :=
  [.binding .var (.name integerLocalName) none (.literal (.integer "1")),
   .assign (.name integerLocalName) .set (.literal (.integer "2")),
   .returnValue (some (.ident integerLocalName))]

def integerLocalCandidate : List StatementNode :=
  [.binding integerLocalBinder .mutable (.primitive .integer) (.integer "1"),
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
  [.binding integerLocalBinder .mutable (.primitive .integer) (.integer "1"),
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


def siblingLocalSource : List Statement :=
  [.ifStmt [(.literal (.bool true), [.binding .var (.name (exampleName 30 (by decide))) none (.literal (.integer "1"))])] none,
   .ifStmt [(.literal (.bool true), [.binding .var (.name (exampleName 31 (by decide))) none (.literal (.integer "2"))])] none,
   .returnValue (some (.literal (.integer "3")))]

def reusedSiblingCandidate : List StatementNode :=
  [.conditional [(.boolean true, [.binding integerLocalBinder .mutable (.primitive .integer) (.integer "1")])] [],
   .conditional [(.boolean true, [.binding integerLocalBinder .mutable (.primitive .integer) (.integer "2")])] [],
   .returned (.integer "3")]

def siblingReuseRejected : Bool :=
  match validateDerivedBlock 32 exampleTypeNames [] (.primitive .integer) siblingLocalSource
      reusedSiblingCandidate integerLocalTypes [.reflexive, .reflexive, .reflexive] with
  | .error .mismatch => true
  | _ => false

theorem sibling_local_binder_reuse_is_rejected : siblingReuseRejected = true := by
  decide +kernel

end Xsh.Typing.Connected
