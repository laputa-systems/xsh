namespace Xsh.Conformance.Transport

/-- The closed v1 exchange documents contain only strings, arrays and objects. -/
inductive WireValue where
  | string (text : String)
  | array (items : List WireValue)
  | object (fields : List (String × WireValue))

def isJsonWhitespace (character : Char) : Bool :=
  character == ' ' || character == '\t' || character == '\n' || character == '\r'

def jsonHexValue? (code : Nat) : Option Nat :=
  if 48 ≤ code ∧ code ≤ 57 then some (code - 48)
  else if 97 ≤ code ∧ code ≤ 102 then some (code - 87)
  else if 65 ≤ code ∧ code ≤ 70 then some (code - 55)
  else none

def isScalarCode (code : Nat) : Bool :=
  decide (code < 55296 ∨ (57344 ≤ code ∧ code < 1114112))

def maxDocumentBytes : Nat := 65536

inductive Token where
  | leftBrace
  | rightBrace
  | leftBracket
  | rightBracket
  | comma
  | colon
  | string (text : String)

/-- Where the lexer is inside the text. Strings accumulate their characters in reverse. -/
inductive LexMode where
  | outside
  | inString (accumulated : List Char)
  | afterBackslash (accumulated : List Char)
  | unicodeEscape (accumulated : List Char) (digits code : Nat)

def simpleEscape? : Nat → Option Char
  | 34 => some '"'
  | 92 => some '\\'
  | 47 => some '/'
  | 98 => some (Char.ofNat 8)
  | 102 => some (Char.ofNat 12)
  | 110 => some '\n'
  | 114 => some '\r'
  | 116 => some '\t'
  | _ => none

/-- One code point per step, structural on the text. Raw control characters, non-ASCII text,
lone surrogate escapes, unknown escapes and any code outside the closed grammar are rejected.
The kernel decodes a `String` literal slowly and reduces natural-number comparison natively,
so the lexer reads a list of code points. -/
def lex : List Nat → LexMode → List Token → Option (List Token)
  | [], .outside, tokens => some tokens.reverse
  | [], _, _ => none
  | code :: rest, .outside, tokens =>
      if code == 34 then lex rest (.inString []) tokens
      else if code == 123 then lex rest .outside (.leftBrace :: tokens)
      else if code == 125 then lex rest .outside (.rightBrace :: tokens)
      else if code == 91 then lex rest .outside (.leftBracket :: tokens)
      else if code == 93 then lex rest .outside (.rightBracket :: tokens)
      else if code == 44 then lex rest .outside (.comma :: tokens)
      else if code == 58 then lex rest .outside (.colon :: tokens)
      else if code == 32 || code == 9 || code == 10 || code == 13 then lex rest .outside tokens
      else none
  | code :: rest, .inString accumulated, tokens =>
      if code == 34 then
        lex rest .outside (.string (String.ofList accumulated.reverse) :: tokens)
      else if code == 92 then lex rest (.afterBackslash accumulated) tokens
      else if code < 32 || 127 < code then none
      else lex rest (.inString (Char.ofNat code :: accumulated)) tokens
  | code :: rest, .afterBackslash accumulated, tokens =>
      if code == 117 then lex rest (.unicodeEscape accumulated 0 0) tokens
      else
        match simpleEscape? code with
        | some escaped => lex rest (.inString (escaped :: accumulated)) tokens
        | none => none
  | character :: rest, .unicodeEscape accumulated digits code, tokens =>
      match jsonHexValue? character with
      | none => none
      | some digit =>
          let extended := code * 16 + digit
          if digits + 1 == 4 then
            if isScalarCode extended then
              lex rest (.inString (Char.ofNat extended :: accumulated)) tokens
            else
              none
          else
            lex rest (.unicodeEscape accumulated (digits + 1) extended) tokens

def keysAreDistinct : List (String × WireValue) → Bool
  | [] => true
  | (key, _) :: rest => !(rest.any fun field => field.1 == key) && keysAreDistinct rest

/-- A container being built. Items and fields are kept in reverse. -/
inductive Frame where
  | array (items : List WireValue)
  | object (fields : List (String × WireValue)) (key : Option String)

/-- What the parser needs next. `allowClose` is true only directly after an opener. -/
inductive Expect where
  | value (allowClose : Bool)
  | key (allowClose : Bool)
  | colon
  | afterValue
  | finished (value : WireValue)

/-- Attach a completed value to the innermost open container, or finish the document. -/
def complete (stack : List Frame) (value : WireValue) : Option (List Frame × Expect) :=
  match stack with
  | [] => some ([], .finished value)
  | .array items :: outer => some (.array (value :: items) :: outer, .afterValue)
  | .object fields (some key) :: outer =>
      some (.object ((key, value) :: fields) none :: outer, .afterValue)
  | .object _ none :: _ => none

/-- One token per step, structural on the token list. -/
def parseTokens : List Token → List Frame → Expect → Option WireValue
  | [], [], .finished value => some value
  | [], _, _ => none
  | _ :: _, _, .finished _ => none
  | .string text :: rest, stack, .value _ =>
      match complete stack (.string text) with
      | some (next, expect) => parseTokens rest next expect
      | none => none
  | .leftBracket :: rest, stack, .value _ =>
      parseTokens rest (.array [] :: stack) (.value true)
  | .leftBrace :: rest, stack, .value _ =>
      parseTokens rest (.object [] none :: stack) (.key true)
  | .rightBracket :: rest, .array [] :: outer, .value true =>
      match complete outer (.array []) with
      | some (next, expect) => parseTokens rest next expect
      | none => none
  | .string text :: rest, .object fields none :: outer, .key _ =>
      parseTokens rest (.object fields (some text) :: outer) .colon
  | .rightBrace :: rest, .object [] none :: outer, .key true =>
      match complete outer (.object []) with
      | some (next, expect) => parseTokens rest next expect
      | none => none
  | .colon :: rest, stack, .colon => parseTokens rest stack (.value false)
  | .comma :: rest, stack@(.array _ :: _), .afterValue =>
      parseTokens rest stack (.value false)
  | .comma :: rest, stack@(.object _ none :: _), .afterValue =>
      parseTokens rest stack (.key false)
  | .rightBracket :: rest, .array items :: outer, .afterValue =>
      match complete outer (.array items.reverse) with
      | some (next, expect) => parseTokens rest next expect
      | none => none
  | .rightBrace :: rest, .object fields none :: outer, .afterValue =>
      if keysAreDistinct fields.reverse then
        match complete outer (.object fields.reverse) with
        | some (next, expect) => parseTokens rest next expect
        | none => none
      else
        none
  | _, _, _ => none

/-- An exchange document is the code points of its text. -/
abbrev ExchangeCodes := List Nat

def codesOf (text : String) : ExchangeCodes := text.toList.map Char.toNat

/-- A document is exactly one value; trailing text, duplicate object keys and documents over
the transport limit are rejected. -/
def parseCodes (text : ExchangeCodes) : Option WireValue :=
  if maxDocumentBytes < text.length then none
  else (lex text .outside []).bind fun tokens => parseTokens tokens [] (.value false)

def parseDocument (text : String) : Option WireValue := parseCodes (codesOf text)

namespace WireValue

def field? (fields : List (String × WireValue)) (name : String) : Option WireValue :=
  (fields.find? fun field => field.1 == name).map Prod.snd

/-- An object with exactly the allowed keys, no more and no fewer. Duplicates were already
rejected by the parser. -/
def closedObject? (allowed : List String) : WireValue → Option (List (String × WireValue))
  | object fields =>
      if fields.length == allowed.length && fields.all (fun field => allowed.contains field.1)
          && allowed.all (fun name => (field? fields name).isSome) then
        some fields
      else
        none
  | _ => none

def text? : WireValue → Option String
  | string value => some value
  | _ => none

def items? : WireValue → Option (List WireValue)
  | array values => some values
  | _ => none

end WireValue

theorem parse_decodes_escapes :
    (parseDocument "\"a\\nb\\u0041\"").bind WireValue.text? = some "a\nbA" := by decide

theorem parse_accepts_a_closed_object :
    ((parseDocument " {\"id\" : \"x\" , \"rows\" : [ \"y\" , \"z\" ] } ").bind
      (·.closedObject? ["id", "rows"])).isSome = true := by decide

theorem parse_rejects_duplicate_keys :
    (parseDocument "{\"a\":\"1\",\"a\":\"2\"}").isNone = true ∧
      (parseDocument "{\"a\":{\"x\":\"1\",\"x\":\"2\"}}").isNone = true := by
  constructor <;> decide

theorem parse_rejects_non_protocol_atoms :
    (parseDocument "null").isNone = true ∧ (parseDocument "true").isNone = true ∧ (parseDocument "42").isNone = true ∧
      (parseDocument "[null]").isNone = true ∧ (parseDocument "1.5").isNone = true := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide

theorem parse_rejects_malformed_text :
    (parseDocument "\"a\" \"b\"").isNone = true ∧ (parseDocument "[\"a\",]").isNone = true ∧
      (parseDocument "{\"a\":\"1\",}").isNone = true ∧ (parseDocument "\"\\q\"").isNone = true ∧
      (parseDocument "\"\\ud800\"").isNone = true ∧ (parseDocument "\"a").isNone = true := by
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_⟩ <;> decide

theorem closed_object_rejects_missing_and_unknown_fields :
    ((parseDocument "{\"a\":\"1\"}").bind (·.closedObject? ["a", "b"])).isNone = true ∧
      ((parseDocument "{\"a\":\"1\",\"c\":\"2\"}").bind
        (·.closedObject? ["a", "b"])).isNone = true := by
  constructor <;> decide

end Xsh.Conformance.Transport
