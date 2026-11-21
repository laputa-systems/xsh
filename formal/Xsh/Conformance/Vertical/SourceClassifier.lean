import Xsh.Conformance.Vertical.SourceAdmissionSound
import Xsh.Conformance.Vertical.SourceStrings

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

/-! ## The total source classifier

`classify` decides, for arbitrary bytes and module inputs, exactly one of three
declared outcomes. Precedence is fixed: a first invalid UTF-8 offset, then a
reconstruction into `D`, then the first string error inside the declared
envelope `E`. Everything else is outside, with no SPEC claim.

The accepted outcome holds exactly on `D`, the UTF-8 outcome exactly on inputs
with a first invalid offset, and the string outcomes exactly on `E` where the
string scan fails. `D` is well-formed ASCII with no quote byte, so an input
that fails the string scan is never in `D`; the ranges are disjoint. -/

inductive Classification where
  | accepted (functions : List SourceFunction) (scopes : List (List (String × Nat)))
  | rejected (failure : ParseFailure)
  | outside
  deriving DecidableEq, Repr

/-- The bytes have no well-formed UTF-8 prefix at `span.start`, and the span runs to the end. -/
def InvalidUtf8At (bytes : List UInt8) (span : Span) : Prop :=
  firstInvalidUtf8 bytes = some span.start ∧ span.stop = bytes.length

def classify (bytes : List UInt8) (moduleInputs : List (List UInt8)) : Classification :=
  match firstInvalidUtf8 bytes with
  | some offset => .rejected (.invalidUtf8 ⟨offset, bytes.length⟩)
  | none =>
      match reconstruct bytes moduleInputs with
      | some (functions, scopes) => .accepted functions scopes
      | none =>
          if InEnvelope bytes then
            match scanStrings 0 bytes with
            | .failed failure => .rejected failure
            | .clean => .outside
          else .outside

/-! ## Branch equations -/

theorem classify_invalid_branch (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (offset : Nat) (found : firstInvalidUtf8 bytes = some offset) :
    classify bytes moduleInputs = .rejected (.invalidUtf8 ⟨offset, bytes.length⟩) := by
  unfold classify
  rw [found]

theorem classify_accepted_branch (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (functions : List SourceFunction) (scopes : List (List (String × Nat)))
    (cleared : firstInvalidUtf8 bytes = none)
    (found : reconstruct bytes moduleInputs = some (functions, scopes)) :
    classify bytes moduleInputs = .accepted functions scopes := by
  unfold classify
  rw [cleared]
  simp only [found]

theorem classify_string_branch (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (failure : ParseFailure)
    (cleared : firstInvalidUtf8 bytes = none)
    (missing : reconstruct bytes moduleInputs = none)
    (envelope : InEnvelope bytes = true)
    (failed : scanStrings 0 bytes = .failed failure) :
    classify bytes moduleInputs = .rejected failure := by
  unfold classify
  rw [cleared]
  simp only [missing, ite_eq_left envelope, failed]

theorem classify_outside_clean (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (cleared : firstInvalidUtf8 bytes = none) (missing : reconstruct bytes moduleInputs = none)
    (envelope : InEnvelope bytes = true) (clean : scanStrings 0 bytes = .clean) :
    classify bytes moduleInputs = .outside := by
  unfold classify
  rw [cleared]
  simp only [missing, ite_eq_left envelope, clean]

theorem classify_outside_no_envelope (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (cleared : firstInvalidUtf8 bytes = none) (missing : reconstruct bytes moduleInputs = none)
    (envelope : InEnvelope bytes = false) :
    classify bytes moduleInputs = .outside := by
  unfold classify
  rw [cleared]
  simp only [missing, envelope, Bool.false_eq_true, ite_false]

/-! ## `D` is well-formed ASCII -/

theorem domain_ascii {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    (domain : Domain bytes moduleInputs) : ∀ byte, byte ∈ bytes → byte.toNat < 128 := by
  obtain ⟨functions, scopes, admitted⟩ := domain
  exact admitted_bytes_are_ascii (declarative_domain_complete admitted)

theorem ascii_valid_utf8_list (bytes : List UInt8)
    (ascii : ∀ byte, byte ∈ bytes → byte.toNat < 128) : ValidUtf8 bytes := by
  induction bytes with
  | nil => exact .nil
  | cons byte rest ih =>
    have here : IsAsciiByte byte := ascii byte (by simp)
    exact .one here (ih (by intro value member; exact ascii value (by simp [member])))

theorem domain_valid_utf8 {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    (domain : Domain bytes moduleInputs) : ValidUtf8 bytes :=
  ascii_valid_utf8_list bytes (domain_ascii domain)

theorem domain_first_invalid_none {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    (domain : Domain bytes moduleInputs) : firstInvalidUtf8 bytes = none :=
  (first_invalid_none_iff_valid bytes).mpr (domain_valid_utf8 domain)

/-! ## The accepted range is exactly `D` -/

theorem classify_accepted_iff (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (functions : List SourceFunction) (scopes : List (List (String × Nat))) :
    classify bytes moduleInputs = .accepted functions scopes ↔
      Domain bytes moduleInputs ∧ reconstruct bytes moduleInputs = some (functions, scopes) := by
  constructor
  · intro same
    have domain : Domain bytes moduleInputs := by
      apply Classical.byContradiction
      intro outside
      have missing : reconstruct bytes moduleInputs = none :=
        (reconstruct_none_iff_outside_domain bytes moduleInputs).mpr outside
      cases cleared : firstInvalidUtf8 bytes with
      | some offset =>
        rw [classify_invalid_branch _ _ _ cleared] at same
        cases same
      | none =>
        cases envelope : InEnvelope bytes with
        | true =>
          cases scanned : scanStrings 0 bytes with
          | failed failure =>
            rw [classify_string_branch _ _ _ cleared missing envelope scanned] at same
            cases same
          | clean =>
            rw [classify_outside_clean _ _ cleared missing envelope scanned] at same
            cases same
        | false =>
          rw [classify_outside_no_envelope _ _ cleared missing envelope] at same
          cases same
    have reconstructed : reconstruct bytes moduleInputs = some (functions, scopes) := by
      cases result : reconstruct bytes moduleInputs with
      | none =>
        exfalso
        exact ((reconstruct_none_iff_outside_domain bytes moduleInputs).mp result) domain
      | some pair =>
        rcases pair with ⟨foundFunctions, foundScopes⟩
        rw [classify_accepted_branch bytes moduleInputs foundFunctions foundScopes
          (domain_first_invalid_none domain) result] at same
        cases same
        rfl
    exact ⟨domain, reconstructed⟩
  · rintro ⟨domain, found⟩
    exact classify_accepted_branch _ _ _ _ (domain_first_invalid_none domain) found

theorem domain_iff_classified_accepted (bytes : List UInt8) (moduleInputs : List (List UInt8)) :
    Domain bytes moduleInputs ↔
      ∃ functions scopes, classify bytes moduleInputs = .accepted functions scopes := by
  constructor
  · rintro ⟨functions, scopes, domain⟩
    exact ⟨functions, scopes, (classify_accepted_iff _ _ _ _).mpr ⟨⟨functions, scopes, domain⟩,
      reconstruct_complete domain⟩⟩
  · rintro ⟨functions, scopes, same⟩
    exact ((classify_accepted_iff _ _ _ _).mp same).1


theorem lexeme_no_quote {rest leading : List Nat} {token : Token}
    (lexeme : Lexeme rest leading token) : ∀ code, code ∈ leading → code ≠ 34 := by
  cases lexeme with
  | punctuation punctuation maximal =>
    simp [Punctuation] at punctuation
    rcases punctuation with h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h | h
    all_goals
      rcases h with ⟨rfl, rfl⟩
      intro code member
      simp at member
      omega
  | decimal nonempty digits boundary =>
    intro code member
    have digit := List.all_eq_true.mp digits code member
    simp only [isDigitCode, Bool.and_eq_true, decide_eq_true_eq] at digit
    omega
  | word first letters boundary =>
    intro code member
    have letter := List.all_eq_true.mp letters code member
    simp only [isWordPartCode, isWordStartCode, isDigitCode, Bool.or_eq_true, Bool.and_eq_true,
      decide_eq_true_eq, beq_iff_eq] at letter
    omega

theorem lexed_no_quote {codes : List Nat} {tokens : List Token}
    (lexical : Lexed codes tokens) : ∀ code, code ∈ codes → code ≠ 34 := by
  induction lexical with
  | empty => simp
  | space previous ih =>
    intro code member
    simp only [List.mem_cons] at member
    rcases member with rfl | member
    · decide
    · exact ih code member
  | token lexeme previous ih =>
    intro code member
    simp only [List.mem_append] at member
    rcases member with member | member
    · exact lexeme_no_quote lexeme code member
    · exact ih code member

theorem domain_no_quote {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    (domain : Domain bytes moduleInputs) : ∀ byte, byte ∈ bytes → byte ≠ 34 := by
  obtain ⟨functions, scopes, domain⟩ := domain
  obtain ⟨_, _, _, _, _, _, tokens, lexical, _, _, _, _, _⟩ := domain
  intro byte member same
  have sameNat : byte.toNat = 34 := by rw [same]; rfl
  exact lexed_no_quote lexical byte.toNat (List.mem_map.mpr ⟨byte, member, rfl⟩) sameNat


/-- A string failure needs a quote byte, so no domain input can produce one. -/
theorem scan_strings_clean_of_no_quote {bytes : List UInt8} {offset : Nat}
    (noQuote : ∀ byte, byte ∈ bytes → byte ≠ 34) : scanStrings offset bytes = .clean := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n →
    ∀ offset, (∀ byte, byte ∈ bytes → byte ≠ 34) → scanStrings offset bytes = .clean
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len offset noQuote
      cases bytes with
      | nil => exact scan_strings_nil offset
      | cons byte rest =>
        have notQuote : byte ≠ 34 := noQuote byte (by simp)
        rw [scan_strings_text byte rest offset notQuote]
        have shorter : rest.length < n := by simp only [List.length_cons] at len; omega
        exact ih _ shorter rest rfl (offset + 1) (by
          intro value member
          exact noQuote value (by simp [member]))
  exact key bytes.length bytes rfl offset noQuote

theorem domain_scan_clean {bytes : List UInt8} {moduleInputs : List (List UInt8)}
    (domain : Domain bytes moduleInputs) : scanStrings 0 bytes = .clean :=
  scan_strings_clean_of_no_quote (domain_no_quote domain)

/-! ## The string range is exactly the envelope's failing scans -/

theorem classify_string_iff (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (failure : ParseFailure)
    (notUtf8 : ∀ span, failure ≠ .invalidUtf8 span) :
    classify bytes moduleInputs = .rejected failure ↔
      InEnvelope bytes = true ∧ scanStrings 0 bytes = .failed failure := by
  constructor
  · intro same
    have cleared : firstInvalidUtf8 bytes = none := by
      cases found : firstInvalidUtf8 bytes with
      | none => rfl
      | some offset =>
        exact absurd (classify_invalid_branch bytes moduleInputs offset found)
          (fun invalidBranch => notUtf8 ⟨offset, bytes.length⟩ (by
            rw [invalidBranch] at same
            cases same
            rfl))
    have notDomain : ¬ Domain bytes moduleInputs := by
      intro domain
      obtain ⟨functions, scopes, declared⟩ := domain
      rw [classify_accepted_branch _ _ _ _ (domain_first_invalid_none ⟨_, _, declared⟩)
        (reconstruct_complete declared)] at same
      cases same
    have missing : reconstruct bytes moduleInputs = none :=
      (reconstruct_none_iff_outside_domain bytes moduleInputs).mpr notDomain
    cases envelope : InEnvelope bytes with
    | true =>
      cases scanned : scanStrings 0 bytes with
      | failed scannedFailure =>
        rw [classify_string_branch _ _ _ cleared missing envelope scanned] at same
        cases same
        exact ⟨rfl, rfl⟩
      | clean =>
        rw [classify_outside_clean _ _ cleared missing envelope scanned] at same
        cases same
    | false =>
      rw [classify_outside_no_envelope _ _ cleared missing envelope] at same
      cases same
  · rintro ⟨envelope, scanned⟩
    have cleared : firstInvalidUtf8 bytes = none :=
      (first_invalid_none_iff_valid bytes).mpr (envelope_valid envelope)
    have missing : reconstruct bytes moduleInputs = none := by
      cases result : reconstruct bytes moduleInputs with
      | none => rfl
      | some pair =>
        exfalso
        rcases pair with ⟨functions, scopes⟩
        have domain : Domain bytes moduleInputs :=
          (reconstruct_some_iff_domain bytes moduleInputs).mp (by rw [result]; rfl)
        rw [domain_scan_clean domain] at scanned
        cases scanned
    exact classify_string_branch _ _ _ cleared missing envelope scanned

/-! ## The UTF-8 range is exactly the non-well-formed inputs -/

theorem invalid_utf8_iff (bytes : List UInt8) :
    (∃ span, InvalidUtf8At bytes span) ↔ ¬ ValidUtf8 bytes := by
  constructor
  · rintro ⟨span, found, _⟩ valid
    have cleared : firstInvalidUtf8 bytes = none :=
      (first_invalid_none_iff_valid bytes).mpr valid
    rw [cleared] at found
    cases found
  · intro invalid
    have notNone : firstInvalidUtf8 bytes ≠ none := by
      intro cleared
      exact invalid ((first_invalid_none_iff_valid bytes).mp cleared)
    cases hfound : firstInvalidUtf8 bytes with
    | none => exact absurd hfound notNone
    | some offset => exact ⟨⟨offset, bytes.length⟩, hfound, rfl⟩

/-- The string scan never reports a UTF-8 failure; only the two string errors. -/
theorem scan_strings_not_invalid_utf8 (offset : Nat) (bytes : List UInt8) (span : Span) :
    scanStrings offset bytes ≠ .failed (.invalidUtf8 span) := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n →
    ∀ offset, scanStrings offset bytes ≠ .failed (.invalidUtf8 span)
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len offset
      cases bytes with
      | nil => rw [scan_strings_nil]; intro impossible; cases impossible
      | cons byte rest =>
        by_cases quote : byte = 34
        · subst quote
          cases body : scanBody rest with
          | unclosed =>
            rw [scan_strings_unterminated rest offset body]
            intro impossible
            cases impossible
          | closed after =>
            cases escape : decodeEscapes (rest.take (rest.length - after.length)) (offset + 1) with
            | some found =>
              rw [scan_strings_invalid rest after offset found body escape]
              intro impossible
              cases impossible
            | none =>
              rw [scan_strings_clean rest after offset body escape]
              have shorter : after.length < n := by
                simp only [List.length_cons] at len
                have bound := closed_drop_shorter rest after body
                omega
              exact ih _ shorter after rfl (offset + (rest.length + 1 - after.length))
        · rw [scan_strings_text byte rest offset quote]
          have shorter : rest.length < n := by simp only [List.length_cons] at len; omega
          exact ih _ shorter rest rfl (offset + 1)
  exact key bytes.length bytes rfl offset

theorem classify_invalid_utf8_iff (bytes : List UInt8) (moduleInputs : List (List UInt8))
    (span : Span) :
    classify bytes moduleInputs = .rejected (.invalidUtf8 span) ↔ InvalidUtf8At bytes span := by
  constructor
  · intro same
    cases found : firstInvalidUtf8 bytes with
    | some offset =>
      rw [classify_invalid_branch _ _ _ found] at same
      cases same
      exact ⟨found, rfl⟩
    | none =>
      have missing : reconstruct bytes moduleInputs = none := by
        cases result : reconstruct bytes moduleInputs with
        | none => rfl
        | some pair =>
          rcases pair with ⟨functions, scopes⟩
          rw [classify_accepted_branch _ _ _ _ found result] at same
          cases same
      cases envelope : InEnvelope bytes with
      | true =>
        cases scanned : scanStrings 0 bytes with
        | failed failure =>
          rw [classify_string_branch _ _ _ found missing envelope scanned] at same
          cases same
          exact absurd scanned (scan_strings_not_invalid_utf8 0 bytes span)
        | clean =>
          rw [classify_outside_clean _ _ found missing envelope scanned] at same
          cases same
      | false =>
        rw [classify_outside_no_envelope _ _ found missing envelope] at same
        cases same
  · rintro ⟨found, stop⟩
    have start : span = ⟨span.start, bytes.length⟩ := by
      cases span
      simp only at stop
      rw [stop]
    rw [start]
    exact classify_invalid_branch _ _ _ found


/-! ## The outside outcome -/

/-- Outside holds exactly when the input is not in `D`, is well-formed UTF-8, and either is
not in the envelope or scans clean. -/
theorem classify_outside_iff (bytes : List UInt8) (moduleInputs : List (List UInt8)) :
    classify bytes moduleInputs = .outside ↔
      ¬ Domain bytes moduleInputs ∧ (∀ span, ¬ InvalidUtf8At bytes span) ∧
        (InEnvelope bytes = false ∨ scanStrings 0 bytes = .clean) := by
  constructor
  · intro same
    have cleared : firstInvalidUtf8 bytes = none := by
      cases found : firstInvalidUtf8 bytes with
      | none => rfl
      | some offset =>
        rw [classify_invalid_branch _ _ _ found] at same
        cases same
    have notDomain : ¬ Domain bytes moduleInputs := by
      intro domain
      obtain ⟨functions, scopes, declared⟩ := domain
      rw [classify_accepted_branch _ _ _ _
        (domain_first_invalid_none ⟨_, _, declared⟩) (reconstruct_complete declared)] at same
      cases same
    refine ⟨notDomain, ?_, ?_⟩
    · rintro span ⟨found, _⟩
      rw [cleared] at found
      cases found
    · have missing : reconstruct bytes moduleInputs = none :=
        (reconstruct_none_iff_outside_domain bytes moduleInputs).mpr notDomain
      by_cases envelope : InEnvelope bytes = true
      · have scanned : scanStrings 0 bytes = .clean := by
          cases attempt : scanStrings 0 bytes with
          | failed failure =>
            rw [classify_string_branch _ _ _ cleared missing envelope attempt] at same
            cases same
          | clean => rfl
        exact Or.inr scanned
      · refine Or.inl ?_
        cases actual : InEnvelope bytes with
        | true => exact absurd actual envelope
        | false => rfl
  · rintro ⟨notDomain, noInvalid, envelopeOrClean⟩
    have cleared : firstInvalidUtf8 bytes = none := by
      cases found : firstInvalidUtf8 bytes with
      | none => rfl
      | some offset => exact absurd ⟨found, rfl⟩ (noInvalid ⟨offset, bytes.length⟩)
    have missing : reconstruct bytes moduleInputs = none :=
      (reconstruct_none_iff_outside_domain bytes moduleInputs).mpr notDomain
    by_cases envelope : InEnvelope bytes = true
    · cases envelopeOrClean with
      | inl absent =>
        have contradict : (false : Bool) = true := by
          rw [← absent]
          exact envelope
        exact absurd contradict (by simp)
      | inr clean => exact classify_outside_clean _ _ cleared missing envelope clean
    · exact classify_outside_no_envelope _ _ cleared missing (by
        cases actual : InEnvelope bytes with
        | true => exact absurd actual envelope
        | false => rfl)

end SourceAdmission

end Xsh.Conformance.Vertical
