import Xsh.Validation.Nominal
import Lean.Elab.Tactic.Omega

/-!
A finite-fuel model of recursive `.require` checks for scalar, list, optional,
and record shapes. Raw composite values remain representable even when a nested
field has the wrong shape. The checked result carries a proof that the entire
modeled schema passed, while erasure returns the original composite value.

The check descends through required fields and list items, treats `null` as a
valid optional value, preserves extra uniquely named record fields, and rejects
duplicate record keys. Insufficient fuel rejects the modeled check. Validation
failures are ordinary result values and preserve their candidate and the
modeled store.
This fragment omits maps, sets, unions, wire-enum conversion, defaults,
module contracts, source checking, Rust correspondence, and all full-language
G2 claims.
-/

namespace Xsh.Validation.RecursiveRequire

open Xsh.Values
open Xsh.Validation.Nominal

/-- A deliberately small target grammar for nested schema checks. -/
inductive Target where
  | integer
  | string
  | optional (inner : Target)
  | list (element : Target)
  | field (name : String) (target : Target)
  | record (fields : List Target)

/-- Record schema fields are required; optionality concerns their values. -/
def findField? (name : String) : List (String × CompositeValue) → Option CompositeValue
  | [] => none
  | (field, value) :: rest =>
      if field == name then some value else findField? name rest

/-- A record is valid only when each key has exactly one stored value. -/
def recordKeysUnique : List (String × CompositeValue) → Bool
  | [] => true
  | (name, _) :: rest =>
      if rest.any (fun (other, _) => other == name) then false
      else recordKeysUnique rest

/-- Each declared field must exist and pass its complete nested target. -/
def requiredFieldsPass (check : Target → CompositeValue → Bool) :
    List Target → List (String × CompositeValue) → Bool
  | [], _ => true
  | .field name target :: rest, actual =>
      match findField? name actual with
      | none => false
      | some value => check target value && requiredFieldsPass check rest actual
  | _ :: _, _ => false

/-- Recursive schema checking decreases the explicit fuel at every nested type. -/
def checkFuel : Nat → Target → CompositeValue → Bool
  | 0, _, _ => false
  | Nat.succ fuel, target, value =>
      match target, value with
      | .integer, .integer _ => true
      | .string, .string _ => true
      | .optional _, .null => true
      | .optional inner, value => checkFuel fuel inner value
      | .list element, .list values => values.all (checkFuel fuel element)
      | .record required, .record actual =>
          recordKeysUnique actual && requiredFieldsPass (checkFuel fuel) required actual
      | _, _ => false

def check (fuel : Nat) (target : Target) (value : CompositeValue) : Bool :=
  checkFuel fuel target value

/-- Each recursive checker call receives its caller's fuel minus one. -/
theorem recursive_check_uses_predecessor_fuel (fuel : Nat) :
    Nat.succ fuel - 1 = fuel := by
  omega

/-- The source value and its input provenance at the validation boundary. -/
structure Candidate where
  value : CompositeValue
  provenance : RequireInput

/-- Proof-carrying nominal value produced only after the whole shape passes. -/
structure CheckedValue where
  fuel : Nat
  target : Target
  nominal : NominalValue
  accepted : check fuel target nominal.value = true

/-- Erasure removes only the modeled nominal evidence. -/
def erase (value : CheckedValue) : CompositeValue := Nominal.erase value.nominal

/-- A rejected candidate remains available in the recoverable error value. -/
structure ValidationFailure where
  fuel : Nat
  target : NominalId
  shape : Target
  candidate : Candidate

/-- `.require` returns a value-level success or recoverable validation error. -/
inductive RequireResult where
  | accepted (value : CheckedValue)
  | rejected (failure : ValidationFailure)

/-- The result is published only in the branch with a whole-shape proof. -/
def require (fuel : Nat) (target : NominalId) (shape : Target) (candidate : Candidate) :
    RequireResult :=
  if accepted : check fuel shape candidate.value = true then
    .accepted ⟨fuel, shape,
      Nominal.introduceByRequire target candidate.provenance candidate.value,
      accepted⟩
  else
    .rejected ⟨fuel, target, shape, candidate⟩

/-- A minimal immutable store makes the boundary's non-mutation observable. -/
structure Store where
  cells : List (String × CompositeValue)

structure Attempt where
  result : RequireResult
  storeAfter : Store

def requireAtBoundary (store : Store) (fuel : Nat) (target : NominalId) (shape : Target)
    (candidate : Candidate) : Attempt :=
  ⟨require fuel target shape candidate, store⟩

theorem require_success_preserves_erased_data_and_provenance
    (fuel : Nat) (target : NominalId) (shape : Target) (candidate : Candidate)
    (checked : CheckedValue)
    (success : require fuel target shape candidate = .accepted checked) :
    erase checked = candidate.value ∧
      checked.nominal.nominalId = target ∧
      checked.nominal.provenance = .require candidate.provenance := by
  cases checkResult : check fuel shape candidate.value with
  | false =>
      simp [require, checkResult] at success
  | true =>
      simp [require, checkResult] at success
      cases success
      exact ⟨rfl, rfl, rfl⟩

theorem rejected_require_is_recoverable_and_nonmutating
    (store : Store) (fuel : Nat) (target : NominalId) (shape : Target)
    (candidate : Candidate) (failure : ValidationFailure)
    (rejected : (requireAtBoundary store fuel target shape candidate).result =
      .rejected failure) :
    failure.fuel = fuel ∧ failure.target = target ∧ failure.shape = shape ∧
      failure.candidate = candidate ∧
      (requireAtBoundary store fuel target shape candidate).storeAfter = store := by
  cases checkResult : check fuel shape candidate.value with
  | false =>
      simp [requireAtBoundary, require, checkResult] at rejected
      cases rejected
      exact ⟨rfl, rfl, rfl, rfl, rfl⟩
  | true =>
      simp [requireAtBoundary, require, checkResult] at rejected

def settingsTarget : Target :=
  .record [
    .field "name" .string,
    .field "ports" (.list .integer),
    .field "retry" (.optional (.list .integer))
  ]

def settingsId : NominalId := ⟨"demo", "Settings"⟩
def settingsInputOrigin : RequireInput := .nominal ⟨"wire", "PartialSettings"⟩

def acceptedSettings : Candidate := ⟨
  .record [
    ("name", .string "api"),
    ("ports", .list [.integer intOne, .integer intOne]),
    ("retry", .list [.integer intOne]),
    ("note", .string "keep this extra field")
  ],
  settingsInputOrigin
⟩

def rejectedNestedSettings : Candidate := ⟨
  .record [
    ("name", .string "api"),
    ("ports", .list [.integer intOne, .string "not an integer"]),
    ("retry", .null),
    ("note", .string "keep this extra field")
  ],
  settingsInputOrigin
⟩

def missingOptionalSettings : Candidate := ⟨
  .record [
    ("name", .string "api"),
    ("ports", .list [.integer intOne])
  ],
  settingsInputOrigin
⟩

def duplicateNameSettings : Candidate := ⟨
  .record [
    ("name", .string "api"),
    ("name", .integer intOne),
    ("ports", .list [.integer intOne]),
    ("retry", .null)
  ],
  settingsInputOrigin
⟩

def emptyStore : Store := ⟨[]⟩
def settingsFuel : Nat := 8

theorem accepted_nested_record_list_and_optional_witness :
    ∃ checked,
      require settingsFuel settingsId settingsTarget acceptedSettings = .accepted checked ∧
      erase checked = acceptedSettings.value ∧
      checked.nominal.provenance = .require acceptedSettings.provenance := by
  have passes : check settingsFuel settingsTarget acceptedSettings.value = true := by
    decide
  refine ⟨⟨settingsFuel, settingsTarget,
    Nominal.introduceByRequire settingsId acceptedSettings.provenance
      acceptedSettings.value,
    passes⟩, ?_, rfl, rfl⟩
  exact dite_eq_left passes

theorem optional_null_is_accepted :
    check 1 (.optional .integer) .null = true := by
  decide

theorem missing_optional_field_is_rejected :
    check settingsFuel settingsTarget missingOptionalSettings.value = false := by
  decide

theorem unique_record_keys_with_extra_field_are_accepted :
    check settingsFuel settingsTarget acceptedSettings.value = true := by
  decide

theorem duplicate_record_key_with_valid_first_value_is_rejected :
    check settingsFuel settingsTarget duplicateNameSettings.value = false := by
  decide

theorem require_rejects (fuel : Nat) (target : NominalId) (shape : Target)
    (candidate : Candidate) (rejected : check fuel shape candidate.value = false) :
    require fuel target shape candidate = .rejected ⟨fuel, target, shape, candidate⟩ := by
  unfold require
  have notAccepted : ¬ check fuel shape candidate.value = true := by
    simp [rejected]
  exact dite_eq_right notAccepted

/-- A shallow mutation checks outer field presence but skips each field type. -/
def shallowRecordCheck (required : List Target)
    (actual : List (String × CompositeValue)) : Bool :=
  required.all (fun field =>
    match field with
    | .field name _ => (findField? name actual).isSome
    | _ => false)

def shallowOnlyMutation (target : Target) (value : CompositeValue) : Bool :=
  match target, value with
  | .integer, .integer _ => true
  | .string, .string _ => true
  | .optional _, .null => true
  | .optional _, _ => true
  | .list _, .list _ => true
  | .record required, .record actual => shallowRecordCheck required actual
  | _, _ => false

/-- A faulty checker that skips the duplicate-key guard for its outer record. -/
def omitDuplicateKeyGuard (fuel : Nat) (target : Target) (value : CompositeValue) : Bool :=
  match target, value with
  | .record required, .record actual =>
      requiredFieldsPass (checkFuel fuel) required actual
  | _, _ => check fuel target value

theorem duplicate_key_guard_detects_first_match_mutation :
    omitDuplicateKeyGuard settingsFuel settingsTarget duplicateNameSettings.value = true ∧
      check settingsFuel settingsTarget duplicateNameSettings.value = false ∧
      require settingsFuel settingsId settingsTarget duplicateNameSettings =
        .rejected ⟨settingsFuel, settingsId, settingsTarget, duplicateNameSettings⟩ := by
  refine ⟨by decide, duplicate_record_key_with_valid_first_value_is_rejected, ?_⟩
  exact require_rejects settingsFuel settingsId settingsTarget duplicateNameSettings
    duplicate_record_key_with_valid_first_value_is_rejected

theorem shallow_only_mutation_misses_nested_bad_item :
    shallowOnlyMutation settingsTarget rejectedNestedSettings.value = true ∧
      check settingsFuel settingsTarget rejectedNestedSettings.value = false ∧
      ∃ failure,
        require settingsFuel settingsId settingsTarget rejectedNestedSettings = .rejected failure := by
  refine ⟨by decide, by decide, ?_⟩
  refine ⟨⟨settingsFuel, settingsId, settingsTarget, rejectedNestedSettings⟩, ?_⟩
  exact require_rejects settingsFuel settingsId settingsTarget rejectedNestedSettings
    (by decide)

end Xsh.Validation.RecursiveRequire
