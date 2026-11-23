namespace Xsh.Values.Digests

/-! Word arithmetic over `Nat` modulo a power of two, so that the kernel
reduces each step with its accelerated natural-number operations. -/

def rotr (width : Nat) (value count : Nat) : Nat :=
  (value >>> count ||| value <<< (width - count)) % 2 ^ width

def rotl (width : Nat) (value count : Nat) : Nat :=
  (value <<< count ||| value >>> (width - count)) % 2 ^ width

def complement (width value : Nat) : Nat := 2 ^ width - 1 - value

/-- The big-endian number the bytes spell. -/
def bigEndian (bytes : List UInt8) : Nat := bytes.foldl (fun value byte => value * 256 + byte.toNat) 0

/-- The little-endian number the bytes spell. -/
def littleEndian (bytes : List UInt8) : Nat := bytes.foldr (fun byte value => value * 256 + byte.toNat) 0

def toBigEndian (count value : Nat) : List UInt8 :=
  (List.range count).map fun index => (value / 2 ^ (8 * (count - 1 - index)) % 256).toUInt8

def toLittleEndian (count value : Nat) : List UInt8 :=
  (List.range count).map fun index => (value / 2 ^ (8 * index) % 256).toUInt8

/-- `items` cut into pieces of `size`. -/
def chunks {α : Type} (size : Nat) : Nat → List α → List (List α)
  | 0, _ => []
  | _ + 1, [] => []
  | fuel + 1, items => items.take size :: chunks size fuel (items.drop size)

/-- Merkle–Damgård padding: `0x80`, zeros to `block - lengthBytes` modulo
`block`, then the bit length in `lengthBytes` bytes, big- or little-endian. -/
def pad (block lengthBytes : Nat) (little : Bool) (message : List UInt8) : List UInt8 :=
  let zeros := (block + block - lengthBytes - (message.length + 1) % block) % block
  let bits := 8 * message.length
  message ++ [0x80] ++ List.replicate zeros 0 ++
    (if little then toLittleEndian lengthBytes bits else toBigEndian lengthBytes bits)

/-- The message's blocks as words of `wordBytes` bytes. -/
def blockWords (block wordBytes : Nat) (little : Bool) (padded : List UInt8) : List (List Nat) :=
  (chunks block (padded.length + 1) padded).map fun piece =>
    (chunks wordBytes (piece.length + 1) piece).map fun word => if little then littleEndian word else bigEndian word

/-! SHA-256 (FIPS 180-4). -/

def sha256K : List Nat :=
  [0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
   0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
   0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
   0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
   0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
   0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
   0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
   0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2]

/-- The SHA-2 message schedule: the block's words, then each later word from
four earlier ones through the two small sigma functions. -/
def schedule (width rounds : Nat) (small0 small1 : Nat → Nat) (words : List Nat) : List Nat :=
  (List.range (rounds - words.length)).foldl (fun built _ =>
    let index := built.length
    let word := (small1 (built.getD (index - 2) 0) + built.getD (index - 7) 0 + small0 (built.getD (index - 15) 0) +
      built.getD (index - 16) 0) % 2 ^ width
    built ++ [word]) words

/-- One SHA-2 compression of a block's schedule into the state `h`. -/
def sha2Compress (width : Nat) (constants : List Nat) (big0 big1 : Nat → Nat) (h : List Nat)
    (scheduled : List Nat) : List Nat :=
  let modulus := 2 ^ width
  let final := (scheduled.zip constants).foldl (fun state (word, constant) =>
    match state with
    | [a, b, c, d, e, f, g, hh] =>
      let choose := (e &&& f) ^^^ (complement width e &&& g)
      let majority := (a &&& b) ^^^ (a &&& c) ^^^ (b &&& c)
      let first := (hh + big1 e + choose + constant + word) % modulus
      let second := (big0 a + majority) % modulus
      [(first + second) % modulus, a, b, c, (d + first) % modulus, e, f, g]
    | other => other) h
  (h.zip final).map fun (start, finish) => (start + finish) % modulus

def sha256 (message : List UInt8) : List UInt8 :=
  let small0 := fun x => rotr 32 x 7 ^^^ rotr 32 x 18 ^^^ x >>> 3
  let small1 := fun x => rotr 32 x 17 ^^^ rotr 32 x 19 ^^^ x >>> 10
  let big0 := fun x => rotr 32 x 2 ^^^ rotr 32 x 13 ^^^ rotr 32 x 22
  let big1 := fun x => rotr 32 x 6 ^^^ rotr 32 x 11 ^^^ rotr 32 x 25
  let start := [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19]
  let state := (blockWords 64 4 false (pad 64 8 false message)).foldl (fun h words =>
    sha2Compress 32 sha256K big0 big1 h (schedule 32 64 small0 small1 words)) start
  state.flatMap (toBigEndian 4)

/-! SHA-512 (FIPS 180-4). -/

def sha512K : List Nat :=
  [0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc, 0x3956c25bf348b538,
   0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118, 0xd807aa98a3030242, 0x12835b0145706fbe,
   0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2, 0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235,
   0xc19bf174cf692694, 0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
   0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5, 0x983e5152ee66dfab,
   0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4, 0xc6e00bf33da88fc2, 0xd5a79147930aa725,
   0x06ca6351e003826f, 0x142929670a0e6e70, 0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed,
   0x53380d139d95b3df, 0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
   0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30, 0xd192e819d6ef5218,
   0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8, 0x19a4c116b8d2d0c8, 0x1e376c085141ab53,
   0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8, 0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373,
   0x682e6ff3d6b2b8a3, 0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
   0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b, 0xca273eceea26619c,
   0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178, 0x06f067aa72176fba, 0x0a637dc5a2c898a6,
   0x113f9804bef90dae, 0x1b710b35131c471b, 0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc,
   0x431d67c49c100d4c, 0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817]

def sha512 (message : List UInt8) : List UInt8 :=
  let small0 := fun x => rotr 64 x 1 ^^^ rotr 64 x 8 ^^^ x >>> 7
  let small1 := fun x => rotr 64 x 19 ^^^ rotr 64 x 61 ^^^ x >>> 6
  let big0 := fun x => rotr 64 x 28 ^^^ rotr 64 x 34 ^^^ rotr 64 x 39
  let big1 := fun x => rotr 64 x 14 ^^^ rotr 64 x 18 ^^^ rotr 64 x 41
  let start := [0x6a09e667f3bcc908, 0xbb67ae8584caa73b, 0x3c6ef372fe94f82b, 0xa54ff53a5f1d36f1,
    0x510e527fade682d1, 0x9b05688c2b3e6c1f, 0x1f83d9abfb41bd6b, 0x5be0cd19137e2179]
  let state := (blockWords 128 8 false (pad 128 16 false message)).foldl (fun h words =>
    sha2Compress 64 sha512K big0 big1 h (schedule 64 80 small0 small1 words)) start
  state.flatMap (toBigEndian 8)

/-! SHA-1 (FIPS 180-4). -/

def sha1 (message : List UInt8) : List UInt8 :=
  let modulus := 2 ^ 32
  let start := [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0]
  let state := (blockWords 64 4 false (pad 64 8 false message)).foldl (fun h words =>
    let scheduled := (List.range 64).foldl (fun built _ =>
      let index := built.length
      built ++ [rotl 32 (built.getD (index - 3) 0 ^^^ built.getD (index - 8) 0 ^^^ built.getD (index - 14) 0 ^^^
        built.getD (index - 16) 0) 1]) words
    let final := scheduled.zipIdx.foldl (fun state (word, index) =>
      match state with
      | [a, b, c, d, e] =>
        let (mixed, constant) :=
          if index < 20 then ((b &&& c) ||| (complement 32 b &&& d), 0x5A827999)
          else if index < 40 then (b ^^^ c ^^^ d, 0x6ED9EBA1)
          else if index < 60 then ((b &&& c) ||| (b &&& d) ||| (c &&& d), 0x8F1BBCDC)
          else (b ^^^ c ^^^ d, 0xCA62C1D6)
        [(rotl 32 a 5 + mixed + e + constant + word) % modulus, a, rotl 32 b 30, c, d]
      | other => other) h
    (h.zip final).map fun (first, second) => (first + second) % modulus) start
  state.flatMap (toBigEndian 4)

/-! MD5 (RFC 1321). -/

def md5Shifts : List Nat :=
  [7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
   5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
   4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
   6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21]

def md5K : List Nat :=
  [0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
   0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
   0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
   0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
   0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
   0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
   0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
   0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391]

def md5 (message : List UInt8) : List UInt8 :=
  let modulus := 2 ^ 32
  let start := [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476]
  let state := (blockWords 64 4 true (pad 64 8 true message)).foldl (fun h words =>
    let final := ((md5K.zip md5Shifts).zipIdx).foldl (fun state ((constant, shift), index) =>
      match state with
      | [a, b, c, d] =>
        let (mixed, pick) :=
          if index < 16 then ((b &&& c) ||| (complement 32 b &&& d), index)
          else if index < 32 then ((d &&& b) ||| (complement 32 d &&& c), (5 * index + 1) % 16)
          else if index < 48 then (b ^^^ c ^^^ d, (3 * index + 5) % 16)
          else (c ^^^ (b ||| complement 32 d), (7 * index) % 16)
        [d, (b + rotl 32 ((a + mixed + constant + words.getD pick 0) % modulus) shift) % modulus, b, c]
      | other => other) h
    (h.zip final).map fun (first, second) => (first + second) % modulus) start
  state.flatMap (toLittleEndian 4)

end Xsh.Values.Digests
