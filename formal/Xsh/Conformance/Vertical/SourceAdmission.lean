import Xsh.Conformance.Vertical.SourceTreeSpans

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

/-! ## The source admission root

`source_admission` combines the pieces the SRC-ADMIT row needs: the classifier's
accepted range is exactly `D`, its UTF-8 range is exactly the spans where a
valid prefix ends and no well-formed sequence starts, and its string range is
stated through the scanner's relations `DeclaredString` and `EscapeError`, with
`outside` covering the rest. For any accepted input it also yields the scope
projection, the positioned tokens, the independent grammar derivation, the
decimal interpretation of every number token, and the fact that each node's
span is exactly its own tokens' span, inside the source. The string relations
are the Lean scanner's relational reading, not an independent SPEC reading. -/

theorem accepted_positioned_tokens {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : classify bytes moduleInputs = .accepted functions scopes) :
    ∃ tokens positioned, PositionedLexed 0 (bytes.map UInt8.toNat) positioned ∧
      positioned.map (fun entry => entry.1) = tokens ∧ FunctionsSyntax tokens functions ∧
      ScopesProjection functions scopes := by
  have domain := ((classify_accepted_iff _ _ _ _).mp accepted).1
  obtain ⟨foundFunctions, foundScopes, declared⟩ := domain
  have same : classify bytes moduleInputs = .accepted foundFunctions foundScopes :=
    classify_accepted_branch _ _ _ _ (domain_first_invalid_none ⟨_, _, declared⟩)
      (reconstruct_complete declared)
  rw [same] at accepted
  cases accepted
  have checkAccepted := declarative_domain_complete declared
  obtain ⟨tokens, positioned, positions, projection, grammar, projected⟩ :=
    check_has_token_positions checkAccepted
  exact ⟨tokens, positioned, positions, projection, grammar, projected⟩

/-- An accepted input's declared domain, with the very functions and scopes the classifier
named. -/
theorem accepted_domain {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : classify bytes moduleInputs = .accepted functions scopes) :
    DeclarativeDomain bytes moduleInputs functions scopes := by
  obtain ⟨domain, reconstructed⟩ := (classify_accepted_iff _ _ _ _).mp accepted
  obtain ⟨foundFunctions, foundScopes, declared⟩ := domain
  have same : (foundFunctions, foundScopes) = (functions, scopes) :=
    Option.some.inj ((reconstruct_complete declared).symm.trans reconstructed)
  cases same
  exact declared

theorem accepted_decimal_interpretation {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : classify bytes moduleInputs = .accepted functions scopes) :
    ∀ (positioned : List (Token × Nat × Nat)),
      PositionedLexed 0 (bytes.map UInt8.toNat) positioned →
      (∀ value start stop, (Token.number value, start, stop) ∈ positioned →
        ∃ before digits rest, bytes.map UInt8.toNat = before ++ digits ++ rest ∧
          value = digitsValue digits ∧ value ≤ maxIntLiteral) := by
  intro positioned positions value start stop member
  obtain ⟨before, digits, rest, same, _, _, _, _, _, valueSame⟩ :=
    positioned_decimal_interpretation positions member
  exact ⟨before, digits, rest, same, valueSame,
    guarded_positioned_decimal_bounded positions
      (accepted_decimal_runs_in_range (declarative_domain_complete
        (accepted_domain accepted))) member⟩

theorem accepted_node_spans {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : classify bytes moduleInputs = .accepted functions scopes) :
    ∃ positioned tokens, PositionedLexed 0 (bytes.map UInt8.toNat) positioned ∧
      positioned.map (fun entry => entry.1) = tokens ∧
      FunctionsSyntax tokens functions ∧
      (∀ span, TokenSpan positioned = some span → span.stop ≤ bytes.length) := by
  obtain ⟨tokens, positioned, positions, projection, grammar, _⟩ :=
    accepted_positioned_tokens accepted
  refine ⟨positioned, tokens, positions, projection, grammar, ?_⟩
  intro span found
  have bounds := token_span_within_source positions found
  rw [List.length_map] at bounds
  simpa using bounds.2

/-- An accepted input's scopes are the projection of the functions the classifier names. -/
theorem accepted_scope_projection {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : classify bytes moduleInputs = .accepted functions scopes) :
    ScopesProjection functions scopes := by
  obtain ⟨_, _, _, _, _, projected⟩ := accepted_positioned_tokens accepted
  exact projected

theorem source_admission (bytes : List UInt8) (moduleInputs : List (List UInt8)) :
    (Domain bytes moduleInputs ↔
        ∃ functions scopes, classify bytes moduleInputs = .accepted functions scopes) ∧
      (∀ span, classify bytes moduleInputs = .rejected (.invalidUtf8 span) ↔
        InvalidUtf8At bytes span) ∧
      (∀ failure, (∀ span, failure ≠ .invalidUtf8 span) →
        (classify bytes moduleInputs = .rejected failure ↔
          InEnvelope bytes = true ∧ DeclaredString bytes 0 (.failed failure))) ∧
      (classify bytes moduleInputs = .outside ↔
        ¬ Domain bytes moduleInputs ∧ (∀ span, ¬ InvalidUtf8At bytes span) ∧
          (InEnvelope bytes = false ∨ DeclaredString bytes 0 .clean)) ∧
      (∀ functions scopes, classify bytes moduleInputs = .accepted functions scopes →
        ScopesProjection functions scopes ∧
          ∃ positioned tokens, PositionedLexed 0 (bytes.map UInt8.toNat) positioned ∧
            positioned.map (fun entry => entry.1) = tokens ∧
            FunctionsSyntax tokens functions ∧
            (∀ span, TokenSpan positioned = some span → span.stop ≤ bytes.length)) := by
  refine ⟨domain_iff_classified_accepted bytes moduleInputs,
    fun span => classify_invalid_utf8_iff bytes moduleInputs span, ?_, ?_,
    fun functions scopes accepted =>
      ⟨accepted_scope_projection accepted, accepted_node_spans accepted⟩⟩
  · intro failure notUtf8
    rw [classify_string_iff bytes moduleInputs failure notUtf8]
    exact ⟨fun found => ⟨found.1, (declared_string_iff bytes 0 (.failed failure)).mp found.2⟩,
      fun found => ⟨found.1, (declared_string_iff bytes 0 (.failed failure)).mpr found.2⟩⟩
  · rw [classify_outside_iff bytes moduleInputs]
    refine ⟨fun outside => ⟨outside.1, outside.2.1, ?_⟩,
      fun outside => ⟨outside.1, outside.2.1, ?_⟩⟩
    · rcases outside.2.2 with envelope | clean
      · exact Or.inl envelope
      · exact Or.inr ((declared_string_iff bytes 0 .clean).mp clean)
    · rcases outside.2.2 with envelope | clean
      · exact Or.inl envelope
      · exact Or.inr ((declared_string_iff bytes 0 .clean).mpr clean)


/-! ## Classifier witnesses

These fix the outcomes the acceptance criteria name. A domain program is
accepted; malformed UTF-8 is rejected at its first invalid offset; a lone quote,
a trailing backslash, and an invalid escape are rejected as the SPEC says.
Because `classify` takes no candidate tree, a mismatched tree cannot make it a
parse failure: acceptance is decided by the bytes alone. -/

theorem arithmetic_classified_accepted :
    classify arithmeticBytes [] = .accepted arithmeticTree [[("x", 0)]] := by
  decide +kernel

theorem byte_255_classified : classify [255] [] = .rejected (.invalidUtf8 ⟨0, 1⟩) := by
  decide +kernel

theorem overlong_classified : classify [0xC0, 0x80] [] = .rejected (.invalidUtf8 ⟨0, 2⟩) := by
  decide +kernel

theorem surrogate_classified :
    classify [0xED, 0xA0, 0x80] [] = .rejected (.invalidUtf8 ⟨0, 3⟩) := by
  decide +kernel

/-- A classifier variant whose UTF-8 rejection always names offset zero. -/
def classifyUtf8AtZero (bytes : List UInt8) (moduleInputs : List (List UInt8)) : Classification :=
  match firstInvalidUtf8 bytes with
  | some _ => .rejected (.invalidUtf8 ⟨0, bytes.length⟩)
  | none => classify bytes moduleInputs

def utf8OffsetTwoBytes : List UInt8 := [0x61, 0x62, 0xFF, 0x63]

/-- `[0x61,0x62,0xFF,0x63]` is rejected at offset 2: the two ASCII bytes are a valid prefix
and `0xFF` starts no well-formed sequence. -/
theorem utf8_offset_two_classified :
    classify utf8OffsetTwoBytes [] = .rejected (.invalidUtf8 ⟨2, 4⟩) := by
  decide +kernel

theorem utf8_offset_two_invalid : InvalidUtf8At utf8OffsetTwoBytes ⟨2, 4⟩ :=
  (classify_invalid_utf8_iff _ _ _).mp utf8_offset_two_classified

theorem utf8_offset_two_least :
    ∀ other, InvalidUtf8At utf8OffsetTwoBytes other → 2 ≤ other.start := by
  have found : firstInvalidUtf8 utf8OffsetTwoBytes = some 2 := by decide +kernel
  exact (invalid_utf8_least (span := ⟨2, utf8OffsetTwoBytes.length⟩) found rfl).2

theorem utf8_offset_zero_not_invalid : ¬ InvalidUtf8At utf8OffsetTwoBytes ⟨0, 4⟩ := by
  intro wrong
  exact wrong.2.2.1 (.ascii (byte := 0x61) (rest := [0x62, 0xFF, 0x63]) (by decide))

/-- The always-offset-zero mutant is refuted by the witness: it reports a span the
declarative predicate rejects. -/
theorem mutant_utf8_zero_refuted :
    classifyUtf8AtZero utf8OffsetTwoBytes [] = .rejected (.invalidUtf8 ⟨0, 4⟩) ∧
      ¬ InvalidUtf8At utf8OffsetTwoBytes ⟨0, 4⟩ :=
  ⟨by decide +kernel, utf8_offset_zero_not_invalid⟩

theorem mutant_utf8_zero_disagrees :
    classifyUtf8AtZero utf8OffsetTwoBytes [] ≠ classify utf8OffsetTwoBytes [] := by
  rw [utf8_offset_two_classified]
  decide +kernel

theorem lone_quote_classified :
    classify [34] [] = .rejected (.unterminatedString ⟨0, 1⟩) := by
  decide +kernel

theorem backslash_end_classified :
    classify [34, 97, 92] [] = .rejected (.unterminatedString ⟨0, 3⟩) := by
  decide +kernel

/-- An unterminated string's span ends at the line end, not at the end of input. -/
theorem unterminated_line_classified :
    classify (witnessBytes "let a = \"abc\nlet b = 1\n") [] =
      .rejected (.unterminatedString ⟨8, 12⟩) := by
  decide +kernel

theorem invalid_escape_classified :
    classify [34, 97, 92, 113, 98, 34] [] =
      .rejected (.invalidEscape ⟨2, 4⟩) := by
  decide +kernel

/-- A later string error reports the offset it accumulated past an earlier clean string:
`"a" "c\q"` fails at offset 6. -/
theorem error_after_clean_string_classified :
    classify [34, 97, 34, 32, 34, 99, 92, 113, 34] [] =
      .rejected (.invalidEscape ⟨6, 8⟩) := by
  decide +kernel

theorem short_hex_classified :
    classify [34, 97, 92, 120, 52, 34] [] =
      .rejected (.invalidEscape ⟨2, 4⟩) := by
  decide +kernel

theorem backslash_newline_classified :
    classify [34, 97, 92, 10, 98, 34] [] =
      .rejected (.invalidEscape ⟨2, 4⟩) := by
  decide +kernel

/-- SPEC 2.6 makes a backslash before a line end an invalid escape; for `\r\n` the span
covers the backslash and both line-end bytes. -/
theorem backslash_crlf_classified :
    classify [34, 97, 92, 13, 10, 98, 34] [] =
      .rejected (.invalidEscape ⟨2, 5⟩) := by
  decide +kernel

theorem comment_only_classified_outside :
    classify (witnessBytes "# a comment\n") [] = .outside := by
  decide +kernel

theorem triple_quote_classified_outside :
    classify (witnessBytes "\"\"\"text\"\"\"") [] = .outside := by
  decide +kernel

theorem valid_literal_classified_outside :
    classify (witnessBytes "\"hello\"") [] = .outside := by
  decide +kernel

/-- Interpolation inside a command word is SPEC-valid source, so the classifier does not
reject it. The narrowed envelope places it outside `E`. -/
theorem command_interpolation_not_rejected :
    classify (witnessBytes "print \"${\"\\\"\"}\"") [] = .outside := by
  decide +kernel

/-- A glob string prefix is not an ordinary single-line string, so the classifier does not
reject it as one. -/
theorem glob_string_not_rejected :
    classify (witnessBytes "g\"*.txt\"") [] = .outside := by
  decide +kernel

/-- A mismatched candidate tree never makes the classifier a parse failure: the classifier
ignores candidate trees and decides only on bytes, so a different tree yields the same
outcome. -/
theorem mismatched_tree_is_not_a_parse_failure :
    classify arithmeticBytes [] = .accepted arithmeticTree [[("x", 0)]] ∧
      classify (witnessBytes "pure f(x: Int) -> Int {\n  return x - 1\n}\n") [] ≠
        .accepted arithmeticTree [[("x", 0)]] := by
  refine ⟨arithmetic_classified_accepted, ?_⟩
  decide +kernel

end SourceAdmission

end Xsh.Conformance.Vertical
