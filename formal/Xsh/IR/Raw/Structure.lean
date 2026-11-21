import Xsh.IR.Raw.Verifier

namespace Xsh.IR.Raw

open Xsh.Foundation
open Xsh.Conformance.Transport

theorem decodeWord_encodeWord (word : Word32) : decodeWord? (encodeWord word) = some word :=
  decode_encode_hex32 word

theorem mapM_decode_comp_encode (words : List Word32) :
    words.mapM (decodeWord? ∘ encodeWord) = some words := by
  induction words with
  | nil => rfl
  | cons word rest inductionHypothesis =>
      simp [List.mapM_cons, decodeWord_encodeWord, inductionHypothesis]

theorem mapM_decodeWord_encodeWord (words : List Word32) :
    (words.map encodeWord).mapM decodeWord? = some words := by
  rw [List.mapM_map]
  exact mapM_decode_comp_encode words

theorem decodeWords_encodeWords (words : List Word32) : decodeWords? (encodeWords words) = some words :=
  mapM_decodeWord_encodeWord words

theorem decodeRows_encodeRows (arity : Nat) (rows : List Row) (shaped : ∀ row ∈ rows, row.length = arity) :
    decodeRows? arity (encodeRows rows) = some rows := by
  induction rows with
  | nil => rfl
  | cons row rest inductionHypothesis =>
      have rowLength : row.length = arity := shaped row (by simp)
      have restShaped : ∀ other ∈ rest, other.length = arity := fun other member => shaped other (by simp [member])
      have restDecoded := inductionHypothesis restShaped
      simp only [decodeRows?, encodeRows, List.map_cons, List.mapM_cons] at restDecoded ⊢
      simp [decodeRow?, encodeWords, rowLength, mapM_decode_comp_encode, restDecoded]

theorem decodeTags_encodeTags (tags : List TagWord) : decodeTags? (encodeTags tags) = some tags := by
  induction tags with
  | nil => rfl
  | cons tag rest inductionHypothesis =>
      simp only [decodeTags?, encodeTags, List.map_cons, List.mapM_cons] at inductionHypothesis ⊢
      simp [encodeTag, decodeTag?, decodeWord_encodeWord, inductionHypothesis]

theorem decodeTexts_encodeTexts (texts : List String) : decodeTexts? (encodeTexts texts) = some texts := by
  induction texts with
  | nil => rfl
  | cons text rest inductionHypothesis =>
      simp only [decodeTexts?, encodeTexts, List.map_cons, List.mapM_cons] at inductionHypothesis ⊢
      simp [WireValue.text?, inductionHypothesis]

theorem decodeBytes_encodeBytes (bytes : List WireByte) : decodeBytes? (encodeBytes bytes) = some bytes :=
  decode_encode_hex_bytes bytes

theorem decodeWord_inverse {value : WireValue} {word : Word32} (decoded : decodeWord? value = some word) :
    value = encodeWord word := by
  cases value with
  | string text =>
      have reencoded := decodeHex32_accepted_reencodes text word decoded
      simp [encodeWord, reencoded]
  | array _ => cases decoded
  | object _ => cases decoded

theorem mapM_decodeWord_inverse :
    ∀ {items : List WireValue} {words : List Word32}, items.mapM decodeWord? = some words → items = words.map encodeWord
  | [], words, decoded => by
      cases decoded
      rfl
  | item :: items, words, decoded => by
      rw [List.mapM_cons] at decoded
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at decoded
      obtain ⟨word, wordEq, rest, restEq, rfl⟩ := decoded
      simp [decodeWord_inverse wordEq, mapM_decodeWord_inverse restEq]

theorem decodeWords_inverse {value : WireValue} {words : List Word32} (decoded : decodeWords? value = some words) :
    value = encodeWords words := by
  cases value with
  | array items => simp [encodeWords, mapM_decodeWord_inverse decoded]
  | string _ => cases decoded
  | object _ => cases decoded

theorem decodeRow_inverse {arity : Nat} {value : WireValue} {row : Row} (decoded : decodeRow? arity value = some row) :
    value = encodeWords row ∧ row.length = arity := by
  cases value with
  | array items =>
      change (if items.length = arity then items.mapM decodeWord? else none) = some row at decoded
      by_cases length : items.length = arity
      · rw [if_pos length] at decoded
        have itemsEq := mapM_decodeWord_inverse decoded
        refine ⟨by simp [encodeWords, itemsEq], ?_⟩
        rw [← length, itemsEq, List.length_map]
      · rw [if_neg length] at decoded
        cases decoded
  | string _ => cases decoded
  | object _ => cases decoded

theorem mapM_decodeRow_inverse {arity : Nat} :
    ∀ {items : List WireValue} {rows : List Row}, items.mapM (decodeRow? arity) = some rows →
      items = rows.map encodeWords ∧ ∀ row ∈ rows, row.length = arity
  | [], rows, decoded => by
      cases decoded
      exact ⟨rfl, fun _ member => by cases member⟩
  | item :: items, rows, decoded => by
      rw [List.mapM_cons] at decoded
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at decoded
      obtain ⟨row, rowEq, rest, restEq, rfl⟩ := decoded
      obtain ⟨itemEq, rowLength⟩ := decodeRow_inverse rowEq
      obtain ⟨restItems, restLengths⟩ := mapM_decodeRow_inverse restEq
      refine ⟨by simp [itemEq, restItems], ?_⟩
      intro other member
      rcases List.mem_cons.mp member with same | inRest
      · rw [same]; exact rowLength
      · exact restLengths other inRest

theorem decodeRows_inverse {arity : Nat} {value : WireValue} {rows : List Row}
    (decoded : decodeRows? arity value = some rows) : value = encodeRows rows ∧ ∀ row ∈ rows, row.length = arity := by
  cases value with
  | array items =>
      obtain ⟨itemsEq, lengths⟩ := mapM_decodeRow_inverse decoded
      exact ⟨by simp [encodeRows, itemsEq], lengths⟩
  | string _ => cases decoded
  | object _ => cases decoded

theorem decodeTag_inverse {value : WireValue} {tag : TagWord} (decoded : decodeTag? value = some tag) :
    value = encodeTag tag := by
  unfold decodeTag? at decoded
  split at decoded
  · next name ordinal =>
      obtain ⟨word, wordEq, rfl⟩ := Option.map_eq_some_iff.mp decoded
      simp [encodeTag, decodeWord_inverse wordEq]
  · cases decoded

theorem mapM_decodeTag_inverse :
    ∀ {items : List WireValue} {tags : List TagWord}, items.mapM decodeTag? = some tags → items = tags.map encodeTag
  | [], tags, decoded => by
      cases decoded
      rfl
  | item :: items, tags, decoded => by
      rw [List.mapM_cons] at decoded
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at decoded
      obtain ⟨tag, tagEq, rest, restEq, rfl⟩ := decoded
      simp [decodeTag_inverse tagEq, mapM_decodeTag_inverse restEq]

theorem decodeTags_inverse {value : WireValue} {tags : List TagWord} (decoded : decodeTags? value = some tags) :
    value = encodeTags tags := by
  cases value with
  | array items => simp [encodeTags, mapM_decodeTag_inverse decoded]
  | string _ => cases decoded
  | object _ => cases decoded

theorem mapM_text_inverse :
    ∀ {items : List WireValue} {texts : List String}, items.mapM WireValue.text? = some texts →
      items = texts.map WireValue.string
  | [], texts, decoded => by
      cases decoded
      rfl
  | item :: items, texts, decoded => by
      rw [List.mapM_cons] at decoded
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at decoded
      obtain ⟨text, textEq, rest, restEq, rfl⟩ := decoded
      cases item with
      | string value =>
          cases textEq
          simp [mapM_text_inverse restEq]
      | array _ => cases textEq
      | object _ => cases textEq

theorem decodeTexts_inverse {value : WireValue} {texts : List String} (decoded : decodeTexts? value = some texts) :
    value = encodeTexts texts := by
  cases value with
  | array items => simp [encodeTexts, mapM_text_inverse decoded]
  | string _ => cases decoded
  | object _ => cases decoded

theorem decodeBytes_inverse {value : WireValue} {bytes : List WireByte} (decoded : decodeBytes? value = some bytes) :
    value = encodeBytes bytes := by
  cases value with
  | string text => simp [encodeBytes, decodeHexBytes_accepted_reencodes text bytes decoded]
  | array _ => cases decoded
  | object _ => cases decoded

theorem expectText_inverse {expected : String} {value : WireValue} {unit : Unit}
    (decoded : expectText? expected value = some unit) : value = .string expected := by
  cases value with
  | string text =>
      change (if text = expected then some () else none) = some unit at decoded
      by_cases same : text = expected
      · rw [same]
      · rw [if_neg same] at decoded
        cases decoded
  | array _ => cases decoded
  | object _ => cases decoded

theorem documentFields_length : documentFields.length = 47 := rfl

theorem wireValues_length (d : Document) : d.wireValues.length = 47 := by
  simp [Document.wireValues]

theorem decode_encode (d : Document) (shaped : d.WellShaped) : Document.decode? d.encode = some d := by
  have lengths : documentFields.length ≤ d.wireValues.length := by
    rw [documentFields_length, wireValues_length]; exact Nat.le_refl _
  have lengths' : d.wireValues.length ≤ documentFields.length := by
    rw [documentFields_length, wireValues_length]; exact Nat.le_refl _
  have names : (documentFields.zip d.wireValues).map Prod.fst = documentFields := List.map_fst_zip lengths
  have values : (documentFields.zip d.wireValues).map Prod.snd = d.wireValues := List.map_snd_zip lengths'
  obtain ⟨s0, s1, s2, s3, s4, s5, s6, s7, s8, s9, s10, s11, s12, s13, s14, s15, s16, s17, s18, s19, s20⟩ := shaped
  simp only [Document.decode?, Document.encode, names, values, ite_true]
  cases d
  simp (config := { maxSteps := 4000000 }) [Document.wireValues, Document.ofValues?, expectText?, decodeWord_encodeWord, decodeWords_encodeWords,
    decodeTags_encodeTags, decodeTexts_encodeTexts, decodeBytes_encodeBytes,
    decodeRows_encodeRows _ _ s0, decodeRows_encodeRows _ _ s1, decodeRows_encodeRows _ _ s2, decodeRows_encodeRows _ _ s3, decodeRows_encodeRows _ _ s4, decodeRows_encodeRows _ _ s5, decodeRows_encodeRows _ _ s6, decodeRows_encodeRows _ _ s7, decodeRows_encodeRows _ _ s8, decodeRows_encodeRows _ _ s9, decodeRows_encodeRows _ _ s10, decodeRows_encodeRows _ _ s11, decodeRows_encodeRows _ _ s12, decodeRows_encodeRows _ _ s13, decodeRows_encodeRows _ _ s14, decodeRows_encodeRows _ _ s15, decodeRows_encodeRows _ _ s16, decodeRows_encodeRows _ _ s17, decodeRows_encodeRows _ _ s18, decodeRows_encodeRows _ _ s19, decodeRows_encodeRows _ _ s20]



theorem check_eq_some {condition : Prop} [Decidable condition] {unit : Unit} :
    check condition = some unit ↔ condition := by
  unfold check
  by_cases holds : condition
  · simp [holds]
  · simp [holds]

theorem readWord_wordAt {doc : Document} {position limit word : Nat}
    (read : readWord doc position limit = some word) : wordAt doc position = some word := by
  unfold readWord at read
  by_cases inRange : position < limit
  · simpa [inRange] using read
  · simp [inRange] at read

theorem joinFit_eq_some {first : Fit} {rest : Nat → Fit} {stop : Nat} {claims : List Claim} :
    joinFit first rest = some (stop, claims) ↔
      ∃ middle firstClaims moreClaims, first = some (middle, firstClaims) ∧
        rest middle = some (stop, moreClaims) ∧ claims = firstClaims ++ moreClaims := by
  unfold joinFit
  constructor
  · intro joined
    obtain ⟨⟨middle, firstClaims⟩, firstEq, mapped⟩ := Option.bind_eq_some_iff.mp joined
    obtain ⟨⟨actual, moreClaims⟩, restEq, pairEq⟩ := Option.map_eq_some_iff.mp mapped
    cases pairEq
    exact ⟨middle, firstClaims, moreClaims, firstEq, restEq, rfl⟩
  · rintro ⟨middle, firstClaims, moreClaims, firstEq, restEq, rfl⟩
    rw [firstEq]
    exact Option.bind_eq_some_iff.mpr ⟨(middle, firstClaims), rfl,
      Option.map_eq_some_iff.mpr ⟨(stop, moreClaims), restEq, rfl⟩⟩

theorem iterate_succ (step : Nat → Fit) (count position : Nat) :
    iterate step (count + 1) position = joinFit (step position) (iterate step count) := rfl

theorem iterate_zero_eq_some {step : Nat → Fit} {position stop : Nat} {claims : List Claim} :
    iterate step 0 position = some (stop, claims) ↔ stop = position ∧ claims = [] := by
  show some (position, []) = some (stop, claims) ↔ _
  constructor
  · intro same
    cases same
    exact ⟨rfl, rfl⟩
  · rintro ⟨rfl, rfl⟩
    rfl

theorem oneBased_iff (word count : Nat) : oneBased word count = true ↔ OneBased word count := by
  simp [oneBased, OneBased, and_assoc]

theorem precedes_iff (scope : Scope) (word : Nat) : precedes scope word = true ↔ Precedes scope word := by
  unfold precedes Precedes
  cases scope.user <;> simp

theorem operandOk_iff (env : Env) (scope : Scope) (kind : RowKind) (word : Nat) :
    operandOk env scope kind word = true ↔ OperandOk env scope kind word := by
  simp [operandOk, OperandOk, precedes_iff, and_assoc]

theorem imports_iff (doc : Document) (scope : Scope) (word : Nat) :
    imports doc scope word = true ↔ Imports doc scope word := by
  unfold imports Imports
  cases scope.importer <;> simp [oneBased_iff]

theorem roleOk_iff (env : Env) (scope : Scope) (role : Role) (word : Nat) :
    roleOk env scope role word = true ↔ RoleOk env scope role word := by
  cases role <;> simp [roleOk, RoleOk, oneBased_iff, operandOk_iff, imports_iff]

theorem wordFit_eq_some {env : Env} {scope : Scope} {role : Role} {position limit stop : Nat} {claims : List Claim} :
    wordFit env scope role position limit = some (stop, claims) ↔
      ∃ word, readWord env.doc position limit = some word ∧ RoleOk env scope role word ∧
        stop = position + 1 ∧ claims = wordClaims role word := by
  unfold wordFit
  constructor
  · intro fitted
    obtain ⟨word, readEq, checked⟩ := Option.bind_eq_some_iff.mp fitted
    obtain ⟨unit, okEq, pairEq⟩ := Option.map_eq_some_iff.mp checked
    cases pairEq
    exact ⟨word, readEq, (roleOk_iff _ _ _ _).mp (check_eq_some.mp okEq), rfl, rfl⟩
  · rintro ⟨word, readEq, ok, rfl, rfl⟩
    rw [readEq]
    exact Option.bind_eq_some_iff.mpr ⟨word, rfl, Option.map_eq_some_iff.mpr ⟨(), check_eq_some.mpr
      ((roleOk_iff _ _ _ _).mpr ok), rfl⟩⟩

theorem iterate_word_iff {env : Env} {scope : Scope} {role : Role} {limit : Nat} :
    ∀ count position stop claims,
      iterate (fun cursor => wordFit env scope role cursor limit) count position = some (stop, claims) ↔
        WordsFit env scope role limit count position stop claims := by
  intro count
  induction count with
  | zero =>
      intro position stop claims
      rw [iterate_zero_eq_some]
      constructor
      · rintro ⟨rfl, rfl⟩; exact .zero
      · intro fits; cases fits; exact ⟨rfl, rfl⟩
  | succ count inductionHypothesis =>
      intro position stop claims
      rw [iterate_succ, joinFit_eq_some]
      constructor
      · rintro ⟨middle, first, more, fitted, rest, rfl⟩
        obtain ⟨word, readEq, ok, rfl, rfl⟩ := wordFit_eq_some.mp fitted
        exact .succ readEq ok ((inductionHypothesis _ _ _).mp rest)
      · intro fits
        cases fits with
        | succ readEq ok rest =>
            exact ⟨_, _, _, wordFit_eq_some.mpr ⟨_, readEq, ok, rfl, rfl⟩,
              (inductionHypothesis _ _ _).mpr rest, rfl⟩

theorem countedFit_eq_some {doc : Document} {step : Nat → Fit} {position limit stop : Nat} {claims : List Claim} :
    countedFit doc step position limit = some (stop, claims) ↔
      ∃ count, readWord doc position limit = some count ∧ iterate step count (position + 1) = some (stop, claims) :=
  Option.bind_eq_some_iff

theorem blockFit_eq_some {doc : Document} {step : Nat → Nat → Fit} {owner word : Nat} {statements : Bool}
    {claims : List Claim} :
    blockFit doc step owner word statements = some claims ↔
      ∃ start length count, blockRange doc owner word statements = some (start, length) ∧
        readWord doc start (start + length) = some count ∧
        iterate (step (start + length)) count (start + 1) = some (start + length, claims) := by
  unfold blockFit
  constructor
  · intro fitted
    obtain ⟨⟨start, length⟩, rangeEq, counted⟩ := Option.bind_eq_some_iff.mp fitted
    obtain ⟨⟨stop, found⟩, countedEq, checked⟩ := Option.bind_eq_some_iff.mp counted
    obtain ⟨unit, stopEq, claimsEq⟩ := Option.map_eq_some_iff.mp checked
    have stopIs := check_eq_some.mp stopEq
    cases stopIs
    cases claimsEq
    obtain ⟨count, readEq, iterated⟩ := countedFit_eq_some.mp countedEq
    exact ⟨start, length, count, rangeEq, readEq, iterated⟩
  · rintro ⟨start, length, count, rangeEq, readEq, iterated⟩
    rw [rangeEq]
    refine Option.bind_eq_some_iff.mpr ⟨(start, length), rfl, ?_⟩
    refine Option.bind_eq_some_iff.mpr ⟨(start + length, claims), countedFit_eq_some.mpr ⟨count, readEq, iterated⟩, ?_⟩
    exact Option.map_eq_some_iff.mpr ⟨(), check_eq_some.mpr rfl, rfl⟩

section
variable {enter : Enter} {entry : EntrySpec}
  (entered : ∀ scope shape position limit stop claims,
    enter scope shape position limit = some (stop, claims) ↔ entry scope shape position limit stop claims)
include entered

theorem entryFit_eq_some {scope : Scope} {fields : List Shape} {start length : Nat} {claims : List Claim} :
    entryFit enter scope fields start length = some claims ↔ EntryFits entry scope fields start length claims := by
  unfold entryFit EntryFits
  constructor
  · intro fitted
    obtain ⟨⟨stop, found⟩, enterEq, checked⟩ := Option.bind_eq_some_iff.mp fitted
    obtain ⟨unit, stopEq, claimsEq⟩ := Option.map_eq_some_iff.mp checked
    have stopIs := check_eq_some.mp stopEq
    cases stopIs
    cases claimsEq
    exact (entered _ _ _ _ _ _).mp enterEq
  · intro fits
    refine Option.bind_eq_some_iff.mpr ⟨(start + length, claims), (entered _ _ _ _ _ _).mpr fits, ?_⟩
    exact Option.map_eq_some_iff.mpr ⟨(), check_eq_some.mpr rfl, rfl⟩

theorem patternFit_eq_some {env : Env} {scope : Scope} {position limit stop : Nat} {claims : List Claim} :
    patternFit env enter scope position limit = some (stop, claims) ↔
      stop = position + 1 ∧ PatternRef env entry scope position limit claims := by
  unfold patternFit PatternRef
  constructor
  · intro fitted
    obtain ⟨word, readEq, bounded⟩ := Option.bind_eq_some_iff.mp fitted
    obtain ⟨unit, boundEq, tagged⟩ := Option.bind_eq_some_iff.mp bounded
    obtain ⟨tag, tagEq, ranged⟩ := Option.bind_eq_some_iff.mp tagged
    obtain ⟨⟨start, length⟩, rangeEq, entered'⟩ := Option.bind_eq_some_iff.mp ranged
    obtain ⟨inner, entryEq, pairEq⟩ := Option.map_eq_some_iff.mp entered'
    cases pairEq
    exact ⟨rfl, word, tag, start, length, inner, readEq, check_eq_some.mp boundEq, tagEq, rangeEq,
      (entryFit_eq_some entered).mp entryEq, rfl⟩
  · rintro ⟨rfl, word, tag, start, length, inner, readEq, bound, tagEq, rangeEq, fits, rfl⟩
    rw [readEq]
    refine Option.bind_eq_some_iff.mpr ⟨word, rfl, Option.bind_eq_some_iff.mpr ⟨(), check_eq_some.mpr bound, ?_⟩⟩
    rw [tagEq]
    refine Option.bind_eq_some_iff.mpr ⟨tag, rfl, ?_⟩
    rw [rangeEq]
    refine Option.bind_eq_some_iff.mpr ⟨(start, length), rfl, ?_⟩
    exact Option.map_eq_some_iff.mpr ⟨inner, (entryFit_eq_some entered).mpr fits, rfl⟩

theorem stageFit_eq_some {env : Env} {scope : Scope} {position limit stop : Nat} {claims : List Claim} :
    stageFit env enter scope position limit = some (stop, claims) ↔
      stop = position + 1 ∧ StageRef env entry scope position limit claims := by
  unfold stageFit StageRef
  constructor
  · intro fitted
    obtain ⟨word, readEq, bounded⟩ := Option.bind_eq_some_iff.mp fitted
    obtain ⟨unit, boundEq, tagged⟩ := Option.bind_eq_some_iff.mp bounded
    obtain ⟨tag, tagEq, ranged⟩ := Option.bind_eq_some_iff.mp tagged
    obtain ⟨⟨start, length⟩, rangeEq, entered'⟩ := Option.bind_eq_some_iff.mp ranged
    obtain ⟨inner, entryEq, pairEq⟩ := Option.map_eq_some_iff.mp entered'
    cases pairEq
    exact ⟨rfl, word, tag, start, length, inner, readEq, check_eq_some.mp boundEq, tagEq, rangeEq,
      (entryFit_eq_some entered).mp entryEq, rfl⟩
  · rintro ⟨rfl, word, tag, start, length, inner, readEq, bound, tagEq, rangeEq, fits, rfl⟩
    rw [readEq]
    refine Option.bind_eq_some_iff.mpr ⟨word, rfl, Option.bind_eq_some_iff.mpr ⟨(), check_eq_some.mpr bound, ?_⟩⟩
    rw [tagEq]
    refine Option.bind_eq_some_iff.mpr ⟨tag, rfl, ?_⟩
    rw [rangeEq]
    refine Option.bind_eq_some_iff.mpr ⟨(start, length), rfl, ?_⟩
    exact Option.map_eq_some_iff.mpr ⟨inner, (entryFit_eq_some entered).mpr fits, rfl⟩

theorem iterate_pattern_iff {env : Env} {scope : Scope} {limit : Nat} :
    ∀ count position stop claims,
      iterate (fun cursor => patternFit env enter scope cursor limit) count position = some (stop, claims) ↔
        PatternRefs env entry scope limit count position stop claims := by
  intro count
  induction count with
  | zero =>
      intro position stop claims
      rw [iterate_zero_eq_some]
      constructor
      · rintro ⟨rfl, rfl⟩; exact .zero
      · intro fits; cases fits; exact ⟨rfl, rfl⟩
  | succ count inductionHypothesis =>
      intro position stop claims
      rw [iterate_succ, joinFit_eq_some]
      constructor
      · rintro ⟨middle, first, more, fitted, rest, rfl⟩
        obtain ⟨rfl, reference⟩ := (patternFit_eq_some entered).mp fitted
        exact .succ reference ((inductionHypothesis _ _ _).mp rest)
      · intro fits
        cases fits with
        | succ reference rest =>
            exact ⟨_, _, _, (patternFit_eq_some entered).mpr ⟨rfl, reference⟩,
              (inductionHypothesis _ _ _).mpr rest, rfl⟩

end



section
variable {env : Env} {enter : Enter} {entry : EntrySpec}
  (entered : ∀ scope shape position limit stop claims,
    enter scope shape position limit = some (stop, claims) ↔ entry scope shape position limit stop claims)
include entered

mutual
theorem fitAt_iff : ∀ (shape : Shape) (scope : Scope) (position limit stop : Nat) (claims : List Claim),
    fitAt env enter scope shape position limit = some (stop, claims) ↔
      Fits env entry scope shape position limit stop claims
  | .word role, scope, position, limit, stop, claims => by
      by_cases isPattern : role = .pattern
      · subst isPattern
        show patternFit env enter scope position limit = some (stop, claims) ↔ _
        rw [patternFit_eq_some entered]
        constructor
        · rintro ⟨rfl, reference⟩
          exact .pattern reference
        · intro fits
          cases fits with
          | pattern reference => exact ⟨rfl, reference⟩
          | word notPattern _ _ _ => exact absurd rfl notPattern
      · by_cases isStage : role = .stage
        · subst isStage
          show stageFit env enter scope position limit = some (stop, claims) ↔ _
          rw [stageFit_eq_some entered]
          constructor
          · rintro ⟨rfl, reference⟩
            exact .stage reference
          · intro fits
            cases fits with
            | stage reference => exact ⟨rfl, reference⟩
            | word _ notStage _ _ => exact absurd rfl notStage
        · have reduce : fitAt env enter scope (.word role) position limit = wordFit env scope role position limit := by
            cases role <;> first | rfl | exact absurd rfl isPattern | exact absurd rfl isStage
          rw [reduce, wordFit_eq_some]
          constructor
          · rintro ⟨word, readEq, ok, rfl, rfl⟩
            exact .word isPattern isStage readEq ok
          · intro fits
            cases fits with
            | word _ _ readEq ok => exact ⟨_, readEq, ok, rfl, rfl⟩
            | pattern _ => exact absurd rfl isPattern
            | stage _ => exact absurd rfl isStage
  | .seq items, scope, position, limit, stop, claims => by
      show fitList env enter scope items position limit = some (stop, claims) ↔ _
      rw [fitList_iff items scope position limit stop claims]
      constructor
      · intro fits
        exact .seq fits
      · intro fits
        cases fits with
        | seq fits => exact fits
  | .choice tags variants, scope, position, limit, stop, claims => by
      show ((readWord env.doc position limit).bind fun tag =>
        fitChoice env enter scope tag tags variants (position + 1) limit) = some (stop, claims) ↔ _
      rw [Option.bind_eq_some_iff]
      constructor
      · rintro ⟨tag, readEq, chosen⟩
        exact .choice readEq ((fitChoice_iff variants tags tag scope (position + 1) limit stop claims).mp chosen)
      · intro fits
        cases fits with
        | choice readEq chosen =>
            exact ⟨_, readEq, (fitChoice_iff variants tags _ scope (position + 1) limit stop claims).mpr chosen⟩
  | .many item, scope, position, limit, stop, claims => by
      have manyIff : ∀ count cursor finish found,
          iterate (fun cursor => fitAt env enter scope item cursor limit) count cursor = some (finish, found) ↔
            FitsMany env entry scope item limit count cursor finish found := by
        intro count
        induction count with
        | zero =>
            intro cursor finish found
            rw [iterate_zero_eq_some]
            constructor
            · rintro ⟨rfl, rfl⟩
              exact .zero
            · intro fits
              cases fits
              exact ⟨rfl, rfl⟩
        | succ count inductionHypothesis =>
            intro cursor finish found
            rw [iterate_succ, joinFit_eq_some]
            constructor
            · rintro ⟨middle, first, more, fitted, rest, rfl⟩
              exact .succ ((fitAt_iff item scope cursor limit middle first).mp fitted)
                ((inductionHypothesis _ _ _).mp rest)
            · intro fits
              cases fits with
              | succ fitted rest =>
                  exact ⟨_, _, _, (fitAt_iff item scope cursor limit _ _).mpr fitted,
                    (inductionHypothesis _ _ _).mpr rest, rfl⟩
      show countedFit env.doc (fun cursor => fitAt env enter scope item cursor limit) position limit =
        some (stop, claims) ↔ _
      rw [countedFit_eq_some]
      constructor
      · rintro ⟨count, readEq, iterated⟩
        exact .many readEq ((manyIff _ _ _ _).mp iterated)
      · intro fits
        cases fits with
        | many readEq rest => exact ⟨_, readEq, (manyIff _ _ _ _).mpr rest⟩
  | .block statements item, scope, position, limit, stop, claims => by
      show ((readWord env.doc position limit).bind fun word =>
        (blockRange env.doc scope.owner word statements).bind fun (start, length) =>
          (fitAt env enter scope item start (start + length)).bind fun (stop, claims) =>
            (check (stop = start + length)).map fun _ => (position + 1, Claim.block (word - 1) :: claims)) =
          some (stop, claims) ↔ _
      constructor
      · intro fitted
        obtain ⟨word, readEq, rest⟩ := Option.bind_eq_some_iff.mp fitted
        obtain ⟨⟨start, length⟩, rangeEq, rest⟩ := Option.bind_eq_some_iff.mp rest
        obtain ⟨⟨finish, inner⟩, innerEq, rest⟩ := Option.bind_eq_some_iff.mp rest
        obtain ⟨unit, finishEq, pairEq⟩ := Option.map_eq_some_iff.mp rest
        cases pairEq
        have finishIs := check_eq_some.mp finishEq
        cases finishIs
        exact .block readEq rangeEq ((fitAt_iff item scope start (start + length) _ _).mp innerEq)
      · intro fits
        cases fits with
        | block readEq rangeEq inner =>
            rw [readEq]
            refine Option.bind_eq_some_iff.mpr ⟨_, rfl, ?_⟩
            rw [rangeEq]
            refine Option.bind_eq_some_iff.mpr ⟨_, rfl, ?_⟩
            refine Option.bind_eq_some_iff.mpr ⟨_, (fitAt_iff item scope _ _ _ _).mpr inner, ?_⟩
            exact Option.map_eq_some_iff.mpr ⟨(), check_eq_some.mpr rfl, rfl⟩
  | .recur name, scope, position, limit, stop, claims => by
      show ((lookupRecursive env.schema name).bind fun shape =>
        enter scope (.choice shape.tags shape.variants) position limit) = some (stop, claims) ↔ _
      rw [Option.bind_eq_some_iff]
      constructor
      · rintro ⟨shape, lookupEq, enteredEq⟩
        exact .recur lookupEq ((entered _ _ _ _ _ _).mp enteredEq)
      · intro fits
        cases fits with
        | recur lookupEq rest => exact ⟨_, lookupEq, (entered _ _ _ _ _ _).mpr rest⟩
  | .textPattern, scope, position, limit, stop, claims => by
      show ((readWord env.doc position limit).bind fun word =>
        (blockFit env.doc (fun bound cursor => patternFit env enter scope cursor bound) scope.owner word false).bind
          fun holes =>
          (blockRange env.doc scope.owner word false).bind fun (start, _) =>
            (wordAt env.doc start).bind fun count =>
              (iterate (fun cursor => wordFit env scope .textHole cursor limit) count (position + 1)).bind
                fun (middle, tags) =>
                  (iterate (fun cursor => wordFit env scope .stringId cursor limit) (count + 1) middle).map
                    fun (stop, texts) => (stop, Claim.block (word - 1) :: holes ++ (tags ++ texts))) =
          some (stop, claims) ↔ _
      constructor
      · intro fitted
        obtain ⟨word, readEq, rest⟩ := Option.bind_eq_some_iff.mp fitted
        obtain ⟨holes, holesEq, rest⟩ := Option.bind_eq_some_iff.mp rest
        obtain ⟨⟨start, length⟩, rangeEq, rest⟩ := Option.bind_eq_some_iff.mp rest
        obtain ⟨count, countEq, rest⟩ := Option.bind_eq_some_iff.mp rest
        obtain ⟨⟨middle, tags⟩, tagsEq, rest⟩ := Option.bind_eq_some_iff.mp rest
        obtain ⟨⟨finish, texts⟩, textsEq, pairEq⟩ := Option.map_eq_some_iff.mp rest
        cases pairEq
        obtain ⟨start', length', count', rangeEq', countReadEq, iterated⟩ := blockFit_eq_some.mp holesEq
        rw [rangeEq] at rangeEq'
        cases rangeEq'
        rw [readWord_wordAt countReadEq] at countEq
        cases countEq
        exact .textPattern readEq rangeEq countReadEq ((iterate_pattern_iff entered _ _ _ _).mp iterated)
          ((iterate_word_iff _ _ _ _).mp tagsEq) ((iterate_word_iff _ _ _ _).mp textsEq)
      · intro fits
        cases fits with
        | textPattern readEq rangeEq countReadEq holes tags texts =>
            rw [readEq]
            refine Option.bind_eq_some_iff.mpr ⟨_, rfl, ?_⟩
            refine Option.bind_eq_some_iff.mpr ⟨_, blockFit_eq_some.mpr
              ⟨_, _, _, rangeEq, countReadEq, (iterate_pattern_iff entered _ _ _ _).mpr holes⟩, ?_⟩
            rw [rangeEq]
            refine Option.bind_eq_some_iff.mpr ⟨_, rfl, ?_⟩
            dsimp only
            rw [readWord_wordAt countReadEq]
            refine Option.bind_eq_some_iff.mpr ⟨_, rfl, ?_⟩
            refine Option.bind_eq_some_iff.mpr ⟨_, (iterate_word_iff _ _ _ _).mpr tags, ?_⟩
            exact Option.map_eq_some_iff.mpr ⟨_, (iterate_word_iff _ _ _ _).mpr texts, rfl⟩

theorem fitList_iff : ∀ (items : List Shape) (scope : Scope) (position limit stop : Nat) (claims : List Claim),
    fitList env enter scope items position limit = some (stop, claims) ↔
      FitsList env entry scope items position limit stop claims
  | [], scope, position, limit, stop, claims => by
      show some (position, []) = some (stop, claims) ↔ _
      constructor
      · intro same
        cases same
        exact .nil
      · intro fits
        cases fits
        rfl
  | item :: rest, scope, position, limit, stop, claims => by
      show joinFit (fitAt env enter scope item position limit)
        (fun middle => fitList env enter scope rest middle limit) = some (stop, claims) ↔ _
      rw [joinFit_eq_some]
      constructor
      · rintro ⟨middle, first, more, fitted, rested, rfl⟩
        exact .cons ((fitAt_iff item scope position limit middle first).mp fitted)
          ((fitList_iff rest scope middle limit stop more).mp rested)
      · intro fits
        cases fits with
        | cons fitted rested =>
            exact ⟨_, _, _, (fitAt_iff item scope position limit _ _).mpr fitted,
              (fitList_iff rest scope _ limit stop _).mpr rested, rfl⟩

theorem fitChoice_iff : ∀ (variants : List Shape) (tags : List Nat) (tag : Nat) (scope : Scope)
    (position limit stop : Nat) (claims : List Claim),
    fitChoice env enter scope tag tags variants position limit = some (stop, claims) ↔
      FitsChoice env entry scope tag tags variants position limit stop claims
  | [], tags, tag, scope, position, limit, stop, claims => by
      have none : fitChoice env enter scope tag tags [] position limit = none := by
        cases tags <;> rfl
      rw [none]
      constructor
      · intro impossible
        cases impossible
      · intro fits
        cases fits
  | variant :: variants, [], tag, scope, position, limit, stop, claims => by
      show none = some (stop, claims) ↔ _
      constructor
      · intro impossible
        cases impossible
      · intro fits
        cases fits
  | variant :: variants, candidate :: tags, tag, scope, position, limit, stop, claims => by
      show (if candidate = tag then fitAt env enter scope variant position limit
        else fitChoice env enter scope tag tags variants position limit) = some (stop, claims) ↔ _
      by_cases same : candidate = tag
      · subst same
        rw [if_pos rfl, fitAt_iff variant scope position limit stop claims]
        constructor
        · intro fits
          exact .here fits
        · intro fits
          cases fits with
          | here fits => exact fits
          | there different _ => exact absurd rfl different
      · rw [if_neg same, fitChoice_iff variants tags tag scope position limit stop claims]
        constructor
        · intro fits
          exact .there same fits
        · intro fits
          cases fits with
          | here _ => exact absurd rfl same
          | there _ fits => exact fits
end

end

theorem fitFuel_iff (env : Env) : ∀ (fuel : Nat) (scope : Scope) (shape : Shape) (position limit stop : Nat)
    (claims : List Claim),
    fitFuel env fuel scope shape position limit = some (stop, claims) ↔ EntryAt env fuel scope shape position limit stop claims
  | 0, scope, shape, position, limit, stop, claims => by
      show none = some (stop, claims) ↔ False
      constructor
      · intro impossible
        cases impossible
      · intro impossible
        exact impossible.elim
  | fuel + 1, scope, shape, position, limit, stop, claims =>
      fitAt_iff (fitFuel_iff env fuel) shape scope position limit stop claims

section
variable {env : Env} {entry wider : EntrySpec}
  (widened : ∀ scope shape position limit stop claims,
    entry scope shape position limit stop claims → wider scope shape position limit stop claims)
include widened

theorem patternRef_mono {scope : Scope} {position limit : Nat} {claims : List Claim}
    (reference : PatternRef env entry scope position limit claims) : PatternRef env wider scope position limit claims := by
  obtain ⟨word, tag, start, length, inner, readEq, bound, tagEq, rangeEq, fits, claimsEq⟩ := reference
  exact ⟨word, tag, start, length, inner, readEq, bound, tagEq, rangeEq, widened _ _ _ _ _ _ fits, claimsEq⟩

theorem stageRef_mono {scope : Scope} {position limit : Nat} {claims : List Claim}
    (reference : StageRef env entry scope position limit claims) : StageRef env wider scope position limit claims := by
  obtain ⟨word, tag, start, length, inner, readEq, bound, tagEq, rangeEq, fits, claimsEq⟩ := reference
  exact ⟨word, tag, start, length, inner, readEq, bound, tagEq, rangeEq, widened _ _ _ _ _ _ fits, claimsEq⟩

theorem patternRefs_mono {scope : Scope} {limit : Nat} :
    ∀ {count position stop : Nat} {claims : List Claim},
      PatternRefs env entry scope limit count position stop claims → PatternRefs env wider scope limit count position stop claims
  | _, _, _, _, .zero => .zero
  | _, _, _, _, .succ first rest => .succ (patternRef_mono widened first) (patternRefs_mono rest)

mutual
theorem fits_mono {scope : Scope} : ∀ {shape : Shape} {position limit stop : Nat} {claims : List Claim},
    Fits env entry scope shape position limit stop claims → Fits env wider scope shape position limit stop claims
  | _, _, _, _, _, .word notPattern notStage readEq ok => .word notPattern notStage readEq ok
  | _, _, _, _, _, .pattern reference => .pattern (patternRef_mono widened reference)
  | _, _, _, _, _, .stage reference => .stage (stageRef_mono widened reference)
  | _, _, _, _, _, .seq items => .seq (fitsList_mono items)
  | _, _, _, _, _, .choice readEq chosen => .choice readEq (fitsChoice_mono chosen)
  | _, _, _, _, _, .many readEq rest => .many readEq (fitsMany_mono rest)
  | _, _, _, _, _, .block readEq rangeEq inner => .block readEq rangeEq (fits_mono inner)
  | _, _, _, _, _, .recur lookupEq entered => .recur lookupEq (widened _ _ _ _ _ _ entered)
  | _, _, _, _, _, .textPattern readEq rangeEq countEq holes tags texts =>
      .textPattern readEq rangeEq countEq (patternRefs_mono widened holes) tags texts
theorem fitsList_mono {scope : Scope} : ∀ {items : List Shape} {position limit stop : Nat} {claims : List Claim},
    FitsList env entry scope items position limit stop claims → FitsList env wider scope items position limit stop claims
  | _, _, _, _, _, .nil => .nil
  | _, _, _, _, _, .cons first rest => .cons (fits_mono first) (fitsList_mono rest)
theorem fitsChoice_mono {scope : Scope} : ∀ {tag : Nat} {tags : List Nat} {variants : List Shape}
    {position limit stop : Nat} {claims : List Claim},
    FitsChoice env entry scope tag tags variants position limit stop claims →
      FitsChoice env wider scope tag tags variants position limit stop claims
  | _, _, _, _, _, _, _, .here fits => .here (fits_mono fits)
  | _, _, _, _, _, _, _, .there different rest => .there different (fitsChoice_mono rest)
theorem fitsMany_mono {scope : Scope} : ∀ {item : Shape} {limit count position stop : Nat} {claims : List Claim},
    FitsMany env entry scope item limit count position stop claims →
      FitsMany env wider scope item limit count position stop claims
  | _, _, _, _, _, _, .zero => .zero
  | _, _, _, _, _, _, .succ first rest => .succ (fits_mono first) (fitsMany_mono rest)
end

end

theorem entryAt_mono (env : Env) : ∀ (fuel : Nat) (scope : Scope) (shape : Shape) (position limit stop : Nat)
    (claims : List Claim),
    EntryAt env fuel scope shape position limit stop claims → EntryAt env (fuel + 1) scope shape position limit stop claims
  | 0, _, _, _, _, _, _ => fun impossible => impossible.elim
  | fuel + 1, _, _, _, _, _, _ => fits_mono (entryAt_mono env fuel)

theorem fitFuel_mono (env : Env) (fuel : Nat) (scope : Scope) (shape : Shape) (position limit stop : Nat)
    (claims : List Claim) (fitted : fitFuel env fuel scope shape position limit = some (stop, claims)) :
    fitFuel env (fuel + 1) scope shape position limit = some (stop, claims) :=
  (fitFuel_iff env (fuel + 1) scope shape position limit stop claims).mpr
    (entryAt_mono env fuel scope shape position limit stop claims
      ((fitFuel_iff env fuel scope shape position limit stop claims).mp fitted))

theorem resolve_doc {schema : Schema} {doc : Document} {env : Env} (resolved : Env.resolve? schema doc = some env) :
    env.doc = doc := by
  unfold Env.resolve? at resolved
  simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq] at resolved
  obtain ⟨_, _, _, _, _, _, _, _, _, _, rfl⟩ := resolved
  rfl

theorem runJob_iff {env : Env} {job : Job} {claims : List Claim} :
    runJob env job = some claims ↔ JobFits env job claims := by
  unfold runJob JobFits
  rw [Option.map_eq_some_iff]
  constructor
  · rintro ⟨inner, fitted, rfl⟩
    exact ⟨inner, (entryFit_eq_some (fitFuel_iff env (budget env.doc + 1))).mp fitted, rfl⟩
  · rintro ⟨inner, fits, rfl⟩
    exact ⟨inner, (entryFit_eq_some (fitFuel_iff env (budget env.doc + 1))).mpr fits, rfl⟩

theorem mapM_runJob_iff {env : Env} :
    ∀ (jobs : List Job) (claims : List (List Claim)),
      jobs.mapM (runJob env) = some claims ↔ AllFit env jobs claims
  | [], claims => by
      constructor
      · intro same
        cases same
        exact .nil
      · intro related
        cases related
        rfl
  | job :: jobs, claims => by
      rw [List.mapM_cons]
      simp only [Option.bind_eq_bind, Option.bind_eq_some_iff, Option.pure_def, Option.some.injEq]
      constructor
      · rintro ⟨first, firstEq, rest, restEq, rfl⟩
        exact .cons (runJob_iff.mp firstEq) ((mapM_runJob_iff jobs rest).mp restEq)
      · intro related
        cases related with
        | cons first rest =>
            exact ⟨_, runJob_iff.mpr first, _, (mapM_runJob_iff jobs _).mpr rest, rfl⟩

theorem distinct_iff {α : Type} [DecidableEq α] : ∀ (items : List α), distinct items = true ↔ items.Nodup
  | [] => by simp [distinct]
  | item :: rest => by
      rw [List.nodup_cons, ← distinct_iff rest]
      simp only [distinct, Bool.and_eq_true, Bool.not_eq_eq_eq_not, Bool.not_true, List.any_eq_false,
        decide_eq_true_eq]
      constructor
      · rintro ⟨absent, distinctRest⟩
        exact ⟨fun member => absent item member rfl, distinctRest⟩
      · rintro ⟨absent, distinctRest⟩
        exact ⟨fun other member same => absent (same ▸ member), distinctRest⟩

theorem rangeWithin_iff (row : Row) (column limit : Nat) : rangeWithin row column limit = true ↔ RangeWithin row column limit := by
  simp [rangeWithin, RangeWithin]

theorem blockRowOk_iff (doc : Document) (row : Row) : blockRowOk doc row = true ↔ BlockRowWF doc row := by
  simp [blockRowOk, BlockRowWF, rangeWithin_iff, oneBased_iff, and_assoc, or_assoc]

theorem functionRowOk_iff (doc : Document) (row : Row) : functionRowOk doc row = true ↔ FunctionRowWF doc row := by
  simp [functionRowOk, FunctionRowWF, rangeWithin_iff, oneBased_iff, and_assoc]

theorem driverStepRowOk_iff (doc : Document) (row : Row) : driverStepRowOk doc row = true ↔ DriverStepRowWF doc row := by
  simp [driverStepRowOk, DriverStepRowWF, rangeWithin_iff, oneBased_iff, and_assoc]

theorem metadataRowOk_iff (schema : Schema) (doc : Document) (row : Row) :
    metadataRowOk schema doc row = true ↔ MetadataRowWF schema doc row := by
  simp [metadataRowOk, MetadataRowWF, oneBased_iff, and_assoc]

theorem paramRowOk_iff (doc : Document) (row : Row) : paramRowOk doc row = true ↔ ParamRowWF doc row := by
  simp [paramRowOk, ParamRowWF, oneBased_iff, and_assoc]

theorem captureRowOk_iff (doc : Document) (slots : Nat) (row : Row) :
    captureRowOk doc slots row = true ↔ CaptureRowWF doc slots row := by
  simp [captureRowOk, CaptureRowWF, oneBased_iff, and_assoc]

theorem rowsIn_iff (table : List Row) (start length : Nat) (holds : Row → Bool) (rule : Row → Prop)
    (agrees : ∀ row, holds row = true ↔ rule row) :
    rowsIn table start length holds = true ↔ RowsIn table start length rule := by
  unfold rowsIn RowsIn
  simp only [List.all_eq_true, List.mem_range]
  constructor
  · intro all offset within
    have here := all offset within
    cases found : table[start + offset]? with
    | none => rw [found] at here; cases here
    | some row => rw [found] at here; exact ⟨row, rfl, (agrees row).mp here⟩
  · intro all offset within
    obtain ⟨row, found, holds'⟩ := all offset within
    rw [found]
    exact (agrees row).mpr holds'

theorem pairsIn_iff (table : List Row) (start count : Nat) (holds : Row → Row → Bool) (rule : Row → Row → Prop)
    (agrees : ∀ left right, holds left right = true ↔ rule left right) :
    pairsIn table start count holds = true ↔ PairsIn table start count rule := by
  unfold pairsIn PairsIn
  simp only [List.all_eq_true, List.mem_range, Bool.or_eq_true, beq_iff_eq]
  constructor
  · intro all first second firstBelow secondBelow different left right leftEq rightEq
    rcases all first firstBelow second secondBelow with same | here
    · exact absurd same different
    · rw [leftEq, rightEq] at here; exact (agrees _ _).mp here
  · intro all first firstBelow second secondBelow
    by_cases same : first = second
    · exact Or.inl same
    · right
      cases leftEq : table[start + first]? with
      | none => simp
      | some left =>
          cases rightEq : table[start + second]? with
          | none => simp
          | some right =>
              exact (agrees _ _).mpr (all first second firstBelow secondBelow same left right leftEq rightEq)

theorem adjacentIn_iff (table : List Row) (start count : Nat) (holds : Row → Row → Bool) (rule : Row → Row → Prop)
    (agrees : ∀ left right, holds left right = true ↔ rule left right) :
    adjacentIn table start count holds = true ↔ AdjacentIn table start count rule := by
  unfold adjacentIn AdjacentIn
  simp only [List.all_eq_true, List.mem_range, Bool.or_eq_true, Bool.not_eq_true', decide_eq_false_iff_not]
  constructor
  · intro all offset inside left right leftEq rightEq
    rcases all offset (by omega) with outside | here
    · exact absurd inside outside
    · rw [leftEq, rightEq] at here; exact (agrees _ _).mp here
  · intro all offset _
    by_cases inside : offset + 1 < count
    · right
      cases leftEq : table[start + offset]? with
      | none => simp
      | some left =>
          cases rightEq : table[start + offset + 1]? with
          | none => simp
          | some right => exact (agrees _ _).mpr (all offset inside left right leftEq rightEq)
    · exact Or.inl inside

theorem stepSlotsOk_iff (doc : Document) : stepSlotsOk doc = true ↔ StepSlotsWF doc := by
  unfold stepSlotsOk StepSlotsWF
  simp only [List.all_eq_true, List.mem_range]
  constructor
  · intro all index row tag rowEq tagEq
    have inside := (List.getElem?_eq_some_iff.mp rowEq).1
    have here := all index inside
    rw [rowEq, tagEq] at here
    simp only [Bool.and_eq_true] at here
    exact ⟨(rowsIn_iff _ _ _ _ _ (fun _ => decide_eq_true_iff)).mp here.1,
      (pairsIn_iff _ _ _ _ _ (fun _ _ => decide_eq_true_iff)).mp here.2⟩
  · intro all index _
    cases rowEq : doc.driverSteps[index]? with
    | none => simp
    | some row =>
        cases tagEq : doc.driverStepTags[index]? with
        | none => simp
        | some tag =>
            obtain ⟨rows, pairs⟩ := all index row tag rowEq tagEq
            simp only [Bool.and_eq_true]
            exact ⟨(rowsIn_iff _ _ _ _ _ (fun _ => decide_eq_true_iff)).mpr rows,
              (pairsIn_iff _ _ _ _ _ (fun _ _ => decide_eq_true_iff)).mpr pairs⟩

theorem regionsTileOk_iff (doc : Document) (program : Row) : regionsTileOk doc program = true ↔ RegionsTile doc program := by
  unfold regionsTileOk RegionsTile
  rw [Bool.and_eq_true, rowsIn_iff _ _ _ _ _ (fun _ => decide_eq_true_iff), decide_eq_true_iff]

theorem syncExactOk_iff (doc : Document) (region : Row) : syncExactOk doc region = true ↔ SyncExact doc region := by
  unfold syncExactOk SyncExact
  rw [Bool.and_eq_true, Bool.and_eq_true, rowsIn_iff _ _ _ _ _ (fun _ => decide_eq_true_iff),
    adjacentIn_iff _ _ _ _ _ (fun _ _ => decide_eq_true_iff), decide_eq_true_iff, and_assoc]

theorem functionTablesOk_iff (doc : Document) (row : Row) :
    functionTablesOk doc row = true ↔ FunctionTablesWF doc row := by
  unfold functionTablesOk FunctionTablesWF
  rw [Bool.and_eq_true, rowsIn_iff _ _ _ _ _ (paramRowOk_iff doc), rowsIn_iff _ _ _ _ _ (captureRowOk_iff doc _)]

theorem transfersOk_iff (schema : Schema) (doc : Document) : transfersOk schema doc = true ↔ TransfersWF schema doc := by
  unfold transfersOk TransfersWF
  simp only [List.all_eq_true, List.mem_range]
  constructor
  · intro all index flag tag flagEq tagEq marked
    have within : index < doc.resourceTransfers.length := by
      rcases List.getElem?_eq_some_iff.mp flagEq with ⟨within, _⟩
      exact within
    have here := all index within
    rw [flagEq, tagEq] at here
    simpa [transferOk, marked] using here
  · intro all index _
    cases flagEq : doc.resourceTransfers[index]? with
    | none => simp [transferOk]
    | some flag =>
        cases tagEq : doc.tags[index]? with
        | none => simp [transferOk]
        | some tag =>
            simp only [transferOk, decide_eq_true_eq]
            exact all index flag tag flagEq tagEq

theorem segmentsOk_iff (doc : Document) : segmentsOk doc = true ↔ SegmentsWF doc := by
  unfold segmentsOk SegmentsWF
  cases starts : ownerStarts doc with
  | nil => simp
  | cons first rest => simp

theorem lengthsOk_iff (doc : Document) : lengthsOk doc = true ↔ LengthsWF doc := by
  simp [lengthsOk, LengthsWF, and_assoc]

theorem scalarsOk_iff (doc : Document) : scalarsOk doc = true ↔ ScalarsWF doc := by
  simp [scalarsOk, ScalarsWF, distinct_iff, oneBased_iff, and_assoc]

theorem rowsOk_iff (schema : Schema) (doc : Document) : rowsOk schema doc = true ↔ RowsWF schema doc := by
  simp [rowsOk, RowsWF, blockRowOk_iff, functionRowOk_iff, metadataRowOk_iff, functionTablesOk_iff,
    driverStepRowOk_iff, stepSlotsOk_iff, regionsTileOk_iff, syncExactOk_iff, rangeWithin_iff, and_assoc]

theorem poolsOk_iff (schema : Schema) (doc : Document) : poolsOk schema doc = true ↔ PoolsWF schema doc := by
  simp [poolsOk, PoolsWF, rangeWithin_iff, and_assoc]

theorem tablesOk_iff (env : Env) : tablesOk env = true ↔ TablesWF env := by
  simp [tablesOk, TablesWF, TablesWFOf, lengthsOk_iff, scalarsOk_iff, transfersOk_iff, segmentsOk_iff, rowsOk_iff,
    poolsOk_iff, and_assoc]

theorem shapedOk_iff (doc : Document) : shapedOk doc = true ↔ doc.WellShaped := by
  simp [shapedOk, Document.WellShaped, and_assoc]

theorem program_check_iff (claims : List Claim) (program root : Nat) :
    (claims.contains (.program program) == decide (program + 1 ≠ root)) = true ↔
      (Claim.program program ∈ claims ↔ program + 1 ≠ root) := by
  by_cases claimed : Claim.program program ∈ claims <;> by_cases rooted : program + 1 = root <;>
    simp [claimed, rooted]

theorem block_check_iff (claims : List Claim) (block : Nat) (owner : Nat) :
    (decide (owner = irNone) || claims.contains (.block block)) = true ↔ (owner ≠ irNone → Claim.block block ∈ claims) := by
  by_cases owned : owner = irNone <;> by_cases claimed : Claim.block block ∈ claims <;> simp [owned, claimed]

theorem claimsOk_iff (doc : Document) (claims : List Claim) : claimsOk doc claims = true ↔ ClaimsWF doc claims := by
  simp only [claimsOk, ClaimsWF, Bool.and_eq_true, distinct_iff, List.all_eq_true, List.mem_range,
    List.contains_iff_mem, block_check_iff, program_check_iff, and_assoc]

theorem verify_iff (schema : Schema) (doc : Document) : verify schema doc = true ↔ StructuralWF schema doc := by
  unfold verify StructuralWF
  cases resolved : Env.resolve? schema doc with
  | none =>
      simp only [Bool.and_false, Bool.false_eq_true, false_iff]
      rintro ⟨_, env, impossible, _⟩
      cases impossible
  | some env =>
      have docEq := resolve_doc resolved
      subst docEq
      cases jobsEq : documentJobs env with
      | none =>
          simp only [jobsEq, Bool.and_false, Bool.false_eq_true, false_iff]
          rintro ⟨_, env', same, _, jobs, impossible, _⟩
          cases same
          rw [jobsEq] at impossible
          cases impossible
      | some jobs =>
          cases runEq : jobs.mapM (runJob env) with
          | none =>
              simp only [jobsEq, runEq, Bool.and_false, Bool.false_eq_true, false_iff]
              rintro ⟨_, env', same, _, jobs', jobsEq', claims, related, _⟩
              cases same
              rw [jobsEq] at jobsEq'
              cases jobsEq'
              rw [(mapM_runJob_iff jobs claims).mpr related] at runEq
              cases runEq
          | some claims =>
              simp only [jobsEq, runEq]
              rw [Bool.and_eq_true, Bool.and_eq_true, shapedOk_iff, tablesOk_iff, claimsOk_iff]
              constructor
              · intro accepted
                exact ⟨accepted.1, env, rfl, accepted.2.1, jobs, jobsEq, claims, (mapM_runJob_iff jobs claims).mp runEq,
                  accepted.2.2⟩
              · rintro ⟨shaped, env', same, tables, jobs', jobsEq', claims', related, claimsWF⟩
                cases same
                rw [jobsEq] at jobsEq'
                cases jobsEq'
                rw [(mapM_runJob_iff jobs claims').mpr related] at runEq
                cases runEq
                exact ⟨shaped, tables, claimsWF⟩

theorem ofValues_inverse {values : List WireValue} {doc : Document} (decoded : Document.ofValues? values = some doc) :
    doc.WellShaped ∧ values = doc.wireValues := by
  unfold Document.ofValues? at decoded
  split at decoded
  · next schemaValue scopeValue value0 value1 value2 value3 value4 value5 value6 value7 value8 value9 value10 value11 value12 value13 value14 value15 value16 value17 value18 value19 value20 value21 value22 value23 value24 value25 value26 value27 value28 value29 value30 value31 value32 value33 value34 value35 value36 value37 value38 value39 value40 value41 value42 value43 value44 =>
    obtain ⟨⟨⟩, schemaDecoded, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨⟨⟩, scopeDecoded, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨symbolBase, decoded0, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨symbols, decoded1, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨sourceId, decoded2, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨sourceCount, decoded3, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨tags, decoded4, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨data, decoded5, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨resourceTransfers, decoded6, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨checkedCallTargetRequired, decoded7, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨extra, decoded8, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨patterns, decoded9, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨patternData, decoded10, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨stages, decoded11, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨stageData, decoded12, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨values, decoded13, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨valueData, decoded14, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨blocks, decoded15, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨driverStepTags, decoded16, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨driverSteps, decoded17, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨driverSlots, decoded18, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨driverWriters, decoded19, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨driverSync, decoded20, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨driverRegions, decoded21, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨driverPrograms, decoded22, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨bindingIdentities, decoded23, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨driverRoot, decoded24, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨functions, decoded25, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨functionInstructionStarts, decoded26, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨functionMetadata, decoded27, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨params, decoded28, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨paramCold, decoded29, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨captures, decoded30, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨validations, decoded31, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨slotTypeFacts, decoded32, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨strings, decoded33, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨stringBytes, decoded34, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨bytes, decoded35, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨byteData, decoded36, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨locations, decoded37, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨locationSources, decoded38, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨runtimeOps, decoded39, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨assignOps, decoded40, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨binaryOps, decoded41, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨runKinds, decoded42, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨redirectionKinds, decoded43, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    obtain ⟨poolCounts, decoded44, decoded⟩ := Option.bind_eq_some_iff.mp decoded
    cases decoded
    have wire0 := decodeWord_inverse decoded0
    have wire1 := decodeTexts_inverse decoded1
    have wire2 := decodeWord_inverse decoded2
    have wire3 := decodeWord_inverse decoded3
    have wire4 := decodeTags_inverse decoded4
    obtain ⟨wire5, shaped5⟩ := decodeRows_inverse decoded5
    have wire6 := decodeWords_inverse decoded6
    have wire7 := decodeWords_inverse decoded7
    have wire8 := decodeWords_inverse decoded8
    have wire9 := decodeTags_inverse decoded9
    obtain ⟨wire10, shaped10⟩ := decodeRows_inverse decoded10
    have wire11 := decodeTags_inverse decoded11
    obtain ⟨wire12, shaped12⟩ := decodeRows_inverse decoded12
    have wire13 := decodeTags_inverse decoded13
    obtain ⟨wire14, shaped14⟩ := decodeRows_inverse decoded14
    obtain ⟨wire15, shaped15⟩ := decodeRows_inverse decoded15
    have wire16 := decodeTags_inverse decoded16
    obtain ⟨wire17, shaped17⟩ := decodeRows_inverse decoded17
    obtain ⟨wire18, shaped18⟩ := decodeRows_inverse decoded18
    obtain ⟨wire19, shaped19⟩ := decodeRows_inverse decoded19
    obtain ⟨wire20, shaped20⟩ := decodeRows_inverse decoded20
    obtain ⟨wire21, shaped21⟩ := decodeRows_inverse decoded21
    obtain ⟨wire22, shaped22⟩ := decodeRows_inverse decoded22
    obtain ⟨wire23, shaped23⟩ := decodeRows_inverse decoded23
    have wire24 := decodeWord_inverse decoded24
    obtain ⟨wire25, shaped25⟩ := decodeRows_inverse decoded25
    have wire26 := decodeWords_inverse decoded26
    obtain ⟨wire27, shaped27⟩ := decodeRows_inverse decoded27
    obtain ⟨wire28, shaped28⟩ := decodeRows_inverse decoded28
    obtain ⟨wire29, shaped29⟩ := decodeRows_inverse decoded29
    obtain ⟨wire30, shaped30⟩ := decodeRows_inverse decoded30
    obtain ⟨wire31, shaped31⟩ := decodeRows_inverse decoded31
    have wire32 := decodeWords_inverse decoded32
    obtain ⟨wire33, shaped33⟩ := decodeRows_inverse decoded33
    have wire34 := decodeBytes_inverse decoded34
    obtain ⟨wire35, shaped35⟩ := decodeRows_inverse decoded35
    have wire36 := decodeBytes_inverse decoded36
    obtain ⟨wire37, shaped37⟩ := decodeRows_inverse decoded37
    have wire38 := decodeWords_inverse decoded38
    have wire39 := decodeWords_inverse decoded39
    have wire40 := decodeWords_inverse decoded40
    have wire41 := decodeWords_inverse decoded41
    have wire42 := decodeWords_inverse decoded42
    have wire43 := decodeWords_inverse decoded43
    have wire44 := decodeWords_inverse decoded44
    refine ⟨⟨shaped5, shaped10, shaped12, shaped14, shaped15, shaped17, shaped18, shaped19, shaped20, shaped21, shaped22, shaped23, shaped25, shaped27, shaped28, shaped29, shaped30, shaped31, shaped33, shaped35, shaped37⟩, ?_⟩
    rw [expectText_inverse schemaDecoded, expectText_inverse scopeDecoded, wire0, wire1, wire2, wire3, wire4, wire5, wire6, wire7, wire8, wire9, wire10, wire11, wire12, wire13, wire14, wire15, wire16, wire17, wire18, wire19, wire20, wire21, wire22, wire23, wire24, wire25, wire26, wire27, wire28, wire29, wire30, wire31, wire32, wire33, wire34, wire35, wire36, wire37, wire38, wire39, wire40, wire41, wire42, wire43, wire44]
    rfl
  · cases decoded

theorem decode_inverse {value : WireValue} {doc : Document} (decoded : Document.decode? value = some doc) :
    doc.WellShaped ∧ value = doc.encode := by
  cases value with
  | object fields =>
      change (if fields.map Prod.fst = documentFields then Document.ofValues? (fields.map Prod.snd) else none) =
        some doc at decoded
      by_cases named : fields.map Prod.fst = documentFields
      · rw [if_pos named] at decoded
        obtain ⟨shaped, valuesEq⟩ := ofValues_inverse decoded
        refine ⟨shaped, ?_⟩
        show WireValue.object fields = .object (documentFields.zip doc.wireValues)
        rw [← List.zip_of_prod named valuesEq]
      · rw [if_neg named] at decoded
        cases decoded
  | string _ => cases decoded
  | array _ => cases decoded

theorem document_decode_iff (value : WireValue) (doc : Document) :
    Document.decode? value = some doc ↔ doc.WellShaped ∧ value = doc.encode := by
  constructor
  · intro decoded
    exact decode_inverse decoded
  · rintro ⟨shaped, rfl⟩
    exact decode_encode doc shaped

theorem decoded_document_is_canonical (codes : ExchangeCodes) (doc : Document)
    (decoded : decodeDocumentCodes codes = some doc) :
    doc.WellShaped ∧ parseRawIrCodes codes = some doc.encode := by
  unfold decodeDocumentCodes at decoded
  obtain ⟨wire, parsed, documentDecoded⟩ := Option.bind_eq_some_iff.mp decoded
  obtain ⟨shaped, wireEq⟩ := decode_inverse documentDecoded
  exact ⟨shaped, by rw [parsed, wireEq]⟩

end Xsh.IR.Raw
