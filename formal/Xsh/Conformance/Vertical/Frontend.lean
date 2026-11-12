import Xsh.Conformance.Vertical.Artifact

namespace Xsh.Conformance.Vertical

/-- The tokens of the admitted source productions: integer functions with integer parameters or
one integer let or var, an optional assignment to that var, and one return of an integer value, a
call of an earlier function on atoms, or a conditional on a comparison of atoms. Any other
character is outside the domain. -/
inductive Token where
  | word (text : String)
  | number (value : Nat)
  | leftParen
  | rightParen
  | leftBrace
  | rightBrace
  | leftBracket
  | rightBracket
  | equals
  | arrow
  | colon
  | comma
  | question
  | operator (op : FragmentOp)
  | newline
  deriving DecidableEq, Repr

def isDigitCode (code : Nat) : Bool := 48 ≤ code && code ≤ 57

def isWordStartCode (code : Nat) : Bool :=
  (97 ≤ code && code ≤ 122) || (65 ≤ code && code ≤ 90) || code == 95

def isWordPartCode (code : Nat) : Bool := isWordStartCode code || isDigitCode code

def takeCodesWhile (keep : Nat → Bool) : List Nat → List Nat × List Nat
  | [] => ([], [])
  | code :: rest =>
      if keep code then
        let (taken, remaining) := takeCodesWhile keep rest
        (code :: taken, remaining)
      else
        ([], code :: rest)

def digitsValue (digits : List Nat) : Nat :=
  digits.foldl (fun value digit => value * 10 + (digit - 48)) 0

/-- Spaces separate tokens, a line feed is a token, and `->` is one token. -/
def tokenize : Nat → List Nat → Option (List Token)
  | _, [] => some []
  | 0, _ => none
  | fuel + 1, code :: rest =>
      let continueWith (token : Token) (remaining : List Nat) : Option (List Token) :=
        (tokenize fuel remaining).map (token :: ·)
      if code == 32 then tokenize fuel rest
      else if code == 10 then continueWith .newline rest
      else if code == 40 then continueWith .leftParen rest
      else if code == 41 then continueWith .rightParen rest
      else if code == 123 then continueWith .leftBrace rest
      else if code == 125 then continueWith .rightBrace rest
      else if code == 91 then continueWith .leftBracket rest
      else if code == 93 then continueWith .rightBracket rest
      else if code == 61 then continueWith .equals rest
      else if code == 58 then continueWith .colon rest
      else if code == 44 then continueWith .comma rest
      else if code == 63 then continueWith .question rest
      else if code == 62 then continueWith (.operator .gt) rest
      else if code == 60 then continueWith (.operator .lt) rest
      else if code == 43 then continueWith (.operator .add) rest
      else if code == 42 then continueWith (.operator .mul) rest
      else if code == 47 then continueWith (.operator .div) rest
      else if code == 37 then continueWith (.operator .rem) rest
      else if code == 45 then
        match rest with
        | 62 :: remaining => continueWith .arrow remaining
        | _ => continueWith (.operator .sub) rest
      else if isDigitCode code then
        let (digits, remaining) := takeCodesWhile isDigitCode (code :: rest)
        continueWith (.number (digitsValue digits)) remaining
      else if isWordStartCode code then
        let (letters, remaining) := takeCodesWhile isWordPartCode (code :: rest)
        continueWith (.word (String.ofList (letters.map Char.ofNat))) remaining
      else none

inductive SourceAtom where
  | int (value : Nat)
  | var (name : String)
  deriving DecidableEq, Repr

/-- An integer value: one atom, or one arithmetic operation on two atoms. -/
inductive SourceValue where
  | atom (value : SourceAtom)
  | binary (op : FragmentOp) (left right : SourceAtom)
  deriving DecidableEq, Repr

inductive SourceExpression where
  | value (value : SourceValue)
  | call (callee : String) (arguments : List SourceAtom)
  | conditional (comparison : FragmentOp) (left right : SourceAtom)
      (thenValue elseValue : SourceValue)
  | assigned (target : String) (assignedValue : SourceValue) (result : SourceValue)
  | comparison (op : FragmentOp) (left right : SourceAtom)
  | constructed (isOk : Bool) (value : SourceValue)
  | computedLet (target : String) (value : SourceValue) (result : SourceValue)
  deriving DecidableEq, Repr

/-- A `let` line whose value is a call under `?`: the target, the callee and its arguments. -/
structure SourcePropagation where
  target : String
  callee : String
  arguments : List SourceAtom
  deriving DecidableEq, Repr

structure SourceFunction where
  name : String
  parameters : List String
  lets : List (String × Nat)
  propagation : Option SourcePropagation
  result : SourceExpression
  deriving DecidableEq, Repr

def isKeyword (name : String) : Bool :=
  name == "pure" || name == "let" || name == "var" || name == "return" || name == "if" ||
    name == "else"

def parseAtom : List Token → Option (SourceAtom × List Token)
  | .number value :: rest => some (.int value, rest)
  | .word name :: rest => if isKeyword name then none else some (.var name, rest)
  | _ => none

def parseValue (tokens : List Token) : Option (SourceValue × List Token) := do
  let (left, rest) ← parseAtom tokens
  match rest with
  | .operator op :: more =>
      if op.isComparison then none
      else
        let (right, remaining) ← parseAtom more
        some (.binary op left right, remaining)
  | _ => some (.atom left, rest)

def parseArgumentList : Nat → List Token → Option (List SourceAtom × List Token)
  | 0, _ => none
  | fuel + 1, tokens => do
      let (first, afterFirst) ← parseAtom tokens
      match afterFirst with
      | .rightParen :: rest => some ([first], rest)
      | .comma :: more =>
          let (others, rest) ← parseArgumentList fuel more
          some (first :: others, rest)
      | _ => none

def parseArguments (fuel : Nat) : List Token → Option (List SourceAtom × List Token)
  | .rightParen :: rest => some ([], rest)
  | tokens => parseArgumentList fuel tokens

/-- `Ok(value)` and `Err(value)` are the two `Result` constructors over one value. -/
def parseConstructor : List Token → Option (SourceExpression × List Token)
  | .word "Ok" :: .leftParen :: rest => do
      let (value, after) ← parseValue rest
      match after with
      | .rightParen :: remaining => some (.constructed true value, remaining)
      | _ => none
  | .word "Err" :: .leftParen :: rest => do
      let (value, after) ← parseValue rest
      match after with
      | .rightParen :: remaining => some (.constructed false value, remaining)
      | _ => none
  | _ => none

def parseExpression (tokens : List Token) : Option (SourceExpression × List Token) :=
  match tokens with
  | .word "Ok" :: .leftParen :: _ => parseConstructor tokens
  | .word "Err" :: .leftParen :: _ => parseConstructor tokens
  | .word "if" :: more => do
      let (left, afterLeft) ← parseAtom more
      match afterLeft with
      | .operator op :: afterOperator =>
          if op.isComparison then do
            let (right, afterRight) ← parseAtom afterOperator
            match afterRight with
            | .leftBrace :: afterBrace => do
                let (thenValue, afterThen) ← parseValue afterBrace
                match afterThen with
                | .rightBrace :: .word "else" :: .leftBrace :: afterElse => do
                    let (elseValue, afterValue) ← parseValue afterElse
                    match afterValue with
                    | .rightBrace :: rest =>
                        some (.conditional op left right thenValue elseValue, rest)
                    | _ => none
                | _ => none
            | _ => none
          else
            none
      | _ => none
  | .word name :: .leftParen :: more =>
      if isKeyword name then none
      else do
        let (arguments, rest) ← parseArguments (more.length + 1) more
        some (.call name arguments, rest)
  | _ => do
      let (first, afterFirst) ← parseAtom tokens
      match afterFirst with
      | .operator op :: afterOperator => do
          let (second, rest) ← parseAtom afterOperator
          if op.isComparison then
            some (.comparison op first second, rest)
          else
            some (.value (.binary op first second), rest)
      | _ => some (.value (.atom first), afterFirst)

/-- The `let` and `var` lines, with the names the `var` lines declare. -/
def parseLets : Nat → List Token → Option (List (String × Nat) × List String × List Token)
  | 0, _ => none
  | fuel + 1, tokens =>
      match tokens with
      | .word keyword :: .word name :: .equals :: .number value :: .newline :: rest =>
          if keyword == "let" || keyword == "var" then do
            let (more, vars, remaining) ← parseLets fuel rest
            some ((name, value) :: more, (if keyword == "var" then [name] else []) ++ vars,
              remaining)
          else some ([], [], tokens)
      | _ => some ([], [], tokens)

/-- A propagation line: `let target = callee(arguments)?`. -/
def parsePropagation : List Token → Option (Option SourcePropagation × List Token)
  | .word "let" :: .word target :: .equals :: .word callee :: .leftParen :: afterParen =>
      if isKeyword target || isKeyword callee then none
      else do
        let (arguments, rest) ← parseArguments (afterParen.length + 1) afterParen
        match rest with
        | .question :: .newline :: remaining =>
            some (some ⟨target, callee, arguments⟩, remaining)
        | _ => none
  | tokens => some (none, tokens)

/-- A computed `let` line: `let target = atom op atom`. A literal binding is taken by
`parseLets` instead, and any other `let` line is left for `parsePropagation`. -/
def parseComputedLet : List Token → Option (Option (String × SourceValue) × List Token)
  | tokens@(.word "let" :: .word name :: .equals :: rest) =>
      if isKeyword name then none
      else
        match parseValue rest with
        | some (value@(.binary _ _ _), .newline :: after) => some (some (name, value), after)
        | _ => some (none, tokens)
  | tokens => some (none, tokens)

/-- An optional assignment line to a declared var, an integer value. -/
def parseAssignment (vars : List String) :
    List Token → Option (Option (String × SourceValue) × List Token)
  | .word target :: .equals :: tokens =>
      if vars.contains target then do
        let (value, afterValue) ← parseValue tokens
        match afterValue with
        | .newline :: rest => some (some (target, value), rest)
        | _ => none
      else none
  | tokens => some (none, tokens)

/-- The assignment, when there is one, goes with an integer value returned after it. -/
def attachAssignment : Option (String × SourceValue) → SourceExpression → Option SourceExpression
  | none, result => some result
  | some (target, value), .value result => some (.assigned target value result)
  | some _, _ => none

def parseParameterList : Nat → List Token → Option (List String × List Token)
  | 0, _ => none
  | fuel + 1, .word name :: .colon :: .word "Int" :: afterType =>
      if isKeyword name then none
      else
        match afterType with
        | .rightParen :: rest => some ([name], rest)
        | .comma :: more => do
            let (others, rest) ← parseParameterList fuel more
            some (name :: others, rest)
        | _ => none
  | _ + 1, _ => none

def parseParameters (fuel : Nat) : List Token → Option (List String × List Token)
  | .rightParen :: rest => some ([], rest)
  | tokens => parseParameterList fuel tokens

/-- The declared return type admits only its own result shape: `Int` every value form, `Bool` a
comparison of two atoms, and `Result[Int, Int]` either constructor over a value. The tree keeps the
result expression, which determines the meaning; the annotation itself is checked here and not
stored. -/
inductive ResultShape where
  | integer
  | boolean
  | result
  deriving DecidableEq, Repr

/-- The shape a result expression has, which the declared return type must admit. -/
inductive ExpressionShape where
  | comparison
  | constructed
  | other
  deriving DecidableEq, Repr

def SourceExpression.shape : SourceExpression → ExpressionShape
  | .comparison _ _ _ => .comparison
  | .constructed _ _ => .constructed
  | _ => .other

def SourceValue.atoms : SourceValue → List SourceAtom
  | .atom single => [single]
  | .binary _ left right => [left, right]

/-- The computed let value reads only the function's parameters and integer literals. -/
def SourceAtom.isParameterOrLiteral (parameters : List String) : SourceAtom → Bool
  | .int _ => true
  | .var name => parameters.contains name

def admitsResult : ResultShape → ExpressionShape → Bool
  | .integer, .comparison => false
  | .integer, .constructed => false
  | .integer, .other => true
  | .boolean, .comparison => true
  | .boolean, .constructed => false
  | .boolean, .other => false
  | .result, .comparison => false
  | .result, .constructed => true
  | .result, .other => false

def parseFunctionBody (shape : ResultShape) (name : String) (parameters : List String)
    (tokens : List Token) : Option (SourceFunction × List Token) := do
  let (lets, vars, afterLets) ← parseLets (tokens.length + 1) tokens
  let (computed, afterComputed) ← parseComputedLet afterLets
  let (propagation, afterPropagation) ← parsePropagation afterComputed
  let (assignment, afterAssignment) ← parseAssignment vars afterPropagation
  match propagation with
  | some _ =>
      if lets.isEmpty && assignment.isNone && shape == .result && parameters.length == 1 then
        pure ()
      else none
  | none => pure ()
  match computed with
  | some (target, value) =>
      if lets.isEmpty && assignment.isNone && propagation.isNone && shape == .integer &&
          parameters.length == 1 && !parameters.contains target &&
          value.atoms.all (·.isParameterOrLiteral parameters) then
        pure ()
      else none
  | none => pure ()
  match afterAssignment with
  | .word "return" :: more => do
      let (returned, afterResult) ← parseExpression more
      let result ← attachAssignment assignment returned
      let result ← match computed with
        | none => pure result
        | some (target, value) =>
            match result with
            | .value resultValue => pure (.computedLet target value resultValue)
            | _ => none
      if admitsResult shape result.shape then pure () else none
      match propagation with
      | some found =>
          if parameters.contains found.target then none
          else pure ()
      | none => pure ()
      match afterResult with
      | .newline :: .rightBrace :: [] => some (⟨name, parameters, lets, propagation, result⟩, [])
      | .newline :: .rightBrace :: .newline :: tail =>
          some (⟨name, parameters, lets, propagation, result⟩, tail)
      | _ => none
  | _ => none

/-- The declared return type, which admits only the result shape it names. -/
def parseReturnType : List Token → Option (ResultShape × List Token)
  | .arrow :: .word "Int" :: rest => some (.integer, rest)
  | .arrow :: .word "Bool" :: rest => some (.boolean, rest)
  | .arrow :: .word "Result" :: .leftBracket :: .word "Int" :: .comma :: .word "Int" ::
      .rightBracket :: rest => some (.result, rest)
  | _ => none

def parseFunction (tokens : List Token) : Option (SourceFunction × List Token) :=
  match tokens with
  | .word "pure" :: .word name :: .leftParen :: afterParen => do
      let (parameters, afterParameters) ← parseParameters (afterParen.length + 1) afterParen
      let (shape, afterShape) ← parseReturnType afterParameters
      match afterShape with
      | .leftBrace :: .newline :: rest => parseFunctionBody shape name parameters rest
      | _ => none
  | _ => none

def skipNewlines : List Token → List Token
  | .newline :: rest => skipNewlines rest
  | tokens => tokens

def parseFunctions : Nat → List Token → Option (List SourceFunction)
  | _, [] => some []
  | 0, _ => none
  | fuel + 1, tokens =>
      match skipNewlines tokens with
      | [] => some []
      | start => do
          let (function, rest) ← parseFunction start
          let functions ← parseFunctions fuel rest
          some (function :: functions)

/-- A function a propagation may call: no let, no propagation and no assignment, returning a
constructor, which is the callee form the meaning keeps. -/
def SourceFunction.callableShape (function : SourceFunction) : Bool :=
  function.lets.isEmpty && function.propagation.isNone && function.result.shape == .constructed &&
    match function.result with
    | .assigned _ _ _ => false
    | _ => true

/-- Every propagation names an earlier callable function with a constructor result and the
argument count of that function. -/
def functionsAdmitted (functions : List SourceFunction) : Bool :=
  let rec go (earlier : List SourceFunction) : List SourceFunction → Bool
    | [] => true
    | function :: rest =>
        let admitted :=
          match function.propagation with
          | none => true
          | some propagation =>
              match earlier.find? (fun callee => callee.name == propagation.callee) with
              | some callee =>
                  callee.callableShape && callee.parameters.length == propagation.arguments.length
              | none => false
        admitted && go (earlier ++ [function]) rest
  go [] functions

/-- The admitted source productions, from the character codes of the source. -/
def parseSourceCodes (codes : List Nat) : Option (List SourceFunction) := do
  let tokens ← tokenize (codes.length + 1) codes
  let functions ← parseFunctions (tokens.length + 1) tokens
  if functionsAdmitted functions then some functions else none

/-- The admitted source productions, from exact source bytes. -/
def parseSource (text : String) : Option (List SourceFunction) :=
  parseSourceCodes (text.toList.map Char.toNat)

theorem the_admitted_productions_parse :
    parseSource "pure f() -> Int {\n  let a = 4\n  return a - 1\n}\n" =
      some [⟨"f", [], [("a", 4)], none, .value (.binary .sub (.var "a") (.int 1))⟩] := by
  decide +kernel

theorem parameters_a_call_and_a_conditional_parse :
    parseSource ("pure g(a: Int, b: Int) -> Int {\n  return if a > b { a - b } else { 0 }\n}\n" ++
        "pure h() -> Int {\n  let x = 7\n  return g(x, 2)\n}\n") =
      some [⟨"g", ["a", "b"], [], none,
          .conditional .gt (.var "a") (.var "b") (.binary .sub (.var "a") (.var "b")) (.atom (.int 0))⟩,
        ⟨"h", [], [("x", 7)], none, .call "g" [.var "x", .int 2]⟩] := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem comparison_functions_parse :
    parseSource ("pure exceeds(value: Int, limit: Int) -> Bool {\n  return value > limit\n}\n" ++
        "pure below(value: Int, limit: Int) -> Bool {\n  return value < limit\n}\n") =
      some [⟨"exceeds", ["value", "limit"], [], none,
          .comparison .gt (.var "value") (.var "limit")⟩,
        ⟨"below", ["value", "limit"], [], none,
          .comparison .lt (.var "value") (.var "limit")⟩] := by
  decide +kernel

theorem a_bool_annotation_admits_only_a_comparison_result :
    parseSource "pure f(a: Int, b: Int) -> Bool {\n  return a\n}\n" = none ∧
      parseSource "pure f(a: Int, b: Int) -> Bool {\n  return a + b\n}\n" = none ∧
      parseSource "pure f(a: Int, b: Int) -> Bool {\n  return a > b + 1\n}\n" = none ∧
      parseSource "pure f(a: Int, b: Int) -> Bool {\n  return if a > b { 1 } else { 0 }\n}\n" =
        none ∧
      parseSource "pure f(a: Int, b: Int) -> Bool {\n  var x = 1\n  x = 2\n  return a > b\n}\n" =
        none := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem an_int_annotation_refuses_a_comparison_result :
    parseSource "pure f(a: Int, b: Int) -> Int {\n  return a > b\n}\n" = none ∧
      parseSource "pure f(a: Int, b: Int) -> Int {\n  return a < b\n}\n" = none := by
  constructor <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_result_annotation_admits_only_constructor_results :
    parseSource "pure f(a: Int) -> Result[Int, Int] {\n  return a\n}\n" = none ∧
      parseSource "pure f(a: Int) -> Result[Int, Int] {\n  return a + 1\n}\n" = none ∧
      parseSource "pure f(a: Int) -> Result[Int, Str] {\n  return Err(a)\n}\n" = none ∧
      parseSource "pure f(a: Int) -> Int {\n  return Ok(a)\n}\n" = none ∧
      parseSource "pure f(a: Int) -> Bool {\n  return Err(a)\n}\n" = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

set_option maxRecDepth 10000 in
theorem a_propagated_call_parses :
    parseSource ("pure keeps(value: Int) -> Result[Int, Int] {\n  return Ok(value)\n}\n" ++
        "pure passes(value: Int) -> Result[Int, Int] {\n  let kept = keeps(value)?\n" ++
        "  return Ok(kept)\n}\n") =
      some [⟨"keeps", ["value"], [], none, .constructed true (.atom (.var "value"))⟩,
        ⟨"passes", ["value"], [], some ⟨"kept", "keeps", [.var "value"]⟩,
          .constructed true (.atom (.var "kept"))⟩] := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem propagation_outside_the_admitted_productions_is_refused :
    parseSource "pure f(value: Int) -> Result[Int, Int] {\n  let kept = g(value)?\n  return Ok(kept)\n}\n" =
        none ∧
      parseSource "pure f(value: Int) -> Result[Int, Int] {\n  let kept = g(value)\n  return Ok(kept)\n}\n" =
        none ∧
      parseSource ("pure g(value: Int) -> Result[Int, Int] {\n  return Ok(value)\n}\n" ++
          "pure f(value: Int) -> Int {\n  let kept = g(value)?\n  return kept\n}\n") = none ∧
      parseSource ("pure g(value: Int) -> Result[Int, Int] {\n  return Ok(value)\n}\n" ++
          "pure f(value: Int, other: Int) -> Result[Int, Int] {\n  let kept = g(value)?\n" ++
          "  return Ok(kept)\n}\n") = none ∧
      parseSource ("pure g(value: Int) -> Result[Int, Int] {\n  return Ok(value)\n}\n" ++
          "pure f(value: Int) -> Result[Int, Int] {\n  let value = g(value)?\n  return Ok(value)\n}\n") =
        none ∧
      parseSource ("pure g(value: Int) -> Result[Int, Int] {\n  return Ok(value)\n}\n" ++
          "pure f(value: Int) -> Result[Int, Int] {\n  let kept = g(value)??\n  return Ok(kept)\n}\n") =
        none := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem text_outside_the_admitted_productions_is_refused :
    parseSource "pure f() -> Int {\n  return 1 + 2 + 3\n}\n" = none ∧
      parseSource "pure f() -> Int {\n  let a = 4\n}\n" = none ∧
      parseSource "pure f() -> Int {\n  return 1 @ 2\n}\n" = none ∧
      parseSource "pure f() -> Int {\n  return 1\n}\npure" = none ∧
      parseSource "pure f() -> Int {\n  return 1\n  let a = 4\n}\n" = none ∧
      parseSource "pure f() -> Float {\n  return 1\n}\n" = none ∧
      parseSource "pure f() -> Int { return 1 }\n" = none ∧
      parseSource "pure f() -> Int {\n  return pure\n}\n" = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

theorem conditionals_calls_and_parameters_outside_the_productions_are_refused :
    parseSource "pure f(a: Int) -> Int {\n  return if a { 1 } else { 2 }\n}\n" = none ∧
      parseSource "pure f(a: Int) -> Int {\n  return if a > 1 { 1 }\n}\n" = none ∧
      parseSource "pure f(a: Int) -> Int {\n  return if a > 1 { 1 } else { a > 2 }\n}\n" = none ∧
      parseSource "pure f(a: Int) -> Int {\n  return a > 1\n}\n" = none ∧
      parseSource "pure f(a: Str) -> Int {\n  return a\n}\n" = none ∧
      parseSource "pure f(a: Int,) -> Int {\n  return a\n}\n" = none ∧
      parseSource "pure f(a) -> Int {\n  return a\n}\n" = none ∧
      parseSource "pure f() -> Int {\n  return g(1,)\n}\n" = none ∧
      parseSource "pure f() -> Int {\n  return g(1 2)\n}\n" = none ∧
      parseSource "pure f() -> Int {\n  return if 1 > 2 { 1 } else { 2 } + 1\n}\n" = none := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

end Xsh.Conformance.Vertical
