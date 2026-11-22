import Xsh.Values.FloatOps

namespace Xsh.Values.FloatLiterals

open Xsh.Values.FloatOps
open Xsh.Values.FloatArith

/-- The value of a nonempty run of decimal digits. -/
def digitsValue : List Char → Option Nat
  | [] => none
  | digits =>
    digits.foldl (fun total digit =>
      total.bind fun sum => if digit.isDigit then some (sum * 10 + (digit.toNat - '0'.toNat)) else none) (some 0)

/-- An exponent: an optional sign and a nonempty run of digits. -/
def exponentValue : List Char → Option Int
  | '+' :: digits => (digitsValue digits).map Int.ofNat
  | '-' :: digits => (digitsValue digits).map fun value => -(value : Int)
  | digits => (digitsValue digits).map Int.ofNat

/-- A Float literal as the lexer reads it (SPEC 2, Numbers): digits, then a
`.` and digits, an exponent `e` or `E` with an optional sign and digits, or
both. The parts are the digits as one integer, how many of them follow the
`.`, and the exponent. -/
def decimalParts (lexeme : String) : Option (Nat × Nat × Int) :=
  let characters := lexeme.toList
  let whole := characters.takeWhile Char.isDigit
  let rest := characters.dropWhile Char.isDigit
  let (fraction, afterFraction) : Option (List Char) × List Char :=
    match rest with
    | '.' :: more => (some (more.takeWhile Char.isDigit), more.dropWhile Char.isDigit)
    | _ => (none, rest)
  let exponent : Option (Option Int) :=
    match afterFraction with
    | [] => some none
    | marker :: digits => if marker = 'e' ∨ marker = 'E' then (exponentValue digits).map some else none
  match fraction, exponent with
  | some [], _ | _, none | none, some none => none
  | fraction, some exponent =>
    let fractionDigits := fraction.getD []
    (digitsValue (whole ++ fractionDigits)).bind fun mantissa =>
      if whole.isEmpty then none else some (mantissa, fractionDigits.length, exponent.getD 0)

/-- The exact value of a Float literal, as a numerator over a denominator. -/
def decimalRational (lexeme : String) : Option (Nat × Nat) :=
  (decimalParts lexeme).map fun (mantissa, fraction, exponent) =>
    let shift := exponent - fraction
    if 0 ≤ shift then (mantissa * 10 ^ shift.toNat, 1) else (mantissa, 10 ^ (-shift).toNat)

/-- A Float literal's value (SPEC 4.3): its exact decimal value rounded to the
nearest binary64, ties to even, overflowing to infinity. -/
def decimalFloat (lexeme : String) : Option FloatBits :=
  (decimalRational lexeme).map fun (numerator, denominator) => roundToFloat false numerator denominator

/-- A literal's bits are the rounding of its exact decimal value. -/
def FloatLiteralMeaning (lexeme : String) (bits : FloatBits) : Prop :=
  ∃ numerator denominator, decimalRational lexeme = some (numerator, denominator) ∧
    (FloatOutcome.rounded false numerator denominator).Holds bits

theorem decimalRational_positive (lexeme : String) (numerator denominator : Nat)
    (parsed : decimalRational lexeme = some (numerator, denominator)) : 0 < denominator := by
  unfold decimalRational at parsed
  cases parts : decimalParts lexeme with
  | none => simp [parts] at parsed
  | some found =>
    obtain ⟨mantissa, fraction, exponent⟩ := found
    simp only [parts, Option.map_some, Option.some.injEq] at parsed
    split at parsed
    · simp only [Prod.mk.injEq] at parsed
      omega
    · simp only [Prod.mk.injEq] at parsed
      rw [← parsed.2]
      exact Nat.pow_pos (by decide)

/-- The exact value: `numerator / denominator = mantissa · 10^exponent / 10^fraction`,
stated by cross-multiplication. -/
theorem decimalRational_value (lexeme : String) (mantissa fraction : Nat) (exponent : Int)
    (parts : decimalParts lexeme = some (mantissa, fraction, exponent)) (numerator denominator : Nat)
    (parsed : decimalRational lexeme = some (numerator, denominator)) :
    numerator * 10 ^ fraction * 10 ^ (max (-exponent) 0).toNat =
      mantissa * denominator * 10 ^ (max exponent 0).toNat := by
  simp only [decimalRational, parts, Option.map_some, Option.some.injEq] at parsed
  split at parsed
  · simp only [Prod.mk.injEq] at parsed
    obtain ⟨rfl, rfl⟩ := parsed
    have split : (max exponent 0).toNat = (exponent - fraction).toNat + fraction + (max (-exponent) 0).toNat := by
      omega
    rw [split, Nat.pow_add, Nat.pow_add]
    simp only [Nat.mul_one, Nat.mul_assoc]
  · simp only [Prod.mk.injEq] at parsed
    obtain ⟨rfl, rfl⟩ := parsed
    have split : fraction + (max (-exponent) 0).toNat = (-(exponent - fraction)).toNat + (max exponent 0).toNat := by
      omega
    rw [Nat.mul_assoc, ← Nat.pow_add, split, Nat.pow_add, Nat.mul_assoc]

theorem decimal_float_meaning (lexeme : String) (bits : FloatBits) (parsed : decimalFloat lexeme = some bits) :
    FloatLiteralMeaning lexeme bits := by
  unfold decimalFloat at parsed
  cases rational : decimalRational lexeme with
  | none => simp [rational] at parsed
  | some found =>
    obtain ⟨numerator, denominator⟩ := found
    simp only [rational, Option.map_some, Option.some.injEq] at parsed
    subst parsed
    exact ⟨numerator, denominator, rational,
      realized_outcome_holds floatQuietNaN quiet_nan_is_quiet (.rounded false numerator denominator)
        (decimalRational_positive lexeme numerator denominator rational)⟩

theorem decimal_float_not_nan (lexeme : String) (bits : FloatBits) (parsed : decimalFloat lexeme = some bits) :
    ¬ floatIsNaN bits := by
  intro isNaN
  unfold decimalFloat at parsed
  cases rational : decimalRational lexeme with
  | none => simp [rational] at parsed
  | some found =>
    obtain ⟨numerator, denominator⟩ := found
    simp only [rational, Option.map_some, Option.some.injEq] at parsed
    subst parsed
    have classified := (float_class_nan_iff _).2 isNaN
    unfold floatClass at classified
    have magnitude : floatMagnitudeRaw (roundToFloat false numerator denominator) =
        roundMagnitudeBits numerator denominator := by
      simp only [roundToFloat, signed_float_magnitude]
    have bounded := round_magnitude_bits_le_infinity numerator denominator
    rw [magnitude] at classified
    split at classified
    · omega
    · split at classified <;> cases classified

/-- The rounded value is the only bits a literal can mean. -/
theorem float_literal_unique (lexeme : String) (bits : FloatBits) (meaning : FloatLiteralMeaning lexeme bits) :
    decimalFloat lexeme = some bits := by
  obtain ⟨numerator, denominator, parsed, holds⟩ := meaning
  have computed : decimalFloat lexeme = some (roundToFloat false numerator denominator) := by
    simp [decimalFloat, parsed]
  obtain ⟨_, _, parsed', computedHolds⟩ := decimal_float_meaning lexeme _ computed
  rw [parsed] at parsed'
  simp only [Option.some.injEq, Prod.mk.injEq] at parsed'
  obtain ⟨rfl, rfl⟩ := parsed'
  rcases outcome_holds_unique (.rounded false numerator denominator)
      (decimalRational_positive lexeme numerator denominator parsed) computedHolds holds with
    ⟨isNaN, _⟩ | same
  · exact absurd isNaN (decimal_float_not_nan lexeme _ computed)
  · rw [computed, same]

/-- Witnesses: the literal's bits, as Rust's `str::parse::<f64>` and the
verification binary give them, and literals the lexer does not read as a
Float. -/
theorem float_literal_witnesses :
    (decimalFloat "1.0").map Fin.val = some 4607182418800017408 ∧
    (decimalFloat "0.25").map Fin.val = some 4598175219545276416 ∧
    (decimalFloat "1.5e6").map Fin.val = some 4699193262664056832 ∧
    (decimalFloat "10e-3").map Fin.val = some 4576918229304087675 ∧
    (decimalFloat "0.1").map Fin.val = some 4591870180066957722 ∧
    (decimalFloat "0.30000000000000004").map Fin.val = some 4599075939470750516 ∧
    (decimalFloat "1E+2").map Fin.val = some 4636737291354636288 ∧
    (decimalFloat "123.456e-2").map Fin.val = some 4608238783128613432 ∧
    (decimalFloat "1e400").map Fin.val = some 9218868437227405312 ∧
    (decimalFloat "5e-324").map Fin.val = some 1 ∧
    (decimalFloat "2e-324").map Fin.val = some 0 ∧
    decimalFloat "1." = none ∧ decimalFloat "1" = none ∧ decimalFloat "1e" = none ∧ decimalFloat ".5" = none := by
  decide +kernel

/-- Truncating instead of rounding to nearest gives `0.1` the bits below
Rust's; the model gives Rust's bits. -/
theorem truncation_rejected :
    (decimalFloat "0.1").map Fin.val = some 4591870180066957722 ∧ 4591870180066957722 ≠ 4591870180066957721 := by
  decide +kernel

/-- The digits of a run, possibly none. -/
def runValue (digits : List Char) : Option Nat :=
  if digits.isEmpty then some 0 else digitsValue digits

/-- A `Float` text hole's text (SPEC 6.10, holes `f`, `e` and `g`): an
optional sign, digits with an optional `.` and fraction holding at least one
digit between them, and an optional exponent; the value rounds as a literal
does and must be finite. -/
def holeRounded (text : List Char) : Option FloatBits :=
  let (negative, unsigned) := match text with
    | '-' :: rest => (true, rest)
    | '+' :: rest => (false, rest)
    | rest => (false, rest)
  let whole := unsigned.takeWhile Char.isDigit
  let afterWhole := unsigned.dropWhile Char.isDigit
  let (fraction, afterFraction) := match afterWhole with
    | '.' :: more => (more.takeWhile Char.isDigit, more.dropWhile Char.isDigit)
    | rest => ([], rest)
  let exponent : Option Int := match afterFraction with
    | [] => some 0
    | marker :: digits => if marker = 'e' ∨ marker = 'E' then exponentValue digits else none
  match exponent, runValue (whole ++ fraction) with
  | some exponent, some mantissa =>
    if (whole ++ fraction).isEmpty then none
    else
      let shift := exponent - fraction.length
      some (if 0 ≤ shift then roundToFloat negative (mantissa * 10 ^ shift.toNat) 1
        else roundToFloat negative mantissa (10 ^ (-shift).toNat))
  | _, _ => none

def finiteOnly (bits : FloatBits) : Option FloatBits :=
  if floatMagnitudeRaw bits = infinityRaw then none else some bits

/-- A `Float` text hole's value: its rounding, when that is finite. -/
def holeFloat (text : List Char) : Option FloatBits := (holeRounded text).bind finiteOnly

theorem holeFloat_finite (text : List Char) (bits : FloatBits) (read : holeFloat text = some bits) :
    floatMagnitudeRaw bits ≠ infinityRaw := by
  obtain ⟨rounded, _, finite⟩ := Option.bind_eq_some_iff.mp read
  unfold finiteOnly at finite
  split at finite
  · cases finite
  · simp only [Option.some.injEq] at finite
    subst finite
    assumption

/-- The `float_literal_rounding` fragment root (SPEC 2 Numbers, 4.3, 6.10): a
Float literal as the lexer reads it means its exact decimal value rounded to
the nearest binary64, ties to even and overflowing to infinity; that value,
by cross-multiplication, is the literal's digits times ten to its exponent
over ten to its fraction length; `decimalFloat` gives it and nothing else
meets it; and a `Float` text hole's reading is never infinite. -/
theorem float_literal_rounding :
    (∀ lexeme bits, decimalFloat lexeme = some bits → FloatLiteralMeaning lexeme bits) ∧
    (∀ lexeme bits, FloatLiteralMeaning lexeme bits → decimalFloat lexeme = some bits) ∧
    (∀ lexeme mantissa fraction exponent numerator denominator,
      decimalParts lexeme = some (mantissa, fraction, exponent) →
      decimalRational lexeme = some (numerator, denominator) →
        numerator * 10 ^ fraction * 10 ^ (max (-exponent) 0).toNat =
          mantissa * denominator * 10 ^ (max exponent 0).toNat) ∧
    (∀ lexeme bits, decimalFloat lexeme = some bits → ¬ floatIsNaN bits) ∧
    (∀ text bits, holeFloat text = some bits → floatMagnitudeRaw bits ≠ infinityRaw) :=
  ⟨decimal_float_meaning, float_literal_unique,
    fun lexeme mantissa fraction exponent numerator denominator parts parsed =>
      decimalRational_value lexeme mantissa fraction exponent parts numerator denominator parsed,
    decimal_float_not_nan, holeFloat_finite⟩

/-- As Rust's `TextHoleKind::Float` reads `-1.5e3` as `-1500.0` and `7` as
`7.0` and refuses `""`, `nan`, `inf`, `-inf`, `1e999`, `.`, `e` and `1.5 `. -/
theorem hole_float_witnesses :
    (holeFloat "-1.5e3".toList).map Fin.val = some 13877683922067783680 ∧
    (holeFloat "7".toList).map Fin.val = some 4619567317775286272 ∧
    (holeFloat "1.".toList).map Fin.val = some 4607182418800017408 ∧
    (holeFloat ".5".toList).map Fin.val = some 4602678819172646912 ∧
    (holeFloat "-0".toList).map Fin.val = some 9223372036854775808 ∧
    holeFloat "".toList = none ∧ holeFloat "nan".toList = none ∧ holeFloat "inf".toList = none ∧
    holeFloat "-inf".toList = none ∧ holeFloat "1e999".toList = none ∧ holeFloat ".".toList = none ∧
    holeFloat "e".toList = none ∧ holeFloat "1.5 ".toList = none := by
  decide +kernel

end Xsh.Values.FloatLiterals
