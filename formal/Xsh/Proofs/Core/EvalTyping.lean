import Xsh.Proofs.Core.Eval
import Xsh.Typing.Core.Expressions

namespace Xsh.Proofs.Core.EvalTyping

open Xsh.Foundation
open Xsh.Values
open Xsh.Values.FloatOps
open Xsh.Core
open Xsh.Semantics.Environments
open Xsh.Semantics.CoreWorld
open Xsh.Semantics.CoreWrites
open Xsh.Semantics.CoreEval
open Xsh.Typing.Core.Expressions

variable {Declaration Identity Source : Type}

section Shapes

variable (world : MembershipWorld Declaration Identity Source) {value : Value Declaration Identity Source}

theorem int_shape (member : Membership.value world intType value) : ∃ a, value.eraseNominal = .scalar (.integer a) := by
  simp only [intType, Membership.value, Membership.primitive] at member
  split at member <;> simp_all

theorem float_shape (member : Membership.value world floatType value) :
    ∃ a, value.eraseNominal = .scalar (.float a) := by
  simp only [floatType, Membership.value, Membership.primitive] at member
  split at member <;> simp_all

theorem bool_shape (member : Membership.value world boolType value) :
    ∃ a, value.eraseNominal = .scalar (.boolean a) := by
  simp only [boolType, Membership.value, Membership.primitive] at member
  split at member <;> simp_all

theorem str_shape (member : Membership.value world strType value) :
    ∃ a, value.eraseNominal = .scalar (.string a) := by
  simp only [strType, Membership.value, Membership.primitive] at member
  split at member <;> simp_all

theorem dur_shape (member : Membership.value world durType value) :
    ∃ a, value.eraseNominal = .scalar (.duration a) := by
  simp only [durType, Membership.value, Membership.primitive] at member
  split at member <;> simp_all

theorem int_member (a : IntValue) : Membership.value world intType (integer a : Value Declaration Identity Source) := by
  simp [intType, integer, Membership.value, Membership.primitive, Value.eraseNominal]

theorem float_member (a : FloatBits) : Membership.value world floatType (float a : Value Declaration Identity Source) := by
  simp [floatType, float, Membership.value, Membership.primitive, Value.eraseNominal]

theorem bool_member (a : Bool) : Membership.value world boolType (boolean a : Value Declaration Identity Source) := by
  simp [boolType, boolean, Membership.value, Membership.primitive, Value.eraseNominal]

theorem str_member (a : String) :
    Membership.value world strType (.scalar (.string a) : Value Declaration Identity Source) := by
  simp [strType, Membership.value, Membership.primitive, Value.eraseNominal]

theorem dur_member (a : DurationValue) : Membership.value world durType (duration a : Value Declaration Identity Source) := by
  simp [durType, duration, Membership.value, Membership.primitive, Value.eraseNominal]

end Shapes

section Results

theorem bind_value {α β : Type} {first : Result Declaration Identity Source α} {next : α → Result Declaration Identity Source β}
    {result : β} (bound : first.bind next = .value result) : ∃ value, first = .value value ∧ next value = .value result := by
  cases first with
  | value value => exact ⟨value, rfl, bound⟩
  | _ => simp [Result.bind] at bound

theorem integerResult_member (world : MembershipWorld Declaration Identity Source) (errors : Errors Declaration Identity Source)
    {result : IntegerOps.IntegerResult} {value : Value Declaration Identity Source}
    (produced : integerResult errors result = .value value) : Membership.value world intType value := by
  cases result with
  | value inner =>
    simp only [integerResult, Result.value.injEq] at produced
    subst produced
    exact int_member world inner
  | fault fault => simp [integerResult] at produced

theorem durationResult_member (world : MembershipWorld Declaration Identity Source)
    (errors : Errors Declaration Identity Source) {result : DurationOps.DurationResult}
    {value : Value Declaration Identity Source} (produced : durationResult errors result = .value value) :
    Membership.value world durType value ∨ Membership.value world intType value := by
  cases result with
  | millis inner =>
    simp only [durationResult, Result.value.injEq] at produced
    subst produced
    exact Or.inl (dur_member world inner)
  | count inner =>
    simp only [durationResult, Result.value.injEq] at produced
    subst produced
    exact Or.inr (int_member world inner)
  | fault fault => simp [durationResult] at produced

theorem millis_member (world : MembershipWorld Declaration Identity Source)
    (errors : Errors Declaration Identity Source) {result : DurationOps.DurationResult}
    {value : Value Declaration Identity Source} (produced : durationResult errors result = .value value)
    (noCount : ∀ count, result ≠ .count count) : Membership.value world durType value := by
  cases result with
  | millis inner =>
    simp only [durationResult, Result.value.injEq] at produced
    subst produced
    exact dur_member world inner
  | count inner => exact absurd rfl (noCount inner)
  | fault fault => simp [durationResult] at produced

theorem checkedMillis_no_count (millis : Nat) : ∀ count, DurationOps.checkedMillis millis ≠ .count count := by
  intro count same
  unfold DurationOps.checkedMillis at same
  split at same <;> simp at same

theorem add_no_count (left right : DurationValue) : ∀ count, DurationOps.add left right ≠ .count count :=
  checkedMillis_no_count _

theorem subtract_no_count (left right : DurationValue) : ∀ count, DurationOps.subtract left right ≠ .count count := by
  intro count same
  unfold DurationOps.subtract at same
  split at same
  · simp at same
  · exact checkedMillis_no_count _ count same

theorem multiply_no_count (left : DurationValue) (right : IntValue) :
    ∀ count, DurationOps.multiply left right ≠ .count count := by
  intro count same
  unfold DurationOps.multiply at same
  split at same
  · simp at same
  · exact checkedMillis_no_count _ count same

theorem divide_no_count (left : DurationValue) (right : IntValue) :
    ∀ count, DurationOps.divideByInt left right ≠ .count count := by
  intro count same
  unfold DurationOps.divideByInt at same
  split at same
  · simp at same
  · exact checkedMillis_no_count _ count same

theorem ratio_member (world : MembershipWorld Declaration Identity Source) (errors : Errors Declaration Identity Source)
    {left right : DurationValue} {value : Value Declaration Identity Source}
    (produced : durationResult errors (DurationOps.ratio left right) = .value value) :
    Membership.value world intType value := by
  unfold DurationOps.ratio at produced
  split at produced
  · simp [durationResult] at produced
  · split at produced
    · rename_i count _
      simp only [durationResult, Result.value.injEq] at produced
      subst produced
      exact int_member world count
    · simp [durationResult] at produced

end Results

section Steps

variable (context : Context Declaration Identity Source) {world : World Declaration Identity Source}
  (environment : Environment) (store : Store world)
  (recurse : Locals Declaration Identity Source → Xsh.Syntax.Expr →
    Result Declaration Identity Source (Value Declaration Identity Source))
  (locals : Locals Declaration Identity Source)

theorem step_binary (operator : Xsh.Syntax.BinaryOp) (left right : Xsh.Syntax.Expr)
    (value : Value Declaration Identity Source) (notAnd : operator ≠ .and) (notOr : operator ≠ .or)
    (produced : step context environment store recurse locals (.binary operator left right) = .value value) :
    ∃ first second, recurse locals left = .value first ∧ recurse locals right = .value second ∧
      binary context operator first second = .value value := by
  cases operator
  case and => exact absurd rfl notAnd
  case or => exact absurd rfl notOr
  all_goals
    simp only [step] at produced
    obtain ⟨first, firstValue, rest⟩ := bind_value produced
    obtain ⟨second, secondValue, combined⟩ := bind_value rest
    exact ⟨first, second, firstValue, secondValue, combined⟩

theorem evaluateAll_aligned {α : Type} (evaluate : α → Result Declaration Identity Source (Value Declaration Identity Source)) :
    ∀ (items : List α) (values : List (Value Declaration Identity Source)),
      evaluateAll evaluate items = .value values →
        items.length = values.length ∧ ∀ pair ∈ items.zip values, evaluate pair.1 = .value pair.2
  | [], values, produced => by
    simp only [evaluateAll, Result.value.injEq] at produced
    subst produced
    simp
  | first :: rest, values, produced => by
    simp only [evaluateAll] at produced
    obtain ⟨head, headValue, tail⟩ := bind_value produced
    obtain ⟨others, othersValue, joined⟩ := bind_value tail
    simp only [Result.value.injEq] at joined
    subst joined
    obtain ⟨lengths, aligned⟩ := evaluateAll_aligned evaluate rest others othersValue
    refine ⟨by simp [lengths], ?_⟩
    intro pair inside
    simp only [List.zip_cons_cons, List.mem_cons] at inside
    rcases inside with rfl | later
    · exact headValue
    · exact aligned pair later

end Steps

/-- What a typing context promises about a local frame, the environment and
the store: every typed name holds a member of its type where evaluation reads
it, and every local name is typed. -/
def Consistent (world : World Declaration Identity Source) (environment : Environment) (store : Store world)
    (types : TypeContext Declaration) (locals : Locals Declaration Identity Source) : Prop :=
  (∀ name type, types name = some type →
    match locals.lookup name with
    | some value => Membership.value world.membership type value
    | none => ∃ cell found, environment.cellOf name = .ok cell ∧ store cell = some found ∧
        Membership.value world.membership type found.contents.value) ∧
  ∀ name value, locals.lookup name = some value → types name ≠ none

/-- Every let-free function's result is typed at its signature in the global
context overlaid with its parameters. -/
def FunctionsTyped (context : Context Declaration Identity Source) (signatures : Signatures Declaration)
    (global : TypeContext Declaration) : Prop :=
  ∀ name parameters body types result, context.functions name = some (parameters, body) →
    signatures name = some (types, result) →
      parameters.length = types.length ∧ parameters.Nodup ∧
        Typed context.spelling signatures (overlay parameters types global) body result

section Lists

theorem zip_combine {α β γ : Type} {first : α → β → Prop} {second : α → γ → Prop} {third : β → γ → Prop}
    (combine : ∀ a b c, first a b → second a c → third b c) :
    ∀ (xs : List α) (ys : List β) (zs : List γ), xs.length = ys.length → xs.length = zs.length →
      (∀ pair ∈ xs.zip ys, first pair.1 pair.2) → (∀ pair ∈ xs.zip zs, second pair.1 pair.2) →
        ∀ pair ∈ ys.zip zs, third pair.1 pair.2
  | [], [], [], _, _, _, _ => by simp
  | x :: xs, y :: ys, z :: zs, lengthY, lengthZ, firsts, seconds => by
    intro pair inside
    simp only [List.zip_cons_cons, List.mem_cons] at inside
    rcases inside with rfl | later
    · exact combine x y z (firsts (x, y) (by simp)) (seconds (x, z) (by simp))
    · exact zip_combine combine xs ys zs (by simpa using lengthY) (by simpa using lengthZ)
        (fun pair inside => firsts pair (by simp [inside])) (fun pair inside => seconds pair (by simp [inside])) pair later
  | [], _ :: _, _, lengthY, _, _, _ | _ :: _, [], _, lengthY, _, _, _ => by simp at lengthY
  | [], [], _ :: _, _, lengthZ, _, _ | _ :: _, _ :: _, [], _, lengthZ, _, _ => by simp at lengthZ

theorem mem_zip_right {α β : Type} : ∀ (xs : List α) (ys : List β), xs.length = ys.length →
    ∀ y ∈ ys, ∃ x, (x, y) ∈ xs.zip ys
  | [], [], _, y, inside => by simp at inside
  | x :: xs, y' :: ys, lengths, y, inside => by
    simp only [List.mem_cons] at inside
    rcases inside with rfl | later
    · exact ⟨x, by simp⟩
    · obtain ⟨found, present⟩ := mem_zip_right xs ys (by simpa using lengths) y later
      exact ⟨found, by simp [present]⟩
  | [], _ :: _, lengths, _, _ | _ :: _, [], lengths, _, _ => by simp at lengths

theorem positional_map : ∀ values : List Xsh.Syntax.Expr, positional (values.map .positional) = some values
  | [] => rfl
  | value :: rest => by simp [positional, positional_map rest]

theorem zip_lookup_none {α β : Type} : ∀ (names : List Xsh.Syntax.RawName) (xs : List α) (ys : List β) name,
    names.length = xs.length → names.length = ys.length →
      (names.zip xs).lookup name = none → (names.zip ys).lookup name = none
  | [], _, _, _, _, _, _ => by simp
  | first :: rest, x :: xs, y :: ys, name, lengthX, lengthY, absent => by
    by_cases same : name = first
    · subst same
      simp at absent
    · have different : (name == first) = false := by simpa using same
      simp only [List.zip_cons_cons, List.lookup, different] at absent ⊢
      exact zip_lookup_none rest xs ys name (by simpa using lengthX) (by simpa using lengthY) absent
  | _ :: _, [], _, _, lengthX, _, _ | _ :: _, _ :: _, [], _, _, lengthY, _ => by simp_all

theorem zip_lookup_aligned {α β : Type} : ∀ (names : List Xsh.Syntax.RawName) (xs : List α) (ys : List β) name x y,
    names.length = xs.length → names.length = ys.length →
      (names.zip xs).lookup name = some x → (names.zip ys).lookup name = some y → (y, x) ∈ ys.zip xs
  | [], _, _, _, _, _, _, _, found, _ => by simp at found
  | first :: rest, x' :: xs, y' :: ys, name, x, y, lengthX, lengthY, foundX, foundY => by
    by_cases same : name = first
    · subst same
      simp at foundX foundY
      subst foundX
      subst foundY
      simp
    · have different : (name == first) = false := by simpa using same
      simp only [List.zip_cons_cons, List.lookup, different] at foundX foundY
      have later := zip_lookup_aligned rest xs ys name x y (by simpa using lengthX) (by simpa using lengthY) foundX foundY
      simp [later]
  | _ :: _, [], _, _, _, _, lengthX, _, _, _ | _ :: _, _ :: _, [], _, _, _, _, lengthY, _, _ => by simp_all

end Lists

section Consistency

theorem overlay_consistent (world : World Declaration Identity Source) (environment : Environment) (store : Store world)
    (global : TypeContext Declaration) (globalConsistent : Consistent world environment store global [])
    (parameters : List Xsh.Syntax.RawName) (types : List (ValueType Declaration))
    (values : List (Value Declaration Identity Source)) (typesLength : parameters.length = types.length)
    (valuesLength : parameters.length = values.length)
    (members : ∀ pair ∈ values.zip types, Membership.value world.membership pair.2 pair.1) :
    Consistent world environment store (overlay parameters types global) (parameters.zip values) := by
  refine ⟨?_, ?_⟩
  · intro name type typed
    unfold overlay at typed
    cases parameterType : (parameters.zip types).lookup name with
    | some found =>
      simp only [parameterType, Option.orElse_some, Option.some.injEq] at typed
      subst typed
      cases parameterValue : (parameters.zip values).lookup name with
      | some value =>
        simp only []
        exact members (value, found) (zip_lookup_aligned parameters types values name found value typesLength valuesLength
          parameterType parameterValue)
      | none =>
        have := zip_lookup_none parameters values types name valuesLength typesLength parameterValue
        rw [this] at parameterType
        cases parameterType
    | none =>
      simp only [parameterType, Option.orElse_none] at typed
      have absent := zip_lookup_none parameters types values name typesLength valuesLength parameterType
      simp only [absent]
      have := globalConsistent.1 name type typed
      simpa using this
  · intro name value found
    unfold overlay
    cases parameterType : (parameters.zip types).lookup name with
    | some type => simp
    | none =>
      have := zip_lookup_none parameters types values name typesLength valuesLength parameterType
      rw [this] at found
      cases found

theorem consistent_unbound (world : World Declaration Identity Source) (environment : Environment) (store : Store world)
    {types : TypeContext Declaration} {locals : Locals Declaration Identity Source}
    (consistent : Consistent world environment store types locals) {name : Xsh.Syntax.RawName}
    (untyped : types name = none) : locals.lookup name = none := by
  cases found : locals.lookup name with
  | none => rfl
  | some value => exact absurd untyped (consistent.2 name value found)

end Consistency

section Rows

theorem row_cons_irrelevant (world : MembershipWorld Declaration Identity Source) (name : String)
    (value : Value Declaration Identity Source) :
    ∀ (row : RecordType Declaration) (fields : List (String × Value Declaration Identity Source)), name ∉ namesOf row →
      (Membership.row world row ((name, value) :: fields) ↔ Membership.row world row fields)
  | .variable id, _, _ => nomatch id
  | .rowEmpty, _, _ => by simp [Membership.row]
  | .rowField other type rest, fields, absent => by
    simp only [namesOf, List.mem_cons, not_or] at absent
    have differentNames : (other == name) = false := by simpa using Ne.symm absent.1
    simp only [Membership.row, List.lookup, differentNames, row_cons_irrelevant world name value rest fields absent.2]

theorem lookupRow_member (world : MembershipWorld Declaration Identity Source) :
    ∀ (row : RecordType Declaration) (fields : List (String × Value Declaration Identity Source)) name type,
      Membership.row world row fields → lookupRow row name = some type →
        ∃ found, fields.lookup name = some found ∧ Membership.value world type found
  | .variable id, _, _, _, _, _ => nomatch id
  | .rowEmpty, _, _, _, _, found => by simp [lookupRow] at found
  | .rowField other type rest, fields, name, wanted, member, found => by
    simp only [Membership.row] at member
    simp only [lookupRow] at found
    by_cases same : other = name
    · simp only [same, ite_true, Option.some.injEq] at found
      subst same
      subst found
      cases present : fields.lookup other with
      | none => simp [present] at member
      | some value =>
        simp only [present] at member
        exact ⟨value, rfl, member.1⟩
    · simp only [same, ite_false] at found
      exact lookupRow_member world rest fields name wanted member.2 found

end Rows

section Preservation

variable (context : Context Declaration Identity Source) (world : World Declaration Identity Source)
  (environment : Environment) (store : Store world) (signatures : Signatures Declaration)
  (global : TypeContext Declaration)

theorem list_member_items {element : ValueType Declaration} {candidate : Value Declaration Identity Source}
    (member : Membership.value world.membership (.list element) candidate) :
    ∃ items, listValues candidate = some items ∧ ∀ item ∈ items, Membership.value world.membership element item := by
  simp only [Membership.value] at member
  unfold listValues
  split at member
  · rename_i items erased
    exact ⟨items, by simp [erased], member⟩
  · rename_i paths erased
    refine ⟨paths.map fun path => .scalar (.path path), by simp [erased], ?_⟩
    intro item inside
    obtain ⟨path, present, rfl⟩ := List.mem_map.mp inside
    exact member path present
  · exact member.elim

theorem indexAt_items {fromEnd : Bool} {container key value : Value Declaration Identity Source}
    {items : List (Value Declaration Identity Source)} (listed : listValues container = some items)
    (found : indexAt context fromEnd container key = .value value) : value ∈ items := by
  unfold indexAt at found
  rw [listed] at found
  split at found
  · rename_i items' position sameItems
    simp only [Option.some.injEq] at sameItems
    subst sameItems
    dsimp only at found
    split at found
    all_goals
      split at found
      · simp only [Result.value.injEq] at found
        subst found
        exact List.getElem_mem _
      · simp at found
  · have notMap : ∀ entries, container.eraseNominal ≠ .map entries := by
      intro entries erased
      simp [listValues, erased] at listed
    have notRecord : ∀ fields, container.eraseNominal ≠ .record fields := by
      intro fields erased
      simp [listValues, erased] at listed
    cases erased : container.eraseNominal with
    | map entries => exact absurd erased (notMap entries)
    | record fields => exact absurd erased (notRecord fields)
    | _ => simp [erased] at found

theorem indexAt_entries {keyType element : ValueType Declaration} {fromEnd : Bool}
    {container key value : Value Declaration Identity Source}
    (member : Membership.value world.membership (.map keyType element) container)
    (found : indexAt context fromEnd container key = .value value) : Membership.value world.membership element value := by
  have notListed : listValues container = none := by
    simp only [Membership.value] at member
    unfold listValues
    cases erased : container.eraseNominal <;> simp [erased] at member ⊢
  unfold indexAt at found
  rw [notListed] at found
  simp only [Membership.value] at member
  cases erased : container.eraseNominal with
  | map entries =>
    simp only [erased] at found member
    cases keyOf : mapKeyOf key with
    | none => simp [keyOf] at found
    | some mapKey =>
      cases present : entries.lookup mapKey with
      | none => simp [keyOf, present] at found
      | some other =>
        simp only [keyOf, present, Result.value.injEq] at found
        subst found
        exact (member _ (Xsh.Proofs.Core.Writes.lookup_entry_mem present)).2
  | record fields =>
    simp only [erased] at found member
    cases keyOf : mapKeyOf key with
    | none => simp [keyOf] at found
    | some mapKey =>
      cases mapKey with
      | string label =>
        cases present : fields.lookup label with
        | none => simp [keyOf, present] at found
        | some other =>
          simp only [keyOf, present, Result.value.injEq] at found
          subst found
          obtain ⟨before, after, rfl, _⟩ := List.lookup_eq_some_iff.mp present
          exact member.2 (label, other) (by simp)
      | _ => simp [keyOf] at found
  | _ => simp [erased] at member

theorem branches_member (types : TypeContext Declaration) (locals : Locals Declaration Identity Source)
    (evaluate' : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source))
    (preserves : ∀ expression type value, Typed context.spelling signatures types expression type →
      evaluate' expression = .value value → Membership.value world.membership type value) (type : ValueType Declaration) :
    ∀ (conditions : List (Xsh.Syntax.Expr × Xsh.Syntax.Expr)) otherwise value,
      (∀ branch ∈ conditions, Typed context.spelling signatures types branch.1 boolType) →
      (∀ branch ∈ conditions, Typed context.spelling signatures types branch.2 type) →
      Typed context.spelling signatures types otherwise type →
      branches evaluate' conditions otherwise = .value value → Membership.value world.membership type value
  | [], otherwise, value, _, _, otherwiseTyped, produced => preserves otherwise type value otherwiseTyped produced
  | (condition, branchValue) :: rest, otherwise, value, conditionsTyped, valuesTyped, otherwiseTyped, produced => by
    simp only [branches] at produced
    obtain ⟨tested, _, chosen⟩ := bind_value produced
    split at chosen
    · exact preserves branchValue type value (valuesTyped (condition, branchValue) (by simp)) chosen
    · exact branches_member types locals evaluate' preserves type rest otherwise value
        (fun branch inside => conditionsTyped branch (by simp [inside]))
        (fun branch inside => valuesTyped branch (by simp [inside])) otherwiseTyped chosen
    · simp at chosen

theorem fields_member (types : TypeContext Declaration)
    (evaluate' : Xsh.Syntax.Expr → Result Declaration Identity Source (Value Declaration Identity Source))
    (preserves : ∀ expression type value, Typed context.spelling signatures types expression type →
      evaluate' expression = .value value → Membership.value world.membership type value) :
    ∀ (fields : List Xsh.Syntax.RecordField) (row : RecordType Declaration) out,
      FieldsTyped context.spelling signatures types fields row →
      evaluateFields context evaluate' fields = .value out → Membership.row world.membership row out
  | [], row, out, typed, produced => by
    cases typed
    simp [Membership.row]
  | .named name operand :: rest, row, out, typed, produced => by
    cases typed with
    | cons operandTyped restTyped fresh =>
      simp only [evaluateFields] at produced
      obtain ⟨value, operandValue, tail⟩ := bind_value produced
      obtain ⟨others, othersValue, joined⟩ := bind_value tail
      simp only [Result.value.injEq] at joined
      subst joined
      simp only [Membership.row, List.lookup, beq_self_eq_true]
      exact ⟨preserves operand _ value operandTyped operandValue,
        (row_cons_irrelevant world.membership _ value _ others fresh).mpr
          (fields_member types evaluate' preserves rest _ others restTyped othersValue)⟩
  | .computed _ _ :: _, _, _, typed, _ | .path _ _ :: _, _, _, typed, _ | .shorthand _ :: _, _, _, typed, _
  | .spread _ :: _, _, _, typed, _ => by cases typed

theorem binary_integers (operator : Xsh.Syntax.BinaryOp) {first second : Value Declaration Identity Source}
    {value : Value Declaration Identity Source}
    (firstMember : Membership.value world.membership intType first)
    (secondMember : Membership.value world.membership intType second)
    (produced : binary context operator first second = .value value) :
    (operator ∈ arithmetic → Membership.value world.membership intType value) ∧
      (operator ∈ comparisons → Membership.value world.membership boolType value) := by
  obtain ⟨a, firstShape⟩ := int_shape world.membership firstMember
  obtain ⟨b, secondShape⟩ := int_shape world.membership secondMember
  simp only [binary, firstShape, secondShape] at produced
  refine ⟨fun arithmeticOp => ?_, fun comparisonOp => ?_⟩
  · simp only [arithmetic, List.mem_cons, List.not_mem_nil, or_false] at arithmeticOp
    rcases arithmeticOp with rfl | rfl | rfl | rfl | rfl <;>
      exact integerResult_member world.membership context.errors produced
  · simp only [comparisons, List.mem_cons, List.not_mem_nil, or_false] at comparisonOp
    rcases comparisonOp with rfl | rfl | rfl | rfl | rfl | rfl <;>
      simp only [integerOperator, ordering, Result.value.injEq] at produced <;>
      (subst produced; exact bool_member world.membership _)

theorem binary_floats (operator : Xsh.Syntax.BinaryOp) {first second : Value Declaration Identity Source}
    {value : Value Declaration Identity Source}
    (firstMember : Membership.value world.membership floatType first)
    (secondMember : Membership.value world.membership floatType second)
    (produced : binary context operator first second = .value value) :
    (operator ∈ floatArithmetic → Membership.value world.membership floatType value) ∧
      (operator ∈ comparisons → Membership.value world.membership boolType value) := by
  obtain ⟨a, firstShape⟩ := float_shape world.membership firstMember
  obtain ⟨b, secondShape⟩ := float_shape world.membership secondMember
  simp only [binary, firstShape, secondShape] at produced
  refine ⟨fun arithmeticOp => ?_, fun comparisonOp => ?_⟩
  · simp only [floatArithmetic, List.mem_cons, List.not_mem_nil, or_false] at arithmeticOp
    rcases arithmeticOp with rfl | rfl | rfl | rfl <;>
      simp only [floatOperator, Result.value.injEq] at produced <;>
      (subst produced; exact float_member world.membership _)
  · simp only [comparisons, List.mem_cons, List.not_mem_nil, or_false] at comparisonOp
    rcases comparisonOp with rfl | rfl | rfl | rfl | rfl | rfl <;>
      simp only [floatOperator, ordering, Result.value.injEq] at produced <;>
      (subst produced; exact bool_member world.membership _)

theorem binary_durations (operator : Xsh.Syntax.BinaryOp) {first second : Value Declaration Identity Source}
    {value : Value Declaration Identity Source}
    (firstMember : Membership.value world.membership durType first)
    (secondMember : Membership.value world.membership durType second)
    (produced : binary context operator first second = .value value) :
    ((operator = .add ∨ operator = .sub) → Membership.value world.membership durType value) ∧
      (operator = .div → Membership.value world.membership intType value) ∧
      (operator ∈ comparisons → Membership.value world.membership boolType value) := by
  obtain ⟨a, firstShape⟩ := dur_shape world.membership firstMember
  obtain ⟨b, secondShape⟩ := dur_shape world.membership secondMember
  simp only [binary, firstShape, secondShape] at produced
  refine ⟨fun sum => ?_, fun divide => ?_, fun comparisonOp => ?_⟩
  · rcases sum with rfl | rfl
    · exact millis_member world.membership context.errors produced (add_no_count a b)
    · exact millis_member world.membership context.errors produced (subtract_no_count a b)
  · subst divide
    exact ratio_member world.membership context.errors produced
  · simp only [comparisons, List.mem_cons, List.not_mem_nil, or_false] at comparisonOp
    rcases comparisonOp with rfl | rfl | rfl | rfl | rfl | rfl <;>
      simp only [ordering, Result.value.injEq] at produced <;>
      (subst produced; exact bool_member world.membership _)

theorem binary_scale (operator : Xsh.Syntax.BinaryOp) {first second : Value Declaration Identity Source}
    {value : Value Declaration Identity Source}
    (firstMember : Membership.value world.membership durType first)
    (secondMember : Membership.value world.membership intType second)
    (scale : operator = .mul ∨ operator = .div)
    (produced : binary context operator first second = .value value) : Membership.value world.membership durType value := by
  obtain ⟨a, firstShape⟩ := dur_shape world.membership firstMember
  obtain ⟨b, secondShape⟩ := int_shape world.membership secondMember
  simp only [binary, firstShape, secondShape] at produced
  rcases scale with rfl | rfl
  · exact millis_member world.membership context.errors produced (multiply_no_count a b)
  · exact millis_member world.membership context.errors produced (divide_no_count a b)

theorem binary_strings (operator : Xsh.Syntax.BinaryOp) {first second : Value Declaration Identity Source}
    {value : Value Declaration Identity Source}
    (firstMember : Membership.value world.membership strType first)
    (secondMember : Membership.value world.membership strType second)
    (produced : binary context operator first second = .value value) :
    (operator = .add → Membership.value world.membership strType value) ∧
      (operator ∈ comparisons → Membership.value world.membership boolType value) := by
  obtain ⟨a, firstShape⟩ := str_shape world.membership firstMember
  obtain ⟨b, secondShape⟩ := str_shape world.membership secondMember
  simp only [binary, firstShape, secondShape] at produced
  refine ⟨fun added => ?_, fun comparisonOp => ?_⟩
  · subst added
    simp only [Result.value.injEq] at produced
    subst produced
    exact str_member world.membership _
  · simp only [comparisons, List.mem_cons, List.not_mem_nil, or_false] at comparisonOp
    rcases comparisonOp with rfl | rfl | rfl | rfl | rfl | rfl <;>
      simp only [ordering, Result.value.injEq] at produced <;>
      (subst produced; exact bool_member world.membership _)

theorem binary_booleans (operator : Xsh.Syntax.BinaryOp) {first second : Value Declaration Identity Source}
    {value : Value Declaration Identity Source}
    (firstMember : Membership.value world.membership boolType first)
    (secondMember : Membership.value world.membership boolType second) (equality : operator ∈ equalities)
    (produced : binary context operator first second = .value value) : Membership.value world.membership boolType value := by
  obtain ⟨a, firstShape⟩ := bool_shape world.membership firstMember
  obtain ⟨b, secondShape⟩ := bool_shape world.membership secondMember
  simp only [binary, firstShape, secondShape] at produced
  simp only [equalities, List.mem_cons, List.not_mem_nil, or_false] at equality
  rcases equality with rfl | rfl <;>
    simp only [scalarEqual, Result.value.injEq] at produced <;>
    (subst produced; exact bool_member world.membership _)

theorem binary_lists {element : ValueType Declaration} {first second : Value Declaration Identity Source}
    {value : Value Declaration Identity Source}
    (firstMember : Membership.value world.membership (.list element) first)
    (secondMember : Membership.value world.membership (.list element) second)
    (produced : binary context .add first second = .value value) :
    Membership.value world.membership (.list element) value := by
  obtain ⟨firstItems, firstListed, firstMembers⟩ := list_member_items world firstMember
  obtain ⟨secondItems, secondListed, secondMembers⟩ := list_member_items world secondMember
  have notScalar : ∀ scalar, first.eraseNominal ≠ .scalar scalar := by
    intro scalar erased
    simp [listValues, erased] at firstListed
  unfold binary at produced
  split at produced
  all_goals first
    | (rename_i a b firstShape _; exact absurd firstShape (notScalar _))
    | skip
  simp only [firstListed, secondListed, Result.value.injEq] at produced
  subst produced
  simp only [Membership.value, Value.eraseNominal]
  intro item inside
  rcases List.mem_append.mp inside with early | late
  · exact firstMembers item early
  · exact secondMembers item late

end Preservation

section Theorem

/-- Type preservation for the covered expressions: in a typing context whose
names hold members, with every let-free function typed at its signature, an
evaluation that produces a value produces a member of the expression's
type. -/
theorem preservation (context : Context Declaration Identity Source) (world : World Declaration Identity Source)
    (environment : Environment) (store : Store world) (signatures : Signatures Declaration)
    (global : TypeContext Declaration) (globalConsistent : Consistent world environment store global [])
    (functions : FunctionsTyped context signatures global) :
    ∀ fuel (types : TypeContext Declaration) (locals : Locals Declaration Identity Source) expression type value,
      Typed context.spelling signatures types expression type → Consistent world environment store types locals →
      evaluate context environment store fuel locals expression = .value value →
        Membership.value world.membership type value
  | 0, _, _, _, _, _, _, _, produced => by simp [evaluate] at produced
  | fuel + 1, types, locals, expression, type, value, typed, consistent, produced => by
    have recurse : ∀ expression type value, Typed context.spelling signatures types expression type →
        evaluate context environment store fuel locals expression = .value value →
          Membership.value world.membership type value :=
      fun expression type value typed produced =>
        preservation context world environment store signatures global globalConsistent functions fuel types locals
          expression type value typed consistent produced
    simp only [evaluate] at produced
    cases typed with
    | null => simp only [step, literal, Result.value.injEq] at produced; subst produced
              simp [Membership.value, Membership.primitive, Value.eraseNominal]
    | boolType => simp only [step, literal, Result.value.injEq] at produced; subst produced; exact bool_member _ _
    | integer =>
      simp only [step, literal] at produced
      split at produced
      · simp only [Result.value.injEq] at produced; subst produced; exact int_member _ _
      · simp at produced
    | float =>
      simp only [step, literal] at produced
      split at produced
      · simp only [Result.value.injEq] at produced; subst produced; exact float_member _ _
      · simp at produced
    | duration =>
      simp only [step, literal] at produced
      split at produced
      · simp only [Result.value.injEq] at produced; subst produced; exact dur_member _ _
      · simp at produced
    | string => simp only [step, literal, Result.value.injEq] at produced; subst produced; exact str_member _ _
    | ident found =>
      rename_i name
      have promised := consistent.1 name type found
      simp only [step] at produced
      split at produced
      · rename_i local' localFound
        simp only [Result.value.injEq] at produced
        subst produced
        simpa [localFound] using promised
      · rename_i noLocal
        simp only [noLocal] at promised
        obtain ⟨cell, stored, resolved, present, member⟩ := promised
        simp only [resolved, present, Result.value.injEq] at produced
        subst produced
        exact member
    | list each =>
      rename_i elements element
      simp only [step] at produced
      obtain ⟨values, evaluated, joined⟩ := bind_value produced
      simp only [Result.value.injEq] at joined
      subst joined
      obtain ⟨lengths, aligned⟩ := evaluateAll_aligned _ elements values evaluated
      simp only [Membership.value, Value.eraseNominal]
      intro item inside
      obtain ⟨source, pairInside⟩ := mem_zip_right elements values lengths item inside
      exact recurse source element item (each source (List.of_mem_zip pairInside).1) (aligned _ pairInside)
    | record fieldsTyped =>
      rename_i fields row
      simp only [step] at produced
      obtain ⟨out, evaluated, joined⟩ := bind_value produced
      simp only [Result.value.injEq] at joined
      subst joined
      simp only [Membership.value, Membership.recordFields, Value.eraseNominal]
      exact fields_member context world signatures types _ recurse fields row out fieldsTyped evaluated
    | @field base name row _ baseTyped found =>
      simp only [step] at produced
      obtain ⟨container, containerValue, selected⟩ := bind_value produced
      have containerMember := recurse base _ container baseTyped containerValue
      simp only [fieldOf] at selected
      simp only [Membership.value, Membership.recordFields] at containerMember
      cases erased : container.eraseNominal with
      | record fields =>
        simp only [erased] at selected containerMember
        cases present : fields.lookup (context.spelling name) with
        | none => simp [present] at selected
        | some other =>
          simp only [present, Result.value.injEq] at selected
          subst selected
          obtain ⟨other', otherPresent, otherMember⟩ :=
            lookupRow_member world.membership row fields _ type containerMember found
          rw [present, Option.some.injEq] at otherPresent
          exact otherPresent ▸ otherMember
      | _ => simp [erased] at selected
    | @listIndex base key _ baseTyped keyTyped =>
      simp only [step] at produced
      obtain ⟨container, containerValue, selected⟩ := bind_value produced
      obtain ⟨items, listed, members⟩ :=
        list_member_items world (recurse base _ container baseTyped containerValue)
      split at selected
      · obtain ⟨distance, _, chosen⟩ := bind_value selected
        exact members _ (indexAt_items context listed chosen)
      · obtain ⟨selector, _, chosen⟩ := bind_value selected
        exact members _ (indexAt_items context listed chosen)
    | @mapIndex base key keyType _ baseTyped keyTyped =>
      simp only [step] at produced
      obtain ⟨container, containerValue, selected⟩ := bind_value produced
      have containerMember := recurse base _ container baseTyped containerValue
      split at selected
      · obtain ⟨distance, _, chosen⟩ := bind_value selected
        exact indexAt_entries context world containerMember chosen
      · obtain ⟨selector, _, chosen⟩ := bind_value selected
        exact indexAt_entries context world containerMember chosen
    | not operandTyped =>
      rename_i operand
      simp only [step] at produced
      obtain ⟨inner, innerValue, applied⟩ := bind_value produced
      obtain ⟨b, shape⟩ := bool_shape world.membership (recurse operand _ inner operandTyped innerValue)
      simp only [unary, shape, Result.value.injEq] at applied
      subst applied
      exact bool_member _ _
    | negInt operandTyped =>
      rename_i operand
      simp only [step] at produced
      obtain ⟨inner, innerValue, applied⟩ := bind_value produced
      obtain ⟨b, shape⟩ := int_shape world.membership (recurse operand _ inner operandTyped innerValue)
      simp only [unary, shape] at applied
      exact integerResult_member world.membership context.errors applied
    | negFloat operandTyped =>
      rename_i operand
      simp only [step] at produced
      obtain ⟨inner, innerValue, applied⟩ := bind_value produced
      obtain ⟨b, shape⟩ := float_shape world.membership (recurse operand _ inner operandTyped innerValue)
      simp only [unary, shape, Result.value.injEq] at applied
      subst applied
      exact float_member _ _
    | logical isLogical leftTyped rightTyped =>
      rename_i operator left right
      rcases isLogical with rfl | rfl
      all_goals
        simp only [step] at produced
        obtain ⟨first, firstValue, rest⟩ := bind_value produced
        obtain ⟨a, firstShape⟩ := bool_shape world.membership (recurse left _ first leftTyped firstValue)
        simp only [firstShape] at rest
        cases a
        all_goals first
          | (simp only [Result.value.injEq] at rest; subst rest; exact bool_member _ _)
          | (obtain ⟨second, secondValue, last⟩ := bind_value rest
             obtain ⟨b, secondShape⟩ := bool_shape world.membership (recurse right _ second rightTyped secondValue)
             simp only [secondShape, Result.value.injEq] at last
             subst last
             exact bool_member _ _)
    | intArithmetic isArithmetic leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by intro same; subst same; simp [arithmetic] at isArithmetic
      have notOr : operator ≠ .or := by intro same; subst same; simp [arithmetic] at isArithmetic
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact (binary_integers context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).1 isArithmetic
    | intComparison isComparison leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by intro same; subst same; simp [comparisons] at isComparison
      have notOr : operator ≠ .or := by intro same; subst same; simp [comparisons] at isComparison
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact (binary_integers context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).2 isComparison
    | floatArithmetic isArithmetic leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by intro same; subst same; simp [floatArithmetic] at isArithmetic
      have notOr : operator ≠ .or := by intro same; subst same; simp [floatArithmetic] at isArithmetic
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact (binary_floats context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).1 isArithmetic
    | floatComparison isComparison leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by intro same; subst same; simp [comparisons] at isComparison
      have notOr : operator ≠ .or := by intro same; subst same; simp [comparisons] at isComparison
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact (binary_floats context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).2 isComparison
    | durationSum isSum leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by rcases isSum with rfl | rfl <;> simp
      have notOr : operator ≠ .or := by rcases isSum with rfl | rfl <;> simp
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact (binary_durations context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).1 isSum
    | durationRatio leftTyped rightTyped =>
      rename_i left right
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals .div left right value (by simp) (by simp) produced
      exact (binary_durations context world .div (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).2.1 rfl
    | durationComparison isComparison leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by intro same; subst same; simp [comparisons] at isComparison
      have notOr : operator ≠ .or := by intro same; subst same; simp [comparisons] at isComparison
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact (binary_durations context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).2.2 isComparison
    | durationScale isScale leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by rcases isScale with rfl | rfl <;> simp
      have notOr : operator ≠ .or := by rcases isScale with rfl | rfl <;> simp
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact binary_scale context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) isScale combined
    | stringConcat leftTyped rightTyped =>
      rename_i left right
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals .add left right value (by simp) (by simp) produced
      exact (binary_strings context world .add (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).1 rfl
    | stringComparison isComparison leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by intro same; subst same; simp [comparisons] at isComparison
      have notOr : operator ≠ .or := by intro same; subst same; simp [comparisons] at isComparison
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact (binary_strings context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined).2 isComparison
    | boolEquality isEquality leftTyped rightTyped =>
      rename_i operator left right
      have notAnd : operator ≠ .and := by intro same; subst same; simp [equalities] at isEquality
      have notOr : operator ≠ .or := by intro same; subst same; simp [equalities] at isEquality
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals operator left right value notAnd notOr produced
      exact binary_booleans context world operator (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) isEquality combined
    | listConcat leftTyped rightTyped =>
      rename_i left right element
      obtain ⟨first, second, firstValue, secondValue, combined⟩ :=
        step_binary context environment store _ locals .add left right value (by simp) (by simp) produced
      exact binary_lists context world (recurse left _ first leftTyped firstValue)
        (recurse right _ second rightTyped secondValue) combined
    | conditional conditionsTyped valuesTyped otherwiseTyped =>
      rename_i conditions otherwise
      simp only [step] at produced
      exact branches_member context world signatures types locals _ recurse type conditions otherwise value
        conditionsTyped valuesTyped otherwiseTyped produced
    | @call name values parameters _ untyped signed lengths argumentsTyped =>
      have noLocal := consistent_unbound world environment store consistent untyped
      simp only [step, noLocal, positional_map] at produced
      cases found : context.functions name with
      | none => simp [found] at produced
      | some entry =>
        obtain ⟨parameterNames, body⟩ := entry
        simp only [found] at produced
        obtain ⟨evaluated, evaluatedValue, called⟩ := bind_value produced
        by_cases sameLength : parameterNames.length = evaluated.length
        · simp only [sameLength, ite_true] at called
          obtain ⟨namesLength, _, bodyTyped⟩ := functions name parameterNames body parameters type found signed
          obtain ⟨valuesLength, aligned⟩ := evaluateAll_aligned _ values evaluated evaluatedValue
          have members : ∀ pair ∈ evaluated.zip parameters, Membership.value world.membership pair.2 pair.1 :=
            zip_combine (fun source result parameterType evaluatedTo typedAt =>
                recurse source parameterType result typedAt evaluatedTo)
              values evaluated parameters valuesLength lengths aligned argumentsTyped
          have bodyConsistent := overlay_consistent world environment store global globalConsistent parameterNames
            parameters evaluated namesLength sameLength members
          exact preservation context world environment store signatures global globalConsistent functions fuel _ _ body
            type value bodyTyped bodyConsistent called
        · simp [sameLength] at called

end Theorem

end Xsh.Proofs.Core.EvalTyping
