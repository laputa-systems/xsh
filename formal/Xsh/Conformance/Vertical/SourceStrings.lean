import Xsh.Conformance.Vertical.SourceClassification

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

/-! ## Spans, parse failures, and the declared string envelope

`Span` is a half-open byte interval `[start, stop)`. `ParseFailure` names the
three lexical failures the classifier decides. `InEnvelope` is the declared
string envelope `E` of the admission boundary: valid UTF-8, no comment byte, no
other quoted-literal prefix immediately before a quote, no triple quote, no
`${` or `$(` interpolation marker, and no `'` byte. `E` admits only sources
whose quotes the Lean scanner reads as ordinary strings; it makes no claim
about interpolation, command words, or glob strings. -/

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
`fp"`, `g"`, `p"`, `rx"`, `e"`. `previous2` and `previous1` are the two bytes before the
quote. -/
def ExcludedBefore (previous2 previous1 : Option UInt8) : Bool :=
  previous1 = some 114 || previous1 = some 98 || previous1 = some 102 ||
    previous1 = some 103 || previous1 = some 112 || previous1 = some 101 ||
    (previous2 = some 102 && previous1 = some 112) ||
    (previous2 = some 114 && previous1 = some 120)

/-- Walk the bytes in string state, checking each quote that *opens* an ordinary string:
inside a string an escape consumes its next byte and an unescaped quote closes it, so a
prefixed opener is caught only where it would open a string. The boolean is whether a string
is open, and the previous bytes are tracked for the opener's prefix. -/
def AvoidsQuotedPrefixes (open2 : Option UInt8) (open1 : Option UInt8) (inString : Bool) :
    List UInt8 → Bool
  | [] => true
  | byte :: rest =>
      if inString then
        if byte = BackslashByte then
          match rest with
          | [] => true
          | _ :: more => AvoidsQuotedPrefixes none none true more
        else if byte = QuoteByte then
          AvoidsQuotedPrefixes none none false rest
        else AvoidsQuotedPrefixes open2 open1 true rest
      else if byte = QuoteByte then
        !ExcludedBefore open2 open1 && AvoidsQuotedPrefixes none none true rest
      else AvoidsQuotedPrefixes open1 (some byte) false rest

def AvoidsTripleQuote : List UInt8 → Bool
  | first :: second :: third :: rest =>
      !(first = QuoteByte && second = QuoteByte && third = QuoteByte) &&
        AvoidsTripleQuote (second :: third :: rest)
  | _ => true

def AvoidsComment (bytes : List UInt8) : Bool := bytes.all (fun byte => byte != CommentByte)

/-- True when no `${` or `$(` interpolation opener occurs. -/
def AvoidsInterpolation (bytes : List UInt8) : Bool :=
  !(Infixed [36, 123] bytes) && !(Infixed [36, 40] bytes)

/-- True when no `'` byte occurs. -/
def AvoidsApostrophe (bytes : List UInt8) : Bool := bytes.all (fun byte => byte != 39)

/-- The declared string envelope `E`. -/
def InEnvelope (bytes : List UInt8) : Bool :=
  utf8ValidB bytes && AvoidsComment bytes && AvoidsQuotedPrefixes none none false bytes &&
    AvoidsTripleQuote bytes && AvoidsInterpolation bytes && AvoidsApostrophe bytes

theorem envelope_parts {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    utf8ValidB bytes = true ∧ AvoidsComment bytes = true ∧
      AvoidsQuotedPrefixes none none false bytes = true ∧ AvoidsTripleQuote bytes = true ∧
      AvoidsInterpolation bytes = true ∧ AvoidsApostrophe bytes = true := by
  unfold InEnvelope at envelope
  simp only [Bool.and_eq_true] at envelope
  rcases envelope with ⟨⟨⟨⟨⟨valid, comment⟩, prefixes⟩, triple⟩, interpolation⟩, apostrophe⟩
  exact ⟨valid, comment, prefixes, triple, interpolation, apostrophe⟩

theorem envelope_valid {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    ValidUtf8 bytes :=
  (valid_iff_valid_b bytes).mp (envelope_parts envelope).1

theorem envelope_no_comment {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    AvoidsComment bytes = true :=
  (envelope_parts envelope).2.1

theorem envelope_no_prefixes {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    AvoidsQuotedPrefixes none none false bytes = true :=
  (envelope_parts envelope).2.2.1

theorem envelope_no_triple {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    AvoidsTripleQuote bytes = true :=
  (envelope_parts envelope).2.2.2.1

theorem envelope_no_interpolation {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    AvoidsInterpolation bytes = true :=
  (envelope_parts envelope).2.2.2.2.1

theorem envelope_no_apostrophe {bytes : List UInt8} (envelope : InEnvelope bytes = true) :
    AvoidsApostrophe bytes = true :=
  (envelope_parts envelope).2.2.2.2.2

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

theorem interpolation_outside_envelope :
    InEnvelope (witnessBytes "print \"${\"\\\"\"}\"") = false := by
  decide +kernel

theorem command_substitution_outside_envelope :
    InEnvelope (witnessBytes "print $(date)") = false := by
  decide +kernel

theorem apostrophe_outside_envelope :
    InEnvelope (witnessBytes "print 'x'") = false := by
  decide +kernel

theorem glob_prefix_outside_envelope :
    InEnvelope (witnessBytes "g\"*.txt\"") = false := by
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
  simp only

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

theorem decode_escapes_skip (rest : List UInt8) (offset k : Nat)
    (valid : classifyEscape rest = .valid k) :
    decodeEscapes (92 :: rest) offset = decodeEscapes (rest.drop k) (offset + 1 + k) := by
  rw [decode_escapes_eq]; simp only [valid]

theorem decode_escapes_here (rest : List UInt8) (offset k : Nat)
    (invalid : classifyEscape rest = .invalid k) :
    decodeEscapes (92 :: rest) offset = some ⟨offset, offset + 1 + k⟩ := by
  rw [decode_escapes_eq]; simp only [invalid]

theorem decode_escapes_past (byte : UInt8) (rest : List UInt8) (offset : Nat)
    (notBackslash : byte ≠ 92) :
    decodeEscapes (byte :: rest) offset = decodeEscapes rest (offset + 1) := by
  rw [decode_escapes_eq]; simp only

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

/-- The first invalid escape in `content`, from absolute offset `offset`, stated declaratively.
An escape's validity is decided by `classifyEscape`; this relation only fixes which one is
first and the span the SPEC names: the backslash through the escape's syntax. -/
inductive EscapeError : List UInt8 → Nat → Span → Prop where
  | here {offset : Nat} {rest : List UInt8} {k : Nat} :
      classifyEscape rest = .invalid k → EscapeError (92 :: rest) offset ⟨offset, offset + 1 + k⟩
  | skip {offset : Nat} {rest : List UInt8} {k : Nat} {span : Span} :
      classifyEscape rest = .valid k → EscapeError (rest.drop k) (offset + 1 + k) span →
      EscapeError (92 :: rest) offset span
  | past {offset : Nat} {byte : UInt8} {rest : List UInt8} {span : Span} :
      byte ≠ 92 → EscapeError rest (offset + 1) span → EscapeError (byte :: rest) offset span

theorem escape_error_sound {content : List UInt8} {offset : Nat} {span : Span}
    (derivation : EscapeError content offset span) : decodeEscapes content offset = some span := by
  induction derivation with
  | here invalid => exact decode_escapes_here _ _ _ invalid
  | skip valid inner ih => rw [decode_escapes_skip _ _ _ valid]; exact ih
  | past notBackslash inner ih => rw [decode_escapes_past _ _ _ notBackslash]; exact ih

theorem escape_error_complete (content : List UInt8) (offset : Nat) (span : Span)
    (found : decodeEscapes content offset = some span) : EscapeError content offset span := by
  let motive (n : Nat) : Prop := ∀ content : List UInt8, content.length = n →
    ∀ offset span, decodeEscapes content offset = some span → EscapeError content offset span
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro content len offset span found
      cases content with
      | nil => rw [decode_escapes_nil] at found; cases found
      | cons byte rest =>
        by_cases backslash : byte = 92
        · subst backslash
          cases check : classifyEscape rest with
          | valid k =>
            rw [decode_escapes_skip rest offset k check] at found
            have dropShorter : (rest.drop k).length < n := by
              simp only [List.length_cons] at len ⊢
              have := List.length_drop (l := rest) (i := k)
              omega
            exact .skip check (ih _ dropShorter _ rfl _ _ found)
          | invalid k =>
            rw [decode_escapes_here rest offset k check] at found
            cases found
            exact .here check
        · rw [decode_escapes_past byte rest offset backslash] at found
          have shorter : rest.length < n := by simp only [List.length_cons] at len; omega
          exact .past backslash (ih _ shorter _ rfl _ _ found)
  exact key content.length content rfl offset span found

theorem decode_escapes_iff (content : List UInt8) (offset : Nat) (span : Span) :
    decodeEscapes content offset = some span ↔ EscapeError content offset span :=
  ⟨escape_error_complete content offset span, escape_error_sound⟩

theorem decode_escapes_none_iff (content : List UInt8) (offset : Nat) :
    decodeEscapes content offset = none ↔ ∀ span, ¬ EscapeError content offset span := by
  constructor
  · intro none span found
    have someEq := (decode_escapes_iff content offset span).mpr found
    rw [none] at someEq
    cases someEq
  · intro absent
    cases found : decodeEscapes content offset with
    | none => rfl
    | some span => exact absurd ((decode_escapes_iff content offset span).mp found) (absent span)

/-- The declarative reading of the whole-source string scan, in the SPEC's three cases over
the declared envelope. `unterminated`'s span runs from the opening quote to the end of input;
`invalid`'s span is the escape; otherwise the scan continues after the closed string. -/
inductive DeclaredString : List UInt8 → Nat → StringResult → Prop where
  | cleanNil (offset : Nat) : DeclaredString [] offset .clean
  | outsideText {offset : Nat} {byte : UInt8} {rest : List UInt8} {result : StringResult} :
      byte ≠ 34 → DeclaredString rest (offset + 1) result →
      DeclaredString (byte :: rest) offset result
  | unterminated {offset : Nat} {rest : List UInt8} :
      BodyScan rest .unclosed →
      DeclaredString (34 :: rest) offset
        (.failed (.unterminatedString ⟨offset, offset + rest.length + 1⟩))
  | invalid {offset : Nat} {rest after : List UInt8} {span : Span} :
      BodyScan rest (.closed after) →
      EscapeError (rest.take (rest.length - after.length)) (offset + 1) span →
      DeclaredString (34 :: rest) offset (.failed (.invalidEscape span))
  | closedClean {offset : Nat} {rest after : List UInt8} {result : StringResult} :
      BodyScan rest (.closed after) →
      (∀ span, ¬ EscapeError (rest.take (rest.length - after.length)) (offset + 1) span) →
      DeclaredString after (offset + (rest.length + 1 - after.length)) result →
      DeclaredString (34 :: rest) offset result

theorem scan_strings_nil (offset : Nat) : scanStrings offset [] = .clean := by
  rw [scanStrings.eq_def]

theorem scan_strings_text (byte : UInt8) (rest : List UInt8) (offset : Nat)
    (notQuote : byte ≠ 34) :
    scanStrings offset (byte :: rest) = scanStrings (offset + 1) rest := by
  rw [scanStrings.eq_def]
  simp only

theorem scan_strings_unterminated (rest : List UInt8) (offset : Nat)
    (unclosed : scanBody rest = .unclosed) :
    scanStrings offset (34 :: rest) =
      .failed (.unterminatedString ⟨offset, offset + rest.length + 1⟩) := by
  conv => lhs; rw [scanStrings.eq_def]
  simp only
  split
  · rfl
  · rename_i after hclosed
    rw [unclosed] at hclosed
    cases hclosed

theorem scan_strings_invalid (rest after : List UInt8) (offset : Nat) (span : Span)
    (closed : scanBody rest = .closed after)
    (invalid : decodeEscapes (rest.take (rest.length - after.length)) (offset + 1) = some span) :
    scanStrings offset (34 :: rest) = .failed (.invalidEscape span) := by
  conv => lhs; rw [scanStrings.eq_def]
  simp only
  split
  · rename_i hopen
    rw [closed] at hopen
    cases hopen
  · rename_i found hfound
    rw [closed] at hfound
    cases hfound
    simp only [invalid]

theorem scan_strings_clean (rest after : List UInt8) (offset : Nat)
    (closed : scanBody rest = .closed after)
    (clean : decodeEscapes (rest.take (rest.length - after.length)) (offset + 1) = none) :
    scanStrings offset (34 :: rest) =
      scanStrings (offset + (rest.length + 1 - after.length)) after := by
  conv => lhs; rw [scanStrings.eq_def]
  simp only
  split
  · rename_i hopen
    rw [closed] at hopen
    cases hopen
  · rename_i found hfound
    rw [closed] at hfound
    cases hfound
    simp only [clean]

theorem declared_string_sound {bytes : List UInt8} {offset : Nat} {result : StringResult}
    (derivation : DeclaredString bytes offset result) : scanStrings offset bytes = result := by
  induction derivation with
  | cleanNil => exact scan_strings_nil _
  | outsideText notQuote inner ih => rw [scan_strings_text _ _ _ notQuote]; exact ih
  | unterminated body =>
    exact scan_strings_unterminated _ _ ((body_scan_iff _ _).mpr body)
  | invalid body escape =>
    exact scan_strings_invalid _ _ _ _
      ((body_scan_iff _ _).mpr body)
      ((decode_escapes_iff _ _ _).mpr escape)
  | closedClean body absent inner ih =>
    rw [scan_strings_clean _ _ _ ((body_scan_iff _ _).mpr body)
      ((decode_escapes_none_iff _ _).mpr absent)]
    exact ih

theorem declared_string_self (bytes : List UInt8) (offset : Nat) :
    DeclaredString bytes offset (scanStrings offset bytes) := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n →
    ∀ offset, DeclaredString bytes offset (scanStrings offset bytes)
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len offset
      cases bytes with
      | nil => rw [scan_strings_nil]; exact .cleanNil offset
      | cons byte rest =>
        by_cases quote : byte = 34
        · subst quote
          cases body : scanBody rest with
          | unclosed =>
            rw [scan_strings_unterminated rest offset body]
            exact .unterminated ((body_scan_iff _ _).mp body)
          | closed after =>
            cases escape : decodeEscapes (rest.take (rest.length - after.length)) (offset + 1) with
            | some span =>
              rw [scan_strings_invalid rest after offset span body escape]
              exact .invalid ((body_scan_iff _ _).mp body)
                ((decode_escapes_iff _ _ _).mp escape)
            | none =>
              rw [scan_strings_clean rest after offset body escape]
              have shorter : after.length < n := by
                simp only [List.length_cons] at len
                have bound := closed_drop_shorter rest after body
                omega
              exact .closedClean ((body_scan_iff _ _).mp body)
                ((decode_escapes_none_iff _ _).mp escape)
                (ih after.length shorter after rfl (offset + (rest.length + 1 - after.length)))
        · rw [scan_strings_text byte rest offset quote]
          have shorter : rest.length < n := by simp only [List.length_cons] at len; omega
          exact .outsideText quote (ih rest.length shorter rest rfl (offset + 1))
  exact key bytes.length bytes rfl offset

theorem declared_string_iff (bytes : List UInt8) (offset : Nat) (result : StringResult) :
    scanStrings offset bytes = result ↔ DeclaredString bytes offset result := by
  constructor
  · intro same
    rw [← same]
    exact declared_string_self bytes offset
  · exact declared_string_sound


/-! ## String witnesses -/

theorem lone_quote_unterminated :
    scanStrings 0 [34] = .failed (.unterminatedString ⟨0, 1⟩) := by
  decide +kernel

theorem lone_quote_unterminated_span :
    DeclaredString [34] 0 (.failed (.unterminatedString ⟨0, 1⟩)) :=
  (declared_string_iff _ _ _).mp lone_quote_unterminated

theorem escaped_quote_scan_clean :
    scanStrings 0 [34, 97, 92, 34, 98, 34] = .clean := by
  decide +kernel

theorem invalid_escape_witness :
    scanStrings 0 [34, 97, 92, 113, 98, 34] =
      .failed (.invalidEscape ⟨2, 4⟩) := by
  decide +kernel

theorem short_hex_escape_witness :
    scanStrings 0 [34, 97, 92, 120, 52, 34] =
      .failed (.invalidEscape ⟨2, 4⟩) := by
  decide +kernel

theorem backslash_before_line_feed_witness :
    scanStrings 0 [34, 97, 92, 10, 98, 34] =
      .failed (.invalidEscape ⟨2, 4⟩) := by
  decide +kernel

theorem unterminated_after_backslash :
    scanStrings 0 [34, 97, 92] =
      .failed (.unterminatedString ⟨0, 3⟩) := by
  decide +kernel

theorem unicode_out_of_range_witness :
    scanStrings 0 [34, 92, 117, 123, 49, 49, 48, 48, 48, 48, 125, 34] =
      .failed (.invalidEscape ⟨1, 11⟩) := by
  decide +kernel

theorem surrogate_unicode_witness :
    scanStrings 0 [34, 92, 117, 123, 68, 56, 48, 48, 125, 34] =
      .failed (.invalidEscape ⟨1, 9⟩) := by
  decide +kernel

theorem valid_unicode_escape_scan_clean :
    scanStrings 0 [34, 92, 117, 123, 52, 56, 125, 34] = .clean := by
  decide +kernel

end SourceAdmission

end Xsh.Conformance.Vertical
