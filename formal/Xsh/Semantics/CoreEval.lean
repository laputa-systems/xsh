import Xsh.Semantics.CoreStatements
import Xsh.Values.IntegerOps
import Xsh.Values.DurationOps
import Xsh.Values.FloatOps
import Xsh.Values.FloatEquality
import Xsh.Values.FloatLiterals
import Xsh.Values.LossyUtf8

namespace Xsh.Semantics.CoreEval

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Values.FloatOps

variable {Declaration Identity Source : Type}

/-- The interpretation of literal lexemes, which the lexer owns. -/
structure Literals where
  integer : String → Option IntValue
  float : String → Option FloatBits
  duration : String → Option DurationValue

/-- The runtime error values of the failures that `try` does not capture. -/
structure Errors (Declaration Identity Source : Type) where
  integer : IntegerOps.IntegerFault → Value Declaration Identity Source
  duration : DurationOps.DurationFault → Value Declaration Identity Source
  indexOutOfRange : Value Declaration Identity Source
  missingField : Value Declaration Identity Source
  mapMissing : Value Declaration Identity Source
  /-- A bitset method given a negative receiver or mask (SPEC 4.2). -/
  bitset : Value Declaration Identity Source
  /-- The error a Float to Int conversion's `Err` holds (SPEC 4.3). -/
  floatConversion : Value Declaration Identity Source
  /-- The error `parse_float`'s `Err` holds. -/
  parseFloat : Value Declaration Identity Source
  /-- A `match` whose arms all fail to select the subject (SPEC 6.8). -/
  matchNoArm : Value Declaration Identity Source
  /-- The error `first()`, `min()` and `max()` give an empty pipeline (SPEC 13.3). -/
  streamEmpty : Value Declaration Identity Source
  /-- The error `List.get` holds for an index outside the list. -/
  listGet : Int → Value Declaration Identity Source
  /-- The error `Map.get` holds for a key the map lacks. -/
  mapGet : MapKey → Value Declaration Identity Source
  /-- The runtime `text-byte-slice` failure of `Str.byte_slice`, by message. -/
  textByteSlice : String → Value Declaration Identity Source
  /-- An error by its kind and message, for the methods whose messages carry
  their operands. -/
  described : String → String → Value Declaration Identity Source

/-- Whether a function returns a `Result`, which decides where its failures
go (SPEC 8.3, 8.7). -/
inductive ReturnKind where
  | result
  | plain
  deriving DecidableEq

/-- A function body: a result expression, or a statement block whose value is
its `return` or its tail expression. -/
inductive FunctionBody where
  | expression (value : Xsh.Syntax.Expr) (kind : ReturnKind)
  | block (statements : List Xsh.Syntax.Statement) (kind : ReturnKind)

/-- A function: the environment it was defined in, its parameter names and its
body. -/
abbrev Functions := Xsh.Syntax.RawName → Option (Environment × List Xsh.Syntax.RawName × FunctionBody)

structure Context (Declaration Identity Source : Type) where
  literals : Literals
  errors : Errors Declaration Identity Source
  policy : NaNPolicy
  spelling : Xsh.Syntax.RawName → String
  functions : Functions
  /-- Whether two nominal declarations are the same one: the identity a
  variant's type carries. -/
  sameDeclaration : Declaration → Declaration → Bool
  /-- Whether two runtime identities are the same handle or callable. -/
  sameIdentity : Identity → Identity → Bool
  /-- Whether two source identities are the same source. -/
  sameSource : Source → Source → Bool

inductive Result (Declaration Identity Source : Type) (α : Type) where
  | value (value : α)
  | failure (failure : Failure Declaration Identity Source)
  | stuck
  | unsupported
  | outOfFuel

def Result.bind {α β : Type} : Result Declaration Identity Source α → (α → Result Declaration Identity Source β) →
    Result Declaration Identity Source β
  | .value inner, next => next inner
  | .failure inner, _ => .failure inner
  | .stuck, _ => .stuck
  | .unsupported, _ => .unsupported
  | .outOfFuel, _ => .outOfFuel

abbrev Locals (Declaration Identity Source : Type) := List (Xsh.Syntax.RawName × Value Declaration Identity Source)

def integer (value : IntValue) : Value Declaration Identity Source := .scalar (.integer value)
def float (bits : FloatBits) : Value Declaration Identity Source := .scalar (.float bits)
def boolean (value : Bool) : Value Declaration Identity Source := .scalar (.boolean value)
def duration (value : DurationValue) : Value Declaration Identity Source := .scalar (.duration value)

def literal (literals : Literals) : Xsh.Syntax.Literal → Result Declaration Identity Source (Value Declaration Identity Source)
  | .null => .value (.scalar .null)
  | .bool value => .value (boolean value)
  | .integer lexeme => match literals.integer lexeme with | some value => .value (integer value) | none => .stuck
  | .float lexeme => match literals.float lexeme with | some bits => .value (float bits) | none => .stuck
  | .duration lexeme => match literals.duration lexeme with | some value => .value (duration value) | none => .stuck
  | .string value => .value (.scalar (.string value))
  | .bytes value => .value (.scalar (.bytes value))
  | _ => .unsupported

def integerResult (errors : Errors Declaration Identity Source) :
    IntegerOps.IntegerResult → Result Declaration Identity Source (Value Declaration Identity Source)
  | .value value => .value (integer value)
  | .fault fault => .failure (.runtime (errors.integer fault))

def durationResult (errors : Errors Declaration Identity Source) :
    DurationOps.DurationResult → Result Declaration Identity Source (Value Declaration Identity Source)
  | .millis value => .value (duration value)
  | .count value => .value (integer value)
  | .fault fault => .failure (.runtime (errors.duration fault))

def integerOperator : Xsh.Syntax.BinaryOp → Option IntegerOps.IntegerBinaryOp
  | .add => some .add
  | .sub => some .subtract
  | .mul => some .multiply
  | .div => some .divide
  | .rem => some .remainder
  | _ => none

def floatOperator : Xsh.Syntax.BinaryOp → Option FloatBinaryOp
  | .add => some .add
  | .sub => some .subtract
  | .mul => some .multiply
  | .div => some .divide
  | _ => none

def ordering : Xsh.Syntax.BinaryOp → Option FloatComparison
  | .lt => some .less
  | .le => some .lessEqual
  | .gt => some .greater
  | .ge => some .greaterEqual
  | _ => none

def orders (comparison : FloatComparison) (less equal : Bool) : Bool :=
  match comparison with
  | .less => less
  | .lessEqual => less || equal
  | .greater => !less && !equal
  | .greaterEqual => !less

def codePoints (text : String) : List Nat := text.toList.map Char.toNat

def scalarEqual : Scalar → Scalar → Option Bool
  | .null, .null => some true
  | .boolean left, .boolean right => some (left == right)
  | .integer left, .integer right => some (decide (left.val = right.val))
  | .float left, .float right => some (floatValueEq left right)
  | .string left, .string right => some (decide (left = right))
  | .duration left, .duration right => some (decide (left.val = right.val))
  | .bytes left, .bytes right => some (decide (left = right))
  | .path left, .path right => some (decide (left = right))
  | .digest left, .digest right => some (decide (left = right))
  | .regex left, .regex right => some (decide (left = right))
  | _, _ => none

def mapKeyOf (candidate : Value Declaration Identity Source) : Option MapKey :=
  match candidate.eraseNominal with
  | .scalar (.string value) => some (.string value)
  | .scalar (.integer value) => some (.integer value)
  | .scalar (.boolean value) => some (.boolean value)
  | .scalar (.bytes value) => some (.bytes value)
  | .scalar (.path value) => some (.path value)
  | .scalar (.duration value) => some (.duration value)
  | _ => none

/-- Scalar equality where a `T` is compared with a `T?` (SPEC 6.2): `null`
equals only `null`. -/
def scalarsEqual : Scalar → Scalar → Option Bool
  | .null, .null => some true
  | .null, _ | _, .null => some false
  | left, right => scalarEqual left right

def isNull (value : Value Declaration Identity Source) : Bool :=
  match value.eraseNominal with
  | .scalar .null => true
  | _ => false

/-- Whether `paths` holds exactly the `Path` items of `items`, in order: an
`env.PATH` value compared with a `List[Path]`. -/
def pathsEqual (paths : List NativePath) (items : List (Value Declaration Identity Source)) : Bool :=
  paths.length == items.length &&
    (paths.zip items).all fun (path, item) =>
      match item.eraseNominal with
      | .scalar (.path other) => decide (path = other)
      | _ => false

/-- The comparisons equality needs from the world: declarations, runtime
identities and sources. -/
structure Sameness (Declaration Identity Source : Type) where
  declaration : Declaration → Declaration → Bool
  identity : Identity → Identity → Bool
  source : Source → Source → Bool

def Context.sameness (context : Context Declaration Identity Source) : Sameness Declaration Identity Source :=
  ⟨context.sameDeclaration, context.sameIdentity, context.sameSource⟩

def spanEqual (same : Sameness Declaration Identity Source) : Option (SourceSpan Source) → Option (SourceSpan Source) → Bool
  | none, none => true
  | some left, some right => same.source left.source right.source && left.start == right.start && left.stop == right.stop
  | _, _ => false

def contextsEqual (same : Sameness Declaration Identity Source) : List (ErrorContext Source) → List (ErrorContext Source) → Bool
  | [], [] => true
  | left :: lefts, right :: rights =>
    left.kind == right.kind && left.message == right.message && spanEqual same left.span right.span &&
      contextsEqual same lefts rights
  | _, _ => false

/-- A handle's family and runtime identity. -/
def handleOf : Value Declaration Identity Source → Option (Nat × Identity)
  | .processHandle reference => some (0, reference.identity)
  | .networkJob reference => some (1, reference.identity)
  | .filesystemRoot reference => some (2, reference.identity)
  | .filesystemLock reference => some (3, reference.identity)
  | .sshSession reference => some (4, reference.identity)
  | _ => none

/-- Two handles of one family are equal when they are the same runtime
identity; a handle is unequal to `null`. -/
def handleEqual (same : Sameness Declaration Identity Source) (left right : Value Declaration Identity Source) : Option Bool :=
  match handleOf left, handleOf right.eraseNominal with
  | some (family, identity), some (other, otherIdentity) =>
    if family = other then some (same.identity identity otherIdentity) else none
  | _, _ => if isNull right then some false else none

/-- Two segment statuses are equal when every field is, the process each ran
as compared by runtime identity, as the binary compares its pid. -/
def segmentEqual (same : Sameness Declaration Identity Source) (left right : SegmentStatus Identity) : Bool :=
  left.index == right.index && left.target == right.target && left.success == right.success &&
    decide (left.kind = right.kind) && left.code == right.code && left.errorKind == right.errorKind &&
    left.errorMessage == right.errorMessage &&
    match left.process, right.process with
    | some process, some other => same.identity process.identity other.identity
    | none, none => true
    | _, _ => false

/-- Two process statuses are equal when every field is (SPEC 6.2), their
segments pairwise by `segmentEqual`. -/
def statusEqual (same : Sameness Declaration Identity Source) (left right : ProcessStatus Identity) : Bool :=
  left.success == right.success && decide (left.kind = right.kind) && left.code == right.code &&
    left.segments.length == right.segments.length &&
    (left.segments.zip right.segments).all fun (one, other) => segmentEqual same one other

mutual
  /-- Structural language equality (SPEC 6.2, owner decision 1): Float
  elements compare by IEEE value, list items pairwise, map entries and record
  fields by key whatever their order, set elements as sets, `Ok` and `Err`
  payloads structurally, variants by declaration, name and payload, and a
  `null` equals only `null`, errors by every field of their data, their
  payloads and causes structurally, and handles and callables by family and
  runtime identity. Nominal wrappers are erased at every level. `none` is a
  comparison the model does not cover: streams, modules, filesystem entries,
  commands, process statuses and errors, or operands of different types. -/
  def valueEqual (same : Sameness Declaration Identity Source) : Value Declaration Identity Source → Value Declaration Identity Source → Option Bool
    | .nominal _ inner, right => valueEqual same inner right
    | .scalar left, right =>
      match right.eraseNominal with
      | .scalar other => scalarsEqual left other
      | _ => match left with
        | .null => some false
        | _ => none
    | .list items, right =>
      match right.eraseNominal with
      | .list others => itemsEqual same items others
      | .environmentPaths paths => some (pathsEqual paths items)
      | .scalar .null => some false
      | _ => none
    | .environmentPaths paths, right =>
      match listValues right with
      | some others => some (pathsEqual paths others)
      | none => if isNull right then some false else none
    | .map entries, right =>
      match right.eraseNominal with
      | .map others => if entries.length = others.length then entriesEqual same entries others else some false
      | .scalar .null => some false
      | _ => none
    | .set keys, right =>
      match right.eraseNominal with
      | .set others => some (keys.length == others.length && keys.all others.contains)
      | .scalar .null => some false
      | _ => none
    | .record fields, right =>
      match right.eraseNominal with
      | .record others => if fields.length = others.length then fieldsEqual same fields others else some false
      | .scalar .null => some false
      | _ => none
    | .ok inner, right =>
      match right.eraseNominal with
      | .ok other => valueEqual same inner other
      | .err _ | .scalar .null => some false
      | _ => none
    | .err inner, right =>
      match right.eraseNominal with
      | .err other => valueEqual same inner other
      | .ok _ | .scalar .null => some false
      | _ => none
    | .tagged declaration variant fields, right =>
      match right.eraseNominal with
      | .tagged other otherVariant others =>
        if same.declaration declaration other && variant == otherVariant then itemsEqual same fields others else some false
      | .scalar .null => some false
      | _ => none
    | .error left, right =>
      match right.eraseNominal with
      | .error other => errorEqual same left other
      | .scalar .null => some false
      | _ => none
    | .callable reference kind, right =>
      match right.eraseNominal with
      | .callable other otherKind => some (kind == otherKind && same.identity reference.identity other.identity)
      | .scalar .null => some false
      | _ => none
    | .module _ exports, right =>
      match right.eraseNominal with
      | .module _ others => if exports.length = others.length then fieldsEqual same exports others else some false
      | .scalar .null => some false
      | _ => none
    | .processError left, right =>
      match right.eraseNominal with
      | .processError other => processErrorEqual same left other
      | .scalar .null => some false
      | _ => none
    | .status status, right =>
      match right.eraseNominal with
      | .status other => some (statusEqual same status other)
      | .scalar .null => some false
      | _ => none
    | .processHandle reference, right => handleEqual same (.processHandle reference) right
    | .networkJob reference, right => handleEqual same (.networkJob reference) right
    | .filesystemRoot reference, right => handleEqual same (.filesystemRoot reference) right
    | .filesystemLock reference, right => handleEqual same (.filesystemLock reference) right
    | .sshSession reference, right => handleEqual same (.sshSession reference) right
    | left, right => if isNull right && !isNull left then some false else none

  /-- Two errors are equal when every field of their data is: payloads and
  causes structurally, spans and contexts by source and position. -/
  def errorEqual (same : Sameness Declaration Identity Source) :
      ErrorData Declaration Identity Source → ErrorData Declaration Identity Source → Option Bool
    | ⟨family, variant, kind, message, payload, facets, span, contexts, cause, abort⟩,
      ⟨family', variant', kind', message', payload', facets', span', contexts', cause', abort'⟩ =>
      if same.declaration family family' && variant == variant' && kind == kind' && message == message' &&
          facets == facets' && spanEqual same span span' && contextsEqual same contexts contexts' &&
          decide (abort = abort') && payload.length == payload'.length then
        (fieldsEqual same payload payload').bind fun equal => if equal then causeEqual same cause cause' else some false
      else some false

  /-- Two process errors are equal when every field is: kind, message, span
  and contexts by source and position, status field by field, and the cause
  structurally. -/
  def processErrorEqual (same : Sameness Declaration Identity Source) :
      ProcessErrorData Declaration Identity Source → ProcessErrorData Declaration Identity Source → Option Bool
    | ⟨kind, message, span, status, contexts, cause⟩, ⟨kind', message', span', status', contexts', cause'⟩ =>
      if kind == kind' && message == message' && spanEqual same span span' && contextsEqual same contexts contexts' &&
          (match status, status' with
            | some left, some right => statusEqual same left right
            | none, none => true
            | _, _ => false) then
        causeEqual same cause cause'
      else some false

  def causeEqual (same : Sameness Declaration Identity Source) :
      Option (Value Declaration Identity Source) → Option (Value Declaration Identity Source) → Option Bool
    | none, none => some true
    | some left, some right => valueEqual same left right
    | _, _ => some false

  def itemsEqual (same : Sameness Declaration Identity Source) : List (Value Declaration Identity Source) → List (Value Declaration Identity Source) → Option Bool
    | [], [] => some true
    | [], _ :: _ | _ :: _, [] => some false
    | item :: items, other :: others =>
      (valueEqual same item other).bind fun equal => if equal then itemsEqual same items others else some false

  /-- Every entry of the first map is in the second with an equal value; with
  equal sizes and distinct keys, the maps are equal. -/
  def entriesEqual (same : Sameness Declaration Identity Source) : List (MapKey × Value Declaration Identity Source) →
      List (MapKey × Value Declaration Identity Source) → Option Bool
    | [], _ => some true
    | (key, value) :: rest, others =>
      match others.lookup key with
      | none => some false
      | some other => (valueEqual same value other).bind fun equal => if equal then entriesEqual same rest others else some false

  def fieldsEqual (same : Sameness Declaration Identity Source) : List (String × Value Declaration Identity Source) →
      List (String × Value Declaration Identity Source) → Option Bool
    | [], _ => some true
    | (label, value) :: rest, others =>
      match others.lookup label with
      | none => some false
      | some other => (valueEqual same value other).bind fun equal => if equal then fieldsEqual same rest others else some false
end

/-- Whether `needle` occurs as a contiguous run of `haystack`. -/
def containsRun {α : Type} [BEq α] (needle : List α) : List α → Bool
  | [] => needle.isEmpty
  | first :: rest => needle.isPrefixOf (first :: rest) || containsRun needle rest

/-- Whether some item equals `needle`; an uncovered comparison before the
first equal item is uncovered. -/
def anyEqual (same : Sameness Declaration Identity Source) (needle : Value Declaration Identity Source) :
    List (Value Declaration Identity Source) → Option Bool
  | [] => some false
  | item :: items => (valueEqual same item needle).bind fun equal => if equal then some true else anyEqual same needle items

/-- Whether a path's display text is its bytes: they are valid UTF-8. -/
def displayable (path : NativePath) : Bool := (String.fromUTF8? ⟨path.bytes.toArray⟩).isSome

/-- `needle in container` (SPEC 6.2): substring containment in a `Str`, byte
containment in `Bytes`, an equal item in a `List`, a `Path` entry of
`env.PATH`, key presence in a `Map` (whatever its value, `null` included), an
element of a `Set`, a `Str` field label of a record, and display-text
containment of a `Str` or `Path` in a `Path`, a path's display text being its
bytes decoded lossily as the binary decodes them. -/
def membership (same : Sameness Declaration Identity Source) (needle container : Value Declaration Identity Source) :
    Option Bool :=
  match needle.eraseNominal, container.eraseNominal with
  | .scalar (.string part), .scalar (.string text) => some (containsRun part.toList text.toList)
  | .scalar (.bytes part), .scalar (.bytes data) => some (containsRun part data)
  | .scalar (.string part), .scalar (.path path) =>
    some (containsRun part.toUTF8.toList (Xsh.Values.LossyUtf8.bytes path.bytes))
  | .scalar (.path part), .scalar (.path path) =>
    some (containsRun (Xsh.Values.LossyUtf8.bytes part.bytes) (Xsh.Values.LossyUtf8.bytes path.bytes))
  | _, .list items => anyEqual same needle items
  | .scalar (.path entry), .environmentPaths paths => some (paths.contains entry)
  | _, .map entries => (mapKeyOf needle).map fun key => entries.any fun entry => entry.1 == key
  | _, .set keys => (mapKeyOf needle).map keys.contains
  | .scalar (.string label), .record fields => some (fields.any fun field => field.1 == label)
  | _, _ => none

/-- `key` placed before the first element it sorts before. -/
def placeKey (key : MapKey) : List MapKey → List MapKey
  | [] => [key]
  | other :: rest => if keyBefore key other then key :: other :: rest else other :: placeKey key rest

/-- `key` added to a set kept in map key order (SPEC 4.5): an element it
holds is not added again. -/
def insertKey (key : MapKey) (keys : List MapKey) : List MapKey :=
  if keys.contains key then keys else placeKey key keys

/-- The set holding `keys`, in map key order without repeats, whatever order
built it. -/
def setOf (keys : List MapKey) : List MapKey := keys.foldl (fun built key => insertKey key built) []

/-- `|`, `&` and `-` on two sets (SPEC 6.2). -/
def setOperation : Xsh.Syntax.BinaryOp → List MapKey → List MapKey → Option (List MapKey)
  | .union, left, right => some (setOf (left ++ right))
  | .intersect, left, right => some (setOf (left.filter right.contains))
  | .sub, left, right => some (setOf (left.filter fun key => !right.contains key))
  | _, _, _ => none

def decided : Option Bool → Result Declaration Identity Source (Value Declaration Identity Source)
  | some value => .value (.scalar (.boolean value))
  | none => .unsupported

/-- `in` and `not in`, and `==` and `!=` where the operands are not two
scalars of one family the operator table gives its own meaning. -/
def structural (same : Sameness Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (left right : Value Declaration Identity Source) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  match operator with
  | .eq => decided (valueEqual same left right)
  | .ne => decided ((valueEqual same left right).map not)
  | .member => decided (membership same left right)
  | .notMember => decided ((membership same left right).map not)
  | _ =>
    match left.eraseNominal, right.eraseNominal with
    | .set first, .set second =>
      match setOperation operator first second with
      | some keys => .value (.set keys)
      | none => .unsupported
    | _, _ => .unsupported

/-- The value meaning of a binary operator on two evaluated operands. -/
def binary (context : Context Declaration Identity Source) (operator : Xsh.Syntax.BinaryOp)
    (left right : Value Declaration Identity Source) : Result Declaration Identity Source (Value Declaration Identity Source) :=
  match left.eraseNominal, right.eraseNominal with
  | .scalar (.integer a), .scalar (.integer b) =>
    match integerOperator operator, ordering operator with
    | some op, _ => integerResult context.errors (IntegerOps.applyBinary op a b)
    | none, some comparison => .value (boolean (orders comparison (decide (a.val < b.val)) (decide (a.val = b.val))))
    | none, none => match operator with
      | .eq => .value (boolean (decide (a.val = b.val)))
      | .ne => .value (boolean (!decide (a.val = b.val)))
      | _ => .unsupported
  | .scalar (.float a), .scalar (.float b) =>
    match floatOperator operator, ordering operator with
    | some op, _ => .value (float (floatApply context.policy op a b))
    | none, some comparison => .value (boolean (floatCompare comparison a b))
    | none, none => match operator with
      | .eq => .value (boolean (floatValueEq a b))
      | .ne => .value (boolean (floatValueNeq a b))
      | _ => .unsupported
  | .scalar (.duration a), .scalar (.duration b) =>
    match operator, ordering operator with
    | .add, _ => durationResult context.errors (DurationOps.add a b)
    | .sub, _ => durationResult context.errors (DurationOps.subtract a b)
    | .div, _ => durationResult context.errors (DurationOps.ratio a b)
    | .eq, _ => .value (boolean (decide (a.val = b.val)))
    | .ne, _ => .value (boolean (!decide (a.val = b.val)))
    | _, some comparison => .value (boolean (orders comparison (decide (a.val < b.val)) (decide (a.val = b.val))))
    | _, none => .unsupported
  | .scalar (.duration a), .scalar (.integer b) =>
    match operator with
    | .mul => durationResult context.errors (DurationOps.multiply a b)
    | .div => durationResult context.errors (DurationOps.divideByInt a b)
    | _ => .unsupported
  | .scalar (.string a), .scalar (.string b) =>
    match operator, ordering operator with
    | .add, _ => .value (.scalar (.string (a ++ b)))
    | .eq, _ => .value (boolean (decide (a = b)))
    | .ne, _ => .value (boolean (!decide (a = b)))
    | .member, _ => decided (membership context.sameness left right)
    | .notMember, _ => decided ((membership context.sameness left right).map not)
    | _, some comparison =>
      .value (boolean (orders comparison (lexBefore (codePoints a) (codePoints b)) (decide (a = b))))
    | _, none => .unsupported
  | .scalar (.integer a), .scalar (.duration b) =>
    match operator with
    | .mul => durationResult context.errors (DurationOps.multiply b a)
    | _ => .unsupported
  | .scalar a, .scalar b =>
    match operator, scalarsEqual a b with
    | .eq, some same => .value (boolean same)
    | .ne, some same => .value (boolean (!same))
    | _, _ => structural context.sameness operator left right
  | _, _ =>
    match operator, listValues left, listValues right with
    | .add, some first, some second => .value (.list (first ++ second))
    | _, _, _ => structural context.sameness operator left right

def unary (context : Context Declaration Identity Source) :
    Xsh.Syntax.UnaryOp → Value Declaration Identity Source → Result Declaration Identity Source (Value Declaration Identity Source)
  | operator, operand =>
    match operator, operand.eraseNominal with
    | .not, .scalar (.boolean value) => .value (boolean !value)
    | .neg, .scalar (.integer value) => integerResult context.errors (IntegerOps.negate value)
    | .neg, .scalar (.float bits) => .value (float (floatNegate bits))
    | _, _ => .unsupported

def fieldOf (context : Context Declaration Identity Source) (name : Xsh.Syntax.RawName) :
    Value Declaration Identity Source → Result Declaration Identity Source (Value Declaration Identity Source)
  | base =>
    match base.eraseNominal with
    | .record fields =>
      match fields.lookup (context.spelling name) with
      | some found => .value found
      | none => .failure (.runtime context.errors.missingField)
    | _ => .unsupported

def indexAt (context : Context Declaration Identity Source) (fromEnd : Bool)
    (base key : Value Declaration Identity Source) : Result Declaration Identity Source (Value Declaration Identity Source) :=
  match listValues base, key with
  | some items, .scalar (.integer position) =>
    let index : Int := if fromEnd ∧ position.val < 0 then (items.length : Int) + position.val else position.val
    if inRange : 0 ≤ index ∧ index < items.length then .value (items[index.toNat]'(by omega))
    else .failure (.runtime context.errors.indexOutOfRange)
  | _, _ =>
    match base.eraseNominal, mapKeyOf key with
    | .map entries, some mapKey =>
      match entries.lookup mapKey with
      | some found => .value found
      | none => .failure (.runtime context.errors.mapMissing)
    | .record fields, some (.string label) =>
      match fields.lookup label with
      | some found => .value found
      | none => .failure (.runtime context.errors.missingField)
    | _, _ => .unsupported

/-- An evaluated brace-literal entry (SPEC 6.4). -/
inductive BraceEntry (Declaration Identity Source : Type) where
  | field (label : String) (value : Value Declaration Identity Source)
  | key (key value : Value Declaration Identity Source)
  | spread (value : Value Declaration Identity Source)
  | path (labels : List String) (value : Value Declaration Identity Source)

/-- `label` given `value`: an existing field is replaced in place, a new one
added at the end. -/
def setField (label : String) (value : Value Declaration Identity Source) :
    List (String × Value Declaration Identity Source) → List (String × Value Declaration Identity Source)
  | [] => [(label, value)]
  | (other, held) :: rest => if other = label then (other, value) :: rest else (other, held) :: setField label value rest

/-- The nested field `labels` replaced by `value`; every label must name an
existing field, and every one but the last a record. -/
def updatePath (value : Value Declaration Identity Source) :
    List String → List (String × Value Declaration Identity Source) → Option (List (String × Value Declaration Identity Source))
  | [], _ => none
  | [label], fields => (fields.lookup label).map fun _ => setField label value fields
  | label :: rest, fields =>
    match fields.lookup label with
    | some inner =>
      match inner.eraseNominal with
      | .record nested => (updatePath value rest nested).map fun updated => setField label (.record updated) fields
      | _ => none
    | none => none

def isKey : BraceEntry Declaration Identity Source → Bool
  | .key _ _ => true
  | _ => false

/-- A record literal's fields, later entries and spreads replacing earlier
fields and path entries replacing nested ones. -/
def recordOf : List (String × Value Declaration Identity Source) → List (BraceEntry Declaration Identity Source) →
    Option (List (String × Value Declaration Identity Source))
  | fields, [] => some fields
  | fields, .field label value :: rest => recordOf (setField label value fields) rest
  | fields, .spread value :: rest =>
    match value.eraseNominal with
    | .record spread => recordOf (spread.foldl (fun built (label, item) => setField label item built) fields) rest
    | _ => none
  | fields, .path labels value :: rest => (updatePath value labels fields).bind fun updated => recordOf updated rest
  | _, .key _ _ :: _ => none

/-- A map literal's entries: constant labels are `Str` keys, and a later
entry or spread replaces an earlier one. -/
def mapOf : List (MapKey × Value Declaration Identity Source) → List (BraceEntry Declaration Identity Source) →
    Option (List (MapKey × Value Declaration Identity Source))
  | entries, [] => some entries
  | entries, .field label value :: rest => mapOf (upsert (.string label) value entries) rest
  | entries, .key key value :: rest => (mapKeyOf key).bind fun mapKey => mapOf (upsert mapKey value entries) rest
  | entries, .spread value :: rest =>
    match value.eraseNominal with
    | .map spread => mapOf (spread.foldl (fun built (key, item) => upsert key item built) entries) rest
    | _ => none
  | _, .path _ _ :: _ => none

/-- A brace literal (SPEC 6.4): a computed key makes it a map, otherwise it
is a record. -/
def braceValue (entries : List (BraceEntry Declaration Identity Source)) : Option (Value Declaration Identity Source) :=
  if entries.any isKey then (mapOf [] entries).map .map else (recordOf [] entries).map .record

/-- A slice bound counted from the end when negative, clamped into
`[0, length]` (SPEC 6.6). -/
def clampBound (length : Nat) (bound : Int) : Nat :=
  let shifted : Int := if bound < 0 then (length : Int) + bound else bound
  (max 0 (min shifted (length : Int))).toNat

/-- The items from the clamped start up to the clamped end; an end before the
start gives no items. -/
def sliceItems {α : Type} (items : List α) (low high : Option Int) : List α :=
  let start := clampBound items.length (low.getD 0)
  let finish := clampBound items.length (high.getD items.length)
  (items.drop start).take (finish - start)

/-- An omitted bound, or an `Int` one. -/
def boundOf : Option (Value Declaration Identity Source) → Option (Option Int)
  | none => some none
  | some bound =>
    match bound.eraseNominal with
    | .scalar (.integer value) => some (some value.val)
    | _ => none

/-- `value[low..high]` (SPEC 6.6) on a `List`, a `Str` by Unicode scalar
values, or `Bytes` by bytes. -/
def sliceOf (base : Value Declaration Identity Source) (low high : Option (Value Declaration Identity Source)) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  match boundOf low, boundOf high with
  | some start, some finish =>
    match listValues base, base.eraseNominal with
    | some items, _ => .value (.list (sliceItems items start finish))
    | none, .scalar (.string text) => .value (.scalar (.string (String.ofList (sliceItems text.toList start finish))))
    | none, .scalar (.bytes data) => .value (.scalar (.bytes (sliceItems data start finish)))
    | none, _ => .unsupported
  | _, _ => .unsupported

def bitsetOperator : String → Option IntegerOps.BitsetOp
  | "bit_and" => some .bitAnd
  | "bit_or" => some .bitOr
  | "clear_bits" => some .clearBits
  | _ => none

/-- The numeric conversion and bitset methods (SPEC 4.2, 4.3): `.float()` on
an `Int`; `.floor()`, `.ceil()` and `.round()` on a `Float`, each `Ok` of the
`Int` or `Err` of the conversion error when the value is NaN, infinite or out
of range; `.parse_float()` on a `Str`, `Ok` of its reading or `Err` of the
parse error; and `.bit_and`, `.bit_or` and `.clear_bits` on non-negative
`Int`s, a negative operand failing with `integer-bitset`. -/
def numericMethod (errors : Errors Declaration Identity Source) (name : String)
    (receiver : Value Declaration Identity Source) (arguments : List (Value Declaration Identity Source)) :
    Result Declaration Identity Source (Value Declaration Identity Source) :=
  let converted := fun (result : Option IntValue) =>
    match result with
    | some value => Result.value (Value.ok (integer value))
    | none => Result.value (Value.err errors.floatConversion)
  match name, receiver.eraseNominal, arguments.map Value.eraseNominal with
  | "float", .scalar (.integer value), [] => .value (float (intToFloat value))
  | "floor", .scalar (.float bits), [] => converted (floatFloor bits)
  | "ceil", .scalar (.float bits), [] => converted (floatCeil bits)
  | "round", .scalar (.float bits), [] => converted (floatRound bits)
  | "parse_float", .scalar (.string text), [] =>
    match Xsh.Values.FloatLiterals.parseFloatText text with
    | some bits => .value (.ok (float bits))
    | none => .value (.err errors.parseFloat)
  | method, .scalar (.integer value), [.scalar (.integer mask)] =>
    match bitsetOperator method with
    | some operator =>
      match IntegerOps.applyBitset operator value mask with
      | .value result => .value (integer result)
      | .negativeOperand => .failure (.runtime errors.bitset)
    | none => .unsupported
  | _, _, _ => .unsupported

def isNumericMethod : String → Bool
  | "float" | "floor" | "ceil" | "round" | "bit_and" | "bit_or" | "clear_bits" | "parse_float" => true
  | _ => false

def negativeLiteral : Xsh.Syntax.Expr → Bool
  | .unary .neg (.literal (.integer _)) => true
  | _ => false

def evaluateAll {α : Type} (evaluate : α → Result Declaration Identity Source (Value Declaration Identity Source)) :
    List α → Result Declaration Identity Source (List (Value Declaration Identity Source))
  | [] => .value []
  | first :: rest => (evaluate first).bind fun value => (evaluateAll evaluate rest).bind fun values => .value (value :: values)

def evaluateFields (context : Context Declaration Identity Source)
    (evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    List Xsh.Syntax.RecordField → Result Declaration Identity Source (List (String × Value Declaration Identity Source))
  | [] => .value []
  | .named name operand :: rest =>
    (evaluate operand).bind fun value =>
      (evaluateFields context evaluate rest).bind fun fields => .value ((context.spelling name, value) :: fields)
  | _ :: _ => .unsupported

def positional : List Xsh.Syntax.CallArgument → Option (List Xsh.Syntax.Expr)
  | [] => some []
  | .positional value :: rest => (positional rest).map (value :: ·)
  | _ :: _ => none

/-- A condition's value: a `Bool`, or a `Result[Bool]`, whose `Err` leaves as
under `?` (SPEC 8.3). -/
def conditionValue (tested : Value Declaration Identity Source) : Option (Except (Value Declaration Identity Source) Bool) :=
  match tested.eraseNominal with
  | .scalar (.boolean value) => some (.ok value)
  | .ok inner =>
    match inner.eraseNominal with
    | .scalar (.boolean value) => some (.ok value)
    | _ => none
  | .err error => some (.error error)
  | _ => none

/-- A condition in a control position (SPEC 8.3): its `Bool`; a
`Result[Bool]` gives the `Bool` of its `Ok` and raises the error of its
`Err`, and so does each operand of `!`, `and` and `or`; `and` and `or` skip
the operand they do not need. -/
def controlCondition (evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)
  | .binary .and left right =>
    (controlCondition evaluate left).bind fun first =>
      match first.eraseNominal with
      | .scalar (.boolean false) => .value (boolean false)
      | .scalar (.boolean true) => controlCondition evaluate right
      | _ => .unsupported
  | .binary .or left right =>
    (controlCondition evaluate left).bind fun first =>
      match first.eraseNominal with
      | .scalar (.boolean true) => .value (boolean true)
      | .scalar (.boolean false) => controlCondition evaluate right
      | _ => .unsupported
  | .unary .not operand =>
    (controlCondition evaluate operand).bind fun tested =>
      match tested.eraseNominal with
      | .scalar (.boolean value) => .value (boolean !value)
      | _ => .unsupported
  | condition =>
    (evaluate condition).bind fun tested =>
      match conditionValue tested with
      | some (.ok value) => .value (boolean value)
      | some (.error error) => .failure (.raised error)
      | none => .unsupported

def branches (evaluate : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)) :
    List (Xsh.Syntax.Expr × Xsh.Syntax.Expr) → Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)
  | [], otherwise => evaluate otherwise
  | (condition, value) :: rest, otherwise =>
    (controlCondition evaluate condition).bind fun tested =>
      match tested.eraseNominal with
      | .scalar (.boolean true) => evaluate value
      | .scalar (.boolean false) => branches evaluate rest otherwise
      | _ => .stuck

/-- One evaluation step over `recurse`, which evaluates a subterm one fuel
level down in the given environment, and `callBlock`, which runs a block
function body. Operands evaluate once, left to right; a function body
evaluates in the environment the function was defined in. -/
def step (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world)
    (recurse : Environment → Locals Declaration Identity Source → Xsh.Syntax.Expr →
      Result Declaration Identity Source (Value Declaration Identity Source))
    (callBlock : Environment → Locals Declaration Identity Source → List Xsh.Syntax.Statement → ReturnKind →
      Result Declaration Identity Source (Value Declaration Identity Source))
    (locals : Locals Declaration Identity Source) :
    Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)
  | .literal value => literal context.literals value
  | .ident name =>
    match locals.lookup name with
    | some value => .value value
    | none =>
      match environment.cellOf name with
      | .ok cell => match store cell with
        | some found => .value found.contents.value
        | none => .stuck
      | .error _ => .stuck
  | .list elements => (evaluateAll (recurse environment locals) elements).bind fun values => .value (.list values)
  | .record fields => (evaluateFields context (recurse environment locals) fields).bind fun values => .value (.record values)
  | .field base name => (recurse environment locals base).bind (fieldOf context name)
  | .index base key false =>
    (recurse environment locals base).bind fun container =>
      (recurse environment locals key).bind fun selector => indexAt context (negativeLiteral key) container selector
  | .unary operator operand => (recurse environment locals operand).bind (unary context operator)
  | .binary .and left right =>
    (recurse environment locals left).bind fun first =>
      match first.eraseNominal with
      | .scalar (.boolean false) => .value (boolean false)
      | .scalar (.boolean true) =>
        (recurse environment locals right).bind fun second =>
          match second.eraseNominal with
          | .scalar (.boolean value) => .value (boolean value)
          | _ => .stuck
      | _ => .stuck
  | .binary .or left right =>
    (recurse environment locals left).bind fun first =>
      match first.eraseNominal with
      | .scalar (.boolean true) => .value (boolean true)
      | .scalar (.boolean false) =>
        (recurse environment locals right).bind fun second =>
          match second.eraseNominal with
          | .scalar (.boolean value) => .value (boolean value)
          | _ => .stuck
      | _ => .stuck
  | .binary operator left right =>
    (recurse environment locals left).bind fun first => (recurse environment locals right).bind fun second => binary context operator first second
  | .ifExpr conditions otherwise => branches (recurse environment locals) conditions otherwise
  | .call (.ident name) arguments =>
    match locals.lookup name, context.functions name, positional arguments with
    | none, some (defining, parameters, body), some values =>
      (evaluateAll (recurse environment locals) values).bind fun evaluated =>
        if parameters.length = evaluated.length then
          match body with
          | .expression result .plain => recurse defining (parameters.zip evaluated) result
          | .expression result .result => callBlock defining (parameters.zip evaluated) [.expression result] .result
          | .block statements kind => callBlock defining (parameters.zip evaluated) statements kind
        else .stuck
    | _, _, _ => .unsupported
  | _ => .unsupported

def evaluate (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
    (environment : Environment) (store : Store world) :
    Nat → Locals Declaration Identity Source → Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source)
  | 0, _, _ => .outOfFuel
  | fuel + 1, locals, expression =>
    step context environment store (fun inner => evaluate context inner store fuel) (fun _ _ _ _ => .unsupported) locals
      expression

def compoundBinary : Xsh.Syntax.AssignOp → Option Xsh.Syntax.BinaryOp
  | .set => none
  | .add => some .add
  | .sub => some .sub
  | .mul => some .mul
  | .div => some .div
  | .rem => some .rem

/-- The compound-assignment operator meaning the evaluator gives where it is
defined; `fallback` stands for the forms it does not cover. -/
def operate (context : Context Declaration Identity Source) (fallback : Operate Declaration Identity Source) :
    Operate Declaration Identity Source := fun op current right =>
  match compoundBinary op with
  | none => fallback op current right
  | some operator =>
    match binary context operator current right with
    | .value result => .ok result
    | .failure (.runtime error) => .error error
    | _ => fallback op current right

/-- `Oracles.evaluate` answered by the evaluator where it gives a result;
`fallback` stands for the forms it does not cover. -/
def oracles {world : World Declaration Identity Source} (context : Context Declaration Identity Source) (fuel : Nat)
    (fallback : Xsh.Semantics.CoreStatements.Oracles world) : Xsh.Semantics.CoreStatements.Oracles world :=
  { fallback with
    evaluate := fun environment store expression =>
      match evaluate context environment store fuel [] expression with
      | .value value => .ok (value, store)
      | .failure failure => .error (failure, store)
      | _ => fallback.evaluate environment store expression }

end Xsh.Semantics.CoreEval
