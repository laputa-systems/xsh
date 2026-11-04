import Xsh.Conformance.Transport.Json
namespace Xsh.Conformance.Transport

def lexPrefix : List Nat → LexMode → List Token → Option (LexMode × List Token)
  | [], mode, tokens => some (mode, tokens)
  | code :: rest, .outside, tokens =>
      if code == 34 then lexPrefix rest (.inString []) tokens
      else if code == 123 then lexPrefix rest .outside (.leftBrace :: tokens)
      else if code == 125 then lexPrefix rest .outside (.rightBrace :: tokens)
      else if code == 91 then lexPrefix rest .outside (.leftBracket :: tokens)
      else if code == 93 then lexPrefix rest .outside (.rightBracket :: tokens)
      else if code == 44 then lexPrefix rest .outside (.comma :: tokens)
      else if code == 58 then lexPrefix rest .outside (.colon :: tokens)
      else if code == 32 || code == 9 || code == 10 || code == 13 then lexPrefix rest .outside tokens
      else none
  | code :: rest, .inString accumulated, tokens =>
      if code == 34 then lexPrefix rest .outside (.string (String.ofList accumulated.reverse) :: tokens)
      else if code == 92 then lexPrefix rest (.afterBackslash accumulated) tokens
      else if code < 32 || 127 < code then none
      else lexPrefix rest (.inString (Char.ofNat code :: accumulated)) tokens
  | code :: rest, .afterBackslash accumulated, tokens =>
      if code == 117 then lexPrefix rest (.unicodeEscape accumulated 0 0) tokens
      else match simpleEscape? code with
        | some escaped => lexPrefix rest (.inString (escaped :: accumulated)) tokens
        | none => none
  | character :: rest, .unicodeEscape accumulated digits code, tokens =>
      match jsonHexValue? character with
      | none => none
      | some digit =>
        let extended := code * 16 + digit
        if digits + 1 == 4 then
          if isScalarCode extended then lexPrefix rest (.inString (Char.ofNat extended :: accumulated)) tokens
          else none
        else lexPrefix rest (.unicodeEscape accumulated (digits + 1) extended) tokens

theorem lex_append_prefix (front suffix : List Nat) (mode : LexMode) (tokens : List Token) :
    lex (front ++ suffix) mode tokens =
      (lexPrefix front mode tokens).bind (fun (next, accumulated) => lex suffix next accumulated) := by
  induction front generalizing mode tokens with
  | nil => simp [lexPrefix]
  | cons code rest ih =>
    cases mode <;> simp only [List.cons_append, lex, lexPrefix]
    all_goals repeat' first | split | simp_all [ih]

theorem lexPrefix_append (front suffix : List Nat) (mode : LexMode) (tokens : List Token) :
    lexPrefix (front ++ suffix) mode tokens =
      (lexPrefix front mode tokens).bind (fun (next, accumulated) => lexPrefix suffix next accumulated) := by
  induction front generalizing mode tokens with
  | nil => simp [lexPrefix]
  | cons code rest ih =>
    cases mode <;> simp only [List.cons_append, lexPrefix]
    all_goals repeat' first | split | simp_all [ih]

def plainJsonCode (code : Nat) : Bool :=
  decide (32 ≤ code ∧ code ≤ 127 ∧ code ≠ 34 ∧ code ≠ 92)

theorem lexPrefix_plain_string (codes : List Nat) (accumulated : List Char) (tokens : List Token)
    (plain : codes.all plainJsonCode = true) :
    lexPrefix codes (.inString accumulated) tokens =
      some (.inString ((codes.map Char.ofNat).reverse ++ accumulated), tokens) := by
  induction codes generalizing accumulated with
  | nil => simp [lexPrefix]
  | cons code rest ih =>
    simp only [List.all_cons, Bool.and_eq_true, plainJsonCode, decide_eq_true_eq] at plain
    have ⟨bounds, upper, quote, slash⟩ := plain.1
    simp [lexPrefix, quote, slash, Nat.not_lt.mpr bounds, Nat.not_lt.mpr upper, ih _ plain.2,
      List.reverse_cons, List.append_assoc]

theorem lexPrefix_plain_quoted (codes : List Nat) (tokens : List Token)
    (plain : codes.all plainJsonCode = true) :
    lexPrefix ([34] ++ codes ++ [34]) .outside tokens =
      some (.outside, .string (String.ofList (codes.map Char.ofNat)) :: tokens) := by
  change lexPrefix (34 :: (codes ++ [34])) .outside tokens = _
  simp only [lexPrefix, beq_self_eq_true, ↓reduceIte]
  rw [lexPrefix_append, lexPrefix_plain_string codes [] tokens plain]
  simp [lexPrefix]

inductive PlainLexeme where
  | leftBrace | rightBrace | leftBracket | rightBracket | comma | colon
  | string (codes : List Nat)

def PlainLexeme.codes : PlainLexeme → List Nat
  | .leftBrace => [123]
  | .rightBrace => [125]
  | .leftBracket => [91]
  | .rightBracket => [93]
  | .comma => [44]
  | .colon => [58]
  | .string codes => [34] ++ codes ++ [34]

def PlainLexeme.token : PlainLexeme → Token
  | .leftBrace => .leftBrace
  | .rightBrace => .rightBrace
  | .leftBracket => .leftBracket
  | .rightBracket => .rightBracket
  | .comma => .comma
  | .colon => .colon
  | .string codes => .string (String.ofList (codes.map Char.ofNat))

def PlainLexeme.valid : PlainLexeme → Bool
  | .string codes => codes.all plainJsonCode
  | _ => true

theorem lexPrefix_plain_lexeme (lexeme : PlainLexeme) (tokens : List Token)
    (valid : lexeme.valid = true) :
    lexPrefix lexeme.codes .outside tokens = some (.outside, lexeme.token :: tokens) := by
  cases lexeme with
  | string codes => exact lexPrefix_plain_quoted codes tokens valid
  | _ => rfl

theorem lexPrefix_plain_lexemes (lexemes : List PlainLexeme) (tokens : List Token)
    (valid : lexemes.all PlainLexeme.valid = true) :
    lexPrefix (lexemes.flatMap PlainLexeme.codes) .outside tokens =
      some (.outside, (lexemes.map PlainLexeme.token).reverse ++ tokens) := by
  induction lexemes generalizing tokens with
  | nil => simp [lexPrefix]
  | cons lexeme rest ih =>
    simp only [List.all_cons, Bool.and_eq_true] at valid
    simp only [List.flatMap_cons, lexPrefix_append, lexPrefix_plain_lexeme lexeme tokens valid.1,
      Option.bind_some]
    rw [ih _ valid.2]
    simp [List.reverse_cons, List.append_assoc]

end Xsh.Conformance.Transport
