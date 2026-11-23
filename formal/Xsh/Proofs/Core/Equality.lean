import Xsh.Proofs.Core.EvalStatements

namespace Xsh.Proofs.Core.Equality

open Xsh.Foundation
open Xsh.Values
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreEval

section Laws

variable {Declaration Identity Source : Type} (same : Sameness Declaration Identity Source)

theorem valueEqual_erase (left right : Value Declaration Identity Source) :
    valueEqual same left.eraseNominal right = valueEqual same left right := by
  match left with
  | .nominal _ inner => simpa [Value.eraseNominal, valueEqual] using valueEqual_erase inner right
  | .scalar _ | .list _ | .map _ | .set _ | .stream _ | .record _ | .filesystemEntry _ | .module _ _ | .ok _
  | .err _ | .status _ | .environmentPaths _ | .error _ | .processError _ | .callable _ _ | .command _
  | .processHandle _ | .networkJob _ | .filesystemRoot _ | .filesystemLock _ | .sshSession _ | .tagged _ _ _ =>
    rfl

/-- A value that is not `null` is unequal to `null`: `T == T?` compares the
`null` case (SPEC 6.2). -/
theorem valueEqual_null (value : Value Declaration Identity Source) (present : isNull value = false) :
    valueEqual same value (.scalar .null) = some false := by
  match value with
  | .nominal _ inner =>
    have : isNull inner = false := by simpa [isNull, Value.eraseNominal] using present
    simpa [valueEqual] using valueEqual_null inner this
  | .scalar scalar =>
    cases scalar <;> simp_all [valueEqual, isNull, scalarsEqual, Value.eraseNominal]
  | .environmentPaths _ => simp [valueEqual, listValues, isNull, Value.eraseNominal]
  | .list _ | .map _ | .set _ | .record _ | .ok _ | .err _ => simp [valueEqual, Value.eraseNominal]
  | .stream _ | .filesystemEntry _ | .module _ _ | .status _ | .error _ | .processError _ | .callable _ _
  | .command _ | .processHandle _ | .networkJob _ | .filesystemRoot _ | .filesystemLock _ | .sshSession _
  | .tagged _ _ _ =>
    simp [valueEqual, handleEqual, handleOf, isNull, Value.eraseNominal]

/-- Record equality reads the other record's fields only by label, so their
order is not observed. -/
theorem fieldsEqual_lookup :
    ∀ (fields others others' : List (String × Value Declaration Identity Source)),
      (∀ label, others.lookup label = others'.lookup label) →
        fieldsEqual same fields others = fieldsEqual same fields others'
  | [], _, _, _ => by simp [fieldsEqual]
  | (label, value) :: rest, others, others', lookups => by
    simp only [fieldsEqual, lookups label]
    split
    · rfl
    · congr 1
      funext equal
      split
      · exact fieldsEqual_lookup rest others others' lookups
      · rfl

theorem record_order_free (fields others others' : List (String × Value Declaration Identity Source))
    (sameSize : others.length = others'.length) (lookups : ∀ label, others.lookup label = others'.lookup label) :
    valueEqual same (.record fields) (.record others) = valueEqual same (.record fields) (.record others') := by
  simp only [valueEqual, Value.eraseNominal, sameSize, fieldsEqual_lookup same fields others others' lookups]

theorem entriesEqual_lookup :
    ∀ (entries others others' : List (MapKey × Value Declaration Identity Source)),
      (∀ key, others.lookup key = others'.lookup key) →
        entriesEqual same entries others = entriesEqual same entries others'
  | [], _, _, _ => by simp [entriesEqual]
  | (key, value) :: rest, others, others', lookups => by
    simp only [entriesEqual, lookups key]
    split
    · rfl
    · congr 1
      funext equal
      split
      · exact entriesEqual_lookup rest others others' lookups
      · rfl

theorem map_order_free (entries others others' : List (MapKey × Value Declaration Identity Source))
    (sameSize : others.length = others'.length) (lookups : ∀ key, others.lookup key = others'.lookup key) :
    valueEqual same (.map entries) (.map others) = valueEqual same (.map entries) (.map others') := by
  simp only [valueEqual, Value.eraseNominal, sameSize, entriesEqual_lookup same entries others others' lookups]

theorem valueEqual_scalar (scalar : Scalar) (right : Value Declaration Identity Source) {other : Scalar}
    (shape : right.eraseNominal = .scalar other) : valueEqual same (.scalar scalar) right = scalarsEqual scalar other := by
  unfold valueEqual
  simp [shape]

/-- `==` and `!=` are structural equality on every pair of operands. -/
theorem equality_is_structural (context : Context Declaration Identity Source) (left right : Value Declaration Identity Source) :
    binary context .eq left right = decided (valueEqual context.sameness left right) ∧
      binary context .ne left right = decided ((valueEqual context.sameness left right).map not) := by
  rw [← valueEqual_erase context.sameness left right]
  constructor <;>
  · unfold binary
    split
    all_goals rename_i leftShape rightShape
    all_goals first
      | (simp only [leftShape, rightShape, valueEqual, scalarsEqual, scalarEqual, decided, integerOperator,
          ordering, floatOperator, boolean, Option.map, floatValueNeq]; done)
      | (simp only [leftShape, valueEqual_scalar _ _ right rightShape]
         split
         all_goals first
           | (simp_all [decided, boolean]; done)
           | (simp only [structural]
              rw [← valueEqual_erase context.sameness left, leftShape, valueEqual_scalar _ _ right rightShape]))
      | (split
         all_goals first
           | (simp_all; done)
           | simp only [structural, valueEqual_erase])

/-- `in` and `not in` are the membership same decision on every pair of operands. -/
theorem membership_is_decision (context : Context Declaration Identity Source)
    (needle container : Value Declaration Identity Source) :
    binary context .member needle container = decided (membership context.sameness needle container) ∧
      binary context .notMember needle container = decided ((membership context.sameness needle container).map not) := by
  constructor <;>
  · unfold binary
    split
    all_goals rename_i leftShape rightShape
    all_goals first
      | (simp only [membership, leftShape, rightShape, decided, integerOperator, ordering, floatOperator,
          Option.map]; done)
      | (split
         all_goals first
           | (simp_all; done)
           | simp only [structural])

/-- A map's key is present whatever value it holds, `null` included. -/
theorem map_key_presence (needle : Value Declaration Identity Source) (key : MapKey)
    (entries : List (MapKey × Value Declaration Identity Source)) (keyed : mapKeyOf needle = some key) :
    membership same needle (.map entries) = some (entries.any fun entry => entry.1 == key) := by
  unfold membership
  split <;> simp_all [Value.eraseNominal]

theorem placeKey_perm (key : MapKey) : ∀ keys, (placeKey key keys).Perm (key :: keys)
  | [] => List.Perm.refl _
  | first :: rest => by
    unfold placeKey
    split
    · exact List.Perm.refl _
    · exact ((placeKey_perm key rest).cons first).trans (List.Perm.swap key first rest)

theorem insertKey_mem (key other : MapKey) (keys : List MapKey) :
    other ∈ insertKey key keys ↔ other = key ∨ other ∈ keys := by
  unfold insertKey
  split
  · rename_i held
    constructor
    · exact Or.inr
    · rintro (rfl | found)
      · simpa using held
      · exact found
  · rw [(placeKey_perm key keys).mem_iff]
    simp

theorem insertKey_nodup (key : MapKey) (keys : List MapKey) (distinct : keys.Nodup) : (insertKey key keys).Nodup := by
  unfold insertKey
  split
  · exact distinct
  · rename_i absent
    rw [(placeKey_perm key keys).nodup_iff, List.nodup_cons]
    exact ⟨by simpa using absent, distinct⟩

theorem setOf_spec (keys : List MapKey) : (setOf keys).Nodup ∧ ∀ key, key ∈ setOf keys ↔ key ∈ keys := by
  unfold setOf
  suffices ∀ (built : List MapKey), built.Nodup →
      (keys.foldl (fun built key => insertKey key built) built).Nodup ∧
        ∀ key, key ∈ keys.foldl (fun built key => insertKey key built) built ↔ key ∈ built ∨ key ∈ keys by
    obtain ⟨distinct, members⟩ := this [] List.nodup_nil
    exact ⟨distinct, fun key => by simpa using members key⟩
  induction keys with
  | nil => intro built distinct; simpa using distinct
  | cons first rest ih =>
    intro built distinct
    obtain ⟨nextDistinct, members⟩ := ih (insertKey first built) (insertKey_nodup first built distinct)
    refine ⟨nextDistinct, fun key => ?_⟩
    rw [List.foldl_cons, members, insertKey_mem]
    simp only [List.mem_cons]
    constructor
    · rintro ((same | found) | found) <;> simp_all
    · rintro (found | same | found) <;> simp_all

/-- Two sets with the same elements are equal, whatever order and repeats
built them (SPEC 4.5). -/
theorem sets_with_same_elements_equal (first second : List MapKey) (members : ∀ key, key ∈ first ↔ key ∈ second) :
    valueEqual same (Declaration := Declaration) (Identity := Identity) (Source := Source)
      (.set (setOf first)) (.set (setOf second)) = some true := by
  obtain ⟨firstDistinct, firstMembers⟩ := setOf_spec first
  obtain ⟨secondDistinct, secondMembers⟩ := setOf_spec second
  have permuted : (setOf first).Perm (setOf second) :=
    (List.perm_ext_iff_of_nodup firstDistinct secondDistinct).mpr fun key => by
      rw [firstMembers, secondMembers, members]
  simp only [valueEqual, Value.eraseNominal, permuted.length_eq, beq_self_eq_true, Bool.true_and,
    Option.some.injEq, List.all_eq_true]
  intro key found
  simpa using permuted.subset found

/-- `|`, `&` and `-` give the sets of the union, intersection and difference of
their operands' elements, each element once. -/
theorem set_operations (first second : List MapKey) :
    (∀ keys, setOperation .union first second = some keys →
      keys.Nodup ∧ ∀ key, key ∈ keys ↔ key ∈ first ∨ key ∈ second) ∧
    (∀ keys, setOperation .intersect first second = some keys →
      keys.Nodup ∧ ∀ key, key ∈ keys ↔ key ∈ first ∧ key ∈ second) ∧
    (∀ keys, setOperation .sub first second = some keys →
      keys.Nodup ∧ ∀ key, key ∈ keys ↔ key ∈ first ∧ key ∉ second) := by
  refine ⟨fun keys built => ?_, fun keys built => ?_, fun keys built => ?_⟩ <;>
    simp only [setOperation, Option.some.injEq] at built <;> subst built <;>
    refine ⟨(setOf_spec _).1, fun key => ?_⟩ <;> rw [(setOf_spec _).2] <;> simp

/-- Float items compare by IEEE value inside composite values (owner decision 1). -/
theorem float_items_ieee (left right : FloatBits) :
    valueEqual same (Declaration := Declaration) (Identity := Identity) (Source := Source)
      (.list [.scalar (.float left)]) (.list [.scalar (.float right)]) = some (floatValueEq left right) := by
  simp only [valueEqual, itemsEqual, Value.eraseNominal, scalarsEqual, scalarEqual, Option.bind_some]
  cases floatValueEq left right <;> rfl

/-- Two variants are equal when they have the same declaration and variant
name and equal payloads, compared with the declarations' own equality. -/
theorem variant_equality [DecidableEq Declaration] (identity : Identity → Identity → Bool) (source : Source → Source → Bool)
    (declaration other : Declaration) (variant otherVariant : String) (fields others : List (Value Declaration Identity Source)) :
    valueEqual ⟨fun first second => decide (first = second), identity, source⟩ (.tagged declaration variant fields)
        (.tagged other otherVariant others) =
      if declaration = other ∧ variant = otherVariant then
        itemsEqual ⟨fun first second => decide (first = second), identity, source⟩ fields others
      else some false := by
  by_cases sameDeclaration : declaration = other <;> by_cases sameVariant : variant = otherVariant <;>
    simp [valueEqual, Value.eraseNominal, sameDeclaration, sameVariant]

/-- Two handles of one family compare by runtime identity. -/
theorem handle_equality (same : Sameness Declaration Identity Source) (handle other : Value Declaration Identity Source)
    (family : Nat) (identity identity' : Identity) (held : handleOf handle = some (family, identity))
    (found : handleOf other = some (family, identity')) :
    valueEqual same handle other = some (same.identity identity identity') := by
  cases handle <;> simp [handleOf] at held <;> cases other <;> simp [handleOf] at found <;>
    obtain ⟨rfl, rfl⟩ := held <;> obtain ⟨_, rfl⟩ := found <;> simp_all [valueEqual, handleEqual, handleOf, Value.eraseNominal]

/-- The `structural_equality_and_membership` fragment root (SPEC 4.5, 6.2,
owner decision 1): `==`/`!=` are structural equality and `in`/`not in` the
membership same decision on every operand pair; Floats compare by IEEE value inside
lists; a non-`null` value is unequal to `null`; record and map equality read
the other operand only by label or key; a map key is present whatever its
value; two sets with the same elements are equal however they were built; variants are equal when their declaration,
name and payload are; two errors are equal when every field of their data is,
their payloads and causes structurally, so a Float in a payload compares by IEEE
value; callables are equal when they have one kind and one runtime identity,
and handles of one family when they have one runtime identity; process
statuses are equal when every field is, each segment's process by runtime
identity; and `|`, `&`,
`-` give distinct elements of the union, intersection and difference. -/
theorem structural_equality_and_membership [DecidableEq Declaration] (context : Context Declaration Identity Source) :
    (∀ left right : Value Declaration Identity Source,
      binary context .eq left right = decided (valueEqual context.sameness left right) ∧
        binary context .ne left right = decided ((valueEqual context.sameness left right).map not)) ∧
    (∀ needle container : Value Declaration Identity Source,
      binary context .member needle container = decided (membership context.sameness needle container) ∧
        binary context .notMember needle container = decided ((membership context.sameness needle container).map not)) ∧
    (∀ left right : FloatBits,
      valueEqual context.sameness (Declaration := Declaration) (Identity := Identity) (Source := Source)
        (.list [.scalar (.float left)]) (.list [.scalar (.float right)]) = some (floatValueEq left right)) ∧
    (∀ value : Value Declaration Identity Source, isNull value = false → valueEqual context.sameness value (.scalar .null) = some false) ∧
    (context.sameDeclaration = (fun first second => decide (first = second)) →
      ∀ (declaration other : Declaration) variant otherVariant (fields others : List (Value Declaration Identity Source)),
        valueEqual context.sameness (.tagged declaration variant fields) (.tagged other otherVariant others) =
          if declaration = other ∧ variant = otherVariant then itemsEqual context.sameness fields others
          else some false) ∧
    (∀ fields others others' : List (String × Value Declaration Identity Source),
      others.length = others'.length → (∀ label, others.lookup label = others'.lookup label) →
        valueEqual context.sameness (.record fields) (.record others) = valueEqual context.sameness (.record fields) (.record others')) ∧
    (∀ entries others others' : List (MapKey × Value Declaration Identity Source),
      others.length = others'.length → (∀ key, others.lookup key = others'.lookup key) →
        valueEqual context.sameness (.map entries) (.map others) = valueEqual context.sameness (.map entries) (.map others')) ∧
    (∀ (needle : Value Declaration Identity Source) key entries, mapKeyOf needle = some key →
      membership context.sameness needle (.map entries) = some (entries.any fun entry => entry.1 == key)) ∧
    (∀ first second : List MapKey, (∀ key, key ∈ first ↔ key ∈ second) →
      valueEqual context.sameness (Declaration := Declaration) (Identity := Identity) (Source := Source)
        (.set (setOf first)) (.set (setOf second)) = some true) ∧
    (∀ first second : List MapKey,
      (∀ keys, setOperation .union first second = some keys →
        keys.Nodup ∧ ∀ key, key ∈ keys ↔ key ∈ first ∨ key ∈ second) ∧
      (∀ keys, setOperation .intersect first second = some keys →
        keys.Nodup ∧ ∀ key, key ∈ keys ↔ key ∈ first ∧ key ∈ second) ∧
      (∀ keys, setOperation .sub first second = some keys →
        keys.Nodup ∧ ∀ key, key ∈ keys ↔ key ∈ first ∧ key ∉ second)) ∧
    (∀ left right : ErrorData Declaration Identity Source,
      valueEqual context.sameness (.error left) (.error right) = errorEqual context.sameness left right) ∧
    (∀ (left right : ErrorData Declaration Identity Source), left.payload.length = right.payload.length →
      context.sameDeclaration left.family right.family = true → left.variant = right.variant → left.kind = right.kind →
      left.message = right.message → left.facets = right.facets → spanEqual context.sameness left.span right.span = true →
      contextsEqual context.sameness left.contexts right.contexts = true → left.abort = right.abort →
      errorEqual context.sameness left right =
        (fieldsEqual context.sameness left.payload right.payload).bind fun equal =>
          if equal then causeEqual context.sameness left.cause right.cause else some false) ∧
    (∀ (reference other : Reference Identity .callable) kind kind',
      valueEqual context.sameness (.callable reference kind) (.callable other kind') =
        some (kind == kind' && context.sameIdentity reference.identity other.identity)) ∧
    (∀ (handle other : Value Declaration Identity Source) family identity identity',
      handleOf handle = some (family, identity) → handleOf other = some (family, identity') →
        valueEqual context.sameness handle other = some (context.sameIdentity identity identity')) ∧
    (∀ status other : ProcessStatus Identity,
      valueEqual context.sameness (Declaration := Declaration) (Source := Source) (.status status) (.status other) =
        some (statusEqual context.sameness status other)) :=
  ⟨equality_is_structural context, membership_is_decision context, float_items_ieee context.sameness,
    valueEqual_null context.sameness,
    fun decides declaration other variant otherVariant fields others => by
      simp only [Context.sameness, decides]
      exact variant_equality _ _ declaration other variant otherVariant fields others,
    record_order_free context.sameness,
    map_order_free context.sameness, map_key_presence context.sameness,
    sets_with_same_elements_equal context.sameness, set_operations, fun _ _ => by simp [valueEqual, Value.eraseNominal],
    fun ⟨_, _, _, _, _, _, _, _, _, _⟩ ⟨_, _, _, _, _, _, _, _, _, _⟩ sized family variant kind message facets span contexts
      abort => by
      simp only at sized family variant kind message facets span contexts abort
      subst variant kind message facets abort
      simp only [Context.sameness] at span contexts
      simp [errorEqual, Context.sameness, sized, family, span, contexts],
    fun _ _ _ _ => by simp [valueEqual, Value.eraseNominal, Context.sameness], fun handle other family identity identity' held found =>
      handle_equality context.sameness handle other family identity identity' held found,
    fun _ _ => by simp [valueEqual, Value.eraseNominal]⟩

end Laws

namespace Witness

open Xsh.Proofs.Core.Env.Witness
open Xsh.Proofs.Core.EvalStatements.Witness

def name (index : Nat) (bounded : index < 4294967296) : Xsh.Syntax.RawName := ⟨⟨index, bounded⟩⟩

def xsName : Xsh.Syntax.RawName := name 60 (by decide)
def ysName : Xsh.Syntax.RawName := name 61 (by decide)
def nansName : Xsh.Syntax.RawName := name 62 (by decide)
def nanName : Xsh.Syntax.RawName := name 63 (by decide)
def optName : Xsh.Syntax.RawName := name 64 (by decide)
def mapName : Xsh.Syntax.RawName := name 65 (by decide)
def sName : Xsh.Syntax.RawName := name 66 (by decide)
def tName : Xsh.Syntax.RawName := name 67 (by decide)
def okName : Xsh.Syntax.RawName := name 68 (by decide)
def errName : Xsh.Syntax.RawName := name 69 (by decide)
def recName : Xsh.Syntax.RawName := name 70 (by decide)
def uName : Xsh.Syntax.RawName := name 71 (by decide)
def pathName : Xsh.Syntax.RawName := name 72 (by decide)
def subPathName : Xsh.Syntax.RawName := name 73 (by decide)
def oddPathName : Xsh.Syntax.RawName := name 74 (by decide)

def pathValue (bytes : List UInt8) : WitnessValue :=
  match NativePath.fromBytes? bytes with
  | some path => .scalar (.path path)
  | none => .scalar .null

def bits (value : Nat) (bounded : value < signed64Width) : WitnessValue := .scalar (.float ⟨value, bounded⟩)
def one : WitnessValue := bits 4607182418800017408 (by decide)
def positiveZero : WitnessValue := bits 0 (by decide)
def negativeZero : WitnessValue := bits 9223372036854775808 (by decide)
def nan : WitnessValue := bits 9221120237041090560 (by decide)

def key (value : Int) : MapKey := match validateInt? value with
  | some valid => .integer valid
  | none => .string ""

def int (value : Int) : WitnessValue := .scalar (match validateInt? value with
  | some valid => .integer valid
  | none => .null)

/-- `let s = set.from([3, 1, 2])`, `let t = set.from([2, 4])`,
`set.from([2, 3, 1])`, the record `{b: 1, a: 2}` and a `Map[Str, Int?]`
holding `x` with a `null` value. -/
def locals : Locals Unit Unit Unit :=
  [(xsName, .list [one, negativeZero]), (ysName, .list [one, positiveZero]), (nansName, .list [nan]),
    (nanName, nan), (optName, int 3), (mapName, .map [(.string "x", .scalar .null)]),
    (sName, .set (setOf [key 3, key 1, key 2])), (tName, .set (setOf [key 2, key 4])),
    (uName, .set (setOf [key 2, key 3, key 1])),
    (okName, .ok (int 1)), (errName, .err (.scalar (.string "e"))),
    (recName, .record [("b", int 1), ("a", int 2)]),
    (pathName, pathValue "usr/share/xsh".toUTF8.toList), (subPathName, pathValue "usr/share".toUTF8.toList),
    (oddPathName, pathValue [0xFF, 0x73])]

def readBool : Result Unit Unit Unit WitnessValue → Option Bool
  | .value (.scalar (.boolean value)) => some value
  | _ => none

def readSet : Result Unit Unit Unit WitnessValue → Option (List MapKey)
  | .value (.set keys) => some keys
  | _ => none

def ask (operator : Xsh.Syntax.BinaryOp) (left right : Xsh.Syntax.Expr) : Option Bool :=
  readBool (run locals (.binary operator left right))

def str (text : String) : Xsh.Syntax.Expr := .literal (.string text)
def ident (target : Xsh.Syntax.RawName) : Xsh.Syntax.Expr := .ident target

/-- The verification binary prints `true false false false true` for
`[1.0, -0.0] == [1.0, 0.0]`, `[nan] == [nan]`, `nan in [nan]`, an `Int?`
holding `3` compared with `null`, and `{b: 1, a: 2} == {a: 2, b: 1}`. -/
theorem structural_equality_witness :
    ask .eq (ident xsName) (ident ysName) = some true ∧
    ask .eq (ident nansName) (ident nansName) = some false ∧
    ask .member (ident nanName) (ident nansName) = some false ∧
    ask .eq (ident optName) (.literal .null) = some false ∧
    ask .eq (.record [.named bName (.literal (.integer "1")), .named aName (.literal (.integer "2"))])
      (.record [.named aName (.literal (.integer "2")), .named bName (.literal (.integer "1"))]) = some true := by
  decide +kernel

/-- The verification binary prints `true true true true true true false` for
`"ell" in "hello"`, `"" in ""`, `"x" in m` and `"y" not in m` where `m` holds
`x` with a `null` value, `"a" in {b: 1, a: 2}`, `2 in s` and `7 in s`. -/
theorem membership_witness :
    ask .member (str "ell") (str "hello") = some true ∧
    ask .member (str "") (str "") = some true ∧
    ask .member (str "x") (ident mapName) = some true ∧
    ask .notMember (str "y") (ident mapName) = some true ∧
    ask .member (str "a") (ident recName) = some true ∧
    ask .member (.literal (.integer "2")) (ident sName) = some true ∧
    ask .member (.literal (.integer "7")) (ident sName) = some false := by
  decide +kernel

/-- The verification binary prints `[1,2,3,4] [2] [1,3] true` for `s | t`,
`s & t`, `s - t` and `s == set.from([2, 3, 1])`. -/
theorem set_operator_witness :
    readSet (run locals (.binary .union (ident sName) (ident tName))) = some [key 1, key 2, key 3, key 4] ∧
    readSet (run locals (.binary .intersect (ident sName) (ident tName))) = some [key 2] ∧
    readSet (run locals (.binary .sub (ident sName) (ident tName))) = some [key 1, key 3] ∧
    ask .eq (ident sName) (ident uName) = some true := by
  decide +kernel

/-- The verification binary prints `false true true false` for `Ok(1) ==
Err("e")`, a list of one `Ok` compared with itself, `[[1], [2]] == [[1], [2]]`
and `{x: [1]} == {x: [2]}`. -/
theorem nested_equality_witness :
    ask .eq (ident okName) (ident errName) = some false ∧
    ask .eq (.list [ident okName]) (.list [ident okName]) = some true ∧
    ask .eq (.list [.list [.literal (.integer "1")], .list [.literal (.integer "2")]])
      (.list [.list [.literal (.integer "1")], .list [.literal (.integer "2")]]) = some true ∧
    ask .eq (.record [.named aName (.list [.literal (.integer "1")])])
      (.record [.named aName (.list [.literal (.integer "2")])]) = some false := by
  decide +kernel

/-- As the verification binary prints `true true true false true` for
`"share" in p`, `"/xs" in p`, `p"usr/share" in p`, `"bin" in p` and `"" in p`
with `p = p"usr/share/xsh"`, and `true true false` for `"s"`, `"\u{fffd}s"`
and `"ts"` in a path whose bytes are `ff 73`, whose display text is lossy. -/
theorem path_membership_witness :
    ask .member (str "share") (ident pathName) = some true ∧
    ask .member (str "/xs") (ident pathName) = some true ∧
    ask .member (ident subPathName) (ident pathName) = some true ∧
    ask .member (str "bin") (ident pathName) = some false ∧
    ask .member (str "") (ident pathName) = some true ∧
    ask .member (str "s") (ident oddPathName) = some true ∧
    ask .member (str "�s") (ident oddPathName) = some true ∧
    ask .member (str "ts") (ident oddPathName) = some false := by
  decide +kernel

/-- Raw-bit equality, the representation equality codecs keep, calls `[nan]`
equal to itself; the language comparison does not. -/
theorem raw_bit_equality_rejected :
    decide (([⟨9221120237041090560, by decide⟩] : List FloatBits) = [⟨9221120237041090560, by decide⟩]) = true ∧
      ask .eq (ident nansName) (ident nansName) = some false := by
  decide +kernel

/-- Comparing record fields in written order calls `{b: 1, a: 2}` and
`{a: 2, b: 1}` unequal; the language comparison calls them equal. -/
theorem positional_record_equality_rejected :
    decide ((["b", "a"] : List String) = ["a", "b"]) = false ∧
      ask .eq (ident recName) (.record [.named aName (.literal (.integer "2")), .named bName (.literal (.integer "1"))]) =
        some true := by
  decide +kernel

/-- Membership that requires a non-`null` value misses `x` in `m`. -/
theorem value_sensitive_membership_rejected :
    (match ([(MapKey.string "x", (.scalar .null : WitnessValue))].lookup (.string "x")) with
      | some (.scalar .null) | none => false
      | some _ => true) = false ∧
      ask .member (str "x") (ident mapName) = some true := by
  decide +kernel

/-- A union that concatenates its operands repeats `2`. -/
theorem concatenating_union_rejected :
    (setOf [key 3, key 1, key 2] ++ setOf [key 2, key 4]).length = 5 ∧
      (readSet (run locals (.binary .union (ident sName) (ident tName)))).map List.length = some 4 := by
  decide +kernel

def tag (declaration : Nat) (variant : String) (payload : List (Value Nat Unit Unit)) : Value Nat Unit Unit :=
  .tagged declaration variant payload

def natInt (value : Int) : Value Nat Unit Unit := .scalar (match validateInt? value with
  | some valid => .integer valid
  | none => .null)

def sameNat : Sameness Nat Unit Unit := ⟨fun first second => decide (first = second), fun _ _ => true, fun _ _ => true⟩

/-- Variants compare by declaration, name and payload: `Fault(1)` equals
`Fault(1)` and differs from `Fault(2)`, from `Info`, and from a `Fault(1)`
of another enum. -/
theorem variant_witness :
    valueEqual sameNat (tag 1 "Fault" [natInt 1]) (tag 1 "Fault" [natInt 1]) = some true ∧
    valueEqual sameNat (tag 1 "Fault" [natInt 1]) (tag 1 "Fault" [natInt 2]) = some false ∧
    valueEqual sameNat (tag 1 "Fault" [natInt 1]) (tag 1 "Info" []) = some false ∧
    valueEqual sameNat (tag 1 "Fault" [natInt 1]) (tag 2 "Fault" [natInt 1]) = some false := by
  decide +kernel

/-- Comparing variants by name alone calls a `Fault(1)` of another enum equal;
the model does not. -/
theorem name_only_variant_equality_rejected :
    valueEqual ⟨fun _ _ => true, fun _ _ => true, fun _ _ => true⟩ (tag 1 "Fault" [natInt 1]) (tag 2 "Fault" [natInt 1]) = some true ∧
      valueEqual sameNat (tag 1 "Fault" [natInt 1]) (tag 2 "Fault" [natInt 1]) = some false := by
  decide +kernel

def anySame : Sameness Unit Unit Unit := ⟨fun _ _ => true, fun _ _ => true, fun _ _ => true⟩

/-- `Probe.Bad(value: x)` for `error Probe = Bad(value: Float) | Other(value: Float)`. -/
def probeError (variant : String) (payload : WitnessValue) : WitnessValue :=
  .error ⟨(), variant, "Probe", variant, [("value", payload)], [], none, [], none, none⟩

/-- As the verification binary gives `true` for
`Probe.Bad(value: 0.0) == Probe.Bad(value: -0.0)`, `false` for two `Bad`s
holding NaN, and `false` for `Bad` against `Other`: a payload's Float
compares by IEEE value. -/
theorem error_payload_witness :
    valueEqual anySame (probeError "Bad" positiveZero) (probeError "Bad" negativeZero) = some true ∧
    valueEqual anySame (probeError "Bad" nan) (probeError "Bad" nan) = some false ∧
    valueEqual anySame (probeError "Bad" positiveZero) (probeError "Other" positiveZero) = some false := by
  decide +kernel

/-- Comparing payloads by their bits, as the binary did before, calls the
two zeros' errors unequal, as `0` and `2^63` differ; the model calls them
equal. -/
theorem bitwise_payload_rejected :
    ((0 : Nat) == 9223372036854775808) = false ∧
      valueEqual anySame (probeError "Bad" positiveZero) (probeError "Bad" negativeZero) = some true := by
  decide +kernel

def sameIds : Sameness Nat Nat Unit := ⟨fun first second => decide (first = second),
  fun first second => decide (first = second), fun _ _ => true⟩

def callableOf (identity : Nat) (kind : CallableKind) : Value Nat Nat Unit := .callable ⟨identity⟩ kind

/-- As the verification binary prints `true false` for `let a = f; let b =
f; a == b` and `a == g`: a callable is equal to one of the same declaration
and kind, and handles compare by identity. -/
theorem handle_witness :
    valueEqual sameIds (callableOf 1 .procedure) (callableOf 1 .procedure) = some true ∧
    valueEqual sameIds (callableOf 1 .procedure) (callableOf 2 .procedure) = some false ∧
    valueEqual sameIds (callableOf 1 .procedure) (callableOf 1 .pure) = some false ∧
    valueEqual sameIds (.processHandle ⟨3⟩) (.processHandle ⟨3⟩) = some true ∧
    valueEqual sameIds (.processHandle ⟨3⟩) (.processHandle ⟨4⟩) = some false := by
  decide +kernel

/-- Comparing callables by kind alone calls `f` and `g` equal; the model
does not. -/
theorem kind_only_callable_rejected :
    valueEqual ⟨fun first second => decide (first = second), fun _ _ => true, fun _ _ => true⟩
        (callableOf 1 .procedure) (callableOf 2 .procedure) = some true ∧
      valueEqual sameIds (callableOf 1 .procedure) (callableOf 2 .procedure) = some false := by
  decide +kernel

/-- The status of `run.status` on `target` as process `pid`, exiting with
`code`. -/
def statusOf (pid : Nat) (target : String) (code : Int) : Value Nat Nat Unit :=
  .status ⟨code == 0, .exit, validateInt? code,
    [⟨0, target.toUTF8.toList, some ⟨pid⟩, code == 0, .exit, validateInt? code, none, none⟩]⟩

/-- As the verification binary prints `false false true` for
`let a = run.status true; let b = run.status true; let c = run.status false`
and `a == b`, `a == c`, `a == a`: statuses compare field by field, the
process each segment ran by identity, so two runs differ. -/
theorem status_witness :
    valueEqual sameIds (statusOf 3 "true" 0) (statusOf 4 "true" 0) = some false ∧
    valueEqual sameIds (statusOf 3 "true" 0) (statusOf 5 "false" 1) = some false ∧
    valueEqual sameIds (statusOf 3 "true" 0) (statusOf 3 "true" 0) = some true ∧
    valueEqual sameIds (statusOf 3 "true" 0) (.scalar .null) = some false := by
  decide +kernel

/-- Comparing statuses by exit code alone calls two runs of `true` equal. -/
theorem code_only_status_rejected :
    valueEqual sameIds (statusOf 3 "true" 0) (statusOf 4 "true" 0) ≠ some true := by
  decide +kernel

end Witness

end Xsh.Proofs.Core.Equality
