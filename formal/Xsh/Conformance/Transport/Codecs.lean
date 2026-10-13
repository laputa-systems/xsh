import Xsh.Foundation

namespace Xsh.Conformance.Transport

abbrev Word64 := Fin 18446744073709551616

abbrev WireByte := Fin 256

private def hexDigit (digit : Nat) : Char :=
  Char.ofNat (if digit < 10 then 48 + digit else 87 + digit)

private def hexDigit? (character : Char) : Option (Fin 16) :=
  let code := character.toNat
  if decimal : 48 ≤ code ∧ code ≤ 57 then
    some ⟨code - 48, by omega⟩
  else if lowercase : 97 ≤ code ∧ code ≤ 102 then
    some ⟨code - 87, by omega⟩
  else
    none

private def fixedHexCharacters (width value : Nat) : List Char :=
  (List.range width).reverse.map fun position =>
    hexDigit (value / 16 ^ position % 16)

private def decodeHexAcc (accumulator : Nat) : List Char → Option Nat
  | [] => some accumulator
  | character :: rest => do
      let digit ← hexDigit? character
      decodeHexAcc (accumulator * 16 + digit.val) rest

/-- The private bridge writes every stored u32 word and table ID as eight
lowercase hexadecimal digits, preserving leading zeroes. -/
def encodeHex32 (value : Xsh.Foundation.Word32) : String :=
  String.ofList (fixedHexCharacters 8 value.val)

/-- Decode exactly eight lowercase hexadecimal digits without truncation. -/
def decodeHex32? (text : String) : Option Xsh.Foundation.Word32 := do
  if text.toList.length != 8 then none else pure ()
  let value ← decodeHexAcc 0 text.toList
  Xsh.Foundation.Word32.decode? value

/-- The private bridge writes every u64 bit pattern as sixteen lowercase
hexadecimal digits. -/
def encodeHex64 (value : Word64) : String :=
  String.ofList (fixedHexCharacters 16 value.val)

/-- Decode exactly sixteen lowercase hexadecimal digits to a u64 bit pattern. -/
def decodeHex64? (text : String) : Option Word64 := do
  if text.toList.length != 16 then none else pure ()
  let value ← decodeHexAcc 0 text.toList
  if fits : value < 18446744073709551616 then
    some ⟨value, fits⟩
  else
    none

private def encodeByte (value : WireByte) : List Char :=
  [hexDigit (value.val / 16), hexDigit (value.val % 16)]

private def encodeByteList : List WireByte → List Char
  | [] => []
  | byte :: rest => encodeByte byte ++ encodeByteList rest

/-- Native path bytes and byte payloads use an even number of lowercase hex
digits, with each byte emitted high nibble first. -/
def encodeHexBytes (bytes : List WireByte) : String :=
  String.ofList (encodeByteList bytes)

private def decodeByteList : List Char → Option (List WireByte)
  | [] => some []
  | high :: low :: rest => do
      let highDigit ← hexDigit? high
      let lowDigit ← hexDigit? low
      let tail ← decodeByteList rest
      some (⟨highDigit.val * 16 + lowDigit.val, by
        have highBound := highDigit.isLt
        have lowBound := lowDigit.isLt
        omega⟩ :: tail)
  | [_] => none

/-- Decode an even-length lowercase hexadecimal byte string. -/
def decodeHexBytes? (text : String) : Option (List WireByte) :=
  decodeByteList text.toList

/-- The bridge reserves raw zero and `IR_NONE` and numbers present table
entries from one. -/
structure BridgeId where
  raw : Xsh.Foundation.Word32
  nonzero : raw.val ≠ 0
  notNone : raw.val ≠ 4294967295
  deriving DecidableEq

def BridgeId.fromIndex? (index : Nat) : Option BridgeId :=
  if fits : index + 1 < 4294967295 then
    have nonzero : index + 1 ≠ 0 := by omega
    have notNone : index + 1 ≠ 4294967295 := by omega
    some ⟨⟨index + 1, by omega⟩, nonzero, notNone⟩
  else
    none

def BridgeId.fromRaw? (raw : Xsh.Foundation.Word32) : Option BridgeId :=
  if valid : raw.val ≠ 0 ∧ raw.val ≠ 4294967295 then
    some ⟨raw, valid.1, valid.2⟩
  else
    none

def BridgeId.toHex (id : BridgeId) : String := encodeHex32 id.raw

def BridgeId.fromHex? (text : String) : Option BridgeId :=
  (decodeHex32? text).bind BridgeId.fromRaw?

structure OptionalBridgeId where
  value : Option BridgeId
  deriving DecidableEq

def OptionalBridgeId.fromRaw? (raw : Xsh.Foundation.Word32) : Option OptionalBridgeId :=
  if raw.val = 4294967295 then
    some ⟨none⟩
  else
    (BridgeId.fromRaw? raw).map fun id => ⟨some id⟩

def OptionalBridgeId.raw (id : OptionalBridgeId) : Xsh.Foundation.Word32 :=
  match id.value with
  | none => ⟨4294967295, by omega⟩
  | some present => present.raw

def OptionalBridgeId.toHex (id : OptionalBridgeId) : String := encodeHex32 id.raw

def OptionalBridgeId.fromHex? (text : String) : Option OptionalBridgeId :=
  (decodeHex32? text).bind OptionalBridgeId.fromRaw?

/-- A canonical encoded hex character is an ASCII digit or a lowercase `a` through `f`. -/
def IsCanonicalHexDigit (character : Char) : Prop :=
  (48 ≤ character.toNat ∧ character.toNat ≤ 57) ∨
  (97 ≤ character.toNat ∧ character.toNat ≤ 102)

private theorem char_ofNat_small_toNat (code : Nat) (bound : code < 55296) :
    (Char.ofNat code).toNat = code := by
  have valid : Nat.isValidChar code := by unfold Nat.isValidChar; omega
  simp [Char.ofNat, Char.ofNatAux, Nat.isValidChar, valid]

private theorem hexDigit_toNat (digit : Nat) (bound : digit < 16) :
    (hexDigit digit).toNat = if digit < 10 then 48 + digit else 87 + digit := by
  unfold hexDigit
  by_cases decimal : digit < 10
  · rw [ite_eq_left decimal, char_ofNat_small_toNat _ (by omega)]
  · rw [ite_eq_right decimal, char_ofNat_small_toNat _ (by omega)]

private theorem decode_hex_digit (digit : Nat) (bound : digit < 16) :
    hexDigit? (hexDigit digit) = some ⟨digit, bound⟩ := by
  by_cases decimal : digit < 10
  · have charBounds : 48 ≤ 48 + digit ∧ 48 + digit ≤ 57 := by omega
    have charCode : (hexDigit digit).toNat = 48 + digit := by
      rw [hexDigit_toNat digit bound]
      simp [decimal]
    have firstDigit : 48 ≤ 48 + digit ∧ 48 + digit ≤ 57 := charBounds
    simp [hexDigit?, charCode, firstDigit]
  · have charBounds : 97 ≤ 87 + digit ∧ 87 + digit ≤ 102 := by omega
    have charCode : (hexDigit digit).toNat = 87 + digit := by
      rw [hexDigit_toNat digit bound]
      simp [decimal]
    have notDecimal : ¬(48 ≤ 87 + digit ∧ 87 + digit ≤ 57) := by omega
    simp [hexDigit?, charCode, notDecimal, charBounds] <;> omega

private theorem decode_hex_digit_inverse {character : Char} {digit : Fin 16}
    (decoded : hexDigit? character = some digit) :
    hexDigit digit.val = character := by
  have charBounds :
      (48 ≤ character.toNat ∧ character.toNat ≤ 57) ∨
        (97 ≤ character.toNat ∧ character.toNat ≤ 102) := by
    unfold hexDigit? at decoded
    by_cases decimal : 48 ≤ character.toNat ∧ character.toNat ≤ 57
    · exact Or.inl decimal
    · by_cases lowercase : 97 ≤ character.toNat ∧ character.toNat ≤ 102
      · exact Or.inr lowercase
      · simp [decimal, lowercase] at decoded
  apply Char.ext
  apply UInt32.toNat.inj
  change (hexDigit digit.val).toNat = character.toNat
  rw [hexDigit_toNat _ digit.isLt]
  rcases charBounds with decimal | lowercase
  · have finEq : (⟨character.toNat - 48, by omega⟩ : Fin 16) = digit := by
      simpa [hexDigit?, decimal] using decoded
    have valueEq : digit.val = character.toNat - 48 := by
      simpa using congrArg Fin.val finEq.symm
    have digitSmall : character.toNat - 48 < 10 := by omega
    simp [digitSmall, valueEq]
    omega
  · have notDecimal : ¬(48 ≤ character.toNat ∧ character.toNat ≤ 57) := by omega
    have finEq : (⟨character.toNat - 87, by omega⟩ : Fin 16) = digit := by
      simpa [hexDigit?, notDecimal, lowercase] using decoded
    have valueEq : digit.val = character.toNat - 87 := by
      simpa using congrArg Fin.val finEq.symm
    have notDigitSmall : ¬character.toNat - 87 < 10 := by omega
    simp [notDigitSmall, valueEq]
    omega

private theorem fixedHexCharacters_succ (width value : Nat) :
    fixedHexCharacters (width + 1) value =
      hexDigit (value / 16 ^ width % 16) :: fixedHexCharacters width value := by
  simp [fixedHexCharacters, List.range_succ]

private theorem fixedHexCharacters_length (width value : Nat) :
    (fixedHexCharacters width value).length = width := by
  simp [fixedHexCharacters]

private theorem decodeHexAcc_accumulator (accumulator : Nat) (characters : List Char) :
    decodeHexAcc accumulator characters =
      (decodeHexAcc 0 characters).map
        (fun value => accumulator * 16 ^ characters.length + value) := by
  induction characters generalizing accumulator with
  | nil =>
      simp [decodeHexAcc]
  | cons character rest ih =>
      cases parsedDigit : hexDigit? character with
      | none => simp [decodeHexAcc, parsedDigit]
      | some digitValue =>
          simp [decodeHexAcc, parsedDigit]
          rw [ih (accumulator := accumulator * 16 + digitValue.val)]
          rw [ih (accumulator := digitValue.val)]
          cases decoded : decodeHexAcc 0 rest with
          | none => simp
          | some value =>
              simp only [Option.map_some]
              have arithmetic :
                  (accumulator * 16 + digitValue.val) * 16 ^ rest.length + value =
                    accumulator * (16 ^ rest.length * 16) +
                      (digitValue.val * 16 ^ rest.length + value) := by
                rw [Nat.add_mul, Nat.mul_assoc accumulator 16,
                  Nat.mul_comm 16 (16 ^ rest.length)]
                simp [Nat.add_assoc]
              rw [arithmetic, Nat.pow_succ]

private theorem decodeHexAcc_shift (accumulator : Nat) (characters : List Char) :
    ∀ {value}, decodeHexAcc accumulator characters = some value →
      ∃ decoded, decodeHexAcc 0 characters = some decoded ∧
        value = accumulator * 16 ^ characters.length + decoded := by
  intro value accepted
  cases zeroDecoded : decodeHexAcc 0 characters with
  | none =>
      have relation := decodeHexAcc_accumulator accumulator characters
      rw [accepted, zeroDecoded] at relation
      have impossible : False := by simpa using relation
      exact False.elim impossible
  | some decoded =>
      have relation := decodeHexAcc_accumulator accumulator characters
      rw [accepted, zeroDecoded] at relation
      simp only [Option.map_some] at relation
      exact ⟨decoded, rfl, Option.some.inj relation⟩

private theorem decodeHexAcc_zero_bound {characters : List Char} {value : Nat}
    (accepted : decodeHexAcc 0 characters = some value) :
    value < 16 ^ characters.length := by
  induction characters generalizing value with
  | nil =>
      simp [decodeHexAcc] at accepted
      subst value
      simp
  | cons character rest ih =>
      cases parsedDigit : hexDigit? character with
      | none => simp [decodeHexAcc, parsedDigit] at accepted
      | some digitValue =>
          have tailAccepted : decodeHexAcc digitValue.val rest = some value := by
            simpa [decodeHexAcc, parsedDigit] using accepted
          obtain ⟨restValue, restDecoded, valueEq⟩ :=
            decodeHexAcc_shift digitValue.val rest tailAccepted
          have restBound := ih restDecoded
          have digitBound : digitValue.val ≤ 15 := by omega
          have digitProductBound :=
            Nat.mul_le_mul_right (16 ^ rest.length) digitBound
          rw [List.length_cons, Nat.pow_succ, valueEq]
          omega

private theorem decodeHexAcc_zero_injective (left right : List Char) (value : Nat)
    (sameLength : left.length = right.length)
    (leftDecoded : decodeHexAcc 0 left = some value)
    (rightDecoded : decodeHexAcc 0 right = some value) :
    left = right := by
  induction left generalizing right value with
  | nil =>
      cases right with
      | nil => rfl
      | cons character rest => simp at sameLength
  | cons character rest ih =>
      cases right with
      | nil => simp at sameLength
      | cons other otherRest =>
          have restLength : rest.length = otherRest.length := by
            simp only [List.length_cons] at sameLength
            omega
          cases parsedLeftDigit : hexDigit? character with
          | none => simp [decodeHexAcc, parsedLeftDigit] at leftDecoded
          | some leftDigitValue =>
              cases parsedRightDigit : hexDigit? other with
              | none => simp [decodeHexAcc, parsedRightDigit] at rightDecoded
              | some rightDigitValue =>
                  have leftTail : decodeHexAcc leftDigitValue.val rest = some value := by
                    simpa [decodeHexAcc, parsedLeftDigit] using leftDecoded
                  have rightTail : decodeHexAcc rightDigitValue.val otherRest = some value := by
                    simpa [decodeHexAcc, parsedRightDigit] using rightDecoded
                  obtain ⟨leftValue, leftZero, leftEquation⟩ :=
                    decodeHexAcc_shift leftDigitValue.val rest leftTail
                  obtain ⟨rightValue, rightZero, rightEquation⟩ :=
                    decodeHexAcc_shift rightDigitValue.val otherRest rightTail
                  have leftBound := decodeHexAcc_zero_bound leftZero
                  have rightBound := decodeHexAcc_zero_bound rightZero
                  have rightBound' : rightValue < 16 ^ rest.length := by
                    simpa [restLength] using rightBound
                  have rightEquation' :
                      value = rightDigitValue.val * 16 ^ rest.length + rightValue := by
                    simpa [restLength] using rightEquation
                  have digitEq : leftDigitValue.val = rightDigitValue.val := by
                    by_cases equal : leftDigitValue.val = rightDigitValue.val
                    · exact equal
                    · have order : leftDigitValue.val < rightDigitValue.val ∨
                          rightDigitValue.val < leftDigitValue.val := by omega
                      rcases order with order | order
                      · have upper : leftDigitValue.val * 16 ^ rest.length + leftValue <
                            (leftDigitValue.val + 1) * 16 ^ rest.length := by
                          calc
                            leftDigitValue.val * 16 ^ rest.length + leftValue <
                                leftDigitValue.val * 16 ^ rest.length + 16 ^ rest.length :=
                                  Nat.add_lt_add_left leftBound _
                            _ = (leftDigitValue.val + 1) * 16 ^ rest.length := by
                                  simp [Nat.add_mul]
                        have productOrder : (leftDigitValue.val + 1) * 16 ^ rest.length ≤
                            rightDigitValue.val * 16 ^ rest.length :=
                          Nat.mul_le_mul_right _ (by omega)
                        have valueOrder : rightDigitValue.val * 16 ^ rest.length ≤ value := by
                          rw [rightEquation']
                          exact Nat.le_add_right _ _
                        have : value < value := by
                          calc
                            value = leftDigitValue.val * 16 ^ rest.length + leftValue :=
                                leftEquation
                            _ < (leftDigitValue.val + 1) * 16 ^ rest.length := upper
                            _ ≤ rightDigitValue.val * 16 ^ rest.length := productOrder
                            _ ≤ value := valueOrder
                        omega
                      · have upper : rightDigitValue.val * 16 ^ rest.length + rightValue <
                            (rightDigitValue.val + 1) * 16 ^ rest.length := by
                          calc
                            rightDigitValue.val * 16 ^ rest.length + rightValue <
                                rightDigitValue.val * 16 ^ rest.length + 16 ^ rest.length :=
                                  Nat.add_lt_add_left rightBound' _
                            _ = (rightDigitValue.val + 1) * 16 ^ rest.length := by
                                  simp [Nat.add_mul]
                        have productOrder : (rightDigitValue.val + 1) * 16 ^ rest.length ≤
                            leftDigitValue.val * 16 ^ rest.length :=
                          Nat.mul_le_mul_right _ (by omega)
                        have valueOrder : leftDigitValue.val * 16 ^ rest.length ≤ value := by
                          rw [leftEquation]
                          exact Nat.le_add_right _ _
                        have : value < value := by
                          calc
                            value = rightDigitValue.val * 16 ^ rest.length + rightValue :=
                                rightEquation'
                            _ < (rightDigitValue.val + 1) * 16 ^ rest.length := upper
                            _ ≤ leftDigitValue.val * 16 ^ rest.length := productOrder
                            _ ≤ value := valueOrder
                        omega
                  have rightEquation'' :
                      value = leftDigitValue.val * 16 ^ rest.length + rightValue := by
                    simpa [digitEq] using rightEquation'
                  have tailEq : leftValue = rightValue := by
                    omega
                  have leftCharacter : hexDigit leftDigitValue.val = character :=
                    decode_hex_digit_inverse parsedLeftDigit
                  have rightCharacter : hexDigit rightDigitValue.val = other :=
                    decode_hex_digit_inverse parsedRightDigit
                  have restEq : rest = otherRest :=
                    ih otherRest leftValue restLength leftZero (by
                      rw [tailEq]
                      exact rightZero)
                  have characterEq : character = other := by
                    calc
                      character = hexDigit leftDigitValue.val := leftCharacter.symm
                      _ = hexDigit rightDigitValue.val := congrArg hexDigit digitEq
                      _ = other := rightCharacter
                  simp [characterEq, restEq]

private theorem decodeHexAcc_fixedHexCharacters (width value accumulator : Nat) :
    decodeHexAcc accumulator (fixedHexCharacters width value) =
      some (accumulator * 16 ^ width + value % 16 ^ width) := by
  induction width generalizing accumulator with
  | zero => simp [fixedHexCharacters, decodeHexAcc, Nat.mod_one]
  | succ width ih =>
      rw [fixedHexCharacters_succ]
      have digitBound : value / 16 ^ width % 16 < 16 := Nat.mod_lt _ (by decide)
      simp only [decodeHexAcc, decode_hex_digit _ digitBound]
      simp
      rw [ih, Nat.mod_pow_succ, Nat.pow_succ]
      simp [Nat.mul_add, Nat.mul_assoc, Nat.mul_comm, Nat.add_comm, Nat.add_left_comm]

private theorem fixedHexCharacters_canonical (width value : Nat) :
    ∀ character ∈ fixedHexCharacters width value, IsCanonicalHexDigit character := by
  induction width with
  | zero => simp [fixedHexCharacters]
  | succ width ih =>
      rw [fixedHexCharacters_succ]
      intro character membership
      simp only [List.mem_cons] at membership
      rcases membership with rfl | membership
      · have digitBound : value / 16 ^ width % 16 < 16 := Nat.mod_lt _ (by decide)
        have code := hexDigit_toNat _ digitBound
        unfold IsCanonicalHexDigit
        by_cases decimal : value / 16 ^ width % 16 < 10
        · rw [code]
          simp only [ite_eq_left decimal]
          left
          omega
        · rw [code]
          simp only [ite_eq_right decimal]
          right
          omega
      · exact ih character membership

private theorem hexDigit_isCanonical (digit : Nat) (bound : digit < 16) :
    IsCanonicalHexDigit (hexDigit digit) := by
  have code := hexDigit_toNat digit bound
  unfold IsCanonicalHexDigit
  by_cases decimal : digit < 10
  · rw [code]
    simp only [ite_eq_left decimal]
    left
    omega
  · rw [code]
    simp only [ite_eq_right decimal]
    right
    omega

private theorem encodeByteList_canonical (bytes : List WireByte) :
    ∀ character ∈ encodeByteList bytes, IsCanonicalHexDigit character := by
  induction bytes with
  | nil => simp [encodeByteList]
  | cons byte rest ih =>
      intro character membership
      simp only [encodeByteList, List.mem_append] at membership
      rcases membership with pairMembership | restMembership
      · simp only [encodeByte, List.mem_cons, List.mem_nil_iff, or_false] at pairMembership
        rcases pairMembership with high | low
        · subst character
          apply hexDigit_isCanonical
          have bound := byte.isLt
          omega
        · subst character
          apply hexDigit_isCanonical
          exact Nat.mod_lt _ (by decide)
      · exact ih character restMembership

private theorem encodeByteList_length (bytes : List WireByte) :
    (encodeByteList bytes).length = 2 * bytes.length := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih => simp [encodeByteList, encodeByte, ih] <;> omega

private theorem decodeByteList_encodeByteList (bytes : List WireByte) :
    decodeByteList (encodeByteList bytes) = some bytes := by
  induction bytes with
  | nil => rfl
  | cons byte rest ih =>
      have highBound : byte.val / 16 < 16 := by have bound := byte.isLt; omega
      have lowBound : byte.val % 16 < 16 := Nat.mod_lt _ (by decide)
      simp only [encodeByteList, encodeByte, List.cons_append,
        List.nil_append, decodeByteList, decode_hex_digit _ highBound,
        decode_hex_digit _ lowBound]
      simp [ih]
      apply Fin.ext
      simpa [Nat.mul_comm] using Nat.div_add_mod byte.val 16

private theorem encodeByteList_decodeByteList {bytes : List WireByte} :
    ∀ (characters : List Char), decodeByteList characters = some bytes →
      encodeByteList bytes = characters := by
  induction bytes with
  | nil =>
      intro characters accepted
      cases characters with
      | nil => rfl
      | cons high rest =>
          cases rest with
          | nil => simp [decodeByteList] at accepted
          | cons low rest =>
              cases highDigit : hexDigit? high <;>
                cases lowDigit : hexDigit? low <;>
                cases tailDecoded : decodeByteList rest <;>
                simp [decodeByteList, highDigit, lowDigit, tailDecoded] at accepted
  | cons byte tail ih =>
      intro characters accepted
      cases characters with
      | nil => simp [decodeByteList] at accepted
      | cons high rest =>
          cases rest with
          | nil => simp [decodeByteList] at accepted
          | cons low rest =>
              cases parsedHigh : hexDigit? high with
              | none => simp [decodeByteList, parsedHigh] at accepted
              | some highDigit =>
                  cases parsedLow : hexDigit? low with
                  | none => simp [decodeByteList, parsedHigh, parsedLow] at accepted
                  | some lowDigit =>
                      cases tailDecoded : decodeByteList rest with
                      | none =>
                          simp [decodeByteList, parsedHigh, parsedLow, tailDecoded] at accepted
                      | some decodedTail =>
                          have decodedEq :
                              (⟨highDigit.val * 16 + lowDigit.val, by omega⟩ :: decodedTail) =
                                byte :: tail := by
                            have accepted' :
                                some (⟨highDigit.val * 16 + lowDigit.val, by omega⟩ :: decodedTail) =
                                  some (byte :: tail) := by
                              simpa [decodeByteList, parsedHigh, parsedLow, tailDecoded]
                                using accepted
                            exact Option.some.inj accepted'
                          injection decodedEq with byteEq tailEq
                          have tailDecoded' : decodeByteList rest = some tail := by
                            rw [tailEq] at tailDecoded
                            exact tailDecoded
                          have tailEncoding : encodeByteList tail = rest :=
                            ih rest tailDecoded'
                          have highPart : byte.val / 16 = highDigit.val := by
                            rw [show byte.val = highDigit.val * 16 + lowDigit.val by
                              simpa using congrArg Fin.val byteEq.symm]
                            omega
                          have lowPart : byte.val % 16 = lowDigit.val := by
                            rw [show byte.val = highDigit.val * 16 + lowDigit.val by
                              simpa using congrArg Fin.val byteEq.symm]
                            omega
                          simp [encodeByteList, encodeByte, highPart, lowPart,
                            decode_hex_digit_inverse parsedHigh,
                            decode_hex_digit_inverse parsedLow, tailEncoding]

/-- Every text accepted by the fixed-width Lean u32 codec is its canonical
lowercase encoding. This does not establish Rust bridge conformance. -/
theorem decodeHex32_accepted_reencodes :
    ∀ (text : String) (value : Xsh.Foundation.Word32),
      decodeHex32? text = some value → encodeHex32 value = text := by
  intro text value accepted
  by_cases length : text.toList.length = 8
  · cases rawDecoded : decodeHexAcc 0 text.toList with
    | none => simp [decodeHex32?, length, rawDecoded] at accepted
    | some raw =>
        have rawBound : raw < 4294967296 := by
          have bound := decodeHexAcc_zero_bound rawDecoded
          have power : 16 ^ 8 = 4294967296 := by decide
          rw [length, power] at bound
          exact bound
        have valueEq : (⟨raw, rawBound⟩ : Xsh.Foundation.Word32) = value := by
          have accepted' :
              some (⟨raw, rawBound⟩ : Xsh.Foundation.Word32) = some value := by
            simpa [decodeHex32?, length, rawDecoded,
              Xsh.Foundation.Word32.decode?, rawBound] using accepted
          exact Option.some.inj accepted'
        have textDecoded : decodeHexAcc 0 text.toList = some value.val := by
          have valueRaw : raw = value.val := by
            simpa using congrArg Fin.val valueEq
          rw [rawDecoded]
          rw [valueRaw]
        have encodedDecoded :
            decodeHexAcc 0 (fixedHexCharacters 8 value.val) = some value.val := by
          rw [decodeHexAcc_fixedHexCharacters]
          have bound : value.val < 16 ^ 8 := by
            rw [show 16 ^ 8 = 4294967296 by decide]
            exact value.isLt
          simp [Nat.mod_eq_of_lt bound]
        have sameLength :
            text.toList.length = (fixedHexCharacters 8 value.val).length := by
          rw [length, fixedHexCharacters_length]
        have charactersEq := decodeHexAcc_zero_injective text.toList
          (fixedHexCharacters 8 value.val) value.val sameLength textDecoded encodedDecoded
        calc
          encodeHex32 value = String.ofList (fixedHexCharacters 8 value.val) := rfl
          _ = String.ofList text.toList := congrArg String.ofList charactersEq.symm
          _ = text := by simp
  · simp [decodeHex32?, length] at accepted

/-- Every text accepted by the fixed-width Lean u64 codec is its canonical
lowercase encoding. This does not establish Rust bridge conformance. -/
theorem decodeHex64_accepted_reencodes :
    ∀ (text : String) (value : Word64),
      decodeHex64? text = some value → encodeHex64 value = text := by
  intro text value accepted
  by_cases length : text.toList.length = 16
  · cases rawDecoded : decodeHexAcc 0 text.toList with
    | none => simp [decodeHex64?, length, rawDecoded] at accepted
    | some raw =>
        have rawBound : raw < 18446744073709551616 := by
          have bound := decodeHexAcc_zero_bound rawDecoded
          have power : 16 ^ 16 = 18446744073709551616 := by decide
          rw [length, power] at bound
          exact bound
        have valueEq : (⟨raw, rawBound⟩ : Word64) = value := by
          have accepted' : some (⟨raw, rawBound⟩ : Word64) = some value := by
            simpa [decodeHex64?, length, rawDecoded, rawBound] using accepted
          exact Option.some.inj accepted'
        have textDecoded : decodeHexAcc 0 text.toList = some value.val := by
          have valueRaw : raw = value.val := by
            simpa using congrArg Fin.val valueEq
          rw [rawDecoded]
          rw [valueRaw]
        have encodedDecoded :
            decodeHexAcc 0 (fixedHexCharacters 16 value.val) = some value.val := by
          rw [decodeHexAcc_fixedHexCharacters]
          have bound : value.val < 16 ^ 16 := by
            rw [show 16 ^ 16 = 18446744073709551616 by decide]
            exact value.isLt
          simp [Nat.mod_eq_of_lt bound]
        have sameLength :
            text.toList.length = (fixedHexCharacters 16 value.val).length := by
          rw [length, fixedHexCharacters_length]
        have charactersEq := decodeHexAcc_zero_injective text.toList
          (fixedHexCharacters 16 value.val) value.val sameLength textDecoded encodedDecoded
        calc
          encodeHex64 value = String.ofList (fixedHexCharacters 16 value.val) := rfl
          _ = String.ofList text.toList := congrArg String.ofList charactersEq.symm
          _ = text := by simp
  · simp [decodeHex64?, length] at accepted

/-- Every text accepted by the Lean byte codec is its canonical lowercase
encoding. This does not establish Rust bridge conformance. -/
theorem decodeHexBytes_accepted_reencodes :
    ∀ (text : String) (value : List WireByte),
      decodeHexBytes? text = some value → encodeHexBytes value = text := by
  intro text value accepted
  change decodeByteList text.toList = some value at accepted
  have charactersEq := encodeByteList_decodeByteList text.toList accepted
  calc
    encodeHexBytes value = String.ofList (encodeByteList value) := rfl
    _ = String.ofList text.toList := congrArg String.ofList charactersEq
    _ = text := by simp

theorem encode_hex32_width (value : Xsh.Foundation.Word32) :
    (encodeHex32 value).toList.length = 8 := by
  simp [encodeHex32, fixedHexCharacters_length]

theorem encode_hex64_width (value : Word64) :
    (encodeHex64 value).toList.length = 16 := by
  simp [encodeHex64, fixedHexCharacters_length]

theorem encode_hex_bytes_width (bytes : List WireByte) :
    (encodeHexBytes bytes).toList.length = 2 * bytes.length := by
  simp [encodeHexBytes, encodeByteList_length]

theorem encode_hex32_is_canonical_lowercase (value : Xsh.Foundation.Word32) :
    ∀ character ∈ (encodeHex32 value).toList, IsCanonicalHexDigit character := by
  simpa [encodeHex32] using fixedHexCharacters_canonical 8 value.val

theorem encode_hex64_is_canonical_lowercase (value : Word64) :
    ∀ character ∈ (encodeHex64 value).toList, IsCanonicalHexDigit character := by
  simpa [encodeHex64] using fixedHexCharacters_canonical 16 value.val

theorem encode_hex_bytes_is_canonical_lowercase (bytes : List WireByte) :
    ∀ character ∈ (encodeHexBytes bytes).toList, IsCanonicalHexDigit character := by
  simpa [encodeHexBytes] using encodeByteList_canonical bytes

theorem decode_encode_hex_bytes (bytes : List WireByte) :
    decodeHexBytes? (encodeHexBytes bytes) = some bytes := by
  simp [decodeHexBytes?, encodeHexBytes, decodeByteList_encodeByteList]

theorem decode_encode_hex32 (value : Xsh.Foundation.Word32) :
    decodeHex32? (encodeHex32 value) = some value := by
  have width : 16 ^ 8 = 4294967296 := by decide
  have valueBound : value.val < 16 ^ 8 := by rw [width]; exact value.isLt
  have decoded : decodeHexAcc 0 (fixedHexCharacters 8 value.val) = some value.val := by
    rw [decodeHexAcc_fixedHexCharacters]
    simp [Nat.mod_eq_of_lt valueBound]
  simp [decodeHex32?, encodeHex32, fixedHexCharacters_length, decoded,
    Xsh.Foundation.Word32.decode?, value.isLt]

theorem decode_encode_hex64 (value : Word64) :
    decodeHex64? (encodeHex64 value) = some value := by
  have width : 16 ^ 16 = 18446744073709551616 := by decide
  have valueBound : value.val < 16 ^ 16 := by rw [width]; exact value.isLt
  have decoded : decodeHexAcc 0 (fixedHexCharacters 16 value.val) = some value.val := by
    rw [decodeHexAcc_fixedHexCharacters]
    simp [Nat.mod_eq_of_lt valueBound]
  simp [decodeHex64?, encodeHex64, fixedHexCharacters_length, decoded, value.isLt]

theorem canonical_reencode_hex32 (value : Xsh.Foundation.Word32) :
    (decodeHex32? (encodeHex32 value)).map encodeHex32 = some (encodeHex32 value) := by
  simp [decode_encode_hex32]

theorem canonical_reencode_hex64 (value : Word64) :
    (decodeHex64? (encodeHex64 value)).map encodeHex64 = some (encodeHex64 value) := by
  simp [decode_encode_hex64]

theorem canonical_reencode_hex_bytes (bytes : List WireByte) :
    (decodeHexBytes? (encodeHexBytes bytes)).map encodeHexBytes = some (encodeHexBytes bytes) := by
  simp [decode_encode_hex_bytes]

theorem decode_hex32_zero : decodeHex32? "00000000" = some ⟨0, by decide⟩ := by decide

theorem decode_hex32_max :
    decodeHex32? "ffffffff" = some ⟨4294967295, by decide⟩ := by decide

theorem encode_hex32_keeps_leading_zeroes :
    encodeHex32 ⟨1, by decide⟩ = "00000001" := by decide

theorem u32_wire_round_trip_example :
    decodeHex32? (encodeHex32 ⟨4294967295, by decide⟩) =
      some ⟨4294967295, by decide⟩ := by decide

theorem encode_hex32_max_is_canonical :
    encodeHex32 ⟨4294967295, by decide⟩ = "ffffffff" := by decide

theorem decode_hex32_rejects_wrong_width : decodeHex32? "0000000" = none := by decide

theorem decode_hex32_rejects_uppercase : decodeHex32? "0000000A" = none := by decide

theorem decode_hex32_rejects_invalid_digit : decodeHex32? "0000000g" = none := by decide

theorem decode_hex64_zero : decodeHex64? "0000000000000000" = some ⟨0, by decide⟩ := by decide

theorem decode_hex64_max :
    decodeHex64? "ffffffffffffffff" = some ⟨18446744073709551615, by decide⟩ := by decide

theorem encode_hex64_keeps_leading_zeroes :
    encodeHex64 ⟨1, by decide⟩ = "0000000000000001" := by decide

theorem u64_wire_round_trip_example :
    decodeHex64? (encodeHex64 ⟨18446744073709551615, by decide⟩) =
      some ⟨18446744073709551615, by decide⟩ := by decide

theorem encode_hex64_max_is_canonical :
    encodeHex64 ⟨18446744073709551615, by decide⟩ = "ffffffffffffffff" := by decide

theorem decode_hex64_rejects_short_input : decodeHex64? "fffffffffffffff" = none := by decide

theorem decode_hex64_rejects_uppercase :
    decodeHex64? "000000000000000A" = none := by decide

def byteZero : WireByte := ⟨0, by decide⟩
def byteFifteen : WireByte := ⟨15, by decide⟩
def byteSixteen : WireByte := ⟨16, by decide⟩
def byteMax : WireByte := ⟨255, by decide⟩

theorem bytes_round_trip_example :
    decodeHexBytes? (encodeHexBytes [byteZero, byteFifteen, byteSixteen, byteMax]) =
      some [byteZero, byteFifteen, byteSixteen, byteMax] := by decide

theorem bytes_encoding_is_lowercase_and_ordered :
    encodeHexBytes [byteZero, byteFifteen, byteSixteen, byteMax] = "000f10ff" := by decide

theorem decode_bytes_rejects_odd_width : decodeHexBytes? "0" = none := by decide

theorem decode_bytes_rejects_uppercase : decodeHexBytes? "0A" = none := by decide

theorem decode_bytes_accepts_empty : decodeHexBytes? "" = some [] := by decide

theorem bridge_id_is_one_based :
    (BridgeId.fromIndex? 0).map BridgeId.raw = some ⟨1, by decide⟩ := by decide

theorem bridge_id_uses_last_nonreserved_value :
    (BridgeId.fromIndex? 4294967293).map BridgeId.raw = some ⟨4294967294, by decide⟩ := by decide

theorem bridge_id_rejects_raw_zero :
    BridgeId.fromRaw? ⟨0, by decide⟩ = none := by decide

theorem bridge_id_rejects_ir_none :
    BridgeId.fromRaw? ⟨4294967295, by decide⟩ = none := by decide

theorem bridge_id_wire_round_trip_example :
    BridgeId.fromHex? (BridgeId.toHex ⟨⟨1, by decide⟩, by decide, by decide⟩) =
      some ⟨⟨1, by decide⟩, by decide, by decide⟩ := by decide

theorem bridge_id_wire_encoding_is_one_based :
    BridgeId.toHex ⟨⟨1, by decide⟩, by decide, by decide⟩ = "00000001" := by decide

theorem bridge_id_wire_rejects_zero : BridgeId.fromHex? "00000000" = none := by decide

theorem bridge_id_wire_rejects_ir_none :
    BridgeId.fromHex? "ffffffff" = none := by decide

theorem optional_bridge_id_preserves_ir_none :
    (OptionalBridgeId.fromRaw? ⟨4294967295, by decide⟩).map OptionalBridgeId.raw =
      some ⟨4294967295, by decide⟩ := by decide

theorem optional_bridge_id_wire_round_trip_example :
    (OptionalBridgeId.fromHex? "ffffffff").map OptionalBridgeId.toHex =
      some "ffffffff" := by decide

theorem bridge_id_rejects_reserved_max_index : BridgeId.fromIndex? 4294967294 = none := by decide

end Xsh.Conformance.Transport
