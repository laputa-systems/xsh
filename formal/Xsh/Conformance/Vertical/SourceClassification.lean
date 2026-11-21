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

end SourceAdmission

end Xsh.Conformance.Vertical
