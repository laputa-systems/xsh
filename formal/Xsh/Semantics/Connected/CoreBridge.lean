import Xsh.Semantics.CoreWorld
import Xsh.Semantics.Connected.Machine
import Std.Data.String.ToNat

namespace Xsh.Semantics.Connected.CoreBridge

open Xsh.Foundation
open Xsh.Values
open Xsh.Syntax.Foundation
open Xsh.Validation.Nominal
open Xsh.Semantics.Environments

inductive Declaration where
  | nominal (identity : NominalId)
  | enumeration (typeName : String)
  | validation
  deriving DecidableEq

structure CallableIdentity where
  declaration : DeclarationId
  type : RuntimeType
  deriving DecidableEq

abbrev CoreType := Xsh.Core.ValueType Declaration
abbrev CoreValue := Xsh.Core.Value Declaration CallableIdentity Unit

def label (name : NameId) : String := Nat.repr name.value

theorem label_injective {first second : NameId} (same : label first = label second) : first = second := by
  have values : first.value = second.value := Nat.repr_inj.mp same
  cases first with
  | mk firstWord =>
    cases second with
    | mk secondWord =>
      have words : firstWord = secondWord := Fin.ext values
      rw [words]

def positionLabel (position : Nat) : String := Nat.repr position

mutual
  def embedType : RuntimeType → Option CoreType
    | .unit => some (.primitive .unit)
    | .integer => some (.primitive .integer)
    | .boolean => some (.primitive .boolean)
    | .string => some (.primitive .string)
    | .error => some (.primitive .error)
    | .record row => (embedRow row).map .record
    | .result value failure => do
        some (.result (← embedType value) (← embedType failure))
    | .function arguments result => do
        some (.callable (.signature (.procedure .unknown) (← embedArguments 0 arguments) (← embedType result)))
    | .schemeFunction => some (.functionClass .procedure)
    | .nominal identity (.record row) => (embedRow row).map (.nominal (.nominal identity))
    | .nominal _ _ => none
    | .rowEmpty | .rowField _ _ _ | .argumentsEmpty | .argument _ _ => none

  def embedRow : RuntimeType → Option (Xsh.Core.RecordType Declaration)
    | .rowEmpty => some .rowEmpty
    | .rowField name value tail => do
        some (.rowField (label name) (← embedType value) (← embedRow tail))
    | _ => none

  def embedArguments (position : Nat) :
      RuntimeType → Option (Xsh.Core.TypeVariableFreeTerm 0 String Declaration .arguments)
    | .argumentsEmpty => some .argumentsEmpty
    | .argument value tail => do
        some (.parameter .required (positionLabel position) (← embedType value)
          (← embedArguments (position + 1) tail))
    | _ => none
end

def embedSignature (arguments result : RuntimeType) : Option (Xsh.Core.SignatureType Declaration) := do
  some (.signature (.procedure .unknown) (← embedArguments 0 arguments) (← embedType result))

def validationErrorData : Xsh.Core.ErrorData Declaration CallableIdentity Unit where
  family := .validation
  variant := "validation"
  kind := "validation"
  message := ""
  payload := []
  facets := []
  span := none
  contexts := []
  cause := none
  abort := none

mutual
  def embedComposite : CompositeValue → CoreValue
    | .null => .scalar .null
    | .boolean value => .scalar (.boolean value)
    | .integer value => .scalar (.integer value)
    | .float bits => .scalar (.float bits)
    | .duration value => .scalar (.duration value)
    | .string value => .scalar (.string value)
    | .bytes value => .scalar (.bytes value)
    | .list values => .list (embedComposites values)
    | .record fields => .record (embedCompositeFields fields)
    | .tagged typeName variant fields => .tagged (.enumeration typeName) variant (embedComposites fields)
    | .result true value => .ok (.scalar (.integer value))
    | .result false value => .err (.scalar (.integer value))

  def embedComposites : List CompositeValue → List CoreValue
    | [] => []
    | value :: rest => embedComposite value :: embedComposites rest

  def embedCompositeFields : List (String × CompositeValue) → List (String × CoreValue)
    | [] => []
    | (name, value) :: rest => (name, embedComposite value) :: embedCompositeFields rest
end

mutual
  def embedValue : Value → CoreValue
    | .unit => .scalar .unit
    | .scalar value => embedComposite value
    | .validationError => .error validationErrorData
    | .record fields => .record (embedFields fields)
    | .ok value _ => .ok (embedValue value)
    | .error value _ => .err (embedValue value)
    | .function declaration arguments result =>
        .callable ⟨⟨declaration, .function arguments result⟩⟩ .procedure
    | .schemeFunction declaration _ => .callable ⟨⟨declaration, .schemeFunction⟩⟩ .procedure
    | .instantiatedFunction declaration arguments result _ _ =>
        .callable ⟨⟨declaration, .function arguments result⟩⟩ .procedure
    | .nominal identity _ value => .nominal (.nominal identity) (embedValue value)

  def embedFields : List (NameId × Value) → List (String × CoreValue)
    | [] => []
    | (name, value) :: rest => (label name, embedValue value) :: embedFields rest
end

def callableContract : RuntimeType → Option (Xsh.Core.SignatureType Declaration)
  | .function arguments result => embedSignature arguments result
  | _ => none

def membershipWorld : Xsh.Core.MembershipWorld Declaration CallableIdentity Unit where
  entryFields := fun _ => []
  stream := fun _ _ => False
  callable := fun reference signature => callableContract reference.identity.type = some signature
  scheme := fun _ _ => False
  generic := fun _ _ => False

def world : Xsh.Semantics.CoreWorld.World Declaration CallableIdentity Unit where
  membership := membershipWorld
  references := fun _ => True

noncomputable def decisions : Xsh.Semantics.CoreWorld.Decisions world where
  contracts := {
    stream := fun _ _ => isFalse (fun ⟨_, held, _⟩ => held)
    callable := fun _ _ => Classical.propDecidable _
    scheme := fun _ _ => instDecidableFalse
    generic := fun _ _ => instDecidableFalse
  }
  references := fun _ => instDecidableTrue

def rowEntries : RuntimeType → List (NameId × RuntimeType)
  | .rowField name value tail => (name, value) :: rowEntries tail
  | _ => []

theorem embedFields_lookup (fields : List (NameId × Value)) (name : NameId) :
    (embedFields fields).lookup (label name) = (fields.lookup name).map embedValue := by
  induction fields with
  | nil => simp [embedFields]
  | cons head rest ih =>
    obtain ⟨other, value⟩ := head
    by_cases same : name = other
    · subst same
      simp [embedFields, List.lookup]
    · have different : (label name == label other) = false := by
        simp only [beq_eq_false_iff_ne]
        intro equal
        exact same (label_injective equal)
      have differentNames : (name == other) = false := by simpa using same
      simp [embedFields, List.lookup, different, differentNames, ih]

theorem rowFieldType_fold (name : NameId) (entries : List (NameId × RuntimeType)) :
    rowFieldType name (entries.foldr (fun (field : NameId × RuntimeType) row =>
      .rowField field.1 field.2 row) .rowEmpty) = entries.lookup name := by
  induction entries with
  | nil => simp [rowFieldType]
  | cons head rest ih =>
    obtain ⟨other, type⟩ := head
    by_cases same : name = other
    · subst same
      simp [rowFieldType, List.lookup]
    · have differentNames : (name == other) = false := by simpa using same
      simp [rowFieldType, List.lookup, same, differentNames, ih]

theorem rowEntries_fold (entries : List (NameId × RuntimeType)) :
    rowEntries (entries.foldr (fun (field : NameId × RuntimeType) row =>
      .rowField field.1 field.2 row) .rowEmpty) = entries := by
  induction entries with
  | nil => simp [rowEntries]
  | cons head rest ih => simp [rowEntries, ih]

theorem insertField_mem (field entry : NameId × RuntimeType) (entries : List (NameId × RuntimeType)) :
    entry ∈ insertField field entries ↔ entry = field ∨ entry ∈ entries := by
  induction entries with
  | nil => simp [insertField]
  | cons head rest ih =>
    simp only [insertField]
    split
    · simp
    · simp only [List.mem_cons, ih]
      constructor
      · rintro (h | h | h) <;> simp [h]
      · rintro (h | h | h) <;> simp [h]

theorem canonicalFields_mem (entry : NameId × RuntimeType) (entries : List (NameId × RuntimeType)) :
    entry ∈ canonicalFields entries ↔ entry ∈ entries := by
  induction entries with
  | nil => simp [canonicalFields]
  | cons head rest ih => simp [canonicalFields, insertField_mem, ih]

theorem recordTypes_entry {fields : List (NameId × Value)} {types : List (NameId × RuntimeType)}
    (typed : recordTypes fields = some types) {name : NameId} {type : RuntimeType}
    (present : (name, type) ∈ types) :
    ∃ value, (name, value) ∈ fields ∧ valueType value = some type := by
  induction fields generalizing types with
  | nil =>
    simp [recordTypes] at typed
    subst typed
    simp at present
  | cons head rest ih =>
    obtain ⟨other, value⟩ := head
    simp only [recordTypes, Option.bind_eq_bind] at typed
    cases headType : valueType value with
    | none => simp [headType] at typed
    | some headTy =>
      cases restTypes : recordTypes rest with
      | none => simp [headType, restTypes] at typed
      | some others =>
        simp [headType, restTypes] at typed
        subst typed
        simp only [List.mem_cons, Prod.mk.injEq] at present
        rcases present with ⟨sameName, sameType⟩ | later
        · subst sameName
          subst sameType
          exact ⟨value, by simp, headType⟩
        · obtain ⟨found, member, foundType⟩ := ih restTypes later
          exact ⟨found, List.mem_cons_of_mem _ member, foundType⟩

theorem recordTypes_keys {fields : List (NameId × Value)} {types : List (NameId × RuntimeType)}
    (typed : recordTypes fields = some types) : types.map Prod.fst = fields.map Prod.fst := by
  induction fields generalizing types with
  | nil =>
    simp [recordTypes] at typed
    subst typed
    rfl
  | cons head rest ih =>
    obtain ⟨other, value⟩ := head
    simp only [recordTypes, Option.bind_eq_bind] at typed
    cases headType : valueType value with
    | none => simp [headType] at typed
    | some headTy =>
      cases restTypes : recordTypes rest with
      | none => simp [headType, restTypes] at typed
      | some others =>
        simp [headType, restTypes] at typed
        subst typed
        simp [ih restTypes]

theorem lookup_of_mem_nodup {α β : Type} [DecidableEq α] {entries : List (α × β)} {key : α} {item : β}
    (unique : (entries.map Prod.fst).Nodup) (present : (key, item) ∈ entries) :
    entries.lookup key = some item := by
  induction entries with
  | nil => simp at present
  | cons head rest ih =>
    obtain ⟨other, value⟩ := head
    simp only [List.map_cons, List.nodup_cons] at unique
    simp only [List.mem_cons, Prod.mk.injEq] at present
    rcases present with ⟨sameKey, sameItem⟩ | later
    · subst sameKey
      subst sameItem
      simp [List.lookup]
    · have different : key ≠ other := by
        intro same
        subst same
        exact unique.1 (List.mem_map.mpr ⟨(key, item), later, rfl⟩)
      have differentKeys : (key == other) = false := by simpa using different
      simp [List.lookup, differentKeys, ih unique.2 later]

theorem lookup_some_mem {α β : Type} [DecidableEq α] {entries : List (α × β)} {key : α} {item : β}
    (found : entries.lookup key = some item) : (key, item) ∈ entries := by
  induction entries with
  | nil => simp [List.lookup] at found
  | cons head rest ih =>
    obtain ⟨other, value⟩ := head
    by_cases same : key = other
    · subst same
      simp [List.lookup] at found
      subst found
      simp
    · have differentKeys : (key == other) = false := by simpa using same
      simp [List.lookup, differentKeys] at found
      exact List.mem_cons_of_mem _ (ih found)

theorem typeFits_refl (type : RuntimeType) : typeFits type type = true := by
  cases type <;> simp [typeFits]

theorem rowFits_entries {expected actual : RuntimeType} (fits : rowFits expected actual = true) :
    ∀ entry ∈ rowEntries expected, ∃ found, rowFieldType entry.1 actual = some found ∧
      typeFits entry.2 found = true := by
  induction expected with
  | rowField name value tail _ tailIh =>
    simp only [rowFits] at fits
    cases lookup : rowFieldType name actual with
    | none => simp [lookup] at fits
    | some found =>
      simp only [lookup, Bool.and_eq_true] at fits
      intro entry present
      simp only [rowEntries, List.mem_cons] at present
      rcases present with same | later
      · subst same
        exact ⟨found, lookup, fits.1⟩
      · exact tailIh fits.2 entry later
  | _ =>
    intro entry present
    simp [rowEntries] at present


theorem typeFits_cases {expected actual : RuntimeType} (fits : typeFits expected actual = true) :
    expected = actual ∨
      (∃ expectedRow actualRow, expected = .record expectedRow ∧ actual = .record actualRow ∧
        rowFits expectedRow actualRow = true) ∨
      (∃ expectedValue expectedError actualValue actualError,
        expected = .result expectedValue expectedError ∧ actual = .result actualValue actualError ∧
        typeFits expectedValue actualValue = true ∧ typeFits expectedError actualError = true) ∨
      (∃ identity expectedBase actualBase, expected = .nominal identity expectedBase ∧
        actual = .nominal identity actualBase ∧ typeFits expectedBase actualBase = true) := by
  by_cases same : expected = actual
  · exact Or.inl same
  · right
    unfold typeFits at fits
    simp only [same, if_false] at fits
    split at fits
    · exact Or.inl ⟨_, _, rfl, rfl, fits⟩
    · simp only [Bool.and_eq_true] at fits
      exact Or.inr (Or.inl ⟨_, _, _, _, rfl, rfl, fits.1, fits.2⟩)
    · simp only [Bool.and_eq_true, decide_eq_true_eq] at fits
      obtain ⟨sameId, baseFits⟩ := fits
      subst sameId
      exact Or.inr (Or.inr ⟨_, _, _, rfl, rfl, baseFits⟩)
    · cases fits

theorem rowFieldType_mem {name : NameId} {row found : RuntimeType}
    (present : rowFieldType name row = some found) : (name, found) ∈ rowEntries row := by
  induction row with
  | rowField other value tail _ tailIh =>
    simp only [rowFieldType] at present
    by_cases same : name = other
    · subst same
      simp at present
      subst present
      simp [rowEntries]
    · simp only [same, if_false] at present
      exact List.mem_cons_of_mem _ (tailIh present)
  | _ => simp [rowFieldType] at present

theorem fits_transitive : ∀ first : RuntimeType,
    (∀ second third, typeFits first second = true → typeFits second third = true →
      typeFits first third = true) ∧
    (∀ second third, rowFits first second = true → rowFits second third = true →
      rowFits first third = true) := by
  intro first
  induction first with
  | record row rowIh =>
    refine ⟨?_, ?_⟩
    · intro second third firstFits secondFits
      rcases typeFits_cases firstFits with same | ⟨expectedRow, actualRow, h1, h2, rows⟩ | ⟨_, _, _, _, h1, _⟩ | ⟨_, _, _, h1, _⟩
      · subst same; exact secondFits
      · cases h1
        subst h2
        rcases typeFits_cases secondFits with same | ⟨middleRow, lastRow, h3, h4, nextRows⟩ | ⟨_, _, _, _, h3, _⟩ | ⟨_, _, _, h3, _⟩
        · subst same; rw [typeFits]; simp [rows]
        · cases h3
          subst h4
          rw [typeFits]
          simp [rowIh.2 _ _ rows nextRows]
        · cases h3
        · cases h3
      · cases h1
      · cases h1
    · intro second third firstFits
      simp [rowFits] at firstFits
  | result value failure valueIh failureIh =>
    refine ⟨?_, ?_⟩
    · intro second third firstFits secondFits
      rcases typeFits_cases firstFits with same | ⟨_, _, h1, _⟩ | ⟨ev, ee, av, ae, h1, h2, fitsValue, fitsError⟩ | ⟨_, _, _, h1, _⟩
      · subst same; exact secondFits
      · cases h1
      · cases h1
        subst h2
        rcases typeFits_cases secondFits with same | ⟨_, _, h3, _⟩ | ⟨mv, me, lv, le, h3, h4, nextValue, nextError⟩ | ⟨_, _, _, h3, _⟩
        · subst same; rw [typeFits]; simp [fitsValue, fitsError]
        · cases h3
        · cases h3
          subst h4
          rw [typeFits]
          simp [valueIh.1 _ _ fitsValue nextValue, failureIh.1 _ _ fitsError nextError]
        · cases h3
      · cases h1
    · intro second third firstFits
      simp [rowFits] at firstFits
  | nominal identity base baseIh =>
    refine ⟨?_, ?_⟩
    · intro second third firstFits secondFits
      rcases typeFits_cases firstFits with same | ⟨_, _, h1, _⟩ | ⟨_, _, _, _, h1, _⟩ | ⟨i, eb, ab, h1, h2, fitsBase⟩
      · subst same; exact secondFits
      · cases h1
      · cases h1
      · cases h1
        subst h2
        rcases typeFits_cases secondFits with same | ⟨_, _, h3, _⟩ | ⟨_, _, _, _, h3, _⟩ | ⟨j, mb, lb, h3, h4, nextBase⟩
        · subst same; rw [typeFits]; simp [fitsBase]
        · cases h3
        · cases h3
        · cases h3
          subst h4
          rw [typeFits]
          simp [baseIh.1 _ _ fitsBase nextBase]
    · intro second third firstFits
      simp [rowFits] at firstFits
  | rowField name value tail valueIh tailIh =>
    refine ⟨?_, ?_⟩
    · intro second third firstFits secondFits
      rcases typeFits_cases firstFits with same | ⟨_, _, h1, _⟩ | ⟨_, _, _, _, h1, _⟩ | ⟨_, _, _, h1, _⟩
      · subst same; exact secondFits
      · cases h1
      · cases h1
      · cases h1
    · intro second third firstFits secondFits
      simp only [rowFits] at firstFits
      cases found : rowFieldType name second with
      | none => simp [found] at firstFits
      | some middle =>
        simp only [found, Bool.and_eq_true] at firstFits
        obtain ⟨last, lastFound, lastFits⟩ :=
          rowFits_entries secondFits (name, middle) (rowFieldType_mem found)
        simp only [rowFits, lastFound, Bool.and_eq_true]
        exact ⟨valueIh.1 _ _ firstFits.1 lastFits, tailIh.2 _ _ firstFits.2 secondFits⟩
  | rowEmpty =>
    refine ⟨?_, ?_⟩
    · intro second third firstFits secondFits
      rcases typeFits_cases firstFits with same | ⟨_, _, h1, _⟩ | ⟨_, _, _, _, h1, _⟩ | ⟨_, _, _, h1, _⟩
      · subst same; exact secondFits
      all_goals cases h1
    · intro second third _ _
      simp [rowFits]
  | _ =>
    refine ⟨?_, ?_⟩
    · intro second third firstFits secondFits
      rcases typeFits_cases firstFits with same | ⟨_, _, h1, _⟩ | ⟨_, _, _, _, h1, _⟩ | ⟨_, _, _, h1, _⟩
      · subst same; exact secondFits
      all_goals cases h1
    · intro second third firstFits
      simp [rowFits] at firstFits


open Xsh.Core in
theorem row_member (fields : List (NameId × Value)) : ∀ (expected : RuntimeType) (core : RecordType Declaration),
    embedRow expected = some core →
    (∀ entry ∈ rowEntries expected, ∀ coreEntry, embedType entry.2 = some coreEntry →
      ∃ found, fields.lookup entry.1 = some found ∧
        Membership.value membershipWorld coreEntry (embedValue found)) →
    Membership.row membershipWorld core (embedFields fields)
  | .rowEmpty, core, embedded, _ => by
    simp [embedRow] at embedded
    subst embedded
    simp [Membership.row]
  | .rowField name value tail, core, embedded, covered => by
    simp only [embedRow, Option.bind_eq_bind] at embedded
    cases valueEmbedded : embedType value with
    | none => simp [valueEmbedded] at embedded
    | some coreValue =>
      cases tailEmbedded : embedRow tail with
      | none => simp [valueEmbedded, tailEmbedded] at embedded
      | some coreTail =>
        simp [valueEmbedded, tailEmbedded] at embedded
        subst embedded
        obtain ⟨found, lookup, member⟩ := covered (name, value) (by simp [rowEntries]) coreValue valueEmbedded
        simp only [Membership.row, embedFields_lookup, lookup, Option.map_some]
        refine ⟨member, row_member fields tail coreTail tailEmbedded ?_⟩
        intro entry present coreEntry entryEmbedded
        exact covered entry (by simp [rowEntries, present]) coreEntry entryEmbedded
  | .unit, _, embedded, _ | .integer, _, embedded, _ | .boolean, _, embedded, _ | .string, _, embedded, _
  | .error, _, embedded, _ | .record _, _, embedded, _ | .argumentsEmpty, _, embedded, _
  | .argument _ _, _, embedded, _ | .result _ _, _, embedded, _ | .function _ _, _, embedded, _
  | .schemeFunction, _, embedded, _ | .nominal _ _, _, embedded, _ => by
    simp [embedRow] at embedded

theorem record_typed {fields : List (NameId × Value)} {actual : RuntimeType}
    (typed : valueType (.record fields) = some actual) :
    ∃ types, (fields.map Prod.fst).Nodup ∧ recordTypes fields = some types ∧
      actual = .record ((canonicalFields types).foldr
        (fun (field : NameId × RuntimeType) row => .rowField field.1 field.2 row) .rowEmpty) := by
  simp only [valueType] at typed
  split at typed
  · rename_i unique
    cases types : recordTypes fields with
    | none => simp [types] at typed
    | some found =>
      simp only [types, Option.map_some, Option.some.injEq] at typed
      exact ⟨found, unique, rfl, typed.symm⟩
  · cases typed

open Xsh.Core in
theorem member_of_fits : ∀ (candidate : Value) (actual expected : RuntimeType) (core : CoreType),
    valueType candidate = some actual → typeFits expected actual = true → embedType expected = some core →
    Membership.value membershipWorld core (embedValue candidate)
  | .unit, actual, expected, core, typed, fits, embedded => by
    simp only [valueType, Option.some.injEq] at typed
    subst typed
    rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨_, _, _, _, _, h, _⟩ | ⟨_, _, _, _, h, _⟩
    · subst same
      simp only [embedType, Option.some.injEq] at embedded
      subst embedded
      simp [embedValue, Membership.value, Membership.primitive, Value.eraseNominal]
    all_goals cases h
  | .validationError, actual, expected, core, typed, fits, embedded => by
    simp only [valueType, Option.some.injEq] at typed
    subst typed
    rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨_, _, _, _, _, h, _⟩ | ⟨_, _, _, _, h, _⟩
    · subst same
      simp only [embedType, Option.some.injEq] at embedded
      subst embedded
      simp [embedValue, Membership.value, Membership.primitive, Value.eraseNominal]
    all_goals cases h
  | .scalar scalar, actual, expected, core, typed, fits, embedded => by
    cases scalar <;> simp only [valueType, Option.some.injEq, reduceCtorEq] at typed <;> subst typed <;>
    rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨_, _, _, _, _, h, _⟩ | ⟨_, _, _, _, h, _⟩ <;>
    (try cases h) <;> subst same <;> simp only [embedType, Option.some.injEq] at embedded <;> subst embedded <;>
    simp [embedValue, embedComposite, Membership.value, Membership.primitive, Value.eraseNominal]
  | .function declaration arguments result, actual, expected, core, typed, fits, embedded => by
    simp only [valueType, Option.some.injEq] at typed
    subst typed
    rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨_, _, _, _, _, h, _⟩ | ⟨_, _, _, _, h, _⟩
    · subst same
      simp only [embedType, Option.bind_eq_bind] at embedded
      cases argumentsEmbedded : embedArguments 0 arguments with
      | none => simp [argumentsEmbedded] at embedded
      | some coreArguments =>
        cases resultEmbedded : embedType result with
        | none => simp [argumentsEmbedded, resultEmbedded] at embedded
        | some coreResult =>
          simp [argumentsEmbedded, resultEmbedded] at embedded
          subst embedded
          simp [embedValue, Membership.value, Membership.callable, Membership.signatureKind,
            Xsh.Core.InvocationPolicy.kind, Value.eraseNominal, membershipWorld, callableContract,
            embedSignature, argumentsEmbedded, resultEmbedded]
          try first | exact ⟨_, rfl, .refl _⟩ | exact .refl _
    all_goals cases h
  | .instantiatedFunction declaration arguments result solution typeArguments, actual, expected, core,
      typed, fits, embedded => by
    simp only [valueType, Option.some.injEq] at typed
    subst typed
    rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨_, _, _, _, _, h, _⟩ | ⟨_, _, _, _, h, _⟩
    · subst same
      simp only [embedType, Option.bind_eq_bind] at embedded
      cases argumentsEmbedded : embedArguments 0 arguments with
      | none => simp [argumentsEmbedded] at embedded
      | some coreArguments =>
        cases resultEmbedded : embedType result with
        | none => simp [argumentsEmbedded, resultEmbedded] at embedded
        | some coreResult =>
          simp [argumentsEmbedded, resultEmbedded] at embedded
          subst embedded
          simp [embedValue, Membership.value, Membership.callable, Membership.signatureKind,
            Xsh.Core.InvocationPolicy.kind, Value.eraseNominal, membershipWorld, callableContract,
            embedSignature, argumentsEmbedded, resultEmbedded]
          try first | exact ⟨_, rfl, .refl _⟩ | exact .refl _
    all_goals cases h
  | .schemeFunction declaration scheme, actual, expected, core, typed, fits, embedded => by
    simp only [valueType, Option.some.injEq] at typed
    subst typed
    rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨_, _, _, _, _, h, _⟩ | ⟨_, _, _, _, h, _⟩
    · subst same
      simp only [embedType, Option.some.injEq] at embedded
      subst embedded
      simp [embedValue, Membership.value, Value.eraseNominal]
    all_goals cases h
  | .ok inner errorType, actual, expected, core, typed, fits, embedded => by
    simp only [valueType] at typed
    cases innerTyped : valueType inner with
    | none => simp [innerTyped] at typed
    | some innerActual =>
      simp only [innerTyped, Option.map_some, Option.some.injEq] at typed
      subst typed
      rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨ev, ee, av, ae, h1, h2, valueFits, _⟩ | ⟨_, _, _, _, h, _⟩
      · subst same
        simp only [embedType, Option.bind_eq_bind] at embedded
        cases valueEmbedded : embedType innerActual with
        | none => simp [valueEmbedded] at embedded
        | some coreValue =>
          cases errorEmbedded : embedType errorType with
          | none => simp [valueEmbedded, errorEmbedded] at embedded
          | some coreError =>
            simp [valueEmbedded, errorEmbedded] at embedded
            subst embedded
            simp only [embedValue, Membership.value, Value.eraseNominal]
            exact member_of_fits inner innerActual innerActual coreValue innerTyped (typeFits_refl _) valueEmbedded
      · cases h
      · subst h1
        cases h2
        simp only [embedType, Option.bind_eq_bind] at embedded
        cases valueEmbedded : embedType ev with
        | none => simp [valueEmbedded] at embedded
        | some coreValue =>
          cases errorEmbedded : embedType ee with
          | none => simp [valueEmbedded, errorEmbedded] at embedded
          | some coreError =>
            simp [valueEmbedded, errorEmbedded] at embedded
            subst embedded
            simp only [embedValue, Membership.value, Value.eraseNominal]
            exact member_of_fits inner innerActual ev coreValue innerTyped valueFits valueEmbedded
      · cases h
  | .error inner valueType', actual, expected, core, typed, fits, embedded => by
    simp only [valueType] at typed
    cases innerTyped : valueType inner with
    | none => simp [innerTyped] at typed
    | some innerActual =>
      simp only [innerTyped, Option.map_some, Option.some.injEq] at typed
      subst typed
      rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨ev, ee, av, ae, h1, h2, _, errorFits⟩ | ⟨_, _, _, _, h, _⟩
      · subst same
        simp only [embedType, Option.bind_eq_bind] at embedded
        cases valueEmbedded : embedType valueType' with
        | none => simp [valueEmbedded] at embedded
        | some coreValue =>
          cases errorEmbedded : embedType innerActual with
          | none => simp [valueEmbedded, errorEmbedded] at embedded
          | some coreError =>
            simp [valueEmbedded, errorEmbedded] at embedded
            subst embedded
            simp only [embedValue, Membership.value, Value.eraseNominal]
            exact member_of_fits inner innerActual innerActual coreError innerTyped (typeFits_refl _) errorEmbedded
      · cases h
      · subst h1
        cases h2
        simp only [embedType, Option.bind_eq_bind] at embedded
        cases valueEmbedded : embedType ev with
        | none => simp [valueEmbedded] at embedded
        | some coreValue =>
          cases errorEmbedded : embedType ee with
          | none => simp [valueEmbedded, errorEmbedded] at embedded
          | some coreError =>
            simp [valueEmbedded, errorEmbedded] at embedded
            subst embedded
            simp only [embedValue, Membership.value, Value.eraseNominal]
            exact member_of_fits inner innerActual ee coreError innerTyped errorFits errorEmbedded
      · cases h
  | .nominal identity base inner, actual, expected, core, typed, fits, embedded => by
    simp only [valueType, Option.bind_eq_bind] at typed
    cases innerTyped : valueType inner with
    | none => simp [innerTyped] at typed
    | some innerActual =>
      cases baseFits : typeFits base innerActual with
      | false => simp [innerTyped, baseFits] at typed
      | true =>
        simp only [innerTyped, baseFits, Option.bind_some, ite_true, Option.some.injEq] at typed
        subst typed
        have reduce : ∀ expectedBase, typeFits expectedBase base = true →
            embedType (.nominal identity expectedBase) = some core →
            Membership.value membershipWorld core (embedValue (.nominal identity base inner)) := by
          intro expectedBase expectedFits expectedEmbedded
          cases expectedBase with
          | record expectedRow =>
            simp only [embedType] at expectedEmbedded
            cases rowEmbedded : embedRow expectedRow with
            | none => simp [rowEmbedded] at expectedEmbedded
            | some coreRow =>
              simp only [rowEmbedded, Option.map_some, Option.some.injEq] at expectedEmbedded
              subst expectedEmbedded
              have innerMember := member_of_fits inner innerActual (.record expectedRow) (.record coreRow)
                innerTyped ((fits_transitive _).1 _ _ expectedFits baseFits)
                (by simp [embedType, rowEmbedded])
              simp only [Membership.value] at innerMember
              simp only [embedValue, Membership.value]
              exact ⟨by trivial, innerMember⟩
          | _ => simp [embedType] at expectedEmbedded
        rcases typeFits_cases fits with same | ⟨_, _, _, h, _⟩ | ⟨_, _, _, _, _, h, _⟩ | ⟨i, eb, ab, h1, h2, nominalFits⟩
        · subst same
          exact reduce base (typeFits_refl _) embedded
        · cases h
        · cases h
        · subst h1
          cases h2
          exact reduce eb nominalFits embedded
  | .record fields, actual, expected, core, typed, fits, embedded => by
    obtain ⟨types, unique, typesOk, actualEq⟩ := record_typed typed
    subst actualEq
    have entryValue : ∀ name found, (name, found) ∈ types →
        ∃ value, fields.lookup name = some value ∧ valueType value = some found ∧
          sizeOf value < sizeOf (Value.record fields) := by
      intro name found present
      obtain ⟨value, member, valueTyped⟩ := recordTypes_entry typesOk present
      have smaller := List.sizeOf_lt_of_mem member
      simp only [Prod.mk.sizeOf_spec] at smaller
      exact ⟨value, lookup_of_mem_nodup unique member, valueTyped, by simp; omega⟩
    have coreRecord : ∀ expectedRow,
        (∀ entry ∈ rowEntries expectedRow, ∃ found, (entry.1, found) ∈ types ∧ typeFits entry.2 found = true) →
        embedType (.record expectedRow) = some core →
        Membership.value membershipWorld core (embedValue (.record fields)) := by
      intro expectedRow covered expectedEmbedded
      simp only [embedType] at expectedEmbedded
      cases rowEmbedded : embedRow expectedRow with
      | none => simp [rowEmbedded] at expectedEmbedded
      | some coreRow =>
        simp only [rowEmbedded, Option.map_some, Option.some.injEq] at expectedEmbedded
        subst expectedEmbedded
        simp only [embedValue, Membership.value, Membership.recordFields, Value.eraseNominal]
        apply row_member fields expectedRow coreRow rowEmbedded
        intro entry present coreEntry entryEmbedded
        obtain ⟨found, foundPresent, entryFits⟩ := covered entry present
        obtain ⟨value, lookup, valueTyped, _⟩ := entryValue entry.1 found foundPresent
        exact ⟨value, lookup, member_of_fits value found entry.2 coreEntry valueTyped entryFits entryEmbedded⟩
    rcases typeFits_cases fits with same | ⟨expectedRow, actualRow, h1, h2, rows⟩ | ⟨_, _, _, _, _, h, _⟩ | ⟨_, _, _, _, h, _⟩
    · subst same
      apply coreRecord _ _ embedded
      intro entry present
      rw [rowEntries_fold] at present
      exact ⟨entry.2, (canonicalFields_mem entry types).mp present, typeFits_refl _⟩
    · subst h1
      cases h2
      apply coreRecord expectedRow _ embedded
      intro entry present
      obtain ⟨found, lookup, entryFits⟩ := rowFits_entries rows entry present
      rw [rowFieldType_fold] at lookup
      exact ⟨found, (canonicalFields_mem _ types).mp (lookup_some_mem lookup), entryFits⟩
    all_goals cases h
termination_by candidate => sizeOf candidate
decreasing_by
  all_goals first
    | (simp_wf; omega)
    | assumption
    | (simp_wf; simp_all; omega)
    | skip

theorem has_type_embeds {type : RuntimeType} {candidate : Value} {core : CoreType}
    (typed : HasType type candidate) (embedded : embedType type = some core) :
    Xsh.Semantics.CoreWorld.HasType world core (embedValue candidate) := by
  refine ⟨fun _ _ => trivial, ?_⟩
  obtain ⟨actual, found, fits⟩ := (has_type_iff_compatible type candidate).mp typed
  exact member_of_fits candidate actual type core found fits embedded

def checkerType (type : Xsh.Typing.Connected.MonoType) : Option CoreType :=
  (monoRuntime type).bind embedType

theorem checked_value_is_core_member {type : Xsh.Typing.Connected.MonoType} {runtime : RuntimeType}
    {candidate : Value} {core : CoreType}
    (lowered : monoRuntime type = some runtime) (typed : HasType runtime candidate)
    (embedded : checkerType type = some core) :
    Xsh.Semantics.CoreWorld.HasType world core (embedValue candidate) := by
  simp only [checkerType, lowered, Option.bind_some] at embedded
  exact has_type_embeds typed embedded


theorem connected_write_commits_in_core (store : Store) (cell : CellId) (candidate : Value)
    (updated : Store) (accepted : write store cell candidate = .accepted updated)
    (coreStore : Xsh.Semantics.CoreWorld.Store world)
    (projected : ∀ current, store cell = some current → ∃ coreCell, coreStore cell = some coreCell ∧
      coreCell.mutability = current.mutability ∧
      embedType current.contents.typeTag = some coreCell.contents.typeTag) :
    ∃ next, Xsh.Semantics.CoreWorld.assign world decisions coreStore cell (embedValue candidate) =
      .committed next ∧
      ∃ coreCell, next cell = some coreCell ∧ coreCell.contents.value = embedValue candidate := by
  obtain ⟨current, present, mutableCell, typed⟩ :=
    (write_accepted_iff (fun _ _ => inferInstance) store cell candidate).mp ⟨updated, accepted⟩
  obtain ⟨coreCell, corePresent, sameMutability, sameType⟩ := projected current present
  have coreTyped := has_type_embeds typed sameType
  have coreMutable : coreCell.mutability = .mutable := sameMutability.trans mutableCell
  obtain ⟨next, coreAccepted⟩ :=
    (Xsh.Core.Store.write_accepted_iff world decisions coreStore cell (embedValue candidate)).mpr
      ⟨coreCell, corePresent, coreMutable, coreTyped.1, coreTyped.2⟩
  refine ⟨next, ?_, ?_⟩
  · simp only [Xsh.Semantics.CoreWorld.assign, corePresent, coreMutable, coreAccepted]
  · obtain ⟨_, _, ⟨replaced, replacedAt, _, replacedValue, _⟩, _⟩ :=
      write_accepted_replaces_only_its_cell _ coreStore cell (embedValue candidate) next coreAccepted
    exact ⟨replaced, replacedAt, replacedValue⟩

def witnessNominal : NominalId := ⟨"main", "Package"⟩

def witnessName : NameId := ⟨⟨1, by decide⟩⟩

def witnessRow : RuntimeType := .rowField witnessName .integer .rowEmpty

def witnessRecord : Value := .record [(witnessName, .scalar (.integer ⟨5, by decide⟩))]

theorem bridge_witnesses :
    HasType (.nominal witnessNominal (.record witnessRow)) (.nominal witnessNominal (.record witnessRow) witnessRecord) ∧
    (∃ core, embedType (.nominal witnessNominal (.record witnessRow)) = some core ∧
      Xsh.Semantics.CoreWorld.HasType world core
        (embedValue (.nominal witnessNominal (.record witnessRow) witnessRecord)) ∧
      ¬ Xsh.Semantics.CoreWorld.HasType world core (embedValue witnessRecord)) ∧
    embedType (.nominal witnessNominal .integer) = none ∧
    valueType (.scalar (.float Xsh.Values.floatPositiveZero)) = none ∧
    Xsh.Semantics.CoreWorld.HasType world (.primitive .float)
      (Xsh.Core.Value.scalar (.float Xsh.Values.floatPositiveZero)) := by
  have typed : HasType (.nominal witnessNominal (.record witnessRow))
      (.nominal witnessNominal (.record witnessRow) witnessRecord) := by decide
  refine ⟨typed, ⟨_, rfl, has_type_embeds typed rfl, ?_⟩, rfl, rfl, ⟨fun _ _ => trivial, ?_⟩⟩
  · intro coreTyped
    have member := coreTyped.2
    simp [embedValue, embedFields, witnessRecord] at member
    simp [Xsh.Core.Membership.value] at member
  · simp [Xsh.Core.Membership.value, Xsh.Core.Membership.primitive, Xsh.Core.Value.eraseNominal]

end Xsh.Semantics.Connected.CoreBridge
