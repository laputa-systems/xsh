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

end SourceAdmission

end Xsh.Conformance.Vertical
