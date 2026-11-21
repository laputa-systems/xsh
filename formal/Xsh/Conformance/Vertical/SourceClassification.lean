import Xsh.Conformance.Vertical.SourceAdmissionBase

namespace Xsh.Conformance.Vertical

namespace SourceAdmission

/-! ## UTF-8 well-formedness

`ValidUtf8` is an independent declarative statement of Unicode Table 3-7
well-formed byte sequences: no overlong forms, no surrogates, and no scalar
above U+10FFFF. `utf8ValidB` decides it. `firstInvalidUtf8` reports the first
offset at which no well-formed prefix extends. -/

def IsAsciiByte (byte : UInt8) : Prop := byte.toNat < 0x80

def IsContByte (byte : UInt8) : Prop := 0x80 ≤ byte.toNat ∧ byte.toNat ≤ 0xBF

def IsLead2Byte (byte : UInt8) : Prop := 0xC2 ≤ byte.toNat ∧ byte.toNat ≤ 0xDF

def IsLead3Byte (byte : UInt8) : Prop := 0xE0 ≤ byte.toNat ∧ byte.toNat ≤ 0xEF

def IsLead4Byte (byte : UInt8) : Prop := 0xF0 ≤ byte.toNat ∧ byte.toNat ≤ 0xF4

/-- Tail conditions of a three-byte sequence's first continuation byte: it is a
continuation byte, an `E0` lead rejects overlong forms, and an `ED` lead rejects
surrogates. -/
def ThreeFirstOk (lead cont : UInt8) : Prop :=
  IsContByte cont ∧ (lead.toNat = 0xE0 → 0xA0 ≤ cont.toNat) ∧
    (lead.toNat = 0xED → cont.toNat ≤ 0x9F)

/-- Tail conditions of a four-byte sequence's first continuation byte: `F0`
rejects overlong forms and `F4` rejects scalars above U+10FFFF. -/
def FourFirstOk (lead cont : UInt8) : Prop :=
  IsContByte cont ∧ (lead.toNat = 0xF0 → 0x90 ≤ cont.toNat) ∧
    (lead.toNat = 0xF4 → cont.toNat ≤ 0x8F)

instance (byte : UInt8) : Decidable (IsAsciiByte byte) := by unfold IsAsciiByte; infer_instance
instance (byte : UInt8) : Decidable (IsContByte byte) := by unfold IsContByte; infer_instance
instance (byte : UInt8) : Decidable (IsLead2Byte byte) := by unfold IsLead2Byte; infer_instance
instance (byte : UInt8) : Decidable (IsLead3Byte byte) := by unfold IsLead3Byte; infer_instance
instance (byte : UInt8) : Decidable (IsLead4Byte byte) := by unfold IsLead4Byte; infer_instance
instance (lead cont : UInt8) : Decidable (ThreeFirstOk lead cont) := by
  unfold ThreeFirstOk; infer_instance
instance (lead cont : UInt8) : Decidable (FourFirstOk lead cont) := by
  unfold FourFirstOk; infer_instance

inductive ValidUtf8 : List UInt8 → Prop where
  | nil : ValidUtf8 []
  | one {byte : UInt8} {rest : List UInt8} :
      IsAsciiByte byte → ValidUtf8 rest → ValidUtf8 (byte :: rest)
  | two {lead cont : UInt8} {rest : List UInt8} :
      IsLead2Byte lead → IsContByte cont → ValidUtf8 rest → ValidUtf8 (lead :: cont :: rest)
  | three {lead first second : UInt8} {rest : List UInt8} :
      IsLead3Byte lead → ThreeFirstOk lead first → IsContByte second → ValidUtf8 rest →
        ValidUtf8 (lead :: first :: second :: rest)
  | four {lead first second third : UInt8} {rest : List UInt8} :
      IsLead4Byte lead → FourFirstOk lead first → IsContByte second → IsContByte third →
        ValidUtf8 rest → ValidUtf8 (lead :: first :: second :: third :: rest)

/-- Boolean form of `ValidUtf8`, with one `if` per condition so each branch is
a plain `ite`. -/
def utf8ValidB : List UInt8 → Bool
  | [] => true
  | byte :: tail =>
      if IsAsciiByte byte then utf8ValidB tail
      else if IsLead2Byte byte then
        match tail with
        | cont :: more => if IsContByte cont then utf8ValidB more else false
        | [] => false
      else if IsLead3Byte byte then
        match tail with
        | first :: second :: more =>
            if ThreeFirstOk byte first then
              if IsContByte second then utf8ValidB more else false
            else false
        | _ => false
      else if IsLead4Byte byte then
        match tail with
        | first :: second :: third :: more =>
            if FourFirstOk byte first then
              if IsContByte second then
                if IsContByte third then utf8ValidB more else false
              else false
            else false
        | _ => false
      else false

theorem not_ascii_of_lead2 {byte : UInt8} (lead : IsLead2Byte byte) : ¬ IsAsciiByte byte := by
  simp only [IsLead2Byte, IsAsciiByte] at *; omega

theorem not_ascii_of_lead3 {byte : UInt8} (lead : IsLead3Byte byte) : ¬ IsAsciiByte byte := by
  simp only [IsLead3Byte, IsAsciiByte] at *; omega

theorem not_ascii_of_lead4 {byte : UInt8} (lead : IsLead4Byte byte) : ¬ IsAsciiByte byte := by
  simp only [IsLead4Byte, IsAsciiByte] at *; omega

theorem not_lead2_of_lead3 {byte : UInt8} (lead : IsLead3Byte byte) : ¬ IsLead2Byte byte := by
  simp only [IsLead2Byte, IsLead3Byte] at *; omega

theorem not_lead2_of_lead4 {byte : UInt8} (lead : IsLead4Byte byte) : ¬ IsLead2Byte byte := by
  simp only [IsLead2Byte, IsLead4Byte] at *; omega

theorem not_lead3_of_lead4 {byte : UInt8} (lead : IsLead4Byte byte) : ¬ IsLead3Byte byte := by
  simp only [IsLead3Byte, IsLead4Byte] at *; omega

theorem ascii_step {byte : UInt8} {rest : List UInt8} (ascii : IsAsciiByte byte) :
    ValidUtf8 (byte :: rest) ↔ ValidUtf8 rest := by
  constructor
  · intro valid
    cases valid with
    | one _ tail => exact tail
    | two lead _ _ => exact absurd ascii (not_ascii_of_lead2 lead)
    | three lead _ _ _ => exact absurd ascii (not_ascii_of_lead3 lead)
    | four lead _ _ _ _ => exact absurd ascii (not_ascii_of_lead4 lead)
  · intro valid; exact .one ascii valid

theorem lead2_step {lead cont : UInt8} {rest : List UInt8} (start : IsLead2Byte lead) :
    ValidUtf8 (lead :: cont :: rest) ↔ IsContByte cont ∧ ValidUtf8 rest := by
  constructor
  · intro valid
    cases valid with
    | one ascii _ => exact absurd ascii (not_ascii_of_lead2 start)
    | two _ contOk tail => exact ⟨contOk, tail⟩
    | three other _ _ _ => exact False.elim ((not_lead2_of_lead3 other) start)
    | four other _ _ _ _ => exact False.elim ((not_lead2_of_lead4 other) start)
  · rintro ⟨contOk, valid⟩; exact .two start contOk valid

theorem lead3_step {lead first second : UInt8} {rest : List UInt8} (start : IsLead3Byte lead) :
    ValidUtf8 (lead :: first :: second :: rest) ↔
      ThreeFirstOk lead first ∧ IsContByte second ∧ ValidUtf8 rest := by
  constructor
  · intro valid
    cases valid with
    | one ascii _ => exact absurd ascii (not_ascii_of_lead3 start)
    | two other _ _ => exact False.elim ((not_lead2_of_lead3 start) other)
    | three _ firstOk secondOk tail => exact ⟨firstOk, secondOk, tail⟩
    | four other _ _ _ _ => exact False.elim ((not_lead3_of_lead4 other) start)
  · rintro ⟨firstOk, secondOk, valid⟩; exact .three start firstOk secondOk valid

theorem lead4_step {lead first second third : UInt8} {rest : List UInt8}
    (start : IsLead4Byte lead) :
    ValidUtf8 (lead :: first :: second :: third :: rest) ↔
      FourFirstOk lead first ∧ IsContByte second ∧ IsContByte third ∧ ValidUtf8 rest := by
  constructor
  · intro valid
    cases valid with
    | one ascii _ => exact absurd ascii (not_ascii_of_lead4 start)
    | two other _ _ => exact False.elim ((not_lead2_of_lead4 start) other)
    | three other _ _ _ => exact False.elim ((not_lead3_of_lead4 start) other)
    | four _ firstOk secondOk thirdOk tail => exact ⟨firstOk, secondOk, thirdOk, tail⟩
  · rintro ⟨firstOk, secondOk, thirdOk, valid⟩; exact .four start firstOk secondOk thirdOk valid

theorem valid_nil : ValidUtf8 ([] : List UInt8) := .nil

theorem valid_singleton {byte : UInt8} : ValidUtf8 [byte] ↔ IsAsciiByte byte := by
  constructor
  · intro valid
    cases valid with
    | one ascii _ => exact ascii
  · intro ascii; exact .one ascii .nil

theorem false_iff_of_refuted {target : Prop} (refuted : ¬ target) :
    (false = true ↔ target) :=
  ⟨fun impossible => False.elim (Bool.false_eq_true.mp impossible), fun valid => False.elim (refuted valid)⟩

theorem not_valid_stray {byte : UInt8} {tail : List UInt8} (ascii : ¬ IsAsciiByte byte)
    (notTwo : ¬ IsLead2Byte byte) (notThree : ¬ IsLead3Byte byte)
    (notFour : ¬ IsLead4Byte byte) : ¬ ValidUtf8 (byte :: tail) := by
  intro valid
  cases valid with
  | one asciiHere _ => exact ascii asciiHere
  | two leadHere _ _ => exact notTwo leadHere
  | three leadHere _ _ _ => exact notThree leadHere
  | four leadHere _ _ _ _ => exact notFour leadHere

theorem not_valid_lone {byte : UInt8} (ascii : ¬ IsAsciiByte byte) :
    ¬ ValidUtf8 [byte] := by
  intro valid
  cases valid with
  | one asciiHere _ => exact ascii asciiHere

theorem not_valid_pair {byte first : UInt8} (ascii : ¬ IsAsciiByte byte)
    (notTwo : ¬ IsLead2Byte byte) : ¬ ValidUtf8 [byte, first] := by
  intro valid
  cases valid with
  | one asciiHere _ => exact ascii asciiHere
  | two leadHere _ _ => exact notTwo leadHere

theorem not_valid_triple {byte first second : UInt8} (ascii : ¬ IsAsciiByte byte)
    (notTwo : ¬ IsLead2Byte byte) (notThree : ¬ IsLead3Byte byte) :
    ¬ ValidUtf8 [byte, first, second] := by
  intro valid
  cases valid with
  | one asciiHere _ => exact ascii asciiHere
  | two leadHere _ _ => exact notTwo leadHere
  | three leadHere _ _ _ => exact notThree leadHere

/-- One step of `utf8ValidB` at a cons, with the tail's shape left abstract. -/
theorem utf8ValidB_cons (byte : UInt8) (tail : List UInt8) :
    utf8ValidB (byte :: tail) =
      (if IsAsciiByte byte then utf8ValidB tail
        else if IsLead2Byte byte then
          match tail with
          | cont :: more => if IsContByte cont then utf8ValidB more else false
          | [] => false
        else if IsLead3Byte byte then
          match tail with
          | first :: second :: more =>
              if ThreeFirstOk byte first then
                if IsContByte second then utf8ValidB more else false
              else false
          | _ => false
        else if IsLead4Byte byte then
          match tail with
          | first :: second :: third :: more =>
              if FourFirstOk byte first then
                if IsContByte second then
                  if IsContByte third then utf8ValidB more else false
                else false
              else false
          | _ => false
        else false) := by
  cases tail with
  | nil => simp only [utf8ValidB]
  | cons cont more =>
    cases more with
    | nil => simp only [utf8ValidB]
    | cons first more1 =>
      cases more1 with
      | nil => simp only [utf8ValidB]
      | cons second more2 =>
        cases more2 with
        | nil => simp only [utf8ValidB]
        | cons third more3 => simp only [utf8ValidB]

theorem valid_iff_valid_b (bytes : List UInt8) : utf8ValidB bytes = true ↔ ValidUtf8 bytes := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n →
    (utf8ValidB bytes = true ↔ ValidUtf8 bytes)
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len
      cases bytes with
      | nil => exact ⟨fun _ => ValidUtf8.nil, fun _ => rfl⟩
      | cons byte tail =>
        have short : tail.length < n := by simp only [List.length_cons] at len; omega
        have IH : utf8ValidB tail = true ↔ ValidUtf8 tail := ih tail.length short tail rfl
        by_cases ascii : IsAsciiByte byte
        · rw [utf8ValidB_cons]; simp only [if_pos ascii]
          exact IH.trans (ascii_step ascii).symm
        · by_cases lead2 : IsLead2Byte byte
          · cases tail with
            | nil =>
              rw [utf8ValidB_cons]; simp only [if_neg ascii, if_pos lead2]
              exact false_iff_of_refuted (not_valid_lone ascii)
            | cons cont more =>
              have short2 : more.length < n := by
                simp only [List.length_cons] at short; omega
              have IH2 : utf8ValidB more = true ↔ ValidUtf8 more := ih more.length short2 more rfl
              by_cases contOk : IsContByte cont
              · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_pos lead2, if_pos contOk]
                refine ⟨fun good => .two lead2 contOk (IH2.mp good), ?_⟩
                intro valid
                exact IH2.mpr ((lead2_step lead2).mp valid).2
              · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_pos lead2, if_neg contOk]
                exact false_iff_of_refuted (fun valid =>
                  contOk ((lead2_step lead2).mp valid).1)
          · by_cases lead3 : IsLead3Byte byte
            · cases tail with
              | nil =>
                rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_pos lead3]
                exact false_iff_of_refuted (not_valid_lone ascii)
              | cons first more1 =>
                cases more1 with
                | nil =>
                  rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_pos lead3]
                  exact false_iff_of_refuted (not_valid_pair ascii lead2)
                | cons second more2 =>
                  have short3 : more2.length < n := by
                    simp only [List.length_cons] at short; omega
                  have IH3 : utf8ValidB more2 = true ↔ ValidUtf8 more2 :=
                    ih more2.length short3 more2 rfl
                  by_cases firstOk : ThreeFirstOk byte first
                  · by_cases secondOk : IsContByte second
                    · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_pos lead3,
                        if_pos firstOk, if_pos secondOk]
                      refine ⟨fun good => .three lead3 firstOk secondOk (IH3.mp good), ?_⟩
                      intro valid
                      exact IH3.mpr ((lead3_step lead3).mp valid).2.2
                    · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_pos lead3,
                        if_pos firstOk, if_neg secondOk]
                      exact false_iff_of_refuted (fun valid =>
                        secondOk ((lead3_step lead3).mp valid).2.1)
                  · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_pos lead3,
                      if_neg firstOk]
                    exact false_iff_of_refuted (fun valid =>
                      firstOk ((lead3_step lead3).mp valid).1)
            · by_cases lead4 : IsLead4Byte byte
              · cases tail with
                | nil =>
                  rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4]
                  exact false_iff_of_refuted (not_valid_lone ascii)
                | cons first more1 =>
                  cases more1 with
                  | nil =>
                    rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4]
                    exact false_iff_of_refuted (not_valid_pair ascii lead2)
                  | cons second more2 =>
                    cases more2 with
                    | nil =>
                      rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_neg lead3,
                        if_pos lead4]
                      exact false_iff_of_refuted (not_valid_triple ascii lead2 (not_lead3_of_lead4 lead4))
                    | cons third more3 =>
                      have short4 : more3.length < n := by
                        simp only [List.length_cons] at short; omega
                      have IH4 : utf8ValidB more3 = true ↔ ValidUtf8 more3 :=
                        ih more3.length short4 more3 rfl
                      by_cases firstOk : FourFirstOk byte first
                      · by_cases secondOk : IsContByte second
                        · by_cases thirdOk : IsContByte third
                          · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_neg lead3,
                              if_pos lead4, if_pos firstOk, if_pos secondOk, if_pos thirdOk]
                            refine ⟨fun good =>
                                .four lead4 firstOk secondOk thirdOk (IH4.mp good), ?_⟩
                            intro valid
                            exact IH4.mpr ((lead4_step lead4).mp valid).2.2.2
                          · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_neg lead3,
                              if_pos lead4, if_pos firstOk, if_pos secondOk, if_neg thirdOk]
                            exact false_iff_of_refuted (fun valid =>
                              thirdOk ((lead4_step lead4).mp valid).2.2.1)
                        · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_neg lead3,
                            if_pos lead4, if_pos firstOk, if_neg secondOk]
                          exact false_iff_of_refuted (fun valid =>
                            secondOk ((lead4_step lead4).mp valid).2.1)
                      · rw [utf8ValidB_cons]; simp only [if_neg ascii, if_neg lead2, if_neg lead3,
                          if_pos lead4, if_neg firstOk]
                        exact false_iff_of_refuted (fun valid =>
                          firstOk ((lead4_step lead4).mp valid).1)
              · rw [utf8ValidB_cons]
                simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_neg lead4]
                exact false_iff_of_refuted
                  (not_valid_stray ascii lead2 lead3 lead4)
  exact key bytes.length bytes rfl

/-! ## Where no well-formed prefix extends

`firstInvalidUtf8` is the smallest offset whose prefix is not the prefix of a
well-formed list, or `none` when the whole list is well formed. It is the
declarative characterization the SPEC's first-invalid-byte span needs; the
detector is `firstInvalidUtf8.go`, and `firstInvalidUtf8_spec` proves the two
agree for every byte list. -/

/-- The first offset at which no well-formed prefix extends, or `none` when the
whole list is well formed. A malformed or incomplete head sequence stops the
walk at its own offset, so the reported offset is exactly where decoding stops. -/
def firstInvalidUtf8 : List UInt8 → Option Nat
  | [] => none
  | byte :: tail =>
      if IsAsciiByte byte then
        (firstInvalidUtf8 tail).map (· + 1)
      else if IsLead2Byte byte then
        match tail with
        | cont :: more =>
            if IsContByte cont then (firstInvalidUtf8 more).map (· + 2) else some 0
        | [] => some 0
      else if IsLead3Byte byte then
        match tail with
        | first :: second :: more =>
            if ThreeFirstOk byte first then
              if IsContByte second then (firstInvalidUtf8 more).map (· + 3) else some 0
            else some 0
        | _ => some 0
      else if IsLead4Byte byte then
        match tail with
        | first :: second :: third :: more =>
            if FourFirstOk byte first then
              if IsContByte second then
                if IsContByte third then (firstInvalidUtf8 more).map (· + 4) else some 0
              else some 0
            else some 0
        | _ => some 0
      else some 0

/-- One step of `firstInvalidUtf8` at a cons. -/
theorem firstInvalidUtf8_cons (byte : UInt8) (tail : List UInt8) :
    firstInvalidUtf8 (byte :: tail) =
      (if IsAsciiByte byte then (firstInvalidUtf8 tail).map (· + 1)
        else if IsLead2Byte byte then
          match tail with
          | cont :: more =>
              if IsContByte cont then (firstInvalidUtf8 more).map (· + 2) else some 0
          | [] => some 0
        else if IsLead3Byte byte then
          match tail with
          | first :: second :: more =>
              if ThreeFirstOk byte first then
                if IsContByte second then (firstInvalidUtf8 more).map (· + 3) else some 0
              else some 0
          | _ => some 0
        else if IsLead4Byte byte then
          match tail with
          | first :: second :: third :: more =>
              if FourFirstOk byte first then
                if IsContByte second then
                  if IsContByte third then (firstInvalidUtf8 more).map (· + 4) else some 0
                else some 0
              else some 0
          | _ => some 0
        else some 0) := by
  cases tail with
  | nil => simp only [firstInvalidUtf8]
  | cons cont more =>
    cases more with
    | nil => simp only [firstInvalidUtf8]
    | cons first more1 =>
      cases more1 with
      | nil => simp only [firstInvalidUtf8]
      | cons second more2 =>
        cases more2 with
        | nil => simp only [firstInvalidUtf8]
        | cons third more3 => simp only [firstInvalidUtf8]

theorem first_invalid_cons_ascii {byte : UInt8} {tail : List UInt8} (ascii : IsAsciiByte byte) :
    firstInvalidUtf8 (byte :: tail) = (firstInvalidUtf8 tail).map (· + 1) := by
  rw [firstInvalidUtf8_cons, if_pos ascii]

theorem first_invalid_cons_lead2 {lead cont : UInt8} {more : List UInt8}
    (leadOk : IsLead2Byte lead) (contOk : IsContByte cont) :
    firstInvalidUtf8 (lead :: cont :: more) = (firstInvalidUtf8 more).map (· + 2) := by
  rw [firstInvalidUtf8_cons]
  simp only [if_neg (not_ascii_of_lead2 leadOk), if_pos leadOk, if_pos contOk]

theorem option_map_none {value : Option Nat} {f : Nat → Nat} :
    value.map f = none ↔ value = none := by
  cases value <;> simp

theorem valid_take_of_first_none {bytes : List UInt8} (cleared : firstInvalidUtf8 bytes = none) :
    ValidUtf8 bytes := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n →
    firstInvalidUtf8 bytes = none → ValidUtf8 bytes
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len cleared
      cases bytes with
      | nil => exact .nil
      | cons byte tail =>
        have shorter : tail.length < n := by simp only [List.length_cons] at len; omega
        by_cases ascii : IsAsciiByte byte
        · rw [first_invalid_cons_ascii ascii] at cleared
          exact .one ascii (ih tail.length shorter tail rfl (option_map_none.mp cleared))
        · by_cases lead2 : IsLead2Byte byte
          · cases tail with
            | nil =>
              rw [firstInvalidUtf8_cons] at cleared
              simp only [if_neg ascii, if_pos lead2] at cleared
              exact absurd cleared (by simp)
            | cons cont more =>
              by_cases contOk : IsContByte cont
              · rw [first_invalid_cons_lead2 lead2 contOk] at cleared
                exact .two lead2 contOk (ih more.length
                  (by simp only [List.length_cons] at shorter ⊢; omega) more rfl
                  (option_map_none.mp cleared))
              · rw [firstInvalidUtf8_cons] at cleared
                simp only [if_neg ascii, if_pos lead2, if_neg contOk] at cleared
                exact absurd cleared (by simp)
          · by_cases lead3 : IsLead3Byte byte
            · cases tail with
              | nil =>
                rw [firstInvalidUtf8_cons] at cleared
                simp only [if_neg ascii, if_neg lead2, if_pos lead3] at cleared
                exact absurd cleared (by simp)
              | cons first more1 =>
                cases more1 with
                | nil =>
                  rw [firstInvalidUtf8_cons] at cleared
                  simp only [if_neg ascii, if_neg lead2, if_pos lead3] at cleared
                  exact absurd cleared (by simp)
                | cons second more2 =>
                  by_cases firstOk : ThreeFirstOk byte first
                  · by_cases secondOk : IsContByte second
                    · rw [firstInvalidUtf8_cons] at cleared
                      simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_pos firstOk,
                        if_pos secondOk] at cleared
                      exact .three lead3 firstOk secondOk (ih more2.length
                        (by simp only [List.length_cons] at shorter ⊢; omega) more2 rfl
                        (option_map_none.mp cleared))
                    · rw [firstInvalidUtf8_cons] at cleared
                      simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_pos firstOk,
                        if_neg secondOk] at cleared
                      exact absurd cleared (by simp)
                  · rw [firstInvalidUtf8_cons] at cleared
                    simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_neg firstOk] at cleared
                    exact absurd cleared (by simp)
            · by_cases lead4 : IsLead4Byte byte
              · cases tail with
                | nil =>
                  rw [firstInvalidUtf8_cons] at cleared
                  simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4] at cleared
                  exact absurd cleared (by simp)
                | cons first more1 =>
                  cases more1 with
                  | nil =>
                    rw [firstInvalidUtf8_cons] at cleared
                    simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4] at cleared
                    exact absurd cleared (by simp)
                  | cons second more2 =>
                    cases more2 with
                    | nil =>
                      rw [firstInvalidUtf8_cons] at cleared
                      simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4] at cleared
                      exact absurd cleared (by simp)
                    | cons third more3 =>
                      by_cases firstOk : FourFirstOk byte first
                      · by_cases secondOk : IsContByte second
                        · by_cases thirdOk : IsContByte third
                          · rw [firstInvalidUtf8_cons] at cleared
                            simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                              if_pos firstOk, if_pos secondOk, if_pos thirdOk] at cleared
                            exact .four lead4 firstOk secondOk thirdOk (ih more3.length
                              (by simp only [List.length_cons] at shorter ⊢; omega) more3 rfl
                              (option_map_none.mp cleared))
                          · rw [firstInvalidUtf8_cons] at cleared
                            simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                              if_pos firstOk, if_pos secondOk, if_neg thirdOk] at cleared
                            exact absurd cleared (by simp)
                        · rw [firstInvalidUtf8_cons] at cleared
                          simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                            if_pos firstOk, if_neg secondOk] at cleared
                          exact absurd cleared (by simp)
                      · rw [firstInvalidUtf8_cons] at cleared
                        simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                          if_neg firstOk] at cleared
                        exact absurd cleared (by simp)
              · rw [firstInvalidUtf8_cons] at cleared
                simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_neg lead4] at cleared
                exact absurd cleared (by simp)
  exact key bytes.length bytes rfl cleared

theorem first_invalid_none_iff_valid (bytes : List UInt8) :
    firstInvalidUtf8 bytes = none ↔ ValidUtf8 bytes :=
  ⟨valid_take_of_first_none, fun valid => by
    induction valid with
    | nil => rfl
    | one ascii _ ih => rw [first_invalid_cons_ascii ascii, ih]; rfl
    | two leadOk contOk _ ih =>
      rw [first_invalid_cons_lead2 leadOk contOk, ih]; rfl
    | three leadOk firstOk secondOk _ ih =>
      simp only [firstInvalidUtf8_cons, if_neg (not_ascii_of_lead3 leadOk),
        if_neg (not_lead2_of_lead3 leadOk), if_pos leadOk, if_pos firstOk, if_pos secondOk,
        option_map_none, ih]
    | four leadOk firstOk secondOk thirdOk _ ih =>
      simp only [firstInvalidUtf8_cons, if_neg (not_ascii_of_lead4 leadOk),
        if_neg (not_lead2_of_lead4 leadOk), if_neg (not_lead3_of_lead4 leadOk), if_pos leadOk,
        if_pos firstOk, if_pos secondOk, if_pos thirdOk, option_map_none, ih]
  ⟩

theorem first_invalid_some_prefix_valid {bytes : List UInt8} {offset : Nat}
    (found : firstInvalidUtf8 bytes = some offset) : ValidUtf8 (bytes.take offset) := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n → ∀ offset,
    firstInvalidUtf8 bytes = some offset → ValidUtf8 (bytes.take offset)
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len offset found
      cases bytes with
      | nil => rw [firstInvalidUtf8] at found; cases found
      | cons byte tail =>
        have shorter : tail.length < n := by simp only [List.length_cons] at len; omega
        by_cases ascii : IsAsciiByte byte
        · rw [first_invalid_cons_ascii ascii] at found
          cases tailFound : firstInvalidUtf8 tail with
          | none => simp [tailFound] at found
          | some inner =>
            simp only [tailFound, Option.map_some, Option.some.injEq] at found
            subst found
            rw [List.take_succ_cons]
            exact .one ascii (ih tail.length shorter tail rfl inner tailFound)
        · by_cases lead2 : IsLead2Byte byte
          · cases tail with
            | nil =>
              rw [firstInvalidUtf8_cons] at found
              simp only [if_neg ascii, if_pos lead2, Option.some.injEq] at found
              subst found
              simp only [List.take_zero]; exact .nil
            | cons cont more =>
              by_cases contOk : IsContByte cont
              · rw [first_invalid_cons_lead2 lead2 contOk] at found
                cases moreFound : firstInvalidUtf8 more with
                | none => simp [moreFound] at found
                | some inner =>
                  simp only [moreFound, Option.map_some, Option.some.injEq] at found
                  subst found
                  rw [show (byte :: cont :: more).take (inner + 2) =
                      byte :: cont :: more.take inner from by
                    simp [List.take_succ_cons]]
                  exact .two lead2 contOk (ih more.length
                    (by simp only [List.length_cons] at shorter ⊢; omega) more rfl inner moreFound)
              · rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_pos lead2, if_neg contOk, Option.some.injEq] at found
                subst found
                simp only [List.take_zero]; exact .nil
          · by_cases lead3 : IsLead3Byte byte
            · cases tail with
              | nil =>
                rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_neg lead2, if_pos lead3, Option.some.injEq] at found
                subst found; simp only [List.take_zero]; exact .nil
              | cons first more1 =>
                cases more1 with
                | nil =>
                  rw [firstInvalidUtf8_cons] at found
                  simp only [if_neg ascii, if_neg lead2, if_pos lead3, Option.some.injEq] at found
                  subst found; simp only [List.take_zero]; exact .nil
                | cons second more2 =>
                  by_cases firstOk : ThreeFirstOk byte first
                  · by_cases secondOk : IsContByte second
                    · rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_pos firstOk,
                        if_pos secondOk] at found
                      cases moreFound : firstInvalidUtf8 more2 with
                      | none => simp [moreFound] at found
                      | some inner =>
                        simp only [moreFound, Option.map_some, Option.some.injEq] at found
                        subst found
                        rw [show (byte :: first :: second :: more2).take (inner + 3) =
                            byte :: first :: second :: more2.take inner from by
                          simp [List.take_succ_cons]]
                        exact .three lead3 firstOk secondOk (ih more2.length
                          (by simp only [List.length_cons] at shorter ⊢; omega) more2 rfl
                          inner moreFound)
                    · rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_pos firstOk,
                        if_neg secondOk, Option.some.injEq] at found
                      subst found; simp only [List.take_zero]; exact .nil
                  · rw [firstInvalidUtf8_cons] at found
                    simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_neg firstOk,
                      Option.some.injEq] at found
                    subst found; simp only [List.take_zero]; exact .nil
            · by_cases lead4 : IsLead4Byte byte
              · cases tail with
                | nil =>
                  rw [firstInvalidUtf8_cons] at found
                  simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                    Option.some.injEq] at found
                  subst found; simp only [List.take_zero]; exact .nil
                | cons first more1 =>
                  cases more1 with
                  | nil =>
                    rw [firstInvalidUtf8_cons] at found
                    simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                      Option.some.injEq] at found
                    subst found; simp only [List.take_zero]; exact .nil
                  | cons second more2 =>
                    cases more2 with
                    | nil =>
                      rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                        Option.some.injEq] at found
                      subst found; simp only [List.take_zero]; exact .nil
                    | cons third more3 =>
                      by_cases firstOk : FourFirstOk byte first
                      · by_cases secondOk : IsContByte second
                        · by_cases thirdOk : IsContByte third
                          · rw [firstInvalidUtf8_cons] at found
                            simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                              if_pos firstOk, if_pos secondOk, if_pos thirdOk] at found
                            cases moreFound : firstInvalidUtf8 more3 with
                            | none => simp [moreFound] at found
                            | some inner =>
                              simp only [moreFound, Option.map_some, Option.some.injEq] at found
                              subst found
                              rw [show (byte :: first :: second :: third :: more3).take
                                    (inner + 4) =
                                  byte :: first :: second :: third :: more3.take inner from by
                                simp [List.take_succ_cons]]
                              exact .four lead4 firstOk secondOk thirdOk (ih more3.length
                                (by simp only [List.length_cons] at shorter ⊢; omega) more3 rfl
                                inner moreFound)
                          · rw [firstInvalidUtf8_cons] at found
                            simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                              if_pos firstOk, if_pos secondOk, if_neg thirdOk,
                              Option.some.injEq] at found
                            subst found; simp only [List.take_zero]; exact .nil
                        · rw [firstInvalidUtf8_cons] at found
                          simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                            if_pos firstOk, if_neg secondOk, Option.some.injEq] at found
                          subst found; simp only [List.take_zero]; exact .nil
                      · rw [firstInvalidUtf8_cons] at found
                        simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                          if_neg firstOk, Option.some.injEq] at found
                        subst found; simp only [List.take_zero]; exact .nil
              · rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_neg lead4,
                  Option.some.injEq] at found
                subst found; simp only [List.take_zero]; exact .nil
  exact key bytes.length bytes rfl offset found

theorem first_invalid_offset_lt {bytes : List UInt8} {offset : Nat}
    (found : firstInvalidUtf8 bytes = some offset) : offset < bytes.length := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n → ∀ offset,
    firstInvalidUtf8 bytes = some offset → offset < bytes.length
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len offset found
      cases bytes with
      | nil => rw [firstInvalidUtf8] at found; cases found
      | cons byte tail =>
        have shorter : tail.length < n := by simp only [List.length_cons] at len; omega
        by_cases ascii : IsAsciiByte byte
        · rw [first_invalid_cons_ascii ascii] at found
          cases tailFound : firstInvalidUtf8 tail with
          | none => simp [tailFound] at found
          | some inner =>
            simp only [tailFound, Option.map_some, Option.some.injEq] at found
            subst found
            have innerBound := ih tail.length shorter tail rfl inner tailFound
            simp only [List.length_cons]; omega
        · by_cases lead2 : IsLead2Byte byte
          · cases tail with
            | nil =>
              rw [firstInvalidUtf8_cons] at found
              simp only [if_neg ascii, if_pos lead2, Option.some.injEq] at found
              subst found; simp only [List.length_cons]; omega
            | cons cont more =>
              by_cases contOk : IsContByte cont
              · rw [first_invalid_cons_lead2 lead2 contOk] at found
                cases moreFound : firstInvalidUtf8 more with
                | none => simp [moreFound] at found
                | some inner =>
                  simp only [moreFound, Option.map_some, Option.some.injEq] at found
                  subst found
                  have innerBound := ih more.length
                    (by simp only [List.length_cons] at shorter ⊢; omega) more rfl inner moreFound
                  simp only [List.length_cons]; omega
              · rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_pos lead2, if_neg contOk, Option.some.injEq] at found
                subst found; simp only [List.length_cons]; omega
          · by_cases lead3 : IsLead3Byte byte
            · cases tail with
              | nil =>
                rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_neg lead2, if_pos lead3, Option.some.injEq] at found
                subst found; simp only [List.length_cons]; omega
              | cons first more1 =>
                cases more1 with
                | nil =>
                  rw [firstInvalidUtf8_cons] at found
                  simp only [if_neg ascii, if_neg lead2, if_pos lead3, Option.some.injEq] at found
                  subst found; simp only [List.length_cons]; omega
                | cons second more2 =>
                  by_cases firstOk : ThreeFirstOk byte first
                  · by_cases secondOk : IsContByte second
                    · rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_pos firstOk,
                        if_pos secondOk] at found
                      cases moreFound : firstInvalidUtf8 more2 with
                      | none => simp [moreFound] at found
                      | some inner =>
                        simp only [moreFound, Option.map_some, Option.some.injEq] at found
                        subst found
                        have innerBound := ih more2.length
                          (by simp only [List.length_cons] at shorter ⊢; omega) more2 rfl
                          inner moreFound
                        simp only [List.length_cons]; omega
                    · rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_pos firstOk,
                        if_neg secondOk, Option.some.injEq] at found
                      subst found; simp only [List.length_cons]; omega
                  · rw [firstInvalidUtf8_cons] at found
                    simp only [if_neg ascii, if_neg lead2, if_pos lead3, if_neg firstOk,
                      Option.some.injEq] at found
                    subst found; simp only [List.length_cons]; omega
            · by_cases lead4 : IsLead4Byte byte
              · cases tail with
                | nil =>
                  rw [firstInvalidUtf8_cons] at found
                  simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                    Option.some.injEq] at found
                  subst found; simp only [List.length_cons]; omega
                | cons first more1 =>
                  cases more1 with
                  | nil =>
                    rw [firstInvalidUtf8_cons] at found
                    simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                      Option.some.injEq] at found
                    subst found; simp only [List.length_cons]; omega
                  | cons second more2 =>
                    cases more2 with
                    | nil =>
                      rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                        Option.some.injEq] at found
                      subst found; simp only [List.length_cons]; omega
                    | cons third more3 =>
                      by_cases firstOk : FourFirstOk byte first
                      · by_cases secondOk : IsContByte second
                        · by_cases thirdOk : IsContByte third
                          · rw [firstInvalidUtf8_cons] at found
                            simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                              if_pos firstOk, if_pos secondOk, if_pos thirdOk] at found
                            cases moreFound : firstInvalidUtf8 more3 with
                            | none => simp [moreFound] at found
                            | some inner =>
                              simp only [moreFound, Option.map_some, Option.some.injEq] at found
                              subst found
                              have innerBound := ih more3.length
                                (by simp only [List.length_cons] at shorter ⊢; omega) more3 rfl
                                inner moreFound
                              simp only [List.length_cons]; omega
                          · rw [firstInvalidUtf8_cons] at found
                            simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                              if_pos firstOk, if_pos secondOk, if_neg thirdOk,
                              Option.some.injEq] at found
                            subst found; simp only [List.length_cons]; omega
                        · rw [firstInvalidUtf8_cons] at found
                          simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                            if_pos firstOk, if_neg secondOk, Option.some.injEq] at found
                          subst found; simp only [List.length_cons]; omega
                      · rw [firstInvalidUtf8_cons] at found
                        simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_pos lead4,
                          if_neg firstOk, Option.some.injEq] at found
                        subst found; simp only [List.length_cons]; omega
              · rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_neg lead4,
                  Option.some.injEq] at found
                subst found; simp only [List.length_cons]; omega
  exact key bytes.length bytes rfl offset found

/-- The reported offset names a well-formed prefix, and it is an interior
position: the byte at that offset starts no well-formed continuation. -/
theorem first_invalid_stops {bytes : List UInt8} {offset : Nat}
    (found : firstInvalidUtf8 bytes = some offset) :
    ValidUtf8 (bytes.take offset) ∧ offset < bytes.length :=
  ⟨first_invalid_some_prefix_valid found, first_invalid_offset_lt found⟩


/-! ## Well-formed sequence starts and the least invalid offset

`StartsSeq` is one complete well-formed sequence at the head of a byte list,
using the same alternatives `ValidUtf8` is built from. `first_invalid_structure`
shows that `firstInvalidUtf8` reports exactly the least offset whose prefix is
well formed while no sequence starts there, so the classifier's UTF-8 clause can
be stated without referring to the detector. -/

inductive StartsSeq : List UInt8 → Prop where
  | ascii {byte : UInt8} {rest : List UInt8} : IsAsciiByte byte → StartsSeq (byte :: rest)
  | two {lead cont : UInt8} {rest : List UInt8} :
      IsLead2Byte lead → IsContByte cont → StartsSeq (lead :: cont :: rest)
  | three {lead first second : UInt8} {rest : List UInt8} :
      IsLead3Byte lead → ThreeFirstOk lead first → IsContByte second →
        StartsSeq (lead :: first :: second :: rest)
  | four {lead first second third : UInt8} {rest : List UInt8} :
      IsLead4Byte lead → FourFirstOk lead first → IsContByte second → IsContByte third →
        StartsSeq (lead :: first :: second :: third :: rest)

set_option maxHeartbeats 1000000 in
/-- A well-formed list admits a sequence at every interior offset whose prefix is itself
well formed: the prefix decomposition continues with the next complete sequence. -/
theorem valid_utf8_boundary_starts {bytes : List UInt8} (valid : ValidUtf8 bytes) :
    ∀ {offset : Nat}, offset < bytes.length → ValidUtf8 (bytes.take offset) →
      StartsSeq (bytes.drop offset) := by
  induction valid with
  | nil => intro offset bound; simp at bound
  | one ascii _ ih =>
    intro offset bound prefixValid
    cases offset with
    | zero => exact .ascii ascii
    | succ m =>
      rw [List.take_succ_cons] at prefixValid
      exact ih (by simp only [List.length_cons] at bound; omega)
        ((ascii_step ascii).mp prefixValid)
  | two leadOk contOk _ ih =>
    intro offset bound prefixValid
    cases offset with
    | zero => exact .two leadOk contOk
    | succ m =>
      cases m with
      | zero =>
        simp only [List.take_succ_cons, List.take_zero] at prefixValid
        cases prefixValid with
        | one asciiHere _ => exact absurd asciiHere (not_ascii_of_lead2 leadOk)
      | succ r =>
        simp only [List.take_succ_cons] at prefixValid
        exact ih (by simp only [List.length_cons] at bound; omega)
          ((lead2_step leadOk).mp prefixValid).2
  | three leadOk firstOk secondOk _ ih =>
    intro offset bound prefixValid
    cases offset with
    | zero => exact .three leadOk firstOk secondOk
    | succ m =>
      cases m with
      | zero =>
        simp only [List.take_succ_cons, List.take_zero] at prefixValid
        cases prefixValid with
        | one asciiHere _ => exact absurd asciiHere (not_ascii_of_lead3 leadOk)
      | succ r =>
        cases r with
        | zero =>
          simp only [List.take_succ_cons, List.take_zero] at prefixValid
          cases prefixValid with
          | one asciiHere _ => exact absurd asciiHere (not_ascii_of_lead3 leadOk)
          | two leadHere _ _ => exact absurd leadHere (not_lead2_of_lead3 leadOk)
        | succ r1 =>
          simp only [List.take_succ_cons] at prefixValid
          exact ih (by simp only [List.length_cons] at bound; omega)
            ((lead3_step leadOk).mp prefixValid).2.2
  | four leadOk firstOk secondOk thirdOk _ ih =>
    intro offset bound prefixValid
    cases offset with
    | zero => exact .four leadOk firstOk secondOk thirdOk
    | succ m =>
      cases m with
      | zero =>
        simp only [List.take_succ_cons, List.take_zero] at prefixValid
        cases prefixValid with
        | one asciiHere _ => exact absurd asciiHere (not_ascii_of_lead4 leadOk)
      | succ r =>
        cases r with
        | zero =>
          simp only [List.take_succ_cons, List.take_zero] at prefixValid
          cases prefixValid with
          | one asciiHere _ => exact absurd asciiHere (not_ascii_of_lead4 leadOk)
          | two leadHere _ _ => exact absurd leadHere (not_lead2_of_lead4 leadOk)
        | succ r1 =>
          cases r1 with
          | zero =>
            simp only [List.take_succ_cons, List.take_zero] at prefixValid
            cases prefixValid with
            | one asciiHere _ => exact absurd asciiHere (not_ascii_of_lead4 leadOk)
            | two leadHere _ _ => exact absurd leadHere (not_lead2_of_lead4 leadOk)
            | three leadHere _ _ _ => exact absurd leadHere (not_lead3_of_lead4 leadOk)
          | succ r2 =>
            simp only [List.take_succ_cons] at prefixValid
            exact ih (by simp only [List.length_cons] at bound; omega)
              ((lead4_step leadOk).mp prefixValid).2.2.2

set_option maxHeartbeats 1000000 in
theorem first_invalid_structure {bytes : List UInt8} {offset : Nat}
    (found : firstInvalidUtf8 bytes = some offset) :
    (∀ k, ValidUtf8 (bytes.take k) → k < offset → StartsSeq (bytes.drop k)) ∧
      (∀ k, offset < k → firstInvalidUtf8 (bytes.take k) = some offset) ∧
      ¬ StartsSeq (bytes.drop offset) := by
  let motive (n : Nat) : Prop := ∀ bytes : List UInt8, bytes.length = n → ∀ offset,
    firstInvalidUtf8 bytes = some offset →
      (∀ k, ValidUtf8 (bytes.take k) → k < offset → StartsSeq (bytes.drop k)) ∧
      (∀ k, offset < k → firstInvalidUtf8 (bytes.take k) = some offset) ∧
      ¬ StartsSeq (bytes.drop offset)
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro bytes len offset found
      cases bytes with
      | nil => rw [firstInvalidUtf8] at found; cases found
      | cons byte tail =>
        have shorter : tail.length < n := by simp only [List.length_cons] at len; omega
        by_cases ascii : IsAsciiByte byte
        · rw [first_invalid_cons_ascii ascii] at found
          cases tailFound : firstInvalidUtf8 tail with
          | none => simp [tailFound] at found
          | some inner =>
            simp only [tailFound, Option.map_some, Option.some.injEq] at found
            subst found
            have IH := ih tail.length shorter tail rfl inner tailFound
            refine ⟨?_, ?_, ?_⟩
            · intro k valid small
              cases k with
              | zero => exact .ascii ascii
              | succ m =>
                rw [List.take_succ_cons] at valid
                simpa only [List.drop_succ_cons] using
                  IH.1 m ((ascii_step ascii).mp valid) (by omega)
            · intro k large
              cases k with
              | zero => omega
              | succ m =>
                rw [List.take_succ_cons, first_invalid_cons_ascii ascii, IH.2.1 m (by omega),
                  Option.map_some]
            · simpa only [List.drop_succ_cons] using IH.2.2
        · by_cases lead2 : IsLead2Byte byte
          · cases tail with
            | nil =>
              rw [firstInvalidUtf8_cons] at found
              simp only [if_neg ascii, if_pos lead2, Option.some.injEq] at found
              subst found
              refine ⟨?_, ?_, ?_⟩
              · intro k valid small; omega
              · intro k large
                rw [List.take_of_length_le (by simp only [List.length_cons, List.length_nil]; omega)]
                rw [firstInvalidUtf8_cons]
                simp only [if_neg ascii, if_pos lead2]
              · intro starts
                cases starts with
                | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead2 lead2)
            | cons cont more =>
              by_cases contOk : IsContByte cont
              · rw [first_invalid_cons_lead2 lead2 contOk] at found
                cases moreFound : firstInvalidUtf8 more with
                | none => simp [moreFound] at found
                | some inner =>
                  simp only [moreFound, Option.map_some, Option.some.injEq] at found
                  subst found
                  have IH := ih more.length
                    (by simp only [List.length_cons] at shorter ⊢; omega) more rfl inner moreFound
                  refine ⟨?_, ?_, ?_⟩
                  · intro k valid small
                    cases k with
                    | zero => exact .two lead2 contOk
                    | succ m =>
                      cases m with
                      | zero =>
                        simp only [List.take_succ_cons, List.take_zero] at valid
                        cases valid with
                        | one asciiHere _ => exact absurd asciiHere (not_ascii_of_lead2 lead2)
                      | succ r =>
                        simp only [List.take_succ_cons] at valid
                        simpa only [List.drop_succ_cons] using
                          IH.1 r ((lead2_step lead2).mp valid).2 (by omega)
                  · intro k large
                    cases k with
                    | zero => omega
                    | succ m =>
                      cases m with
                      | zero => omega
                      | succ r =>
                        simp only [List.take_succ_cons]
                        rw [first_invalid_cons_lead2 lead2 contOk, IH.2.1 r (by omega),
                          Option.map_some]
                  · simpa only [List.drop_succ_cons] using IH.2.2
              · rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_pos lead2, if_neg contOk, Option.some.injEq] at found
                subst found
                refine ⟨?_, ?_, ?_⟩
                · intro k valid small; omega
                · intro k large
                  cases k with
                  | zero => omega
                  | succ m =>
                    cases m with
                    | zero =>
                      simp only [List.take_succ_cons, List.take_zero]
                      rw [firstInvalidUtf8_cons]
                      simp only [if_neg ascii, if_pos lead2]
                    | succ r =>
                      simp only [List.take_succ_cons]
                      rw [firstInvalidUtf8_cons]
                      simp only [if_neg ascii, if_pos lead2, if_neg contOk]
                · intro starts
                  cases starts with
                  | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead2 lead2)
                  | two _ contHere => exact absurd contHere contOk
                  | three leadHere _ _ => exact absurd lead2 (not_lead2_of_lead3 leadHere)
                  | four leadHere _ _ _ => exact absurd lead2 (not_lead2_of_lead4 leadHere)
          · by_cases lead3 : IsLead3Byte byte
            · cases tail with
              | nil =>
                rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3,
                  Option.some.injEq] at found
                subst found
                refine ⟨?_, ?_, ?_⟩
                · intro k valid small; omega
                · intro k large
                  rw [List.take_of_length_le (by simp only [List.length_cons, List.length_nil]; omega)]
                  rw [firstInvalidUtf8_cons]
                  simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3]
                · intro starts
                  cases starts with
                  | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead3 lead3)
              | cons first more1 =>
                cases more1 with
                | nil =>
                  rw [firstInvalidUtf8_cons] at found
                  simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3,
                    Option.some.injEq] at found
                  subst found
                  refine ⟨?_, ?_, ?_⟩
                  · intro k valid small; omega
                  · intro k large
                    cases k with
                    | zero => omega
                    | succ m =>
                      cases m with
                      | zero =>
                        simp only [List.take_succ_cons, List.take_zero]
                        rw [firstInvalidUtf8_cons]
                        simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3]
                      | succ r =>
                        rw [List.take_of_length_le (by simp only [List.length_cons, List.length_nil]; omega)]
                        rw [firstInvalidUtf8_cons]
                        simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3]
                  · intro starts
                    cases starts with
                    | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead3 lead3)
                    | two leadHere _ => exact absurd leadHere (not_lead2_of_lead3 lead3)
                | cons second more2 =>
                  by_cases firstOk : ThreeFirstOk byte first
                  · by_cases secondOk : IsContByte second
                    · rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3,
                        if_pos firstOk, if_pos secondOk] at found
                      cases moreFound : firstInvalidUtf8 more2 with
                      | none => simp [moreFound] at found
                      | some inner =>
                        simp only [moreFound, Option.map_some, Option.some.injEq] at found
                        subst found
                        have IH := ih more2.length
                          (by simp only [List.length_cons] at shorter ⊢; omega)
                          more2 rfl inner moreFound
                        refine ⟨?_, ?_, ?_⟩
                        · intro k valid small
                          cases k with
                          | zero => exact .three lead3 firstOk secondOk
                          | succ m =>
                            cases m with
                            | zero =>
                              simp only [List.take_succ_cons, List.take_zero] at valid
                              cases valid with
                              | one asciiHere _ => exact absurd asciiHere (not_ascii_of_lead3 lead3)
                            | succ r =>
                              cases r with
                              | zero =>
                                simp only [List.take_succ_cons, List.take_zero] at valid
                                cases valid with
                                | one asciiHere _ =>
                                  exact absurd asciiHere (not_ascii_of_lead3 lead3)
                                | two leadHere _ _ => exact absurd leadHere (not_lead2_of_lead3 lead3)
                              | succ r' =>
                                simp only [List.take_succ_cons] at valid
                                simpa only [List.drop_succ_cons] using
                                  IH.1 r' ((lead3_step lead3).mp valid).2.2 (by omega)
                        · intro k large
                          cases k with
                          | zero => omega
                          | succ m =>
                            cases m with
                            | zero => omega
                            | succ r =>
                              cases r with
                              | zero => omega
                              | succ r' =>
                                simp only [List.take_succ_cons]
                                rw [firstInvalidUtf8_cons]
                                simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3),
                                  if_pos lead3, if_pos firstOk, if_pos secondOk,
                                  IH.2.1 r' (by omega), Option.map_some]
                        · simpa only [List.drop_succ_cons] using IH.2.2
                    · rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3,
                        if_pos firstOk, if_neg secondOk, Option.some.injEq] at found
                      subst found
                      refine ⟨?_, ?_, ?_⟩
                      · intro k valid small; omega
                      · intro k large
                        cases k with
                        | zero => omega
                        | succ m =>
                          cases m with
                          | zero =>
                            simp only [List.take_succ_cons, List.take_zero]
                            rw [firstInvalidUtf8_cons]
                            simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3]
                          | succ r =>
                            cases r with
                            | zero =>
                              simp only [List.take_succ_cons, List.take_zero]
                              rw [firstInvalidUtf8_cons]
                              simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3]
                            | succ r' =>
                              simp only [List.take_succ_cons]
                              rw [firstInvalidUtf8_cons]
                              simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3),
                                if_pos lead3, if_pos firstOk, if_neg secondOk]
                      · intro starts
                        cases starts with
                        | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead3 lead3)
                        | two leadHere _ => exact absurd leadHere (not_lead2_of_lead3 lead3)
                        | three _ _ secondHere => exact absurd secondHere secondOk
                        | four leadHere _ _ _ => exact absurd lead3 (not_lead3_of_lead4 leadHere)
                  · rw [firstInvalidUtf8_cons] at found
                    simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3,
                      if_neg firstOk, Option.some.injEq] at found
                    subst found
                    refine ⟨?_, ?_, ?_⟩
                    · intro k valid small; omega
                    · intro k large
                      cases k with
                      | zero => omega
                      | succ m =>
                        cases m with
                        | zero =>
                          simp only [List.take_succ_cons, List.take_zero]
                          rw [firstInvalidUtf8_cons]
                          simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3]
                        | succ r =>
                          cases r with
                          | zero =>
                            simp only [List.take_succ_cons, List.take_zero]
                            rw [firstInvalidUtf8_cons]
                            simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3), if_pos lead3]
                          | succ r' =>
                            simp only [List.take_succ_cons]
                            rw [firstInvalidUtf8_cons]
                            simp only [if_neg ascii, if_neg (not_lead2_of_lead3 lead3),
                              if_pos lead3, if_neg firstOk]
                    · intro starts
                      cases starts with
                      | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead3 lead3)
                      | two leadHere _ => exact absurd leadHere (not_lead2_of_lead3 lead3)
                      | three _ firstHere _ => exact absurd firstHere firstOk
                      | four leadHere _ _ _ => exact absurd lead3 (not_lead3_of_lead4 leadHere)
            · by_cases lead4 : IsLead4Byte byte
              · cases tail with
                | nil =>
                  rw [firstInvalidUtf8_cons] at found
                  simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                    if_neg (not_lead3_of_lead4 lead4), if_pos lead4, Option.some.injEq] at found
                  subst found
                  refine ⟨?_, ?_, ?_⟩
                  · intro k valid small; omega
                  · intro k large
                    rw [List.take_of_length_le (by simp only [List.length_cons, List.length_nil]; omega)]
                    rw [firstInvalidUtf8_cons]
                    simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                      if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                  · intro starts
                    cases starts with
                    | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead4 lead4)
                | cons first more1 =>
                  cases more1 with
                  | nil =>
                    rw [firstInvalidUtf8_cons] at found
                    simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                      if_neg (not_lead3_of_lead4 lead4), if_pos lead4, Option.some.injEq] at found
                    subst found
                    refine ⟨?_, ?_, ?_⟩
                    · intro k valid small; omega
                    · intro k large
                      cases k with
                      | zero => omega
                      | succ m =>
                        cases m with
                        | zero =>
                          simp only [List.take_succ_cons, List.take_zero]
                          rw [firstInvalidUtf8_cons]
                          simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                            if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                        | succ r =>
                          rw [List.take_of_length_le (by simp only [List.length_cons, List.length_nil]; omega)]
                          rw [firstInvalidUtf8_cons]
                          simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                            if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                    · intro starts
                      cases starts with
                      | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead4 lead4)
                      | two leadHere _ => exact absurd leadHere (not_lead2_of_lead4 lead4)
                  | cons second more2 =>
                    cases more2 with
                    | nil =>
                      rw [firstInvalidUtf8_cons] at found
                      simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                        if_neg (not_lead3_of_lead4 lead4), if_pos lead4, Option.some.injEq] at found
                      subst found
                      refine ⟨?_, ?_, ?_⟩
                      · intro k valid small; omega
                      · intro k large
                        cases k with
                        | zero => omega
                        | succ m =>
                          cases m with
                          | zero =>
                            simp only [List.take_succ_cons, List.take_zero]
                            rw [firstInvalidUtf8_cons]
                            simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                              if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                          | succ r =>
                            cases r with
                            | zero =>
                              simp only [List.take_succ_cons, List.take_zero]
                              rw [firstInvalidUtf8_cons]
                              simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                            | succ r1 =>
                              rw [List.take_of_length_le (by simp only [List.length_cons, List.length_nil]; omega)]
                              rw [firstInvalidUtf8_cons]
                              simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                      · intro starts
                        cases starts with
                        | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead4 lead4)
                        | two leadHere _ => exact absurd leadHere (not_lead2_of_lead4 lead4)
                        | three leadHere _ _ => exact absurd leadHere (not_lead3_of_lead4 lead4)
                    | cons third more3 =>
                      by_cases firstOk : FourFirstOk byte first
                      · by_cases secondOk : IsContByte second
                        · by_cases thirdOk : IsContByte third
                          · rw [firstInvalidUtf8_cons] at found
                            simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                              if_neg (not_lead3_of_lead4 lead4), if_pos lead4, if_pos firstOk,
                              if_pos secondOk, if_pos thirdOk] at found
                            cases moreFound : firstInvalidUtf8 more3 with
                            | none => simp [moreFound] at found
                            | some inner =>
                              simp only [moreFound, Option.map_some, Option.some.injEq] at found
                              subst found
                              have IH := ih more3.length
                                (by simp only [List.length_cons] at shorter ⊢; omega)
                                more3 rfl inner moreFound
                              refine ⟨?_, ?_, ?_⟩
                              · intro k valid small
                                cases k with
                                | zero => exact .four lead4 firstOk secondOk thirdOk
                                | succ m =>
                                  cases m with
                                  | zero =>
                                    simp only [List.take_succ_cons, List.take_zero] at valid
                                    cases valid with
                                    | one asciiHere _ =>
                                      exact absurd asciiHere (not_ascii_of_lead4 lead4)
                                  | succ r =>
                                    cases r with
                                    | zero =>
                                      simp only [List.take_succ_cons, List.take_zero] at valid
                                      cases valid with
                                      | one asciiHere _ =>
                                        exact absurd asciiHere (not_ascii_of_lead4 lead4)
                                      | two leadHere _ _ => exact absurd leadHere (not_lead2_of_lead4 lead4)
                                    | succ r1 =>
                                      cases r1 with
                                      | zero =>
                                        simp only [List.take_succ_cons, List.take_zero] at valid
                                        cases valid with
                                        | one asciiHere _ =>
                                          exact absurd asciiHere (not_ascii_of_lead4 lead4)
                                        | two leadHere _ _ =>
                                          exact absurd leadHere (not_lead2_of_lead4 lead4)
                                        | three leadHere _ _ _ =>
                                          exact absurd leadHere (not_lead3_of_lead4 lead4)
                                      | succ r2 =>
                                        simp only [List.take_succ_cons] at valid
                                        simpa only [List.drop_succ_cons] using
                                          IH.1 r2 ((lead4_step lead4).mp valid).2.2.2 (by omega)
                              · intro k large
                                cases k with
                                | zero => omega
                                | succ m =>
                                  cases m with
                                  | zero => omega
                                  | succ r =>
                                    cases r with
                                    | zero => omega
                                    | succ r1 =>
                                      cases r1 with
                                      | zero => omega
                                      | succ r2 =>
                                        simp only [List.take_succ_cons]
                                        rw [firstInvalidUtf8_cons]
                                        simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                          if_neg (not_lead3_of_lead4 lead4), if_pos lead4,
                                          if_pos firstOk, if_pos secondOk, if_pos thirdOk,
                                          IH.2.1 r2 (by omega), Option.map_some]
                              · simpa only [List.drop_succ_cons] using IH.2.2
                          · rw [firstInvalidUtf8_cons] at found
                            simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                              if_neg (not_lead3_of_lead4 lead4), if_pos lead4, if_pos firstOk,
                              if_pos secondOk, if_neg thirdOk, Option.some.injEq] at found
                            subst found
                            refine ⟨?_, ?_, ?_⟩
                            · intro k valid small; omega
                            · intro k large
                              cases k with
                              | zero => omega
                              | succ m =>
                                cases m with
                                | zero =>
                                  simp only [List.take_succ_cons, List.take_zero]
                                  rw [firstInvalidUtf8_cons]
                                  simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                    if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                                | succ r =>
                                  cases r with
                                  | zero =>
                                    simp only [List.take_succ_cons, List.take_zero]
                                    rw [firstInvalidUtf8_cons]
                                    simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                      if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                                  | succ r1 =>
                                    cases r1 with
                                    | zero =>
                                      simp only [List.take_succ_cons, List.take_zero]
                                      rw [firstInvalidUtf8_cons]
                                      simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                        if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                                    | succ r2 =>
                                      simp only [List.take_succ_cons]
                                      rw [firstInvalidUtf8_cons]
                                      simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                        if_neg (not_lead3_of_lead4 lead4), if_pos lead4,
                                        if_pos firstOk, if_pos secondOk, if_neg thirdOk]
                            · intro starts
                              cases starts with
                              | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead4 lead4)
                              | two leadHere _ => exact absurd leadHere (not_lead2_of_lead4 lead4)
                              | three leadHere _ _ => exact absurd leadHere (not_lead3_of_lead4 lead4)
                              | four _ _ _ thirdHere => exact absurd thirdHere thirdOk
                        · rw [firstInvalidUtf8_cons] at found
                          simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                            if_neg (not_lead3_of_lead4 lead4), if_pos lead4, if_pos firstOk,
                            if_neg secondOk, Option.some.injEq] at found
                          subst found
                          refine ⟨?_, ?_, ?_⟩
                          · intro k valid small; omega
                          · intro k large
                            cases k with
                            | zero => omega
                            | succ m =>
                              cases m with
                              | zero =>
                                simp only [List.take_succ_cons, List.take_zero]
                                rw [firstInvalidUtf8_cons]
                                simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                  if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                              | succ r =>
                                cases r with
                                | zero =>
                                  simp only [List.take_succ_cons, List.take_zero]
                                  rw [firstInvalidUtf8_cons]
                                  simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                    if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                                | succ r1 =>
                                  cases r1 with
                                  | zero =>
                                    simp only [List.take_succ_cons, List.take_zero]
                                    rw [firstInvalidUtf8_cons]
                                    simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                      if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                                  | succ r2 =>
                                    simp only [List.take_succ_cons]
                                    rw [firstInvalidUtf8_cons]
                                    simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                      if_neg (not_lead3_of_lead4 lead4), if_pos lead4,
                                      if_pos firstOk, if_neg secondOk]
                          · intro starts
                            cases starts with
                            | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead4 lead4)
                            | two leadHere _ => exact absurd leadHere (not_lead2_of_lead4 lead4)
                            | three leadHere _ _ => exact absurd leadHere (not_lead3_of_lead4 lead4)
                            | four _ _ secondHere _ => exact absurd secondHere secondOk
                      · rw [firstInvalidUtf8_cons] at found
                        simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                          if_neg (not_lead3_of_lead4 lead4), if_pos lead4,
                          if_neg firstOk, Option.some.injEq] at found
                        subst found
                        refine ⟨?_, ?_, ?_⟩
                        · intro k valid small; omega
                        · intro k large
                          cases k with
                          | zero => omega
                          | succ m =>
                            cases m with
                            | zero =>
                              simp only [List.take_succ_cons, List.take_zero]
                              rw [firstInvalidUtf8_cons]
                              simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                            | succ r =>
                              cases r with
                              | zero =>
                                simp only [List.take_succ_cons, List.take_zero]
                                rw [firstInvalidUtf8_cons]
                                simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                  if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                              | succ r1 =>
                                cases r1 with
                                | zero =>
                                  simp only [List.take_succ_cons, List.take_zero]
                                  rw [firstInvalidUtf8_cons]
                                  simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                    if_neg (not_lead3_of_lead4 lead4), if_pos lead4]
                                | succ r2 =>
                                  simp only [List.take_succ_cons]
                                  rw [firstInvalidUtf8_cons]
                                  simp only [if_neg ascii, if_neg (not_lead2_of_lead4 lead4),
                                    if_neg (not_lead3_of_lead4 lead4), if_pos lead4,
                                    if_neg firstOk]
                        · intro starts
                          cases starts with
                          | ascii asciiHere => exact absurd asciiHere (not_ascii_of_lead4 lead4)
                          | two leadHere _ => exact absurd leadHere (not_lead2_of_lead4 lead4)
                          | three leadHere _ _ => exact absurd leadHere (not_lead3_of_lead4 lead4)
                          | four _ firstHere _ _ => exact absurd firstHere firstOk
              · rw [firstInvalidUtf8_cons] at found
                simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_neg lead4,
                  Option.some.injEq] at found
                subst found
                refine ⟨?_, ?_, ?_⟩
                · intro k valid small; omega
                · intro k large
                  cases k with
                  | zero => omega
                  | succ m =>
                    cases m with
                    | zero =>
                      simp only [List.take_succ_cons, List.take_zero]
                      rw [firstInvalidUtf8_cons]
                      simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_neg lead4]
                    | succ r =>
                      simp only [List.take_succ_cons]
                      rw [firstInvalidUtf8_cons]
                      simp only [if_neg ascii, if_neg lead2, if_neg lead3, if_neg lead4]
                · intro starts
                  cases starts with
                  | ascii asciiHere => exact absurd asciiHere ascii
                  | two leadHere _ => exact absurd leadHere lead2
                  | three leadHere _ _ => exact absurd leadHere lead3
                  | four leadHere _ _ _ => exact absurd leadHere lead4
  exact key bytes.length bytes rfl offset found


/-! ## Unterminated single-line string literals

The SPEC's string rule (2.6) gives `"..."` on one line only; the triple-quoted
form is a distinct delimiter and may span lines. So on valid UTF-8 source a
`"` whose line holds no further unescaped `"` is unterminated. The SPEC fixes
no span; the model ends the span at the line end, excluding the line-end bytes,
or at the end of input when no line end follows, as the Rust scanner does.
`lineEnd` and `scanLine` state the rule; `safe_line_iff_scan` relates them. -/

/-- The first line feed at or after `start`, or the end of the list. -/
def lineEnd (bytes : List UInt8) (start : Nat) : Nat :=
  ((List.range (bytes.length - start)).findSome? fun step =>
    if bytes[start + step]? = some 10 then some (start + step) else none).getD bytes.length

def HasQuote (bytes : List UInt8) (index stop : Nat) : Prop :=
  ∃ position, index ≤ position ∧ position < stop ∧ bytes[position]? = some 34

/-- Scanning one line for an unescaped `"`. The boolean records whether the
previous byte was an unescaped backslash, so its successor is text. -/
def scanLine : Bool → List UInt8 → Bool
  | _, [] => true
  | true, _ :: rest => scanLine false rest
  | false, byte :: rest =>
      if byte = 92 then scanLine true rest
      else if byte = 34 then false
      else scanLine false rest

/-- The declarative reading with an escape state. In state `true` the next byte
is text; in state `false` a backslash begins an escape, a quote ends the
literal, and any other byte is text. -/
inductive SafeLine : Bool → List UInt8 → Prop where
  | stopTrue : SafeLine true []
  | stopFalse : SafeLine false []
  | resume {rest : List UInt8} {byte : UInt8} :
      SafeLine false rest → SafeLine true (byte :: rest)
  | escaped {rest : List UInt8} {byte : UInt8} :
      SafeLine false rest → SafeLine false (92 :: byte :: rest)
  | trailingBackslash : SafeLine false [92]
  | text {rest : List UInt8} {byte : UInt8} :
      byte ≠ 92 → byte ≠ 34 → SafeLine false rest → SafeLine false (byte :: rest)

theorem false_iff_safe {target : Prop} (refuted : ¬ target) :
    (false = true ↔ target) :=
  ⟨fun impossible => False.elim (Bool.false_eq_true.mp impossible),
    fun valid => False.elim (refuted valid)⟩

theorem not_safe_quote {rest : List UInt8} : ¬ SafeLine false (34 :: rest) := by
  intro valid
  cases valid with
  | text notBackslash notQuote inner => exact absurd rfl notQuote

theorem safe_line_iff_scan (state : Bool) (line : List UInt8) :
    scanLine state line = true ↔ SafeLine state line := by
  let motive (n : Nat) : Prop := ∀ line : List UInt8, line.length = n →
    ∀ state, scanLine state line = true ↔ SafeLine state line
  have key : ∀ n, motive n := by
    intro n
    induction n using Nat.strongRecOn with
    | ind n ih =>
      intro line len state
      cases line with
      | nil =>
        cases state with
        | true => exact ⟨fun _ => .stopTrue, fun _ => rfl⟩
        | false => exact ⟨fun _ => .stopFalse, fun _ => rfl⟩
      | cons byte rest =>
        have shorter : rest.length < n := by simp only [List.length_cons] at len; omega
        have IH : scanLine false rest = true ↔ SafeLine false rest :=
          ih rest.length shorter rest rfl false
        cases state with
        | true =>
          exact ⟨fun good => .resume (IH.mp good),
            fun valid => IH.mpr (by cases valid with | resume inner => assumption)⟩
        | false =>
          by_cases backslash : byte = 92
          · subst backslash
            cases rest with
            | nil => simp only [scanLine]; exact ⟨fun _ => .trailingBackslash, fun _ => rfl⟩
            | cons next more =>
              have innerShorter : more.length < n := by
                simp only [List.length_cons] at shorter; omega
              have IH2 : scanLine false more = true ↔ SafeLine false more :=
                ih more.length innerShorter more rfl false
              have step : scanLine false (92 :: next :: more) = scanLine false more := by
                simp only [scanLine, ite_true]
              rw [step]
              exact ⟨fun good => .escaped (byte := next) (IH2.mp good),
                fun valid => by cases valid with
                  | escaped inner => exact IH2.mpr inner
                  | text notBackslash _ _ => exact absurd rfl notBackslash⟩
          · by_cases quote : byte = 34
            · subst quote
              simp only [scanLine, if_neg (by decide : (34 : UInt8) ≠ 92), if_pos rfl]
              exact false_iff_safe not_safe_quote
            · simp only [scanLine, if_neg backslash, if_neg quote]
              exact ⟨fun good => .text backslash quote (IH.mp good),
                fun valid => IH.mpr (by
                  cases valid with
                  | text _ _ inner => assumption
                  | escaped _ => exact absurd rfl backslash
                  | trailingBackslash => exact absurd rfl backslash)⟩
  exact key line.length line rfl state

end SourceAdmission
