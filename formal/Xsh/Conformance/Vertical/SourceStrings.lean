import Xsh.Conformance.Vertical.SourceClassification

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

/-! ## Spans, parse failures, and the declared string envelope

`Span` is a half-open byte interval `[start, stop)`. `ParseFailure` names the
three lexical failures the classifier decides. `InEnvelope` is the declared
string envelope `E` of the admission boundary: valid UTF-8, no comment byte, no
other quoted-literal prefix immediately before a quote, and no triple quote.
Within `E` every `"` outside a string opens an ordinary single-line string. -/

structure Span where
  start : Nat
  stop : Nat
  deriving DecidableEq, Repr

def Span.length (span : Span) : Nat := span.stop - span.start

inductive ParseFailure where
  | invalidUtf8 (span : Span)
  | invalidEscape (span : Span)
  | unterminatedString (span : Span)
  deriving DecidableEq, Repr

def QuoteByte : UInt8 := 34

def BackslashByte : UInt8 := 92

def LineFeedByte : UInt8 := 10

def CarriageReturnByte : UInt8 := 13

def CommentByte : UInt8 := 35

/-- `window` occurs in `bytes`. -/
def Infixed (window : List UInt8) : List UInt8 → Bool
  | [] => window.isEmpty
  | byte :: rest => window.isPrefixOf (byte :: rest) || Infixed window rest

/-- True when the bytes before a quote name another quoted literal form: `r"`, `b"`, `f"`,
`fp"`, `p"`, `rx"`, `e"`. `previous2` and `previous1` are the two bytes before the quote. -/
def ExcludedBefore (previous2 previous1 : Option UInt8) : Bool :=
  previous1 = some 114 || previous1 = some 98 || previous1 = some 102 ||
    previous1 = some 112 || previous1 = some 101 ||
    (previous2 = some 102 && previous1 = some 112) ||
    (previous2 = some 114 && previous1 = some 120)

def AvoidsQuotedPrefixes : Option UInt8 → Option UInt8 → List UInt8 → Bool
  | _, _, [] => true
  | previous2, previous1, byte :: rest =>
      (if byte = QuoteByte then !ExcludedBefore previous2 previous1 else true) &&
        AvoidsQuotedPrefixes previous1 (some byte) rest

def AvoidsTripleQuote : List UInt8 → Bool
  | first :: second :: third :: rest =>
      !(first = QuoteByte && second = QuoteByte && third = QuoteByte) &&
        AvoidsTripleQuote (second :: third :: rest)
  | _ => true

def AvoidsComment (bytes : List UInt8) : Bool := bytes.all (fun byte => byte != CommentByte)

/-- The declared string envelope `E`. -/
def InEnvelope (bytes : List UInt8) : Bool :=
  utf8ValidB bytes && AvoidsComment bytes && AvoidsQuotedPrefixes none none bytes &&
    AvoidsTripleQuote bytes

theorem envelope_parts {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    utf8ValidB bytes = true ∧ AvoidsComment bytes = true ∧
      AvoidsQuotedPrefixes none none bytes = true ∧ AvoidsTripleQuote bytes = true := by
  unfold InEnvelope at envelope
  simp only [Bool.and_eq_true] at envelope
  exact ⟨envelope.1.1.1, envelope.1.1.2, envelope.1.2, envelope.2⟩

theorem envelope_valid {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    ValidUtf8 bytes :=
  (valid_iff_valid_b bytes).mp (envelope_parts envelope).1

theorem envelope_no_comment {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    AvoidsComment bytes = true :=
  (envelope_parts envelope).2.1

theorem envelope_no_prefixes {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    AvoidsQuotedPrefixes none none bytes = true :=
  (envelope_parts envelope).2.2.1

theorem envelope_no_triple {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    AvoidsTripleQuote bytes = true :=
  (envelope_parts envelope).2.2.2

/-! ## Witnesses for the envelope and the UTF-8 decision -/

theorem byte_255_has_no_valid_prefix :
    firstInvalidUtf8 [255] = some 0 := by
  decide +kernel

theorem overlong_c0_80_has_no_valid_prefix :
    firstInvalidUtf8 [0xC0, 0x80] = some 0 := by
  decide +kernel

theorem surrogate_ed_a0_80_has_no_valid_prefix :
    firstInvalidUtf8 [0xED, 0xA0, 0x80] = some 0 := by
  decide +kernel

theorem comment_only_outside_envelope :
    InEnvelope (witnessBytes "# a comment\n") = false := by
  decide +kernel

theorem triple_quote_outside_envelope :
    InEnvelope (witnessBytes "\"\"\"text\"\"\"") = false := by
  decide +kernel

theorem raw_prefix_outside_envelope :
    InEnvelope (witnessBytes "r\"text\"") = false := by
  decide +kernel

theorem plain_literal_in_envelope :
    InEnvelope (witnessBytes "\"hello\"") = true := by
  decide +kernel

theorem empty_in_envelope : InEnvelope [] = true := by
  decide +kernel

/-! ## Escape forms -/

def isHexByte (byte : UInt8) : Bool :=
  (48 ≤ byte.toNat && byte.toNat ≤ 57) || (65 ≤ byte.toNat && byte.toNat ≤ 70) ||
    (97 ≤ byte.toNat && byte.toNat ≤ 102)

def hexDigitValue (byte : UInt8) : Nat :=
  if byte.toNat ≤ 57 then byte.toNat - 48
  else if byte.toNat ≤ 70 then byte.toNat - 55
  else byte.toNat - 87

def hexValue (digits : List UInt8) : Nat :=
  digits.foldl (fun value digit => value * 16 + hexDigitValue digit) 0

/-- A Unicode scalar value: at most U+10FFFF and outside the surrogate range. -/
def isScalarValue (value : Nat) : Bool :=
  value ≤ 0x10FFFF && !(0xD800 ≤ value && value ≤ 0xDFFF)

/-- The one-byte escapes: `\\`, `\"`, `\$`, `\n`, `\r`, `\t`, `\0`. -/
def isSimpleEscape (byte : UInt8) : Bool :=
  byte = 92 || byte = 34 || byte = 36 || byte = 110 || byte = 114 || byte = 116 || byte = 48

inductive EscapeCheck where
  | valid (consumed : Nat)
  | invalid (consumed : Nat)
  deriving DecidableEq, Repr

def EscapeCheck.isValid : EscapeCheck → Bool
  | .valid _ => true
  | .invalid _ => false

def EscapeCheck.consumed : EscapeCheck → Nat
  | .valid k => k
  | .invalid k => k

/-- The `\x` form, given the bytes after the `x`. -/
def hexCheck (rest : List UInt8) : EscapeCheck :=
  match rest with
  | first :: second :: _ =>
      if isHexByte first && isHexByte second then .valid 3 else .invalid 1
  | _ => .invalid 1

/-- The `\u{` form, given the bytes after the `{`. A present closing brace is part of the
span even when the digits are rejected, matching the discarded delimiter. -/
def unicodeCheck (after : List UInt8) : EscapeCheck :=
  let digits := after.takeWhile isHexByte
  let rest := after.drop digits.length
  match rest with
  | 125 :: _ =>
      if digits ≠ [] && isScalarValue (hexValue digits) then .valid (digits.length + 3)
      else .invalid (digits.length + 3)
  | _ => .invalid (digits.length + 2)

/-- `rest` starts immediately after a backslash. -/
def classifyEscape : List UInt8 → EscapeCheck
  | [] => .invalid 0
  | byte :: rest =>
      if isSimpleEscape byte then .valid 1
      else if byte = 120 then hexCheck rest
      else if byte = 117 then
        match rest with
        | 123 :: after => unicodeCheck after
        | _ => .invalid 1
      else .invalid 1

/-- Bytes consumed after a backslash by the escape's syntax, matching the scanner's skip: one
byte for every form except `\u{...}`, which spans its digits and any closing brace. -/
def escapeSyntax : List UInt8 → Nat
  | 117 :: 123 :: after =>
      let digits := after.takeWhile isHexByte
      match after.drop digits.length with
      | 125 :: _ => digits.length + 3
      | _ => digits.length + 2
  | _ => 1

/-- The result of scanning one string body from the byte after its opening quote. -/
inductive BodyResult where
  | closed (after : List UInt8)
  | unclosed
  deriving DecidableEq, Repr

/-- Scan one single-line body. A line feed, a carriage return, the end of input, or a lone
trailing backslash ends an open body; otherwise the first unescaped quote closes it. -/
def scanBody : List UInt8 → BodyResult
  | [] => .unclosed
  | 34 :: after => .closed after
  | 10 :: _ => .unclosed
  | 13 :: _ => .unclosed
  | 92 :: rest => scanBody (rest.drop (escapeSyntax rest))
  | _ :: rest => scanBody rest
termination_by bytes => bytes.length
decreasing_by
  · simp only [List.length_cons, List.length_drop]; omega
  · simp

theorem scan_body_eq (bytes : List UInt8) :
    scanBody bytes =
      (match bytes with
      | [] => .unclosed
      | 34 :: after => .closed after
      | 10 :: _ => .unclosed
      | 13 :: _ => .unclosed
      | 92 :: rest => scanBody (rest.drop (escapeSyntax rest))
      | _ :: rest => scanBody rest) := by
  rw [scanBody.eq_def]

/-- The declarative reading of one string body. `closed after` means an unescaped quote ends
it with `after` following; `open` means a line feed, a carriage return, the end of input, or a
lone trailing backslash comes first. -/
inductive BodyScan : List UInt8 → BodyResult → Prop where
  | closed (after : List UInt8) : BodyScan (34 :: after) (.closed after)
  | openNil : BodyScan [] .unclosed
  | openLine (rest : List UInt8) : BodyScan (10 :: rest) .unclosed
  | openCarriage (rest : List UInt8) : BodyScan (13 :: rest) .unclosed
  | escape {rest : List UInt8} {after : BodyResult} :
      BodyScan (rest.drop (escapeSyntax rest)) after → BodyScan (92 :: rest) after
  | text {byte : UInt8} {rest : List UInt8} {after : BodyResult} :
      byte ≠ 34 → byte ≠ 92 → byte ≠ 10 → byte ≠ 13 →
      BodyScan rest after → BodyScan (byte :: rest) after
theorem scan_body_nil : scanBody [] = .unclosed := by rw [scan_body_eq]

theorem scan_body_quote (after : List UInt8) : scanBody (34 :: after) = .closed after := by
  rw [scan_body_eq]; simp only

theorem scan_body_line (rest : List UInt8) : scanBody (10 :: rest) = .unclosed := by
  rw [scan_body_eq]; simp only

theorem scan_body_carriage (rest : List UInt8) : scanBody (13 :: rest) = .unclosed := by
  rw [scan_body_eq]; simp only

theorem scan_body_escape (rest : List UInt8) :
    scanBody (92 :: rest) = scanBody (rest.drop (escapeSyntax rest)) := by
  rw [scan_body_eq]; simp only

theorem scan_body_text (byte : UInt8) (rest : List UInt8) (notQuote : byte ≠ 34)
    (notBackslash : byte ≠ 92) (notLine : byte ≠ 10) (notCarriage : byte ≠ 13) :
    scanBody (byte :: rest) = scanBody rest := by
  rw [scan_body_eq]
  simp only [notQuote, notBackslash, notLine, notCarriage]

theorem body_scan_sound {bytes : List UInt8} {outcome : BodyResult}
    (derivation : BodyScan bytes outcome) : scanBody bytes = outcome := by
  induction derivation with
  | closed => rw [scan_body_quote]
  | openNil => rw [scan_body_nil]
  | openLine => rw [scan_body_line]
  | openCarriage => rw [scan_body_carriage]
  | escape inner ih => rw [scan_body_escape]; exact ih
  | text notQuote notBackslash notLine notCarriage inner ih =>
      rw [scan_body_text _ _ notQuote notBackslash notLine notCarriage]; exact ih

/-- The scanner's own outcome always has a declarative derivation. -/
theorem body_scan_self (bytes : List UInt8) : BodyScan bytes (scanBody bytes) := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n →
    BodyScan bytes (scanBody bytes)
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len
      cases bytes with
      | nil => rw [scan_body_nil]; exact .openNil
      | cons byte rest =>
        by_cases quote : byte = 34
        · subst quote; rw [scan_body_quote]; exact .closed rest
        · by_cases line : byte = 10
          · subst line; rw [scan_body_line]; exact .openLine rest
          · by_cases carriage : byte = 13
            · subst carriage; rw [scan_body_carriage]; exact .openCarriage rest
            · by_cases backslash : byte = 92
              · subst backslash
                rw [scan_body_escape]
                have dropShorter :
                    (rest.drop (escapeSyntax rest)).length < n := by
                  simp only [List.length_cons] at len ⊢
                  have := List.length_drop (l := rest) (i := escapeSyntax rest)
                  omega
                exact .escape (ih _ dropShorter _ rfl)
              · rw [scan_body_text byte rest quote backslash line carriage]
                have shorter : rest.length < n := by
                  simp only [List.length_cons] at len; omega
                exact .text quote backslash line carriage (ih _ shorter _ rfl)
  exact key bytes.length bytes rfl

theorem body_scan_complete {bytes : List UInt8} {outcome : BodyResult}
    (scanned : scanBody bytes = outcome) : BodyScan bytes outcome := by
  rw [← scanned]
  exact body_scan_self bytes

theorem body_scan_iff (bytes : List UInt8) (outcome : BodyResult) :
    scanBody bytes = outcome ↔ BodyScan bytes outcome :=
  ⟨body_scan_complete, body_scan_sound⟩

/-! ## Escape decoding and the string scan

Two passes match the Rust lexer: `scanBody` finds the closing quote by
skipping escape syntax, then `decodeEscapes` walks the body and reports the
first invalid escape. A later invalid escape does not mask an earlier one. -/


/-- A body that closes leaves strictly fewer bytes than the body: the closing quote is one of
them. -/
theorem closed_drop_shorter (bytes after : List UInt8) (closed : scanBody bytes = .closed after) :
    after.length < bytes.length := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n →
    ∀ after, scanBody bytes = .closed after → after.length < bytes.length
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len after closed
      cases bytes with
      | nil => rw [scan_body_nil] at closed; cases closed
      | cons byte rest =>
        by_cases quote : byte = 34
        · subst quote; rw [scan_body_quote] at closed
          cases closed
          simp only [List.length_cons]; omega
        · by_cases line : byte = 10
          · subst line; rw [scan_body_line] at closed; cases closed
          · by_cases carriage : byte = 13
            · subst carriage; rw [scan_body_carriage] at closed; cases closed
            · by_cases backslash : byte = 92
              · subst backslash
                rw [scan_body_escape] at closed
                have dropShorter : (rest.drop (escapeSyntax rest)).length < n := by
                  simp only [List.length_cons] at len ⊢
                  have := List.length_drop (l := rest) (i := escapeSyntax rest)
                  omega
                have inner := ih _ dropShorter _ rfl after closed
                simp only [List.length_cons]
                have := List.length_drop (l := rest) (i := escapeSyntax rest)
                omega
              · rw [scan_body_text byte rest quote backslash line carriage] at closed
                have shorter : rest.length < n := by
                  simp only [List.length_cons] at len; omega
                have inner := ih _ shorter _ rfl after closed
                simp only [List.length_cons]
                omega
  exact key bytes.length bytes rfl after closed

/-- The first invalid escape in `content`, from absolute offset `offset`. -/
def decodeEscapes (content : List UInt8) (offset : Nat) : Option Span :=
  match content with
  | [] => none
  | 92 :: rest =>
      match classifyEscape rest with
      | .valid k => decodeEscapes (rest.drop k) (offset + 1 + k)
      | .invalid k => some ⟨offset, offset + 1 + k⟩
  | _ :: rest => decodeEscapes rest (offset + 1)
termination_by content.length
decreasing_by
  · simp only [List.length_cons, List.length_drop]; omega
  · simp

theorem decode_escapes_eq (content : List UInt8) (offset : Nat) :
    decodeEscapes content offset =
      (match content with
      | [] => none
      | 92 :: rest =>
          match classifyEscape rest with
          | .valid k => decodeEscapes (rest.drop k) (offset + 1 + k)
          | .invalid k => some ⟨offset, offset + 1 + k⟩
      | _ :: rest => decodeEscapes rest (offset + 1)) := by
  rw [decodeEscapes.eq_def]

theorem decode_escapes_nil (offset : Nat) : decodeEscapes [] offset = none := by
  rw [decode_escapes_eq]

/-- The result of scanning a whole byte string for its first string error. -/
inductive StringResult where
  | clean
  | failed (failure : ParseFailure)
  deriving DecidableEq, Repr

/-- Scan a whole byte string, from absolute offset `offset`, for the first string error. -/
def scanStrings (offset : Nat) : List UInt8 → StringResult
  | [] => .clean
  | 34 :: rest =>
      match hbody : scanBody rest with
      | .unclosed => .failed (.unterminatedString ⟨offset, offset + rest.length + 1⟩)
      | .closed after =>
          let content := rest.take (rest.length - after.length)
          match decodeEscapes content (offset + 1) with
          | some span => .failed (.invalidEscape span)
          | none => scanStrings (offset + (rest.length + 1 - after.length)) after
  | _ :: rest => scanStrings (offset + 1) rest
termination_by bytes => bytes.length
decreasing_by
  · have bound := closed_drop_shorter rest after hbody
    simp only [List.length_cons]; omega
  · simp

end SourceAdmission

end Xsh.Conformance.Vertical
