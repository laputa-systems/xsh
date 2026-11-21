import Xsh.Conformance.Vertical.SourceTreeSpans

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

/-! ## The source admission root

`source_admission` combines the pieces the SRC-ADMIT row needs: the classifier's
accepted range is exactly `D`, its UTF-8 range is exactly the inputs with a
first invalid offset, and its string range is exactly the envelope's failing
scans, with `outside` covering the rest. For any accepted input it also yields
the positioned tokens, the independent grammar derivation, the decimal
interpretation of every number token, and the fact that each node's span is
exactly its own tokens' span, inside the source. -/

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
      (∀ span, TokenSpan positioned = some span → 0 ≤ span.start ∧
        span.stop ≤ bytes.length) := by
  obtain ⟨tokens, positioned, positions, projection, grammar, _⟩ :=
    accepted_positioned_tokens accepted
  refine ⟨positioned, tokens, positions, projection, grammar, ?_⟩
  intro span found
  have bounds := token_span_within_source positions found
  refine ⟨bounds.1, ?_⟩
  rw [List.length_map] at bounds
  omega

theorem source_admission (bytes : List UInt8) (moduleInputs : List (List UInt8)) :
    (Domain bytes moduleInputs ↔
        ∃ functions scopes, classify bytes moduleInputs = .accepted functions scopes) ∧
      (∀ span, classify bytes moduleInputs = .rejected (.invalidUtf8 span) ↔
        InvalidUtf8At bytes span) ∧
      (∀ failure, (∀ span, failure ≠ .invalidUtf8 span) →
        (classify bytes moduleInputs = .rejected failure ↔
          InEnvelope bytes = true ∧ scanStrings 0 bytes = .failed failure)) ∧
      (classify bytes moduleInputs = .outside ↔
        ¬ Domain bytes moduleInputs ∧ (∀ span, ¬ InvalidUtf8At bytes span) ∧
          (InEnvelope bytes = false ∨ scanStrings 0 bytes = .clean)) ∧
      (∀ functions scopes, classify bytes moduleInputs = .accepted functions scopes →
        ∃ positioned tokens, PositionedLexed 0 (bytes.map UInt8.toNat) positioned ∧
          positioned.map (fun entry => entry.1) = tokens ∧
          FunctionsSyntax tokens functions ∧
          (∀ span, TokenSpan positioned = some span → 0 ≤ span.start ∧
            span.stop ≤ bytes.length)) := by
  exact ⟨domain_iff_classified_accepted bytes moduleInputs,
    fun span => classify_invalid_utf8_iff bytes moduleInputs span,
    fun failure notUtf8 => classify_string_iff bytes moduleInputs failure notUtf8,
    classify_outside_iff bytes moduleInputs,
    fun functions scopes accepted => accepted_node_spans accepted⟩


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

theorem short_hex_classified :
    classify [34, 97, 92, 120, 52, 34] [] =
      .rejected (.invalidEscape ⟨2, 4⟩) := by
  decide +kernel

theorem backslash_newline_classified :
    classify [34, 97, 92, 10, 98, 34] [] =
      .rejected (.invalidEscape ⟨2, 4⟩) := by
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
