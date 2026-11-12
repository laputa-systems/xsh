import Xsh.Conformance.Vertical.LoweringSound

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

inductive AtomSyntax : List Token → SourceAtom → List Token → Prop where
  | number (value : Nat) (rest : List Token) :
      AtomSyntax (.number value :: rest) (.int value) rest
  | name (text : String) (rest : List Token) : isKeyword text = false →
      AtomSyntax (.word text :: rest) (.var text) rest

inductive ValueSyntax : List Token → SourceValue → List Token → Prop where
  | atom {tokens rest : List Token} {atom : SourceAtom} :
      AtomSyntax tokens atom rest → ValueSyntax tokens (.atom atom) rest
  | binary {tokens afterLeft rest : List Token} {left right : SourceAtom} {op : FragmentOp} :
      AtomSyntax tokens left (.operator op :: afterLeft) →
      op.isComparison = false → AtomSyntax afterLeft right rest →
      ValueSyntax tokens (.binary op left right) rest

inductive ArgumentsSyntax : List Token → List SourceAtom → List Token → Prop where
  | empty (rest : List Token) : ArgumentsSyntax (.rightParen :: rest) [] rest
  | single {tokens rest : List Token} {atom : SourceAtom} :
      AtomSyntax tokens atom (.rightParen :: rest) → ArgumentsSyntax tokens [atom] rest
  | cons {tokens afterFirst rest : List Token} {atom : SourceAtom} {others : List SourceAtom} :
      AtomSyntax tokens atom (.comma :: afterFirst) → ArgumentsSyntax afterFirst others rest →
      others ≠ [] → ArgumentsSyntax tokens (atom :: others) rest

inductive ExpressionSyntax : List Token → SourceExpression → List Token → Prop where
  | value {tokens rest : List Token} {value : SourceValue} :
      ValueSyntax tokens value rest → ExpressionSyntax tokens (.value value) rest
  | comparison {tokens afterLeft rest : List Token} {left right : SourceAtom} {op : FragmentOp} :
      AtomSyntax tokens left (.operator op :: afterLeft) → op.isComparison = true →
      AtomSyntax afterLeft right rest → ExpressionSyntax tokens (.comparison op left right) rest
  | call {name : String} {tokens rest : List Token} {arguments : List SourceAtom} :
      isKeyword name = false → ArgumentsSyntax tokens arguments rest →
      ExpressionSyntax (.word name :: .leftParen :: tokens) (.call name arguments) rest
  | ok {tokens rest : List Token} {value : SourceValue} :
      ValueSyntax tokens value (.rightParen :: rest) →
      ExpressionSyntax (.word "Ok" :: .leftParen :: tokens) (.constructed true value) rest
  | err {tokens rest : List Token} {value : SourceValue} :
      ValueSyntax tokens value (.rightParen :: rest) →
      ExpressionSyntax (.word "Err" :: .leftParen :: tokens) (.constructed false value) rest
  | conditional {tokens afterLeft afterRight afterThen rest : List Token}
      {left right : SourceAtom} {op : FragmentOp} {thenValue elseValue : SourceValue} :
      AtomSyntax tokens left (.operator op :: afterLeft) → op.isComparison = true →
      AtomSyntax afterLeft right (.leftBrace :: afterRight) →
      ValueSyntax afterRight thenValue (.rightBrace :: .word "else" :: .leftBrace :: afterThen) →
      ValueSyntax afterThen elseValue (.rightBrace :: rest) →
      ExpressionSyntax (.word "if" :: tokens) (.conditional op left right thenValue elseValue) rest

 theorem atom_syntax {tokens rest : List Token} {atom : SourceAtom}
    (parsed : parseAtom tokens = some (atom, rest)) : AtomSyntax tokens atom rest := by
  unfold parseAtom at parsed
  split at parsed
  · cases parsed
    exact .number _ _
  · split at parsed
    · cases parsed
    · cases parsed
      exact .name _ _ (by simpa using (show ¬ _ = true from by assumption))
  · cases parsed

 theorem value_syntax {tokens rest : List Token} {value : SourceValue}
    (parsed : parseValue tokens = some (value, rest)) : ValueSyntax tokens value rest := by
  unfold parseValue at parsed
  cases first : parseAtom tokens with
  | none => simp [first] at parsed
  | some pair =>
    rcases pair with ⟨left, afterLeft⟩
    simp only [first, Option.bind_some, Option.bind_eq_bind] at parsed
    split at parsed
    · split at parsed
      · cases parsed
      · rename_i ignored op more notComparison
        cases second : parseAtom more with
        | none => simp [second] at parsed
        | some pair =>
          rcases pair with ⟨right, remaining⟩
          simp only [second, Option.bind_some, Option.some.injEq, Prod.mk.injEq] at parsed
          rcases parsed with ⟨rfl, rfl⟩
          exact .binary (atom_syntax first) (by simpa using notComparison)
            (atom_syntax second)
    · cases parsed
      exact .atom (atom_syntax first)

 theorem argument_list_syntax (fuel : Nat) {tokens rest : List Token} {arguments : List SourceAtom}
    (parsed : parseArgumentList fuel tokens = some (arguments, rest)) :
    ArgumentsSyntax tokens arguments rest ∧ arguments ≠ [] := by
  induction fuel generalizing tokens arguments rest with
  | zero => simp [parseArgumentList] at parsed
  | succ fuel ih =>
    unfold parseArgumentList at parsed
    cases first : parseAtom tokens with
    | none => simp [first] at parsed
    | some pair =>
      rcases pair with ⟨atom, afterFirst⟩
      simp only [first, Option.bind_some, Option.bind_eq_bind] at parsed
      split at parsed
      · cases parsed
        exact ⟨.single (atom_syntax first), by simp⟩
      · rename_i ignored more
        cases others : parseArgumentList fuel more with
        | none => simp [others] at parsed
        | some pair =>
          rcases pair with ⟨tail, remaining⟩
          simp only [others, Option.bind_some, Option.some.injEq, Prod.mk.injEq] at parsed
          rcases parsed with ⟨rfl, rfl⟩
          exact ⟨.cons (atom_syntax first) (ih others).1 (ih others).2, by simp⟩
      · cases parsed

 theorem arguments_syntax (fuel : Nat) {tokens rest : List Token} {arguments : List SourceAtom}
    (parsed : parseArguments fuel tokens = some (arguments, rest)) :
    ArgumentsSyntax tokens arguments rest := by
  unfold parseArguments at parsed
  split at parsed
  · cases parsed
    exact .empty _
  · exact (argument_list_syntax fuel parsed).1


theorem constructor_syntax {tokens rest : List Token} {expression : SourceExpression}
    (parsed : parseConstructor tokens = some (expression, rest)) :
    ExpressionSyntax tokens expression rest := by
  unfold parseConstructor at parsed
  split at parsed
  · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
    rcases parsed with ⟨⟨value, after⟩, valueParsed, parsed⟩
    dsimp only [Prod.fst, Prod.snd] at parsed
    split at parsed
    · cases parsed
      exact .ok (value_syntax valueParsed)
    · cases parsed
  · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
    rcases parsed with ⟨⟨value, after⟩, valueParsed, parsed⟩
    dsimp only [Prod.fst, Prod.snd] at parsed
    split at parsed
    · cases parsed
      exact .err (value_syntax valueParsed)
    · cases parsed
  · cases parsed

theorem expression_syntax {tokens rest : List Token} {expression : SourceExpression}
    (parsed : parseExpression tokens = some (expression, rest)) :
    ExpressionSyntax tokens expression rest := by
  unfold parseExpression at parsed
  split at parsed
  · exact constructor_syntax parsed
  · exact constructor_syntax parsed
  · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
    rcases parsed with ⟨⟨left, afterLeft⟩, leftParsed, parsed⟩
    dsimp only [Prod.fst, Prod.snd] at parsed
    split at parsed
    · split at parsed
      · simp only [Option.bind_eq_some_iff] at parsed
        rcases parsed with ⟨⟨right, afterRight⟩, rightParsed, parsed⟩
        dsimp only [Prod.fst, Prod.snd] at parsed
        split at parsed
        · simp only [Option.bind_eq_some_iff] at parsed
          rcases parsed with ⟨⟨thenValue, afterThen⟩, thenParsed, parsed⟩
          dsimp only [Prod.fst, Prod.snd] at parsed
          split at parsed
          · simp only [Option.bind_eq_some_iff] at parsed
            rcases parsed with ⟨⟨elseValue, afterElse⟩, elseParsed, parsed⟩
            dsimp only [Prod.fst, Prod.snd] at parsed
            split at parsed
            · cases parsed
              exact .conditional (atom_syntax leftParsed) (by assumption)
                (atom_syntax rightParsed) (value_syntax thenParsed) (value_syntax elseParsed)
            · cases parsed
          · cases parsed
        · cases parsed
      · cases parsed
    · cases parsed
  · split at parsed
    · cases parsed
    · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
      rcases parsed with ⟨⟨arguments, remaining⟩, argumentsParsed, parsed⟩
      dsimp only [Prod.fst, Prod.snd] at parsed
      cases parsed
      exact .call (by simpa using (show ¬ _ = true from by assumption))
        (arguments_syntax _ argumentsParsed)
  · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
    rcases parsed with ⟨⟨first, afterFirst⟩, firstParsed, parsed⟩
    dsimp only [Prod.fst, Prod.snd] at parsed
    split at parsed
    · simp only [Option.bind_eq_some_iff] at parsed
      rcases parsed with ⟨⟨second, remaining⟩, secondParsed, parsed⟩
      dsimp only [Prod.fst, Prod.snd] at parsed
      split at parsed
      · cases parsed
        exact .comparison (atom_syntax firstParsed) (by assumption) (atom_syntax secondParsed)
      · cases parsed
        exact .value (.binary (atom_syntax firstParsed)
          (by simpa using (show ¬ _ = true from by assumption)) (atom_syntax secondParsed))
    · cases parsed
      exact .value (.atom (atom_syntax firstParsed))


inductive LetsSyntax : List Token → List (String × Nat) → List String → List Token → Prop where
  | empty (tokens : List Token) : LetsSyntax tokens [] [] tokens
  | immutable {name : String} {value : Nat} {tokens rest : List Token}
      {lets : List (String × Nat)} {vars : List String} :
      LetsSyntax tokens lets vars rest →
      LetsSyntax (.word "let" :: .word name :: .equals :: .number value :: .newline :: tokens)
        ((name, value) :: lets) vars rest
  | mutable {name : String} {value : Nat} {tokens rest : List Token}
      {lets : List (String × Nat)} {vars : List String} :
      LetsSyntax tokens lets vars rest →
      LetsSyntax (.word "var" :: .word name :: .equals :: .number value :: .newline :: tokens)
        ((name, value) :: lets) (name :: vars) rest

inductive ComputedSyntax : List Token → List (String × SourceValue) → List Token → Prop where
  | empty (tokens : List Token) : ComputedSyntax tokens [] tokens
  | cons {name : String} {op : FragmentOp} {left right : SourceAtom}
      {tokens afterValue rest : List Token} {others : List (String × SourceValue)} :
      isKeyword name = false → ValueSyntax tokens (.binary op left right) (.newline :: afterValue) →
      ComputedSyntax afterValue others rest →
      ComputedSyntax (.word "let" :: .word name :: .equals :: tokens)
        ((name, .binary op left right) :: others) rest

inductive PropagationSyntax : List Token → Option SourcePropagation → List Token → Prop where
  | empty (tokens : List Token) : PropagationSyntax tokens none tokens
  | call {target callee : String} {tokens rest : List Token} {arguments : List SourceAtom} :
      isKeyword target = false → isKeyword callee = false →
      ArgumentsSyntax tokens arguments (.question :: .newline :: rest) →
      PropagationSyntax (.word "let" :: .word target :: .equals :: .word callee :: .leftParen :: tokens)
        (some ⟨target, callee, arguments⟩) rest

inductive AssignmentSyntax (vars : List String) :
    List Token → Option (String × SourceValue) → List Token → Prop where
  | empty (tokens : List Token) : AssignmentSyntax vars tokens none tokens
  | assign {target : String} {tokens rest : List Token} {value : SourceValue} :
      target ∈ vars → ValueSyntax tokens value (.newline :: rest) →
      AssignmentSyntax vars (.word target :: .equals :: tokens) (some (target, value)) rest

inductive ParametersSyntax : List Token → List String → List Token → Prop where
  | empty (rest : List Token) : ParametersSyntax (.rightParen :: rest) [] rest
  | single {name : String} {rest : List Token} : isKeyword name = false →
      ParametersSyntax (.word name :: .colon :: .word "Int" :: .rightParen :: rest) [name] rest
  | cons {name : String} {tokens rest : List Token} {others : List String} :
      isKeyword name = false → ParametersSyntax tokens others rest → others ≠ [] →
      ParametersSyntax (.word name :: .colon :: .word "Int" :: .comma :: tokens)
        (name :: others) rest

inductive ReturnTypeSyntax : List Token → ResultShape → List Token → Prop where
  | integer (rest : List Token) : ReturnTypeSyntax (.arrow :: .word "Int" :: rest) .integer rest
  | boolean (rest : List Token) : ReturnTypeSyntax (.arrow :: .word "Bool" :: rest) .boolean rest
  | result (rest : List Token) :
      ReturnTypeSyntax (.arrow :: .word "Result" :: .leftBracket :: .word "Int" :: .comma ::
        .word "Int" :: .rightBracket :: rest) .result rest

theorem lets_syntax (fuel : Nat) {tokens rest : List Token} {lets : List (String × Nat)}
    {vars : List String} (parsed : parseLets fuel tokens = some (lets, vars, rest)) :
    LetsSyntax tokens lets vars rest := by
  induction fuel generalizing tokens lets vars rest with
  | zero => simp [parseLets] at parsed
  | succ fuel ih =>
    unfold parseLets at parsed
    split at parsed
    · split at parsed
      · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
        rcases parsed with ⟨⟨more, foundVars, remaining⟩, moreParsed, parsed⟩
        rename_i tail keyword name value _ keywordAllowed
        have keywordCases : keyword = "let" ∨ keyword = "var" := by simpa using keywordAllowed
        rcases keywordCases with rfl | rfl
        · simp only [Option.some.injEq, Prod.mk.injEq] at parsed
          rcases parsed with ⟨rfl, rfl, rfl⟩
          exact .immutable (ih moreParsed)
        · simp only [beq_self_eq_true, ↓reduceIte, List.singleton_append,
            Option.some.injEq, Prod.mk.injEq] at parsed
          rcases parsed with ⟨rfl, rfl, rfl⟩
          exact .mutable (ih moreParsed)
      · cases parsed
        exact .empty _
    · cases parsed
      exact .empty _

theorem computed_syntax (fuel : Nat) {tokens rest : List Token}
    {bindings : List (String × SourceValue)}
    (parsed : parseComputedLets fuel tokens = some (bindings, rest)) :
    ComputedSyntax tokens bindings rest := by
  induction fuel generalizing tokens rest bindings with
  | zero => simp [parseComputedLets] at parsed
  | succ fuel ih =>
    unfold parseComputedLets at parsed
    split at parsed
    · split at parsed
      · cases parsed
        exact .empty _
      · split at parsed
        · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
          rcases parsed with ⟨⟨more, remaining⟩, moreParsed, parsed⟩
          dsimp only [Prod.fst, Prod.snd] at parsed
          cases parsed
          exact .cons (by simpa using (show ¬ _ = true from by assumption))
            (value_syntax (by assumption)) (ih moreParsed)
        · cases parsed
          exact .empty _
    · cases parsed
      exact .empty _

theorem propagation_syntax {tokens rest : List Token} {propagation : Option SourcePropagation}
    (parsed : parsePropagation tokens = some (propagation, rest)) :
    PropagationSyntax tokens propagation rest := by
  unfold parsePropagation at parsed
  split at parsed
  · split at parsed
    · cases parsed
    · rename_i afterParen target callee _ validNames
      have names : isKeyword target = false ∧ isKeyword callee = false := by
        simpa using validNames
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
      rcases parsed with ⟨⟨arguments, afterArguments⟩, argumentsParsed, parsed⟩
      dsimp only [Prod.fst, Prod.snd] at parsed
      split at parsed
      · cases parsed
        exact .call names.1 names.2 (arguments_syntax _ argumentsParsed)
      · cases parsed
  · cases parsed
    exact .empty _

theorem assignment_syntax {vars : List String} {tokens rest : List Token}
    {assignment : Option (String × SourceValue)}
    (parsed : parseAssignment vars tokens = some (assignment, rest)) :
    AssignmentSyntax vars tokens assignment rest := by
  unfold parseAssignment at parsed
  split at parsed
  · split at parsed
    · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
      rcases parsed with ⟨⟨value, afterValue⟩, valueParsed, parsed⟩
      dsimp only [Prod.fst, Prod.snd] at parsed
      split at parsed
      · cases parsed
        exact .assign (by simpa using (show vars.contains _ = true from by assumption))
          (value_syntax valueParsed)
      · cases parsed
    · cases parsed
  · cases parsed
    exact .empty _

theorem parameter_list_syntax (fuel : Nat) {tokens rest : List Token} {parameters : List String}
    (parsed : parseParameterList fuel tokens = some (parameters, rest)) :
    ParametersSyntax tokens parameters rest ∧ parameters ≠ [] := by
  induction fuel generalizing tokens rest parameters with
  | zero => simp [parseParameterList] at parsed
  | succ fuel ih =>
    unfold parseParameterList at parsed
    split at parsed
    · cases parsed
    · rename_i ignoredNat ignoredTokens ignoredFuel name afterType equalFuel
      have sameFuel : ignoredFuel = fuel := by omega
      subst ignoredFuel
      split at parsed
      · cases parsed
      · rename_i validName
        have valid : isKeyword name = false := by simpa using validName
        split at parsed
        · cases parsed
          exact ⟨.single valid, by simp⟩
        · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
          rcases parsed with ⟨⟨others, remaining⟩, othersParsed, parsed⟩
          dsimp only [Prod.fst, Prod.snd] at parsed
          cases parsed
          exact ⟨.cons valid (ih othersParsed).1 (ih othersParsed).2, by simp⟩
        · cases parsed
    · cases parsed

theorem parameters_syntax (fuel : Nat) {tokens rest : List Token} {parameters : List String}
    (parsed : parseParameters fuel tokens = some (parameters, rest)) :
    ParametersSyntax tokens parameters rest := by
  unfold parseParameters at parsed
  split at parsed
  · cases parsed
    exact .empty _
  · exact (parameter_list_syntax fuel parsed).1

theorem return_type_syntax {tokens rest : List Token} {shape : ResultShape}
    (parsed : parseReturnType tokens = some (shape, rest)) :
    ReturnTypeSyntax tokens shape rest := by
  unfold parseReturnType at parsed
  split at parsed
  · cases parsed; exact .integer _
  · cases parsed; exact .boolean _
  · cases parsed; exact .result _
  · cases parsed


inductive AssignedResult : Option (String × SourceValue) → SourceExpression → SourceExpression → Prop where
  | absent (expression : SourceExpression) : AssignedResult none expression expression
  | present (target : String) (value result : SourceValue) :
      AssignedResult (some (target, value)) (.value result) (.assigned target value result)

inductive ComputedResult : List (String × SourceValue) → SourceExpression → SourceExpression → Prop where
  | absent (expression : SourceExpression) : ComputedResult [] expression expression
  | single (name : String) (value result : SourceValue) :
      ComputedResult [(name, value)] (.value result) (.computedLet name value result)
  | several (first second : String × SourceValue) (others : List (String × SourceValue))
      (result : SourceValue) :
      ComputedResult (first :: second :: others) (.value result)
        (.computedLets (first :: second :: others) result)

def PropagationAllowed (parameters : List String) (lets : List (String × Nat))
    (assignment : Option (String × SourceValue)) (shape : ResultShape) :
    Option SourcePropagation → Prop
  | none => True
  | some _ => lets = [] ∧ assignment = none ∧ shape = .result ∧ parameters.length = 1

def ComputedAllowed (parameters : List String) (lets : List (String × Nat))
    (assignment : Option (String × SourceValue)) (propagation : Option SourcePropagation)
    (shape : ResultShape) : List (String × SourceValue) → Prop
  | [] => True
  | bindings@(_ :: _) => lets = [] ∧ assignment = none ∧ propagation = none ∧
      shape = .integer ∧ parameters.length = 1 ∧ computedBindingsAdmitted parameters bindings = true

inductive ClosingSyntax : List Token → List Token → Prop where
  | endOfInput : ClosingSyntax [.newline, .rightBrace] []
  | more (rest : List Token) : ClosingSyntax (.newline :: .rightBrace :: .newline :: rest) rest

inductive BodySyntax (shape : ResultShape) (name : String) (parameters : List String) :
    List Token → SourceFunction → List Token → Prop where
  | body {tokens afterLets afterComputed afterPropagation afterAssignment expressionTokens closing rest : List Token}
      {lets : List (String × Nat)} {vars : List String} {computed : List (String × SourceValue)}
      {propagation : Option SourcePropagation} {assignment : Option (String × SourceValue)}
      {returned assigned result : SourceExpression} :
      LetsSyntax tokens lets vars afterLets → ComputedSyntax afterLets computed afterComputed →
      PropagationSyntax afterComputed propagation afterPropagation →
      AssignmentSyntax vars afterPropagation assignment afterAssignment →
      PropagationAllowed parameters lets assignment shape propagation →
      ComputedAllowed parameters lets assignment propagation shape computed →
      afterAssignment = .word "return" :: expressionTokens →
      ExpressionSyntax expressionTokens returned closing → AssignedResult assignment returned assigned →
      ComputedResult computed assigned result → admitsResult shape result.shape = true →
      (∀ found, propagation = some found → found.target ∉ parameters) → ClosingSyntax closing rest →
      BodySyntax shape name parameters tokens ⟨name, parameters, lets, propagation, result⟩ rest

inductive FunctionSyntax : List Token → SourceFunction → List Token → Prop where
  | function {name : String} {parameters : List String} {shape : ResultShape}
      {tokens afterParameters bodyTokens rest : List Token} {function : SourceFunction} :
      ParametersSyntax tokens parameters afterParameters →
      ReturnTypeSyntax afterParameters shape (.leftBrace :: .newline :: bodyTokens) →
      BodySyntax shape name parameters bodyTokens function rest →
      FunctionSyntax (.word "pure" :: .word name :: .leftParen :: tokens) function rest

inductive FunctionsSyntax : List Token → List SourceFunction → Prop where
  | empty : FunctionsSyntax [] []
  | newline {tokens : List Token} {functions : List SourceFunction} :
      FunctionsSyntax tokens functions → FunctionsSyntax (.newline :: tokens) functions
  | cons {tokens rest : List Token} {function : SourceFunction} {functions : List SourceFunction} :
      FunctionSyntax tokens function rest → FunctionsSyntax rest functions →
      FunctionsSyntax tokens (function :: functions)

theorem assigned_result {assignment : Option (String × SourceValue)} {returned result : SourceExpression}
    (attached : attachAssignment assignment returned = some result) :
    AssignedResult assignment returned result := by
  unfold attachAssignment at attached
  split at attached
  · cases attached; exact .absent _
  · cases attached; exact .present _ _ _
  · cases attached

theorem computed_result {computed : List (String × SourceValue)} {returned result : SourceExpression}
    (built : buildComputedResult computed returned = some result) :
    ComputedResult computed returned result := by
  unfold buildComputedResult at built
  split at built
  · cases built; exact .absent _
  · cases built; exact .single _ _ _
  · cases built; exact .several _ _ _ _
  · cases built

theorem body_syntax {shape : ResultShape} {name : String} {parameters : List String}
    {tokens rest : List Token} {function : SourceFunction}
    (parsed : parseFunctionBody shape name parameters tokens = some (function, rest)) :
    BodySyntax shape name parameters tokens function rest := by
  unfold parseFunctionBody at parsed
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
  rcases parsed with ⟨⟨lets, vars, afterLets⟩, letsParsed,
    ⟨⟨computed, afterComputed⟩, computedParsed,
    ⟨⟨propagation, afterPropagation⟩, propagationParsed,
    ⟨⟨assignment, afterAssignment⟩, assignmentParsed, parsed⟩⟩⟩⟩
  dsimp only [Prod.fst, Prod.snd] at *
  simp only [Option.bind_none] at parsed
  have propAllowed : PropagationAllowed parameters lets assignment shape propagation := by
    cases propagation with
    | none => trivial
    | some found =>
      by_cases valid : (lets.isEmpty && assignment.isNone && shape == .result && parameters.length == 1) = true
      · simpa [PropagationAllowed, ComputedAllowed, and_assoc] using valid
      · simp only [valid] at parsed
        cases parsed
  cases propCase : propagation
  all_goals
    simp only [propCase] at parsed propAllowed propagationParsed
    try
      have guard : (lets.isEmpty && assignment.isNone && shape == .result && parameters.length == 1) = true := by
        simpa [PropagationAllowed, and_assoc] using propAllowed
      simp only [guard, ↓reduceIte] at parsed
    try dsimp only at parsed
  all_goals
    have compAllowed : ComputedAllowed parameters lets assignment propagation shape computed := by
      cases computed with
      | nil => trivial
      | cons first others =>
        by_cases valid : (lets.isEmpty && assignment.isNone &&
            propagation.isNone && shape == .integer &&
            parameters.length == 1 && computedBindingsAdmitted parameters (first :: others)) = true
        · simpa [ComputedAllowed, and_assoc] using valid
        · simp only [propCase] at valid
          simp only [valid] at parsed
          cases parsed
    have compGuard : (match computed with
      | [] => true
      | bindings => lets.isEmpty && assignment.isNone && propagation.isNone && shape == .integer &&
          parameters.length == 1 && computedBindingsAdmitted parameters bindings) = true := by
      cases computed with
      | nil => rfl
      | cons first others => simpa [ComputedAllowed, and_assoc] using compAllowed
    cases computed
    all_goals
      simp only [propCase] at compGuard
      try dsimp only at compGuard
      try simp only [compGuard, ↓reduceIte] at parsed
      try dsimp only at parsed
      split at parsed
      · rename_i expressionTokens _
        simp only [Option.bind_eq_some_iff] at parsed
        rcases parsed with ⟨⟨returned, afterResult⟩, returnedParsed,
          ⟨assigned, assignedBuilt, ⟨result, resultBuilt, parsed⟩⟩⟩
        dsimp only [Prod.fst, Prod.snd] at *
        split at parsed
        · rename_i shapeAdmitted
          have fresh : ∀ target, propagation = some target → parameters.contains target.target = false := by
            intro target same
            simp only [propCase] at same
            cases flag : parameters.contains target.target <;> cases same
            all_goals
              first
              | rfl
              | simp only [flag, ↓reduceIte] at parsed
                cases parsed
          have freshNames : ∀ target, propagation = some target → target.target ∉ parameters := by
            intro target same
            simpa using fresh target same
          have noShadow := fresh
          simp [propCase] at noShadow
          try simp [noShadow] at parsed
          split at parsed
          · cases parsed
            exact .body (lets_syntax _ letsParsed) (computed_syntax _ computedParsed)
              (propagation_syntax propagationParsed) (assignment_syntax assignmentParsed)
              propAllowed (by simpa [propCase] using compAllowed) rfl (expression_syntax returnedParsed)
              (assigned_result assignedBuilt) (computed_result resultBuilt) shapeAdmitted
              (by simpa only [propCase] using freshNames)
              .endOfInput
          · cases parsed
            exact .body (lets_syntax _ letsParsed) (computed_syntax _ computedParsed)
              (propagation_syntax propagationParsed) (assignment_syntax assignmentParsed)
              propAllowed (by simpa [propCase] using compAllowed) rfl (expression_syntax returnedParsed)
              (assigned_result assignedBuilt) (computed_result resultBuilt) shapeAdmitted
              (by simpa only [propCase] using freshNames)
              (.more _)
          · cases parsed
        · cases parsed
      · cases parsed

theorem function_syntax {tokens rest : List Token} {function : SourceFunction}
    (parsed : parseFunction tokens = some (function, rest)) : FunctionSyntax tokens function rest := by
  unfold parseFunction at parsed
  split at parsed
  · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
    rcases parsed with ⟨⟨parameters, afterParameters⟩, parametersParsed,
      ⟨⟨shape, afterShape⟩, shapeParsed, parsed⟩⟩
    dsimp only [Prod.fst, Prod.snd] at parsed
    split at parsed
    · exact .function (parameters_syntax _ parametersParsed) (return_type_syntax shapeParsed)
        (body_syntax parsed)
    · cases parsed
  · cases parsed

theorem skip_newlines_syntax {tokens : List Token} {functions : List SourceFunction}
    (derivation : FunctionsSyntax (skipNewlines tokens) functions) : FunctionsSyntax tokens functions := by
  induction tokens with
  | nil => exact derivation
  | cons token tokens ih =>
    cases token <;> try exact derivation
    exact .newline (ih derivation)

theorem functions_syntax (fuel : Nat) {tokens : List Token} {functions : List SourceFunction}
    (parsed : parseFunctions fuel tokens = some functions) : FunctionsSyntax tokens functions := by
  induction fuel generalizing tokens functions with
  | zero =>
    cases tokens with
    | nil => cases parsed; exact .empty
    | cons token tokens => simp [parseFunctions] at parsed
  | succ fuel ih =>
    cases tokens with
    | nil => cases parsed; exact .empty
    | cons token tokens =>
      simp only [parseFunctions] at parsed
      split at parsed
      · cases parsed
        apply skip_newlines_syntax
        simpa only [show skipNewlines (token :: tokens) = [] from by assumption] using FunctionsSyntax.empty
      · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
        rcases parsed with ⟨⟨function, rest⟩, functionParsed, ⟨others, othersParsed, parsed⟩⟩
        cases parsed
        apply skip_newlines_syntax
        exact .cons (function_syntax functionParsed) (ih othersParsed)


def Punctuation (codes : List Nat) (token : Token) : Prop :=
  (codes, token) ∈ [([10], .newline), ([40], .leftParen), ([41], .rightParen),
    ([123], .leftBrace), ([125], .rightBrace), ([91], .leftBracket), ([93], .rightBracket),
    ([61], .equals), ([58], .colon), ([44], .comma), ([63], .question),
    ([62], .operator .gt), ([60], .operator .lt), ([43], .operator .add),
    ([42], .operator .mul), ([47], .operator .div), ([37], .operator .rem),
    ([45], .operator .sub), ([45, 62], .arrow)]

def Boundary (keep : Nat → Bool) (rest : List Nat) : Prop :=
  ∀ head tail, rest = head :: tail → keep head = false

inductive Lexeme (rest : List Nat) : List Nat → Token → Prop where
  | punctuation {codes : List Nat} {token : Token} : Punctuation codes token →
      (codes = [45] → rest.head? ≠ some 62) → Lexeme rest codes token
  | decimal {digits : List Nat} : digits ≠ [] → digits.all isDigitCode = true →
      Boundary isDigitCode rest → Lexeme rest digits (.number (digitsValue digits))
  | word {head : Nat} {tail : List Nat} : isWordStartCode head = true →
      (head :: tail).all isWordPartCode = true → Boundary isWordPartCode rest →
      Lexeme rest (head :: tail) (.word (String.ofList ((head :: tail).map Char.ofNat)))

inductive Lexed : List Nat → List Token → Prop where
  | empty : Lexed [] []
  | space {codes : List Nat} {tokens : List Token} : Lexed codes tokens → Lexed (32 :: codes) tokens
  | token {leading rest : List Nat} {token : Token} {tokens : List Token} :
      Lexeme rest leading token → Lexed rest tokens → Lexed (leading ++ rest) (token :: tokens)

theorem taken_codes (keep : Nat → Bool) (codes : List Nat) :
    (takeCodesWhile keep codes).1 ++ (takeCodesWhile keep codes).2 = codes ∧
      (takeCodesWhile keep codes).1.all keep = true ∧ Boundary keep (takeCodesWhile keep codes).2 := by
  induction codes with
  | nil => exact ⟨rfl, rfl, by intro head tail impossible; cases impossible⟩
  | cons code rest ih =>
    simp only [takeCodesWhile]
    split
    · rename_i kept
      rcases taken : takeCodesWhile keep rest with ⟨leading, remaining⟩
      simp only [taken] at ih ⊢
      refine ⟨?_, ?_, ih.2.2⟩
      · exact congrArg (code :: ·) ih.1
      · simpa [kept] using ih.2.1
    · rename_i stopped
      refine ⟨rfl, rfl, ?_⟩
      intro head tail same
      cases same
      simpa using stopped

theorem lexed_from_token {fuel : Nat} {leading rest : List Nat} {token : Token} {tokens : List Token}
    (continuation : ∀ {codes : List Nat} {tokens : List Token},
      tokenize fuel codes = some tokens → Lexed codes tokens)
    (lexeme : Lexeme rest leading token)
    (parsed : (tokenize fuel rest).map (token :: ·) = some tokens) :
    Lexed (leading ++ rest) tokens := by
  cases tailParsed : tokenize fuel rest with
  | none => simp [tailParsed] at parsed
  | some others =>
    simp only [tailParsed, Option.map_some, Option.some.injEq] at parsed
    subst tokens
    exact .token lexeme (continuation tailParsed)

theorem tokenizer_syntax (fuel : Nat) {codes : List Nat} {tokens : List Token}
    (parsed : tokenize fuel codes = some tokens) : Lexed codes tokens := by
  induction fuel generalizing codes tokens with
  | zero =>
    cases codes with
    | nil => cases parsed; exact .empty
    | cons code rest => simp [tokenize] at parsed
  | succ fuel ih =>
    cases codes with
    | nil => cases parsed; exact .empty
    | cons code rest =>
      unfold tokenize at parsed
      dsimp only at parsed
      by_cases same : code = 32
      · subst code
        change tokenize fuel rest = some tokens at parsed
        exact .space (ih parsed)
      · rw [ite_eq_right (by simpa using same)] at parsed
        by_cases same : code = 10
        · subst code
          change (tokenize fuel rest).map (fun tail => (.newline : Token) :: tail) = some tokens at parsed
          change Lexed ([10] ++ rest) tokens
          exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
        · rw [ite_eq_right (by simpa using same)] at parsed
          by_cases same : code = 40
          · subst code
            change (tokenize fuel rest).map (fun tail => (.leftParen : Token) :: tail) = some tokens at parsed
            change Lexed ([40] ++ rest) tokens
            exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
          · rw [ite_eq_right (by simpa using same)] at parsed
            by_cases same : code = 41
            · subst code
              change (tokenize fuel rest).map (fun tail => (.rightParen : Token) :: tail) = some tokens at parsed
              change Lexed ([41] ++ rest) tokens
              exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
            · rw [ite_eq_right (by simpa using same)] at parsed
              by_cases same : code = 123
              · subst code
                change (tokenize fuel rest).map (fun tail => (.leftBrace : Token) :: tail) = some tokens at parsed
                change Lexed ([123] ++ rest) tokens
                exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
              · rw [ite_eq_right (by simpa using same)] at parsed
                by_cases same : code = 125
                · subst code
                  change (tokenize fuel rest).map (fun tail => (.rightBrace : Token) :: tail) = some tokens at parsed
                  change Lexed ([125] ++ rest) tokens
                  exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                · rw [ite_eq_right (by simpa using same)] at parsed
                  by_cases same : code = 91
                  · subst code
                    change (tokenize fuel rest).map (fun tail => (.leftBracket : Token) :: tail) = some tokens at parsed
                    change Lexed ([91] ++ rest) tokens
                    exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                  · rw [ite_eq_right (by simpa using same)] at parsed
                    by_cases same : code = 93
                    · subst code
                      change (tokenize fuel rest).map (fun tail => (.rightBracket : Token) :: tail) = some tokens at parsed
                      change Lexed ([93] ++ rest) tokens
                      exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                    · rw [ite_eq_right (by simpa using same)] at parsed
                      by_cases same : code = 61
                      · subst code
                        change (tokenize fuel rest).map (fun tail => (.equals : Token) :: tail) = some tokens at parsed
                        change Lexed ([61] ++ rest) tokens
                        exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                      · rw [ite_eq_right (by simpa using same)] at parsed
                        by_cases same : code = 58
                        · subst code
                          change (tokenize fuel rest).map (fun tail => (.colon : Token) :: tail) = some tokens at parsed
                          change Lexed ([58] ++ rest) tokens
                          exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                        · rw [ite_eq_right (by simpa using same)] at parsed
                          by_cases same : code = 44
                          · subst code
                            change (tokenize fuel rest).map (fun tail => (.comma : Token) :: tail) = some tokens at parsed
                            change Lexed ([44] ++ rest) tokens
                            exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                          · rw [ite_eq_right (by simpa using same)] at parsed
                            by_cases same : code = 63
                            · subst code
                              change (tokenize fuel rest).map (fun tail => (.question : Token) :: tail) = some tokens at parsed
                              change Lexed ([63] ++ rest) tokens
                              exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                            · rw [ite_eq_right (by simpa using same)] at parsed
                              by_cases same : code = 62
                              · subst code
                                change (tokenize fuel rest).map (fun tail => (.operator .gt : Token) :: tail) = some tokens at parsed
                                change Lexed ([62] ++ rest) tokens
                                exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                              · rw [ite_eq_right (by simpa using same)] at parsed
                                by_cases same : code = 60
                                · subst code
                                  change (tokenize fuel rest).map (fun tail => (.operator .lt : Token) :: tail) = some tokens at parsed
                                  change Lexed ([60] ++ rest) tokens
                                  exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                                · rw [ite_eq_right (by simpa using same)] at parsed
                                  by_cases same : code = 43
                                  · subst code
                                    change (tokenize fuel rest).map (fun tail => (.operator .add : Token) :: tail) = some tokens at parsed
                                    change Lexed ([43] ++ rest) tokens
                                    exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                                  · rw [ite_eq_right (by simpa using same)] at parsed
                                    by_cases same : code = 42
                                    · subst code
                                      change (tokenize fuel rest).map (fun tail => (.operator .mul : Token) :: tail) = some tokens at parsed
                                      change Lexed ([42] ++ rest) tokens
                                      exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                                    · rw [ite_eq_right (by simpa using same)] at parsed
                                      by_cases same : code = 47
                                      · subst code
                                        change (tokenize fuel rest).map (fun tail => (.operator .div : Token) :: tail) = some tokens at parsed
                                        change Lexed ([47] ++ rest) tokens
                                        exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                                      · rw [ite_eq_right (by simpa using same)] at parsed
                                        by_cases same : code = 37
                                        · subst code
                                          change (tokenize fuel rest).map (fun tail => (.operator .rem : Token) :: tail) = some tokens at parsed
                                          change Lexed ([37] ++ rest) tokens
                                          exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                                        · rw [ite_eq_right (by simpa using same)] at parsed
                                          by_cases same : code = 45
                                          · subst code
                                            rw [ite_eq_left (show (45 == 45) = true by decide)] at parsed
                                            split at parsed
                                            · rename_i ignored remaining
                                              change Lexed ([45, 62] ++ remaining) tokens
                                              exact lexed_from_token ih (.punctuation (by unfold Punctuation; decide) (by intro impossible; cases impossible)) parsed
                                            · change Lexed ([45] ++ rest) tokens
                                              apply lexed_from_token ih (Lexeme.punctuation (by unfold Punctuation; decide) ?_) parsed
                                              intro _ headEq
                                              cases rest with
                                              | nil => cases headEq
                                              | cons head tail =>
                                                simp only [List.head?_cons, Option.some.injEq] at headEq
                                                subst head
                                                simp_all
                                          · rw [ite_eq_right (by simpa using same)] at parsed
                                            by_cases digit : isDigitCode code = true
                                            · rw [ite_eq_left digit] at parsed
                                              rcases trailing : takeCodesWhile isDigitCode rest with ⟨digits, remaining⟩
                                              have taken : takeCodesWhile isDigitCode (code :: rest) = (code :: digits, remaining) := by
                                                simp only [takeCodesWhile, digit, ↓reduceIte, trailing]
                                              have spec := taken_codes isDigitCode (code :: rest)
                                              simp only [taken] at spec parsed
                                              rw [← spec.1]
                                              exact lexed_from_token ih (.decimal (by simp) spec.2.1 spec.2.2) parsed
                                            · rw [ite_eq_right digit] at parsed
                                              by_cases word : isWordStartCode code = true
                                              · rw [ite_eq_left word] at parsed
                                                have part : isWordPartCode code = true := by simp [isWordPartCode, word]
                                                rcases trailing : takeCodesWhile isWordPartCode rest with ⟨letters, remaining⟩
                                                have taken : takeCodesWhile isWordPartCode (code :: rest) = (code :: letters, remaining) := by
                                                  simp only [takeCodesWhile, part, ↓reduceIte, trailing]
                                                have spec := taken_codes isWordPartCode (code :: rest)
                                                simp only [taken] at spec parsed
                                                rw [← spec.1]
                                                exact lexed_from_token ih (.word word spec.2.1 spec.2.2) parsed
                                              · rw [ite_eq_right word] at parsed
                                                cases parsed


theorem lexeme_ascii {rest leading : List Nat} {token : Token}
    (lexeme : Lexeme rest leading token) : ∀ code, code ∈ leading → code < 128 := by
  cases lexeme with
  | punctuation punctuation maximal =>
    simp [Punctuation] at punctuation
    rcases punctuation with h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h
    all_goals
      rcases h with ⟨rfl, rfl⟩
      intro code member
      simp at member
      omega
  | decimal nonempty digits boundary =>
    intro code member
    have digit := List.all_eq_true.mp digits code member
    simp only [isDigitCode, Bool.and_eq_true, decide_eq_true_eq] at digit
    omega
  | word first letters boundary =>
    intro code member
    have letter := List.all_eq_true.mp letters code member
    simp only [isWordPartCode, isWordStartCode, isDigitCode, Bool.or_eq_true, Bool.and_eq_true,
      decide_eq_true_eq, beq_iff_eq] at letter
    omega

theorem lexed_ascii {codes : List Nat} {tokens : List Token}
    (lexical : Lexed codes tokens) : ∀ code, code ∈ codes → code < 128 := by
  induction lexical with
  | empty => simp
  | space previous ih =>
    intro code member
    simp only [List.mem_cons] at member
    rcases member with rfl | member
    · decide
    · exact ih code member
  | token lexeme previous ih =>
    intro code member
    simp only [List.mem_append] at member
    rcases member with member | member
    · exact lexeme_ascii lexeme code member
    · exact ih code member

inductive FunctionsInOrder : List SourceFunction → List SourceFunction → Prop where
  | empty (earlier : List SourceFunction) : FunctionsInOrder earlier []
  | cons {earlier : List SourceFunction} {function : SourceFunction} {rest : List SourceFunction} :
      (∀ propagation, function.propagation = some propagation →
        ∃ callee, earlier.find? (fun found => found.name == propagation.callee) = some callee ∧
          callee.callableShape = true ∧ callee.parameters.length = propagation.arguments.length) →
      FunctionsInOrder (earlier ++ [function]) rest → FunctionsInOrder earlier (function :: rest)

theorem functions_in_order {earlier functions : List SourceFunction}
    (admitted : functionsAdmitted.go earlier functions = true) : FunctionsInOrder earlier functions := by
  induction functions generalizing earlier with
  | nil => exact .empty _
  | cons function rest ih =>
    simp only [functionsAdmitted.go, Bool.and_eq_true] at admitted
    refine .cons ?_ (ih admitted.2)
    intro propagation same
    rw [same] at admitted
    cases found : earlier.find? (fun callee => callee.name == propagation.callee) with
    | none => simp [found] at admitted
    | some callee =>
      simp only [found, Bool.and_eq_true, beq_iff_eq] at admitted
      exact ⟨callee, rfl, admitted.1.1, admitted.1.2⟩

inductive AdmittedSource (codes : List Nat) (functions : List SourceFunction) : Prop where
  | admitted (tokens : List Token) : Lexed codes tokens → FunctionsSyntax tokens functions →
      FunctionsInOrder [] functions → AdmittedSource codes functions

theorem parse_source_admitted {codes : List Nat} {functions : List SourceFunction}
    (parsed : parseSourceCodes codes = some functions) : AdmittedSource codes functions := by
  unfold parseSourceCodes at parsed
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at parsed
  rcases parsed with ⟨tokens, tokenized, ⟨found, parsedFunctions, parsed⟩⟩
  split at parsed
  · cases parsed
    exact ⟨tokens, tokenizer_syntax _ tokenized, functions_syntax _ parsedFunctions,
      functions_in_order (by assumption)⟩
  · cases parsed

inductive ComputedScope (parameter : String) :
    Nat → List (String × SourceValue) → List (String × Nat) → Prop where
  | empty (slot : Nat) : ComputedScope parameter slot [] []
  | cons {slot : Nat} {name : String} {value : SourceValue}
      {rest : List (String × SourceValue)} {scope : List (String × Nat)} :
      name ≠ parameter → name ∉ rest.map (·.1) → ComputedScope parameter (slot + 1) rest scope →
      ComputedScope parameter slot ((name, value) :: rest) ((name, slot) :: scope)

def OrdinaryResult : SourceExpression → Prop
  | .computedLet _ _ _ => False
  | .computedLets _ _ => False
  | _ => True

inductive ScopeProjection : SourceFunction → List (String × Nat) → Prop where
  | propagated (name parameter : String) (propagation : SourcePropagation) (result : SourceExpression) :
      parameter ≠ propagation.target →
      ScopeProjection ⟨name, [parameter], [], some propagation, result⟩
        [(parameter, 0), (propagation.target, 2)]
  | literal (name localName : String) (value : Nat) (result : SourceExpression) :
      ScopeProjection ⟨name, [], [(localName, value)], none, result⟩ [(localName, letSlot)]
  | parameterLiteral (name parameter localName : String) (value : Nat) (result : SourceExpression) :
      result.isAssigned = false → parameter ≠ localName →
      ScopeProjection ⟨name, [parameter], [(localName, value)], none, result⟩
        [(parameter, 0), (localName, 2)]
  | computed (name parameter localName : String) (value result : SourceValue) :
      parameter ≠ localName →
      ScopeProjection ⟨name, [parameter], [], none, .computedLet localName value result⟩
        [(parameter, 0), (localName, 2)]
  | computedSeveral (name parameter : String) (bindings : List (String × SourceValue))
      (result : SourceValue) (scope : List (String × Nat)) :
      ComputedScope parameter 2 bindings scope →
      ScopeProjection ⟨name, [parameter], [], none, .computedLets bindings result⟩
        ((parameter, 0) :: scope)
  | single (name parameter : String) (result : SourceExpression) : OrdinaryResult result →
      ScopeProjection ⟨name, [parameter], [], none, result⟩ [(parameter, 0)]
  | several (name first second : String) (rest : List String) (result : SourceExpression) :
      (first :: second :: rest).Nodup →
      ScopeProjection ⟨name, first :: second :: rest, [], none, result⟩
        ((first :: second :: rest).zip (List.range (first :: second :: rest).length))

theorem computed_scope_projection {parameter : String} {slot : Nat}
    {bindings : List (String × SourceValue)} {scope : List (String × Nat)}
    (projected : computedLetsScope? parameter slot bindings = some scope) :
    ComputedScope parameter slot bindings scope := by
  induction bindings generalizing slot scope with
  | nil => cases projected; exact .empty _
  | cons binding rest ih =>
    rcases binding with ⟨name, value⟩
    simp only [computedLetsScope?] at projected
    split at projected
    · cases projected
    · rename_i fresh
      have distinct : name ≠ parameter ∧ name ∉ rest.map (·.1) := by simpa using fresh
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at projected
      rcases projected with ⟨others, othersProjected, projected⟩
      cases projected
      exact .cons distinct.1 distinct.2 (ih othersProjected)

theorem scope_projection {function : SourceFunction} {scope : List (String × Nat)}
    (projected : function.scope? = some scope) : ScopeProjection function scope := by
  rcases function with ⟨name, parameters, lets, propagation, result⟩
  unfold SourceFunction.scope? at projected
  split at projected <;> simp_all only <;> subst_vars
  · split at projected
    · cases projected
    · cases projected
      apply ScopeProjection.propagated
      simp_all
  · cases projected
    exact .literal _ _ _ _
  · split at projected
    · cases projected
    · split at projected
      · cases projected
      · cases projected
        apply ScopeProjection.parameterLiteral <;> simp_all
  · split at projected
    · split at projected
      · cases projected
      · cases projected
        apply ScopeProjection.computed
        simp_all
    · simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at projected
      rcases projected with ⟨bindings, bindingsProjected, projected⟩
      cases projected
      exact .computedSeveral _ _ _ _ _ (computed_scope_projection bindingsProjected)
    · cases projected
      exact .single _ _ _ (by cases result <;> simp_all [OrdinaryResult])
  · split at projected
    · cases projected
      rename_i first rest nonempty distinct
      cases rest with
      | nil => exact False.elim (nonempty rfl)
      | cons second others => exact .several _ _ _ _ _ distinct
    · cases projected
  · cases projected


inductive ScopesProjection : List SourceFunction → List (List (String × Nat)) → Prop where
  | empty : ScopesProjection [] []
  | cons {function : SourceFunction} {functions : List SourceFunction}
      {scope : List (String × Nat)} {scopes : List (List (String × Nat))} :
      ScopeProjection function scope → ScopesProjection functions scopes →
      ScopesProjection (function :: functions) (scope :: scopes)

theorem scopes_projection {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (projected : functions.mapM SourceFunction.scope? = some scopes) : ScopesProjection functions scopes := by
  induction functions generalizing scopes with
  | nil => cases projected; exact .empty
  | cons function rest ih =>
    rw [List.mapM_cons] at projected
    simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at projected
    rcases projected with ⟨scope, scopeProjected, ⟨others, othersProjected, projected⟩⟩
    cases projected
    exact .cons (scope_projection scopeProjected) (ih othersProjected)

/-- A witness checker for one module with no imported inputs. The submitted trees and slot
projections are candidates; both are reconstructed from the original byte list. -/
def check (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (functions : List SourceFunction) (scopes : List (List (String × Nat))) : Bool :=
  moduleInputs.isEmpty && decide (functions.map (·.name)).Nodup &&
    match parseSourceCodes (bytes.map UInt8.toNat) with
    | none => false
    | some parsed => parsed == functions && parsed.mapM SourceFunction.scope? == some scopes

/-- Accepted bytes have a complete derivation in the independent grammar, including decimal
interpretation and ordered propagation targets; every submitted slot table has an independent
projection derivation. This statement concerns the Lean grammar, not Rust parser agreement. -/
theorem check_sound {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    moduleInputs = [] ∧ (functions.map (·.name)).Nodup ∧
      AdmittedSource (bytes.map UInt8.toNat) functions ∧ ScopesProjection functions scopes := by
  unfold check at accepted
  simp only [Bool.and_eq_true, List.isEmpty_iff, decide_eq_true_eq] at accepted
  rcases accepted with ⟨⟨noModules, distinct⟩, accepted⟩
  cases parsed : parseSourceCodes (bytes.map UInt8.toNat) with
  | none => simp [parsed] at accepted
  | some found =>
    simp only [parsed, Bool.and_eq_true, beq_iff_eq] at accepted
    rcases accepted with ⟨rfl, projected⟩
    exact ⟨noModules, distinct, parse_source_admitted parsed, scopes_projection projected⟩

theorem accepted_projection_unique {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {first second : List SourceFunction} {firstScopes secondScopes : List (List (String × Nat))}
    (firstAccepted : check bytes moduleInputs first firstScopes = true)
    (secondAccepted : check bytes moduleInputs second secondScopes = true) :
    first = second ∧ firstScopes = secondScopes := by
  unfold check at firstAccepted secondAccepted
  simp only [Bool.and_eq_true] at firstAccepted secondAccepted
  cases parsed : parseSourceCodes (bytes.map UInt8.toNat) with
  | none => simp [parsed] at firstAccepted
  | some functions =>
    simp only [parsed, Bool.and_eq_true, beq_iff_eq] at firstAccepted secondAccepted
    have firstTree := firstAccepted.2.1
    have secondTree := secondAccepted.2.1
    subst first
    subst second
    refine ⟨rfl, ?_⟩
    exact Option.some.inj (firstAccepted.2.2.symm.trans secondAccepted.2.2)

/-- The source admission boundary composes with the existing universal lowerer theorem. The
artifact's entire function meaning is the meaning of the tree derived from these exact bytes. -/
theorem admitted_lowering_means_source {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))} {artifact : RawArtifact}
    (accepted : check bytes moduleInputs functions scopes = true)
    (lowered : lowersTo artifact functions = true) :
    moduleInputs = [] ∧ AdmittedSource (bytes.map UInt8.toNat) functions ∧
      ScopesProjection functions scopes ∧ functionPrograms artifact = sourcePrograms functions := by
  have admission := check_sound accepted
  exact ⟨admission.1, admission.2.2.1, admission.2.2.2, lowers_to_means_the_source lowered⟩

/-- A certified source/IR pair starts at the original code bytes: its grammar derivation is
independent of the compiler's tree, and its IR meaning follows from the universal lowerer proof.
Source-document decoding and extraction from a Rust parser remain separate boundaries. -/
theorem certified_codes_admitted {sourceId : Xsh.Conformance.Transport.BridgeId} {codes : List Nat}
    {artifact : RawArtifact} {results : List ObservedResult}
    (certified : certifyCodes sourceId codes artifact = some results) :
    ∃ functions, AdmittedSource codes functions ∧
      functionPrograms artifact = sourcePrograms functions ∧
      artifact.sourceId = sourceId ∧ validateArtifact artifact = true ∧
      namedResults artifact = some results := by
  unfold certifyCodes at certified
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at certified
  rcases certified with ⟨functions, parsed, certified⟩
  split at certified
  · rename_i checks
    simp only [Bool.and_eq_true, decide_eq_true_eq] at checks
    exact ⟨functions, parse_source_admitted parsed, lowers_to_means_the_source checks.1.2,
      checks.1.1, checks.2, certified⟩
  · cases certified

theorem admitted_bytes_are_ascii {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    ∀ byte, byte ∈ bytes → byte.toNat < 128 := by
  have admission := (check_sound accepted).2.2.1
  cases admission with
  | admitted tokens lexical productions earlier =>
    intro byte member
    exact lexed_ascii lexical byte.toNat (List.mem_map.mpr ⟨byte, member, rfl⟩)

def witnessBytes (text : String) : List UInt8 := text.toUTF8.toList

def arithmeticTree : List SourceFunction :=
  [⟨"f", ["x"], [], none, .value (.binary .add (.var "x") (.int 1))⟩]

def arithmeticBytes : List UInt8 := witnessBytes "pure f(x: Int) -> Int {\n  return x + 1\n}\n"

theorem arithmetic_bytes_admitted :
    check arithmeticBytes [] arithmeticTree [[("x", 0)]] = true := by
  decide +kernel

theorem assignment_bytes_admitted :
    check (witnessBytes "pure f() -> Int {\n  var x = 1\n  x = x + 2\n  return x\n}\n") []
      [⟨"f", [], [("x", 1)], none, .assigned "x" (.binary .add (.var "x") (.int 2))
        (.atom (.var "x"))⟩] [[("x", 1)]] = true := by
  decide +kernel

theorem computed_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let a = x + 1\n  let b = a * 2\n  return b\n}\n") []
      [⟨"f", ["x"], [], none,
        .computedLets [("a", .binary .add (.var "x") (.int 1)),
          ("b", .binary .mul (.var "a") (.int 2))] (.atom (.var "b"))⟩]
      [[("x", 0), ("a", 2), ("b", 3)]] = true := by
  decide +kernel

theorem boolean_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Bool {\n  return x > 1\n}\n") []
      [⟨"f", ["x"], [], none, .comparison .gt (.var "x") (.int 1)⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem result_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Result[Int, Int] {\n  return Err(x)\n}\n") []
      [⟨"f", ["x"], [], none, .constructed false (.atom (.var "x"))⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem propagation_bytes_admitted :
    check (witnessBytes ("pure g(x: Int) -> Result[Int, Int] {\n  return Ok(x)\n}\n" ++
        "pure f(x: Int) -> Result[Int, Int] {\n  let y = g(x)?\n  return Ok(y)\n}\n")) []
      [⟨"g", ["x"], [], none, .constructed true (.atom (.var "x"))⟩,
        ⟨"f", ["x"], [], some ⟨"y", "g", [.var "x"]⟩, .constructed true (.atom (.var "y"))⟩]
      [[("x", 0)], [("x", 0), ("y", 2)]] = true := by
  decide +kernel

theorem conditional_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return if x > 0 { x + 1 } else { 0 }\n}\n") []
      [⟨"f", ["x"], [], none, .conditional .gt (.var "x") (.int 0)
        (.binary .add (.var "x") (.int 1)) (.atom (.int 0))⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem call_bytes_admitted :
    check (witnessBytes ("pure g(x: Int) -> Int {\n  return x + 1\n}\n" ++
        "pure f(y: Int) -> Int {\n  return g(y)\n}\n")) []
      [⟨"g", ["x"], [], none, .value (.binary .add (.var "x") (.int 1))⟩,
        ⟨"f", ["y"], [], none, .call "g" [.var "y"]⟩] [[("x", 0)], [("y", 0)]] = true := by
  decide +kernel

theorem single_computed_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let y = x + 1\n  return y\n}\n") []
      [⟨"f", ["x"], [], none, .computedLet "y" (.binary .add (.var "x") (.int 1))
        (.atom (.var "y"))⟩] [[("x", 0), ("y", 2)]] = true := by
  decide +kernel

theorem admitted_byte_domain_nonempty :
    ∃ bytes functions scopes, bytes ≠ [] ∧ functions ≠ [] ∧ check bytes [] functions scopes = true := by
  exact ⟨arithmeticBytes, arithmeticTree, [[("x", 0)]], by decide +kernel, by decide +kernel,
    arithmetic_bytes_admitted⟩

theorem substituted_source_and_projection_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return x - 1\n}\n") []
      arithmeticTree [[("x", 0)]] = false ∧
    check arithmeticBytes [] arithmeticTree [[("x", 1)]] = false ∧
    check arithmeticBytes [] arithmeticTree [[("y", 0)]] = false ∧
    check arithmeticBytes [] arithmeticTree [] = false ∧
    check arithmeticBytes [arithmeticBytes] arithmeticTree [[("x", 0)]] = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem unsupported_trailing_bytes_refused :
    check (arithmeticBytes ++ [0]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ [64]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ [195, 169]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ witnessBytes "pure") [] arithmeticTree [[("x", 0)]] = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end SourceAdmission
end Xsh.Conformance.Vertical
