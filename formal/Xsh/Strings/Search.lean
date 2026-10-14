/-!
This fragment models `Str.find` over UTF-8 bytes. The start is a byte offset;
negative and past-end starts are absent, while an empty needle matches every
valid offset, including the end. It does not model normalization, parsing,
storage, byte-value search APIs, runtime evaluation, or Rust correspondence.
-/

namespace Xsh.Strings.Search

private def startsWithList [BEq α] : List α → List α → Bool
  | [], _ => true
  | _ :: _, [] => false
  | wanted :: wantedRest, found :: foundRest =>
      (wanted == found) && startsWithList wantedRest foundRest

/-- Encode text as the finite byte sequence used by byte-offset operations. -/
def utf8Bytes (text : String) : List UInt8 :=
  text.toUTF8.data.toList

/-- Check whether a needle's encoded bytes start at a byte offset in text. -/
def needleMatchesAtByte (haystack needle : String) (offset : Nat) : Bool :=
  startsWithList (utf8Bytes needle) ((utf8Bytes haystack).drop offset)

/-- Return the first matching suffix position, measured in list elements. -/
private def firstMatch [BEq α] (needle : List α) : List α → Option Nat
  | [] => if startsWithList needle [] then some 0 else none
  | values@(_ :: tail) =>
      if startsWithList needle values then some 0
      else (firstMatch needle tail).map Nat.succ

/-- Find a needle from a valid offset in a finite byte sequence. -/
private def findFrom [BEq α] (values needle : List α) (start : Nat) : Option Nat :=
  if start ≤ values.length then
    (firstMatch needle (values.drop start)).map (fun relative => start + relative)
  else none

/-- Search UTF-8 storage bytes, returning the first matching byte offset. -/
def findByteOffset (haystack needle : String) (start : Int) : Option Nat :=
  match start with
  | .ofNat byteOffset => findFrom (utf8Bytes haystack) (utf8Bytes needle) byteOffset
  | .negSucc _ => none

private theorem firstMatch_some_spec [BEq α] (needle : List α)
    (values : List α) (found : Nat)
    (result : firstMatch needle values = some found) :
    found ≤ values.length ∧
      startsWithList needle (values.drop found) = true ∧
      ∀ earlier, earlier < found → startsWithList needle (values.drop earlier) = false := by
  induction values generalizing found with
  | nil =>
      by_cases matched : startsWithList needle [] = true
      · simp [firstMatch, matched] at result
        cases result
        refine ⟨by simp, ?_, ?_⟩
        · simpa using matched
        · intro earlier earlierLt
          omega
      · have unmatched : startsWithList needle [] = false := by
          cases value : startsWithList needle [] <;> simp_all
        simp [firstMatch, unmatched] at result
  | cons value tail inductionHypothesis =>
      by_cases matched : startsWithList needle (value :: tail) = true
      · simp [firstMatch, matched] at result
        cases result
        refine ⟨by simp, ?_, ?_⟩
        · simpa using matched
        · intro earlier earlierLt
          omega
      · have unmatched : startsWithList needle (value :: tail) = false := by
          cases check : startsWithList needle (value :: tail) <;> simp_all
        simp only [firstMatch, unmatched] at result
        cases tailResult : firstMatch needle tail with
        | none => simp [tailResult] at result
        | some relative =>
            simp [tailResult] at result
            have tailSpec := inductionHypothesis relative tailResult
            cases result
            refine ⟨by simp; omega, ?_, ?_⟩
            · simpa [List.drop] using tailSpec.2.1
            · intro earlier earlierLt
              cases earlier with
              | zero => simpa [List.drop] using unmatched
              | succ index =>
                  have indexLt : index < relative := by omega
                  simpa [List.drop] using tailSpec.2.2 index indexLt

private theorem firstMatch_none_spec [BEq α] (needle : List α)
    (values : List α) (result : firstMatch needle values = none) :
    ∀ index, index ≤ values.length → startsWithList needle (values.drop index) = false := by
  induction values with
  | nil =>
      by_cases matched : startsWithList needle [] = true
      · simp [firstMatch, matched] at result
      · have unmatched : startsWithList needle [] = false := by
          cases value : startsWithList needle [] <;> simp_all
        intro index bound
        have indexZero : index = 0 := by simp at bound; omega
        subst index
        simpa [List.drop] using unmatched
  | cons value tail inductionHypothesis =>
      by_cases matched : startsWithList needle (value :: tail) = true
      · simp [firstMatch, matched] at result
      · have unmatched : startsWithList needle (value :: tail) = false := by
          cases check : startsWithList needle (value :: tail) <;> simp_all
        simp only [firstMatch, unmatched] at result
        have tailResult : firstMatch needle tail = none := by
          cases check : firstMatch needle tail with
          | none => rfl
          | some found => simp [check] at result
        intro index bound
        cases index with
        | zero => simpa [List.drop] using unmatched
        | succ index =>
            have tailBound : index ≤ tail.length := by simp at bound; omega
            simpa [List.drop] using inductionHypothesis tailResult index tailBound

private theorem firstMatch_at_first_candidate [BEq α] (needle values : List α)
    (found : Nat) (within : found ≤ values.length)
    (matchedAt : startsWithList needle (values.drop found) = true)
    (noEarlier : ∀ earlier, earlier < found →
      startsWithList needle (values.drop earlier) = false) :
    firstMatch needle values = some found := by
  induction values generalizing found with
  | nil =>
      have foundZero : found = 0 := by simp at within; omega
      subst found
      have matchedEmpty : startsWithList needle [] = true := by
        simpa using matchedAt
      simp [firstMatch, matchedEmpty]
  | cons value tail inductionHypothesis =>
      by_cases matchedAtZero : startsWithList needle (value :: tail) = true
      · have foundZero : found = 0 := by
          by_cases foundIsZero : found = 0
          · exact foundIsZero
          · have zeroEarlier : 0 < found := by omega
            have earlierFalse := noEarlier 0 zeroEarlier
            have earlierTrue : startsWithList needle ((value :: tail).drop 0) = true := by
              simpa [List.drop] using matchedAtZero
            have absurd : true = false := by
              rw [← earlierTrue]
              exact earlierFalse
            cases absurd
        subst found
        simp [firstMatch, matchedAtZero]
      · have foundPositive : 0 < found := by
          by_cases foundIsZero : found = 0
          · subst found
            have zeroTrue : startsWithList needle (value :: tail) = true := by
              simpa [List.drop] using matchedAt
            exact False.elim (matchedAtZero zeroTrue)
          · omega
        cases found with
        | zero => omega
        | succ relative =>
            have relativeWithin : relative ≤ tail.length := by simp at within; omega
            have relativeMatches : startsWithList needle (tail.drop relative) = true := by
              simpa [List.drop] using matchedAt
            have relativeNoEarlier : ∀ earlier, earlier < relative →
                startsWithList needle (tail.drop earlier) = false := by
              intro earlier earlierLt
              have absoluteFalse := noEarlier (earlier + 1) (by omega)
              simpa [List.drop] using absoluteFalse
            have recursive := inductionHypothesis relative relativeWithin
              relativeMatches relativeNoEarlier
            simp [firstMatch, matchedAtZero, recursive]

private theorem findFrom_some_spec [BEq α] (values needle : List α)
    (start found : Nat) (result : findFrom values needle start = some found) :
    start ≤ found ∧ found ≤ values.length ∧
      startsWithList needle (values.drop found) = true ∧
      ∀ earlier, start ≤ earlier → earlier < found →
        startsWithList needle (values.drop earlier) = false := by
  unfold findFrom at result
  by_cases valid : start ≤ values.length
  · simp [valid] at result
    cases search : firstMatch needle (values.drop start) with
    | none => simp [search] at result
    | some relative =>
        simp [search] at result
        have spec := firstMatch_some_spec needle (values.drop start) relative search
        cases result
        have dropLength : (values.drop start).length = values.length - start := by
          simp
        refine ⟨by omega, ?_, ?_, ?_⟩
        · have relativeBound : relative ≤ values.length - start := by
            simpa [dropLength] using spec.1
          omega
        · simpa [List.drop_drop] using spec.2.1
        · intro earlier startLe earlierLt
          have relativeLt : earlier - start < relative := by omega
          have absent := spec.2.2 (earlier - start) relativeLt
          have suffixAtEarlier : (values.drop start).drop (earlier - start) =
              values.drop earlier := by
            rw [List.drop_drop]
            congr 1
            omega
          rw [← suffixAtEarlier]
          exact absent
  · simp [valid] at result

/-- A negative byte start never selects a suffix. -/
theorem negative_start_is_absent (haystack needle : String) (start : Int)
    (negative : start < 0) :
    findByteOffset haystack needle start = none := by
  cases start with
  | ofNat value =>
      exact False.elim ((Int.not_lt.mpr (Int.natCast_nonneg value)) negative)
  | negSucc value => simp [findByteOffset]

/-- A start beyond the UTF-8 byte length is absent. -/
theorem past_end_start_is_absent (haystack needle : String) (start : Nat)
    (pastEnd : (utf8Bytes haystack).length < start) :
    findByteOffset haystack needle (.ofNat start) = none := by
  change findFrom (utf8Bytes haystack) (utf8Bytes needle) start = none
  have beyond : ¬ start ≤ (utf8Bytes haystack).length := by omega
  simp [findFrom, beyond]

private theorem firstMatch_empty [BEq α] (values : List α) :
    firstMatch [] values = some 0 := by
  induction values with
  | nil => simp [firstMatch, startsWithList]
  | cons value tail inductionHypothesis => simp [firstMatch, startsWithList]

/-- An empty needle matches every requested byte offset within the text. -/
theorem empty_needle_matches_requested_byte_offset (haystack : String) (start : Nat)
    (valid : start ≤ (utf8Bytes haystack).length) :
    findByteOffset haystack "" (.ofNat start) = some start := by
  change findFrom (utf8Bytes haystack) [] start = some start
  simp [findFrom, valid, firstMatch_empty]

/-- An empty needle also matches exactly at the end byte offset. -/
theorem empty_needle_matches_end_offset (haystack : String) :
    findByteOffset haystack "" (.ofNat (utf8Bytes haystack).length) =
      some (utf8Bytes haystack).length := by
  apply empty_needle_matches_requested_byte_offset
  exact Nat.le_refl _

/-- Empty text and an empty needle match at byte offset zero. -/
theorem empty_text_empty_needle_matches_zero :
    findByteOffset "" "" (.ofNat 0) = some 0 := by
  decide

/-- Every successful result remains within the haystack's UTF-8 byte length. -/
theorem result_is_bounded_by_haystack_bytes (haystack needle : String) (start found : Nat)
    (result : findByteOffset haystack needle (.ofNat start) = some found) :
    found ≤ (utf8Bytes haystack).length := by
  exact (findFrom_some_spec (utf8Bytes haystack) (utf8Bytes needle) start found
    (by simpa [findByteOffset] using result)).2.1

/-- A candidate is returned when it is the first matching byte suffix at or after start. -/
private theorem first_matching_byte_is_returned (haystack needle : String) (start found : Nat)
    (validStart : start ≤ (utf8Bytes haystack).length)
    (validFound : start ≤ found ∧ found ≤ (utf8Bytes haystack).length)
    (matchedAt : startsWithList (utf8Bytes needle) ((utf8Bytes haystack).drop found) = true)
    (noEarlier : ∀ earlier, start ≤ earlier → earlier < found →
      startsWithList (utf8Bytes needle) ((utf8Bytes haystack).drop earlier) = false) :
    findByteOffset haystack needle (.ofNat start) = some found := by
  change findFrom (utf8Bytes haystack) (utf8Bytes needle) start = some found
  have relativeWithin : found - start ≤ ((utf8Bytes haystack).drop start).length := by
    simp
    omega
  have relativeMatches : startsWithList (utf8Bytes needle)
      (((utf8Bytes haystack).drop start).drop (found - start)) = true := by
    have dropEq : ((utf8Bytes haystack).drop start).drop (found - start) =
        (utf8Bytes haystack).drop found := by
      rw [List.drop_drop]
      congr 1
      omega
    rw [dropEq]
    exact matchedAt
  have relativeNoEarlier : ∀ earlier, earlier < found - start →
      startsWithList (utf8Bytes needle)
        (((utf8Bytes haystack).drop start).drop earlier) = false := by
    intro earlier earlierLt
    have absoluteFalse := noEarlier (start + earlier) (by omega) (by omega)
    have dropEq : ((utf8Bytes haystack).drop start).drop earlier =
        (utf8Bytes haystack).drop (start + earlier) := by
      rw [List.drop_drop]
    rw [dropEq]
    exact absoluteFalse
  have relativeSearch := firstMatch_at_first_candidate (utf8Bytes needle)
    ((utf8Bytes haystack).drop start) (found - start) relativeWithin
    relativeMatches relativeNoEarlier
  unfold findFrom
  simp [validStart, relativeSearch, Nat.add_sub_of_le validFound.1]

/-- A result exists exactly when its byte suffix is the first matching one at or after start. -/
theorem first_match_returns_byte_offset (haystack needle : String) (start found : Nat) :
    findByteOffset haystack needle (.ofNat start) = some found ↔
      start ≤ found ∧ found ≤ (utf8Bytes haystack).length ∧
        needleMatchesAtByte haystack needle found = true ∧
        (∀ earlier, start ≤ earlier → earlier < found →
          needleMatchesAtByte haystack needle earlier = false) := by
  constructor
  · intro result
    have facts := findFrom_some_spec (utf8Bytes haystack) (utf8Bytes needle) start found
      (by simpa [findByteOffset] using result)
    refine ⟨facts.1, facts.2.1, ?_, ?_⟩
    · simpa [needleMatchesAtByte] using facts.2.2.1
    · intro earlier startLe earlierLt
      simpa [needleMatchesAtByte] using facts.2.2.2 earlier startLe earlierLt
  · rintro ⟨startLe, foundLe, matchedAt, noEarlier⟩
    have validStart : start ≤ (utf8Bytes haystack).length := Nat.le_trans startLe foundLe
    have matchedAtBytes : startsWithList (utf8Bytes needle)
        ((utf8Bytes haystack).drop found) = true := by
      simpa [needleMatchesAtByte] using matchedAt
    have noEarlierBytes : ∀ earlier, start ≤ earlier → earlier < found →
        startsWithList (utf8Bytes needle) ((utf8Bytes haystack).drop earlier) = false := by
      intro earlier earlierStart earlierBeforeFound
      simpa [needleMatchesAtByte] using noEarlier earlier earlierStart earlierBeforeFound
    exact first_matching_byte_is_returned haystack needle start found validStart
      ⟨startLe, foundLe⟩ matchedAtBytes noEarlierBytes

/-- A missing byte match remains absent. -/
theorem no_match_is_absent (haystack needle : String) (start : Nat)
    (noMatch : ∀ candidate, start ≤ candidate → candidate ≤ (utf8Bytes haystack).length →
      needleMatchesAtByte haystack needle candidate = false) :
    findByteOffset haystack needle (.ofNat start) = none := by
  change findFrom (utf8Bytes haystack) (utf8Bytes needle) start = none
  by_cases noResult : findFrom (utf8Bytes haystack) (utf8Bytes needle) start = none
  · exact noResult
  · cases result : findFrom (utf8Bytes haystack) (utf8Bytes needle) start with
    | none => exact False.elim (noResult result)
    | some found =>
        have resultFacts := findFrom_some_spec (utf8Bytes haystack) (utf8Bytes needle)
          start found result
        have absent := noMatch found resultFacts.1 resultFacts.2.1
        change startsWithList (utf8Bytes needle) ((utf8Bytes haystack).drop found) = false at absent
        rw [resultFacts.2.2.1] at absent
        cases absent

/-- Starting at byte two skips the earlier `ba` match and returns byte three. -/
theorem start_skips_earlier_match :
    findByteOffset "ababa" "ba" (.ofNat 2) = some 3 := by
  decide

/-- The multibyte prefix `aé` occupies three bytes before the crab scalar. -/
theorem multibyte_prefix_counts_all_utf8_bytes :
    findByteOffset "aé🦀" "🦀" (.ofNat 0) = some 3 := by
  decide

/-- Mutant that searches Unicode scalars and reports a scalar index. -/
def scalarIndexMutation (haystack needle : String) : Option Nat :=
  (firstMatch needle.toList haystack.toList)

/-- Treating byte positions as scalar positions changes the first result. -/
theorem scalar_index_mutation_changes_multibyte_witness :
    findByteOffset "aé🦀" "🦀" (.ofNat 0) = some 3 ∧
      scalarIndexMutation "aé🦀" "🦀" = some 2 ∧
      findByteOffset "aé🦀" "🦀" (.ofNat 0) ≠
        scalarIndexMutation "aé🦀" "🦀" := by
  decide

end Xsh.Strings.Search
