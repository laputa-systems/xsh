import Xsh.Conformance.Vertical.Frontend

namespace Xsh.Conformance.Vertical

/-- The types of the admitted productions. -/
inductive FactType where
  | int
  | bool
  deriving DecidableEq, Repr

/-- What a name use or call target resolves to: a parameter by position, a let by position, or an
earlier function by position. -/
inductive Resolution where
  | parameter (index : Nat)
  | letBinding (index : Nat)
  | function (index : Nat)
  deriving DecidableEq, Repr

/-- One checked fact per evaluated node, operands before the operation that uses them. -/
inductive NodeFact where
  | literal (value : Nat)
  | read (resolution : Resolution)
  | operate (op : FragmentOp) (operands result : FactType)
  | call (callee : Nat) (arguments : Nat)
  | branch
  deriving DecidableEq, Repr

/-- The checked facts a candidate frontend claims for one function: its scheme, the types of its
let bindings, its effects, and its nodes in evaluation order. -/
structure FunctionFacts where
  name : String
  parameterTypes : List FactType
  localTypes : List FactType
  resultType : FactType
  effects : List String
  nodes : List NodeFact
  deriving DecidableEq, Repr

def literalInRange (value : Nat) : Bool := decide (value < 9223372036854775808)

abbrev Scope := List (String × Resolution)

abbrev Callees := List (String × Nat)

/-- The declarative rules. An atom is an integer literal in the signed 64-bit range or a name the
scope resolves. -/
inductive AtomHolds (scope : Scope) : SourceAtom → List NodeFact → Prop where
  | literal {value : Nat} : literalInRange value = true → AtomHolds scope (.int value) [.literal value]
  | read {name : String} {resolution : Resolution} :
      scope.lookup name = some resolution → AtomHolds scope (.var name) [.read resolution]

inductive AtomsHold (scope : Scope) : List SourceAtom → List NodeFact → Prop where
  | nil : AtomsHold scope [] []
  | cons {atom : SourceAtom} {rest : List SourceAtom} {first others : List NodeFact} :
      AtomHolds scope atom first → AtomsHold scope rest others →
        AtomsHold scope (atom :: rest) (first ++ others)

inductive ValueHolds (scope : Scope) : SourceValue → List NodeFact → Prop where
  | atom {atom : SourceAtom} {nodes : List NodeFact} :
      AtomHolds scope atom nodes → ValueHolds scope (.atom atom) nodes
  | arithmetic {op : FragmentOp} {left right : SourceAtom} {leftNodes rightNodes : List NodeFact} :
      op.isComparison = false → AtomHolds scope left leftNodes → AtomHolds scope right rightNodes →
        ValueHolds scope (.binary op left right) (leftNodes ++ rightNodes ++ [.operate op .int .int])

inductive ExpressionHolds (callees : Callees) (scope : Scope) :
    SourceExpression → List NodeFact → Prop where
  | value {value : SourceValue} {nodes : List NodeFact} :
      ValueHolds scope value nodes → ExpressionHolds callees scope (.value value) nodes
  | call {callee : String} {arguments : List SourceAtom} {index : Nat} {arity : Nat}
      {nodes : List NodeFact} :
      callees.findIdx? (fun entry => entry.1 == callee) = some index →
      (callees[index]?).map (·.2) = some arity → arguments.length = arity →
      AtomsHold scope arguments nodes →
        ExpressionHolds callees scope (.call callee arguments)
          (nodes ++ [.call index arguments.length])
  | conditional {comparison : FragmentOp} {left right : SourceAtom}
      {thenValue elseValue : SourceValue}
      {leftNodes rightNodes thenNodes elseNodes : List NodeFact} :
      comparison.isComparison = true → AtomHolds scope left leftNodes →
      AtomHolds scope right rightNodes → ValueHolds scope thenValue thenNodes →
      ValueHolds scope elseValue elseNodes →
        ExpressionHolds callees scope (.conditional comparison left right thenValue elseValue)
          (leftNodes ++ rightNodes ++ [.operate comparison .int .bool] ++ thenNodes ++ elseNodes ++
            [.branch])

/-- Parameters resolve by position, then lets by position; every name is distinct and every let
literal is in range. -/
def SourceFunction.scope (function : SourceFunction) : Option Scope :=
  let parameters := function.parameters.zipIdx.map fun (name, index) => (name, .parameter index)
  let lets := function.lets.zipIdx.map fun ((name, _), index) => (name, .letBinding index)
  if (parameters ++ lets).map (·.1) |>.Nodup then
    if function.lets.all (fun entry => literalInRange entry.2) then some (parameters ++ lets)
    else none
  else none

def FunctionHolds (callees : Callees) (function : SourceFunction) (facts : FunctionFacts) :
    Prop :=
  ∃ scope nodes, function.scope = some scope ∧
    ExpressionHolds callees scope function.result nodes ∧
    facts = ⟨function.name, function.parameters.map fun _ => .int, function.lets.map fun _ => .int,
      .int, [], nodes⟩

def calleesAfter (callees : Callees) (function : SourceFunction) : Callees :=
  callees ++ [(function.name, function.parameters.length)]

/-- The facts of the functions in source order, each against the functions before it. -/
inductive ProgramInOrder : Callees → List SourceFunction → List FunctionFacts → Prop where
  | nil {callees : Callees} : ProgramInOrder callees [] []
  | cons {callees : Callees} {function : SourceFunction} {rest : List SourceFunction}
      {facts : FunctionFacts} {others : List FunctionFacts} :
      FunctionHolds callees function facts →
      ProgramInOrder (calleesAfter callees function) rest others →
        ProgramInOrder callees (function :: rest) (facts :: others)

/-- A candidate fact set is a valid derivation for the functions when it lists exactly the facts
the rules determine, once each, in any order, for functions with distinct names. -/
def ProgramHolds (functions : List SourceFunction) (candidate : List FunctionFacts) : Prop :=
  ∃ expected, ProgramInOrder [] functions expected ∧ (functions.map (·.name)).Nodup ∧
    (∀ facts, facts ∈ candidate ↔ facts ∈ expected) ∧ candidate.length = expected.length ∧
    (candidate.map (·.name)).Nodup

def atomFacts (scope : Scope) : SourceAtom → Option (List NodeFact)
  | .int value => if literalInRange value then some [.literal value] else none
  | .var name => (scope.lookup name).map fun resolution => [.read resolution]

def atomsFacts (scope : Scope) : List SourceAtom → Option (List NodeFact)
  | [] => some []
  | atom :: rest => do
      let first ← atomFacts scope atom
      let others ← atomsFacts scope rest
      some (first ++ others)

def valueFacts (scope : Scope) : SourceValue → Option (List NodeFact)
  | .atom atom => atomFacts scope atom
  | .binary op left right => do
      check (!op.isComparison)
      let leftNodes ← atomFacts scope left
      let rightNodes ← atomFacts scope right
      some (leftNodes ++ rightNodes ++ [.operate op .int .int])

def expressionFacts (callees : Callees) (scope : Scope) : SourceExpression → Option (List NodeFact)
  | .value value => valueFacts scope value
  | .call callee arguments => do
      let index ← callees.findIdx? (fun entry => entry.1 == callee)
      let entry ← callees[index]?
      check (arguments.length == entry.2)
      let nodes ← atomsFacts scope arguments
      some (nodes ++ [.call index arguments.length])
  | .conditional comparison left right thenValue elseValue => do
      check comparison.isComparison
      let leftNodes ← atomFacts scope left
      let rightNodes ← atomFacts scope right
      let thenNodes ← valueFacts scope thenValue
      let elseNodes ← valueFacts scope elseValue
      some (leftNodes ++ rightNodes ++ [.operate comparison .int .bool] ++ thenNodes ++ elseNodes ++
        [.branch])

def functionFacts (callees : Callees) (function : SourceFunction) : Option FunctionFacts := do
  let scope ← function.scope
  let nodes ← expressionFacts callees scope function.result
  some ⟨function.name, function.parameters.map fun _ => .int, function.lets.map fun _ => .int, .int,
    [], nodes⟩

def programFacts : Callees → List SourceFunction → Option (List FunctionFacts)
  | _, [] => some []
  | callees, function :: rest => do
      let facts ← functionFacts callees function
      let others ← programFacts (calleesAfter callees function) rest
      some (facts :: others)

/-- The fact validator. It computes the facts the rules determine from the parsed source and
accepts a candidate only when it lists exactly those facts; it takes no resolution, operation,
type, effect or order from the candidate. -/
def validateFacts (functions : List SourceFunction) (candidate : List FunctionFacts) : Bool :=
  decide (functions.map (·.name)).Nodup && decide (candidate.map (·.name)).Nodup &&
    match programFacts [] functions with
    | some expected =>
        candidate.length == expected.length && expected.all (fun facts => candidate.contains facts) &&
          candidate.all (fun facts => expected.contains facts)
    | none => false


theorem atom_facts_iff {scope : Scope} {atom : SourceAtom} {nodes : List NodeFact} :
    atomFacts scope atom = some nodes ↔ AtomHolds scope atom nodes := by
  constructor
  · intro found
    cases atom with
    | int value =>
      simp only [atomFacts] at found
      by_cases inRange : literalInRange value = true
      · rw [if_pos inRange] at found
        simp only [Option.some.injEq] at found
        subst found
        exact .literal inRange
      · rw [if_neg inRange] at found
        simp at found
    | var name =>
      simp only [atomFacts] at found
      cases looked : scope.lookup name with
      | none => simp [looked] at found
      | some resolution =>
        simp only [looked, Option.map_some, Option.some.injEq] at found
        subst found
        exact .read looked
  · intro holds
    cases holds with
    | literal inRange => simp [atomFacts, inRange]
    | read looked => simp [atomFacts, looked]

theorem atoms_facts_iff {scope : Scope} :
    ∀ {atoms : List SourceAtom} {nodes : List NodeFact},
      atomsFacts scope atoms = some nodes ↔ AtomsHold scope atoms nodes := by
  intro atoms
  induction atoms with
  | nil =>
    intro nodes
    constructor
    · intro found
      simp only [atomsFacts, Option.some.injEq] at found
      subst found
      exact .nil
    · intro holds
      cases holds
      rfl
  | cons atom rest ih =>
    intro nodes
    constructor
    · intro found
      simp only [atomsFacts, Option.bind_eq_bind, Option.bind_eq_some_iff] at found
      obtain ⟨first, firstFound, others, othersFound, joined⟩ := found
      simp only [Option.some.injEq] at joined
      subst joined
      exact .cons (atom_facts_iff.mp firstFound) (ih.mp othersFound)
    · intro holds
      cases holds with
      | cons firstHolds othersHold =>
        simp [atomsFacts, atom_facts_iff.mpr firstHolds, ih.mpr othersHold]

theorem value_facts_iff {scope : Scope} {value : SourceValue} {nodes : List NodeFact} :
    valueFacts scope value = some nodes ↔ ValueHolds scope value nodes := by
  constructor
  · intro found
    cases value with
    | atom atom => exact .atom (atom_facts_iff.mp found)
    | binary op left right =>
      simp only [valueFacts, Option.bind_eq_bind, check_bind] at found
      split at found
      · rename_i plain
        simp only [Option.bind_eq_some_iff] at found
        obtain ⟨leftNodes, leftFound, rightNodes, rightFound, joined⟩ := found
        simp only [Option.some.injEq] at joined
        subst joined
        exact .arithmetic (by simpa using plain) (atom_facts_iff.mp leftFound)
          (atom_facts_iff.mp rightFound)
      · simp at found
  · intro holds
    cases holds with
    | atom atomHolds => exact atom_facts_iff.mpr atomHolds
    | arithmetic plain leftHolds rightHolds =>
      simp [valueFacts, plain, atom_facts_iff.mpr leftHolds, atom_facts_iff.mpr rightHolds]


theorem expression_facts_iff {callees : Callees} {scope : Scope} {expression : SourceExpression}
    {nodes : List NodeFact} :
    expressionFacts callees scope expression = some nodes ↔
      ExpressionHolds callees scope expression nodes := by
  constructor
  · intro found
    cases expression with
    | value value => exact .value (value_facts_iff.mp found)
    | call callee arguments =>
      simp only [expressionFacts, Option.bind_eq_bind, Option.bind_eq_some_iff] at found
      obtain ⟨index, indexFound, entry, entryFound, rest⟩ := found
      simp only [Option.bind_eq_some_iff, Option.some.injEq] at rest
      obtain ⟨_, counted, argumentNodes, argumentsFound, joined⟩ := rest
      subst joined
      refine .call (arity := entry.2) indexFound (by simp [entryFound]) ?_ (atoms_facts_iff.mp argumentsFound)
      simpa using check_eq_some.mp counted
    | conditional comparison left right thenValue elseValue =>
      simp only [expressionFacts, Option.bind_eq_bind, check_bind] at found
      split at found
      · rename_i isComparison
        simp only [Option.bind_eq_some_iff, Option.some.injEq] at found
        obtain ⟨leftNodes, leftFound, rightNodes, rightFound, thenNodes, thenFound, elseNodes,
          elseFound, joined⟩ := found
        subst joined
        exact .conditional isComparison (atom_facts_iff.mp leftFound) (atom_facts_iff.mp rightFound)
          (value_facts_iff.mp thenFound) (value_facts_iff.mp elseFound)
      · simp at found
  · intro holds
    cases holds with
    | value valueHolds => exact value_facts_iff.mpr valueHolds
    | call indexFound arityFound counted argumentsHold =>
      obtain ⟨entry, entryFound, rfl⟩ := Option.map_eq_some_iff.mp arityFound
      simp [expressionFacts, indexFound, entryFound, counted, atoms_facts_iff.mpr argumentsHold]
    | conditional isComparison leftHolds rightHolds thenHolds elseHolds =>
      simp [expressionFacts, isComparison, atom_facts_iff.mpr leftHolds,
        atom_facts_iff.mpr rightHolds, value_facts_iff.mpr thenHolds, value_facts_iff.mpr elseHolds]

theorem function_facts_iff {callees : Callees} {function : SourceFunction}
    {facts : FunctionFacts} :
    functionFacts callees function = some facts ↔ FunctionHolds callees function facts := by
  constructor
  · intro found
    simp only [functionFacts, Option.bind_eq_bind, Option.bind_eq_some_iff,
      Option.some.injEq] at found
    obtain ⟨scope, scopeFound, nodes, nodesFound, joined⟩ := found
    exact ⟨scope, nodes, scopeFound, expression_facts_iff.mp nodesFound, joined.symm⟩
  · rintro ⟨scope, nodes, scopeFound, expressionHolds, rfl⟩
    simp [functionFacts, scopeFound, expression_facts_iff.mpr expressionHolds]

theorem function_facts_are_determined {callees : Callees} {function : SourceFunction}
    {first second : FunctionFacts} (a : FunctionHolds callees function first)
    (b : FunctionHolds callees function second) : first = second := by
  have x := function_facts_iff.mpr a
  have y := function_facts_iff.mpr b
  rw [x] at y
  exact Option.some.inj y

theorem program_facts_iff :
    ∀ {callees : Callees} {functions : List SourceFunction} {expected : List FunctionFacts},
      programFacts callees functions = some expected ↔ ProgramInOrder callees functions expected := by
  intro callees functions
  induction functions generalizing callees with
  | nil =>
    intro expected
    constructor
    · intro found
      simp only [programFacts, Option.some.injEq] at found
      subst found
      exact .nil
    · intro holds
      cases holds
      rfl
  | cons function rest ih =>
    intro expected
    constructor
    · intro found
      simp only [programFacts, Option.bind_eq_bind, Option.bind_eq_some_iff,
        Option.some.injEq] at found
      obtain ⟨facts, factsFound, others, othersFound, joined⟩ := found
      subst joined
      exact .cons (function_facts_iff.mp factsFound) (ih.mp othersFound)
    · intro holds
      cases holds with
      | cons factsHold othersHold =>
        simp [programFacts, function_facts_iff.mpr factsHold, ih.mpr othersHold]

/-- The validator accepts a candidate fact set exactly when it is a valid derivation for the
functions. -/
theorem validate_facts_iff {functions : List SourceFunction} {candidate : List FunctionFacts} :
    validateFacts functions candidate = true ↔ ProgramHolds functions candidate := by
  unfold validateFacts ProgramHolds
  constructor
  · intro accepted
    simp only [Bool.and_eq_true, decide_eq_true_eq] at accepted
    obtain ⟨⟨functionsNodup, candidateNodup⟩, rest⟩ := accepted
    cases expected : programFacts [] functions with
    | none => simp [expected] at rest
    | some facts =>
      simp only [expected, Bool.and_eq_true, beq_iff_eq, List.all_eq_true,
        List.contains_iff_mem] at rest
      obtain ⟨⟨sameLength, expectedIn⟩, candidateIn⟩ := rest
      exact ⟨facts, program_facts_iff.mp expected, functionsNodup,
        fun f => ⟨fun member => candidateIn f member, fun member => expectedIn f member⟩,
        sameLength, candidateNodup⟩
  · rintro ⟨expected, inOrder, functionsNodup, same, sameLength, candidateNodup⟩
    have computed := program_facts_iff.mpr inOrder
    simp only [Bool.and_eq_true, decide_eq_true_eq, computed, beq_iff_eq, List.all_eq_true,
      List.contains_iff_mem]
    exact ⟨⟨functionsNodup, candidateNodup⟩, ⟨sameLength, fun f member => (same f).mpr member⟩,
      fun f member => (same f).mp member⟩

end Xsh.Conformance.Vertical
