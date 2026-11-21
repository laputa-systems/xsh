import Xsh.Conformance.Vertical.SourceAdmissionBase
import Xsh.Conformance.Vertical.LoweringSound

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

theorem no_conflicting_adjacency_tail {head : Nat} {rest : List Nat}
    (safe : noConflictingAdjacency (head :: rest) = true) : noConflictingAdjacency rest = true := by
  cases rest with
  | nil => rfl
  | cons next tail =>
    simp only [noConflictingAdjacency, Bool.and_eq_true] at safe
    exact safe.2

theorem no_conflicting_adjacency_suffix (before rest : List Nat)
    (safe : noConflictingAdjacency (before ++ rest) = true) : noConflictingAdjacency rest = true := by
  induction before with
  | nil => exact safe
  | cons head tail ih =>
    apply ih
    exact no_conflicting_adjacency_tail safe

theorem no_word_hyphen_in_guarded_source {codes before after : List Nat} {head : Nat}
    (safe : noConflictingAdjacency codes = true)
    (location : codes = before ++ head :: 45 :: after) : isWordPartCode head = false := by
  subst codes
  have suffix := no_conflicting_adjacency_suffix before (head :: 45 :: after) safe
  simp only [noConflictingAdjacency, Bool.and_eq_true, Bool.not_eq_true, beq_self_eq_true,
    Bool.and_true] at suffix
  simpa using suffix.1.1.1.1.1.1.1

inductive PositionedLexed : Nat → List Nat → List (Token × Nat × Nat) → Prop where
  | empty (offset : Nat) : PositionedLexed offset [] []
  | space {offset : Nat} {codes : List Nat} {tokens : List (Token × Nat × Nat)} :
      PositionedLexed (offset + 1) codes tokens → PositionedLexed offset (32 :: codes) tokens
  | token {offset : Nat} {leading rest : List Nat} {token : Token}
      {tokens : List (Token × Nat × Nat)} :
      Lexeme rest leading token → PositionedLexed (offset + leading.length) rest tokens →
      PositionedLexed offset (leading ++ rest) ((token, offset, offset + leading.length) :: tokens)

theorem lexed_has_positions {codes : List Nat} {tokens : List Token}
    (lexed : Lexed codes tokens) (offset : Nat) :
    ∃ positioned, PositionedLexed offset codes positioned ∧
      positioned.map (fun entry => entry.1) = tokens := by
  induction lexed generalizing offset with
  | empty => exact ⟨[], .empty offset, rfl⟩
  | space lexical ih =>
    obtain ⟨positioned, positions, projection⟩ := ih (offset + 1)
    exact ⟨positioned, .space positions, projection⟩
  | token lexical tail ih =>
    obtain ⟨positioned, positions, projection⟩ := ih (offset + _)
    exact ⟨_ :: positioned, .token lexical positions, by simp [projection]⟩

theorem positioned_spans_bounded {offset : Nat} {codes : List Nat}
    {tokens : List (Token × Nat × Nat)} (lexed : PositionedLexed offset codes tokens) :
    ∀ token start stop, (token, start, stop) ∈ tokens →
      offset ≤ start ∧ start ≤ stop ∧ stop ≤ offset + codes.length := by
  induction lexed with
  | empty => simp
  | space lexed ih =>
    intro token start stop member
    have bounds := ih token start stop member
    simp only [List.length_cons]
    omega
  | @token offset leading rest head tokens lexical tail ih =>
    intro token start stop member
    simp only [List.mem_cons, Prod.mk.injEq] at member
    rcases member with ⟨rfl, rfl, rfl⟩ | member
    · simp only [List.length_append]
      omega
    · have bounds := ih token start stop member
      simp only [List.length_append]
      omega

theorem positioned_span_lexeme {offset : Nat} {codes : List Nat}
    {tokens : List (Token × Nat × Nat)} (lexed : PositionedLexed offset codes tokens) :
    ∀ token start stop, (token, start, stop) ∈ tokens →
      ∃ before leading rest, codes = before ++ leading ++ rest ∧
        start = offset + before.length ∧ stop = start + leading.length ∧ Lexeme rest leading token := by
  induction lexed with
  | empty => simp
  | @space offset codes tokens lexical ih =>
    intro token start stop member
    obtain ⟨before, leading, rest, same, first, last, lexeme⟩ := ih token start stop member
    refine ⟨32 :: before, leading, rest, ?_, ?_, last, lexeme⟩
    · simp [same]
    · simp only [List.length_cons]
      omega
  | @token offset leading rest head tokens lexical tail ih =>
    intro token start stop member
    simp only [List.mem_cons, Prod.mk.injEq] at member
    rcases member with ⟨rfl, rfl, rfl⟩ | member
    · exact ⟨[], leading, rest, by simp, by simp, rfl, lexical⟩
    · obtain ⟨before, tokenBytes, remaining, same, first, last, lexeme⟩ :=
        ih token start stop member
      refine ⟨leading ++ before, tokenBytes, remaining, ?_, ?_, last, lexeme⟩
      · simp [same, List.append_assoc]
      · simp only [List.length_append]
        omega

theorem positioned_decimal_interpretation {offset : Nat} {codes : List Nat}
    {tokens : List (Token × Nat × Nat)} (lexed : PositionedLexed offset codes tokens)
    {value start stop : Nat} (member : (Token.number value, start, stop) ∈ tokens) :
    ∃ before digits rest, codes = before ++ digits ++ rest ∧
      start = offset + before.length ∧ stop = start + digits.length ∧
      digits ≠ [] ∧ digits.all isDigitCode = true ∧ Boundary isDigitCode rest ∧
      value = digitsValue digits := by
  obtain ⟨before, digits, rest, same, first, last, lexeme⟩ :=
    positioned_span_lexeme lexed (.number value) start stop member
  cases lexeme with
  | punctuation punctuation maximal => simp [Punctuation] at punctuation
  | decimal nonempty digitCodes boundary =>
    exact ⟨before, digits, rest, same, first, last, nonempty, digitCodes, boundary, rfl⟩

theorem ascii_byte_valid_utf8 (byte : UInt8) (ascii : byte.toNat < 128) :
    [byte].toByteArray.IsValidUTF8 := by
  let c := Char.ofNat byte.toNat
  have valid : byte.toNat.isValidChar := by
    left
    omega
  have bound : byte.toNat < UInt32.size := by
    simp [UInt32.size]
    omega
  have value : c.val = UInt32.ofNatLT byte.toNat bound := by
    simp only [c, Char.ofNat, dif_pos valid, Char.ofNatAux, UInt32.ofNatLT]
  have size : c.utf8Size = 1 := by
    apply Char.utf8Size_eq_one_iff.mpr
    rw [value, UInt32.le_iff_toNat_le, UInt32.toNat_ofNatLT]
    simp
    omega
  have encoded := Char.isValidUTF8_toByteArray_utf8EncodeChar (c := c)
  rw [String.utf8EncodeChar_eq_singleton size, value, UInt32.toUInt8_ofNatLT,
    UInt8.ofNat_toNat] at encoded
  exact encoded

theorem ascii_bytes_valid_utf8 (bytes : List UInt8)
    (ascii : ∀ byte, byte ∈ bytes → byte.toNat < 128) : bytes.toByteArray.IsValidUTF8 := by
  induction bytes with
  | nil => simpa using ByteArray.isValidUTF8_empty
  | cons byte rest ih =>
    have first := ascii_byte_valid_utf8 byte (ascii byte (by simp))
    have tail := ih (by intro value member; exact ascii value (by simp [member]))
    simpa [← List.toByteArray_append] using first.append tail

theorem check_has_token_positions {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    ∃ tokens positioned, PositionedLexed 0 (bytes.map UInt8.toNat) positioned ∧
      positioned.map (fun entry => entry.1) = tokens ∧ FunctionsSyntax tokens functions ∧
      ScopesProjection functions scopes := by
  have sound := check_sound accepted
  cases sound.2.2.1 with
  | admitted tokens lexical grammar earlier recordCalls allRecordCalls =>
    obtain ⟨positioned, positions, projection⟩ := lexed_has_positions lexical 0
    exact ⟨tokens, positioned, positions, projection, grammar, sound.2.2.2⟩

theorem check_has_valid_utf8 {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    bytes.toByteArray.IsValidUTF8 := by
  apply ascii_bytes_valid_utf8
  have admitted := (check_sound accepted).2.2.1
  cases admitted with
  | admitted tokens lexical grammar earlier recordCalls allRecordCalls =>
    intro byte member
    exact lexed_ascii lexical byte.toNat (List.mem_map.mpr ⟨byte, member, rfl⟩)

theorem check_sound_with_positions {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    moduleInputs = [] ∧ bytes.toByteArray.IsValidUTF8 ∧
      (functions.map (·.name)).Nodup ∧
      AdmittedSource (bytes.map UInt8.toNat) functions ∧ ScopesProjection functions scopes ∧
      ∃ tokens positioned, PositionedLexed 0 (bytes.map UInt8.toNat) positioned ∧
        positioned.map (fun entry => entry.1) = tokens ∧ FunctionsSyntax tokens functions := by
  have sound := check_sound accepted
  obtain ⟨tokens, positioned, positions, projection, grammar, _⟩ :=
    check_has_token_positions accepted
  exact ⟨sound.1, check_has_valid_utf8 accepted, sound.2.1, sound.2.2.1, sound.2.2.2,
    tokens, positioned, positions, projection, grammar⟩

/-- The source admission boundary composes with the existing universal lowerer theorem. The
artifact's entire function meaning is the meaning of the tree derived from these exact bytes. -/
theorem admitted_lowering_means_source {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))} {artifact : RawArtifact}
    (accepted : check bytes moduleInputs functions scopes = true)
    (lowered : lowersTo artifact functions = true) :
    moduleInputs = [] ∧ AdmittedSource (bytes.map UInt8.toNat) functions ∧
      ScopesProjection functions scopes ∧ functionPrograms artifact = sourcePrograms functions := by
  have admission := check_sound accepted
  exact ⟨admission.1, admission.2.2.1, admission.2.2.2, lowers_to_means_the_source lowered⟩

/-- A certified source/IR pair starts at the original code bytes: its grammar derivation is
independent of the compiler's tree, and its IR meaning follows from the universal lowerer proof.
Source-document decoding and extraction from a Rust parser remain separate boundaries. -/
theorem certified_codes_admitted {sourceId : Xsh.Conformance.Transport.BridgeId} {codes : List Nat}
    {artifact : RawArtifact} {results : List ObservedResult}
    (certified : certifyCodes sourceId codes artifact = some results) :
    ∃ functions, AdmittedSource codes functions ∧
      functionPrograms artifact = sourcePrograms functions ∧
      artifact.sourceId = sourceId ∧ validateArtifact artifact = true ∧
      namedResults artifact = some results := by
  unfold certifyCodes at certified
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff] at certified
  rcases certified with ⟨functions, parsed, certified⟩
  split at certified
  · rename_i checks
    simp only [Bool.and_eq_true, decide_eq_true_eq] at checks
    exact ⟨functions, parse_source_admitted parsed, lowers_to_means_the_source checks.1.2,
      checks.1.1, checks.2, certified⟩
  · cases certified

theorem admitted_bytes_are_ascii {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    {functions : List SourceFunction} {scopes : List (List (String × Nat))}
    (accepted : check bytes moduleInputs functions scopes = true) :
    ∀ byte, byte ∈ bytes → byte.toNat < 128 := by
  have admission := (check_sound accepted).2.2.1
  cases admission with
  | admitted tokens lexical productions earlier recordCalls allRecordCalls =>
    intro byte member
    exact lexed_ascii lexical byte.toNat (List.mem_map.mpr ⟨byte, member, rfl⟩)

def arithmeticTree : List SourceFunction :=
  [⟨"f", ["x"], [], none, .value (.binary .add (.var "x") (.int 1))⟩]

def arithmeticBytes : List UInt8 := witnessBytes "pure f(x: Int) -> Int {\n  return x + 1\n}\n"

theorem arithmetic_bytes_admitted :
    check arithmeticBytes [] arithmeticTree [[("x", 0)]] = true := by
  decide +kernel

theorem assignment_bytes_admitted :
    check (witnessBytes "pure f() -> Int {\n  var x = 1\n  x = x + 2\n  return x\n}\n") []
      [⟨"f", [], [("x", 1)], none, .assigned "x" (.binary .add (.var "x") (.int 2))
        (.atom (.var "x"))⟩] [[("x", 1)]] = true := by
  decide +kernel

theorem computed_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let a = x + 1\n  let b = a * 2\n  return b\n}\n") []
      [⟨"f", ["x"], [], none,
        .computedLets [("a", .binary .add (.var "x") (.int 1)),
          ("b", .binary .mul (.var "a") (.int 2))] (.atom (.var "b"))⟩]
      [[("x", 0), ("a", 2), ("b", 3)]] = true := by
  decide +kernel

theorem boolean_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Bool {\n  return x > 1\n}\n") []
      [⟨"f", ["x"], [], none, .comparison .gt (.var "x") (.int 1)⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem result_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Result[Int, Int] {\n  return Err(x)\n}\n") []
      [⟨"f", ["x"], [], none, .constructed false (.atom (.var "x"))⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem propagation_bytes_admitted :
    check (witnessBytes ("pure g(x: Int) -> Result[Int, Int] {\n  return Ok(x)\n}\n" ++
        "pure f(x: Int) -> Result[Int, Int] {\n  let y = g(x)?\n  return Ok(y)\n}\n")) []
      [⟨"g", ["x"], [], none, .constructed true (.atom (.var "x"))⟩,
        ⟨"f", ["x"], [], some ⟨"y", "g", [.var "x"]⟩, .constructed true (.atom (.var "y"))⟩]
      [[("x", 0)], [("x", 0), ("y", 2)]] = true := by
  decide +kernel

theorem conditional_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return if x > 0 { x + 1 } else { 0 }\n}\n") []
      [⟨"f", ["x"], [], none, .conditional .gt (.var "x") (.int 0)
        (.binary .add (.var "x") (.int 1)) (.atom (.int 0))⟩] [[("x", 0)]] = true := by
  decide +kernel

theorem call_bytes_admitted :
    check (witnessBytes ("pure g(x: Int) -> Int {\n  return x + 1\n}\n" ++
        "pure f(y: Int) -> Int {\n  return g(y)\n}\n")) []
      [⟨"g", ["x"], [], none, .value (.binary .add (.var "x") (.int 1))⟩,
        ⟨"f", ["y"], [], none, .call "g" [.var "y"]⟩] [[("x", 0)], [("y", 0)]] = true := by
  decide +kernel

theorem single_computed_bytes_admitted :
    check (witnessBytes "pure f(x: Int) -> Int {\n  let y = x + 1\n  return y\n}\n") []
      [⟨"f", ["x"], [], none, .computedLet "y" (.binary .add (.var "x") (.int 1))
        (.atom (.var "y"))⟩] [[("x", 0), ("y", 2)]] = true := by
  decide +kernel

theorem admitted_byte_domain_nonempty :
    ∃ bytes functions scopes, bytes ≠ [] ∧ functions ≠ [] ∧ check bytes [] functions scopes = true := by
  exact ⟨arithmeticBytes, arithmeticTree, [[("x", 0)]], by decide +kernel, by decide +kernel,
    arithmetic_bytes_admitted⟩

theorem substituted_source_and_projection_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return x - 1\n}\n") []
      arithmeticTree [[("x", 0)]] = false ∧
    check arithmeticBytes [] arithmeticTree [[("x", 1)]] = false ∧
    check arithmeticBytes [] arithmeticTree [[("y", 0)]] = false ∧
    check arithmeticBytes [] arithmeticTree [] = false ∧
    check arithmeticBytes [arithmeticBytes] arithmeticTree [[("x", 0)]] = false := by
  refine ⟨?_, ?_, ?_, ?_, ?_⟩ <;> decide +kernel

def substitutedSourceMutant (_bytes : List UInt8) (moduleInputs : List (List UInt8))
    (functions : List SourceFunction) (scopes : List (List (String × Nat))) : Bool :=
  check arithmeticBytes moduleInputs functions scopes

theorem substituted_source_mutant_exposed :
    let other := witnessBytes "pure f(x: Int) -> Int {\n  return x - 1\n}\n"
    substitutedSourceMutant other [] arithmeticTree [[("x", 0)]] = true ∧
    check other [] arithmeticTree [[("x", 0)]] = false := by
  decide +kernel

theorem reserved_word_source_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return while\n}\n") []
      [⟨"f", ["x"], [], none, .value (.atom (.var "while"))⟩] [[("x", 0)]] = false := by
  decide +kernel

theorem adjacent_word_hyphen_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return x-1\n}\n") []
      [⟨"f", ["x"], [], none, .value (.binary .sub (.var "x") (.int 1))⟩]
      [[("x", 0)]] = false := by
  decide +kernel

theorem adjacent_error_redirect_refused :
    check (witnessBytes "pure f(E: Int) -> Bool {\n  return E>1\n}\n") []
      [⟨"f", ["E"], [], none, .comparison .gt (.var "E") (.int 1)⟩]
      [[("E", 0)]] = false := by
  decide +kernel

theorem out_of_range_decimal_refused :
    check (witnessBytes "pure f(x: Int) -> Int {\n  return 9223372036854775808\n}\n") []
      [⟨"f", ["x"], [], none, .value (.atom (.int 9223372036854775808))⟩]
      [[("x", 0)]] = false := by
  decide +kernel

theorem unsupported_trailing_bytes_refused :
    check (arithmeticBytes ++ [0]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ [64]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ [195, 169]) [] arithmeticTree [[("x", 0)]] = false ∧
    check (arithmeticBytes ++ witnessBytes "pure") [] arithmeticTree [[("x", 0)]] = false := by
  refine ⟨?_, ?_, ?_, ?_⟩ <;> decide +kernel

end SourceAdmission
end Xsh.Conformance.Vertical
