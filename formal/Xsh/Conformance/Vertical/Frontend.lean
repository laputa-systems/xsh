import Xsh.Conformance.Vertical.Artifact

namespace Xsh.Conformance.Vertical

/-- The tokens of the admitted source productions: integer functions made of integer lets and
one return of at most one binary operation. Any other character is outside the domain. -/
inductive Token where
  | word (text : String)
  | number (value : Nat)
  | leftParen
  | rightParen
  | leftBrace
  | rightBrace
  | equals
  | arrow
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

inductive SourceExpression where
  | atom (value : SourceAtom)
  | binary (op : FragmentOp) (left right : SourceAtom)
  deriving DecidableEq, Repr

structure SourceFunction where
  name : String
  lets : List (String × Nat)
  result : SourceExpression
  deriving DecidableEq, Repr

def parseAtom : List Token → Option (SourceAtom × List Token)
  | .number value :: rest => some (.int value, rest)
  | .word name :: rest =>
      if name == "pure" || name == "let" || name == "return" then none else some (.var name, rest)
  | _ => none

def parseExpression (tokens : List Token) : Option (SourceExpression × List Token) := do
  let (left, rest) ← parseAtom tokens
  match rest with
  | .operator op :: more =>
      let (right, remaining) ← parseAtom more
      some (.binary op left right, remaining)
  | _ => some (.atom left, rest)

def parseLets : Nat → List Token → Option (List (String × Nat) × List Token)
  | 0, _ => none
  | fuel + 1, .word "let" :: .word name :: .equals :: .number value :: .newline :: rest => do
      let (more, remaining) ← parseLets fuel rest
      some ((name, value) :: more, remaining)
  | _ + 1, tokens => some ([], tokens)

def parseFunction (tokens : List Token) : Option (SourceFunction × List Token) :=
  match tokens with
  | .word "pure" :: .word name :: .leftParen :: .rightParen :: .arrow :: .word "Int" ::
      .leftBrace :: .newline :: rest => do
      let (lets, afterLets) ← parseLets (rest.length + 1) rest
      match afterLets with
      | .word "return" :: more => do
          let (result, afterResult) ← parseExpression more
          match afterResult with
          | .newline :: .rightBrace :: [] => some (⟨name, lets, result⟩, [])
          | .newline :: .rightBrace :: .newline :: tail => some (⟨name, lets, result⟩, tail)
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
      some [⟨"f", [("a", 4)], .binary .sub (.var "a") (.int 1)⟩] := by
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

end Xsh.Conformance.Vertical
