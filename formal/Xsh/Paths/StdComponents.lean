namespace Xsh.Paths.StdComponents

/-- A component of a POSIX path as Rust's `std::path::Path::components` gives
it. -/
inductive Part where
  | root
  | current
  | parentDir
  | normal (bytes : List UInt8)
  deriving DecidableEq, Repr

/-- A component and the byte span `[start, finish)` it occupies. -/
structure Spanned where
  part : Part
  start : Nat
  finish : Nat
  deriving DecidableEq, Repr

/-- The components after a leading root or `.`: the `/`-separated pieces from
`offset`, skipping empty ones and `.`. -/
def restParts : Nat → Nat → List UInt8 → List Spanned
  | 0, _, _ => []
  | _ + 1, _, [] => []
  | fuel + 1, offset, 47 :: rest => restParts fuel (offset + 1) rest
  | fuel + 1, offset, bytes =>
    let piece := bytes.takeWhile (· != 47)
    let after := restParts fuel (offset + piece.length) (bytes.drop piece.length)
    if piece = [46] then after
    else if piece = [46, 46] then ⟨.parentDir, offset, offset + 2⟩ :: after
    else ⟨.normal piece, offset, offset + piece.length⟩ :: after

/-- The components of `bytes`: a root for a leading `/`, a `.` only when the
path starts with `.` alone or `./`, then the named, `..` and skipped pieces. -/
def parts (bytes : List UInt8) : List Spanned :=
  match bytes with
  | 47 :: _ => ⟨.root, 0, 1⟩ :: restParts (bytes.length + 1) 0 bytes
  | [46] => [⟨.current, 0, 1⟩]
  | 46 :: 47 :: _ => ⟨.current, 0, 1⟩ :: restParts (bytes.length + 1) 1 (bytes.drop 1)
  | _ => restParts (bytes.length + 1) 0 bytes

/-- The bytes that spell a component. -/
def Part.bytes : Part → List UInt8
  | .root => [47]
  | .current => [46]
  | .parentDir => [46, 46]
  | .normal bytes => bytes

/-- `Components::as_path` of a run of components: the original bytes from the
first one's start to the last one's end. -/
def spanBytes (bytes : List UInt8) : List Spanned → List UInt8
  | [] => []
  | first :: rest =>
    let last := (rest.getLast?).getD first
    (bytes.take last.finish).drop first.start

/-- `Path::parent`: the bytes of all but the last component, when the last is
not the root; `none` for the root or no components. -/
def parent? (bytes : List UInt8) : Option (List UInt8) :=
  match (parts bytes).getLast? with
  | none | some ⟨.root, _, _⟩ => none
  | some _ => some (spanBytes bytes (parts bytes).dropLast)

/-- `Path::file_name`: the last component when it is a name. -/
def fileName? (bytes : List UInt8) : Option Spanned :=
  match (parts bytes).getLast? with
  | some ⟨.normal name, start, finish⟩ => some ⟨.normal name, start, finish⟩
  | _ => none

/-- A file name's stem and extension, split at its last `.` unless that
leaves an empty stem. -/
def splitAtDot (name : List UInt8) : List UInt8 × Option (List UInt8) :=
  match (name.zipIdx.filter fun (byte, _) => byte == 46).getLast? with
  | some (_, index) => if index = 0 then (name, none) else (name.take index, some (name.drop (index + 1)))
  | none => (name, none)

/-- `Path::extension`. -/
def extension? (bytes : List UInt8) : Option (List UInt8) :=
  (fileName? bytes).bind fun spanned => (splitAtDot spanned.part.bytes).2

/-- `PathBuf::set_extension`: the bytes up to the end of the file stem, then
`.` and the extension unless it is empty; unchanged without a file name. -/
def withExtension (bytes extension : List UInt8) : List UInt8 :=
  match fileName? bytes with
  | none => bytes
  | some spanned =>
    let stem := (splitAtDot spanned.part.bytes).1
    bytes.take (spanned.start + stem.length) ++ (if extension.isEmpty then [] else 46 :: extension)

/-- The components of `bytes` after those of `prefix`, when they begin with
them. -/
def afterPrefix? (bytes pre : List UInt8) : Option (List Spanned) :=
  let mine := parts bytes
  let theirs := (parts pre).map Spanned.part
  if (mine.map Spanned.part).take theirs.length = theirs then some (mine.drop theirs.length) else none

/-- `Path::starts_with` and `Path::ends_with`, component by component. -/
def startsWith (bytes pre : List UInt8) : Bool := (afterPrefix? bytes pre).isSome

def endsWith (bytes suffix : List UInt8) : Bool :=
  let mine := (parts bytes).map Spanned.part
  let theirs := (parts suffix).map Spanned.part
  mine.drop (mine.length - theirs.length) = theirs ∧ theirs.length ≤ mine.length

/-- `Path::strip_prefix` as XSH uses it: the remaining components' bytes,
`.` when none remain. -/
def stripPrefix? (bytes pre : List UInt8) : Option (List UInt8) :=
  (afterPrefix? bytes pre).map fun rest =>
    match spanBytes bytes rest with
    | [] => [46]
    | remaining => remaining

end Xsh.Paths.StdComponents
