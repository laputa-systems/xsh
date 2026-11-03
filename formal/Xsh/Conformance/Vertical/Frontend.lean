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
  | equals
  | arrow
  | colon
  | comma
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
      else if code == 61 then continueWith .equals rest
      else if code == 58 then continueWith .colon rest
      else if code == 44 then continueWith .comma rest
      else if code == 62 then continueWith (.operator .gt) rest
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
  deriving DecidableEq, Repr

structure SourceFunction where
  name : String
  parameters : List String
  lets : List (String × Nat)
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

def parseExpression (tokens : List Token) : Option (SourceExpression × List Token) :=
  match tokens with
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
      let (value, rest) ← parseValue tokens
      some (.value value, rest)

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

def parseFunction (tokens : List Token) : Option (SourceFunction × List Token) :=
  match tokens with
  | .word "pure" :: .word name :: .leftParen :: afterParen => do
      let (parameters, afterParameters) ← parseParameters (afterParen.length + 1) afterParen
      match afterParameters with
      | .arrow :: .word "Int" :: .leftBrace :: .newline :: rest => do
          let (lets, vars, afterLets) ← parseLets (rest.length + 1) rest
          let (assignment, afterAssignment) ← parseAssignment vars afterLets
          match afterAssignment with
          | .word "return" :: more => do
              let (returned, afterResult) ← parseExpression more
              let result ← attachAssignment assignment returned
              match afterResult with
              | .newline :: .rightBrace :: [] =>
                  some (⟨name, parameters, lets, result⟩, [])
              | .newline :: .rightBrace :: .newline :: tail =>
                  some (⟨name, parameters, lets, result⟩, tail)
              | _ => none
          | _ => none
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

/-- The admitted source productions, from the character codes of the source. -/
def parseSourceCodes (codes : List Nat) : Option (List SourceFunction) := do
  let tokens ← tokenize (codes.length + 1) codes
  parseFunctions (tokens.length + 1) tokens

/-- The admitted source productions, from exact source bytes. -/
def parseSource (text : String) : Option (List SourceFunction) :=
  parseSourceCodes (text.toList.map Char.toNat)

theorem the_admitted_productions_parse :
    parseSource "pure f() -> Int {\n  let a = 4\n  return a - 1\n}\n" =
      some [⟨"f", [], [("a", 4)], .value (.binary .sub (.var "a") (.int 1))⟩] := by
  decide +kernel

theorem parameters_a_call_and_a_conditional_parse :
    parseSource ("pure g(a: Int, b: Int) -> Int {\n  return if a > b { a - b } else { 0 }\n}\n" ++
        "pure h() -> Int {\n  let x = 7\n  return g(x, 2)\n}\n") =
      some [⟨"g", ["a", "b"], [],
          .conditional .gt (.var "a") (.var "b") (.binary .sub (.var "a") (.var "b")) (.atom (.int 0))⟩,
        ⟨"h", [], [("x", 7)], .call "g" [.var "x", .int 2]⟩] := by
  decide +kernel

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
