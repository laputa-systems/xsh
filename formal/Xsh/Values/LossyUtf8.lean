namespace Xsh.Values.LossyUtf8

/-- The range a well-formed UTF-8 sequence's second byte must fall in, after
a lead byte, and the sequence's length (Unicode Table 3-7). -/
def leadRange (lead : UInt8) : Option (UInt8 × UInt8 × Nat) :=
  if 0xC2 ≤ lead ∧ lead ≤ 0xDF then some (0x80, 0xBF, 2)
  else if lead = 0xE0 then some (0xA0, 0xBF, 3)
  else if 0xE1 ≤ lead ∧ lead ≤ 0xEC then some (0x80, 0xBF, 3)
  else if lead = 0xED then some (0x80, 0x9F, 3)
  else if 0xEE ≤ lead ∧ lead ≤ 0xEF then some (0x80, 0xBF, 3)
  else if lead = 0xF0 then some (0x90, 0xBF, 4)
  else if 0xF1 ≤ lead ∧ lead ≤ 0xF3 then some (0x80, 0xBF, 4)
  else if lead = 0xF4 then some (0x80, 0x8F, 4)
  else none

def isContinuation (byte : UInt8) : Bool := 0x80 ≤ byte && byte ≤ 0xBF

/-- The scalar value a well-formed sequence encodes. -/
def scalarOf (lead : UInt8) (rest : List UInt8) : Nat :=
  let leadBits := match rest.length with
    | 1 => lead.toNat % 32
    | 2 => lead.toNat % 16
    | _ => lead.toNat % 8
  rest.foldl (fun value byte => value * 64 + byte.toNat % 64) leadBits

/-- Bytes decoded as Rust's `String::from_utf8_lossy` does: each maximal
prefix of a well-formed sequence that cannot be completed, and each byte that
cannot begin one, becomes one U+FFFD. -/
def decode : Nat → List UInt8 → List Char
  | 0, _ => []
  | _ + 1, [] => []
  | fuel + 1, lead :: rest =>
    if lead < 0x80 then Char.ofNat lead.toNat :: decode fuel rest
    else
      match leadRange lead, rest with
      | some (low, high, width), second :: more =>
        if low ≤ second ∧ second ≤ high then
          let tail := (more.take (width - 2)).takeWhile isContinuation
          if tail.length = width - 2 then
            Char.ofNat (scalarOf lead (second :: tail)) :: decode fuel (more.drop tail.length)
          else '�' :: decode fuel (more.drop tail.length)
        else '�' :: decode fuel rest
      | _, _ => '�' :: decode fuel rest

/-- The UTF-8 encoding of the lossy text of `bytes`. -/
def bytes (data : List UInt8) : List UInt8 := (decode (data.length + 1) data).flatMap String.utf8EncodeChar

/-- The lossy text of `bytes`. -/
def text (bytes : List UInt8) : String := String.ofList (decode (bytes.length + 1) bytes)

end Xsh.Values.LossyUtf8
