namespace Xsh.Validation.IdentityStorage

abbrev ValidationIdentity := Nat
abbrev ResourceIdentity := Nat
abbrev VariableIdentity := Nat
abbrev PathBytes := List Nat

inductive CollectionKind where
  | list
  | stream
  deriving DecidableEq, Repr

inductive XshType where
  | any
  | text
  | path
  | validated (identity : ValidationIdentity) (base : XshType)
  | collection (kind : CollectionKind) (element : XshType)
  | set (element : XshType)
  | map (key value : XshType)
  | optional (value : XshType)
  | result (value error : XshType)
  deriving DecidableEq, Repr

def baseView : XshType → XshType → Bool
  | .validated sourceId sourceBase, .validated targetId targetBase =>
      decide (sourceId = targetId ∧ sourceBase = targetBase)
  | .validated _ sourceBase, target => baseView sourceBase target
  | .collection sourceKind sourceElement, .collection targetKind targetElement =>
      decide (sourceKind = targetKind) && baseView sourceElement targetElement
  | .set sourceElement, .set targetElement =>
      decide (sourceElement = targetElement)
  | .map sourceKey sourceValue, .map targetKey targetValue =>
      decide (sourceKey = targetKey) && baseView sourceValue targetValue
  | .optional sourceValue, .optional targetValue =>
      baseView sourceValue targetValue
  | .result sourceValue sourceError, .result targetValue targetError =>
      baseView sourceValue targetValue && baseView sourceError targetError
  | source, target => decide (source = target)

def optionalInjectionMatches (source : XshType) : XshType → Bool
  | .optional value =>
      match source with
      | .any | .optional _ => false
      | _ => baseView source value || decide (value = .any) || optionalInjectionMatches source value
  | _ => false

def assignable (source target : XshType) : Bool :=
  decide (target = .any) || baseView source target || optionalInjectionMatches source target

inductive RuntimeElement where
  | text (value : String)
  | path (bytes : PathBytes)
  | resource (identity : ResourceIdentity)
  deriving DecidableEq, Repr

inductive RuntimeValue where
  | text (value : String)
  | path (bytes : PathBytes)
  | resource (identity : ResourceIdentity)
  | list (values : List RuntimeElement)
  deriving DecidableEq, Repr

structure Binding where
  declared : XshType
  value : RuntimeValue
  deriving DecidableEq, Repr

abbrev Store := VariableIdentity → Option Binding

def writeBinding (store : Store) (identity : VariableIdentity) (binding : Binding) : Store :=
  fun candidate => if candidate = identity then some binding else store candidate

def widenBinding (source : Binding) (target : XshType)
    (_eligible : assignable source.declared target = true) : Binding :=
  ⟨target, source.value⟩

def persistentView (store : Store) (sourceId targetId : VariableIdentity)
    (source : Binding) (_present : store sourceId = some source)
    (_different : sourceId ≠ targetId)
    (target : XshType) (eligible : assignable source.declared target = true) : Store :=
  writeBinding store targetId (widenBinding source target eligible)

def persistentUpdate (store : Store) (targetId : VariableIdentity)
    (current : Binding) (_present : store targetId = some current)
    (replacement : RuntimeValue) : Store :=
  writeBinding store targetId { current with value := replacement }

def replacePathAfterResource : RuntimeValue → PathBytes → Option RuntimeValue
  | .list (.resource identity :: .path _ :: rest), path =>
      some (.list (.resource identity :: .path path :: rest))
  | _, _ => none

def leadingResourceIdentity? : RuntimeValue → Option ResourceIdentity
  | .list (.resource identity :: _) => some identity
  | _ => none

def persistentPathUpdate (store : Store) (sourceId targetId : VariableIdentity)
    (source : Binding) (present : store sourceId = some source)
    (different : sourceId ≠ targetId) (target : XshType)
    (eligible : assignable source.declared target = true) (path : PathBytes) :
    Option Store :=
  match replacePathAfterResource source.value path with
  | none => none
  | some replacement =>
      let viewed := persistentView store sourceId targetId source present different target eligible
      some (persistentUpdate viewed targetId (widenBinding source target eligible)
        (by simp [viewed, persistentView, writeBinding]) replacement)

def replacePathAfterResourceChangesHandle : RuntimeValue → PathBytes → Option RuntimeValue
  | .list (.resource identity :: .path _ :: rest), path =>
      some (.list (.resource (identity + 1) :: .path path :: rest))
  | _, _ => none

def disablePersistentUpdateMutation (store : Store) (_ : VariableIdentity)
    (_ : Binding) (_ : RuntimeValue) : Store :=
  store

/-- A validated target accepts only the same validation identity and base. -/
theorem same_validation_identity_assigns (identity : ValidationIdentity)
    (base : XshType) :
    assignable (.validated identity base) (.validated identity base) = true := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- Distinct validated declarations stay distinct even when their base types match. -/
theorem distinct_validations_do_not_assign (sourceId targetId : ValidationIdentity)
    (base : XshType) (different : sourceId ≠ targetId) :
    assignable (.validated sourceId base) (.validated targetId base) = false := by
  simp [assignable, baseView, optionalInjectionMatches, different]

/-- A validated collection has the same structural view as its declared base. -/
theorem validated_list_has_its_base_view (identity : ValidationIdentity) :
    assignable (.validated identity (.collection .list .path))
      (.collection .list .path) = true := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- Structural views preserve the container while widening a nested validated element. -/
theorem nested_validated_element_has_base_view (identity : ValidationIdentity) :
    assignable (.collection .list (.validated identity .path))
      (.collection .list .path) = true := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- Set element types remain invariant even when a validation exposes a base. -/
theorem validated_set_element_widening_is_rejected (identity : ValidationIdentity) :
    assignable (.set (.validated identity .path)) (.set .path) = false := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- Map keys remain invariant when a validation exposes a base. -/
theorem validated_map_key_widening_is_rejected (identity : ValidationIdentity) :
    assignable (.map (.validated identity .path) .text) (.map .path .text) = false := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- Map values may use an eligible validated base view while keys stay fixed. -/
theorem validated_map_value_has_base_view (identity : ValidationIdentity) :
    assignable (.map .text (.validated identity .path)) (.map .text .path) = true := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- The base view may continue through optional and result structure. -/
theorem validated_result_field_has_base_view (identity : ValidationIdentity) :
    assignable (.result (.optional (.validated identity .path)) .text)
      (.result (.optional .path) .text) = true := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- A non-Optional, non-Any source can be injected when it matches the target type. -/
theorem optional_injection_preserves_assignability {source target : XshType}
    (sourceNotOptional : ∀ value, source ≠ .optional value)
    (sourceNotAny : source ≠ .any)
    (eligible : assignable source target = true) :
    assignable source (.optional target) = true := by
  have injectionMatches :
      optionalInjectionMatches source (.optional target) = assignable source target := by
    cases source <;>
      simp_all [optionalInjectionMatches, assignable, Bool.or_assoc, Bool.or_comm]
  have targetNotAny : decide ((.optional target : XshType) = .any) = false := rfl
  simp only [assignable, targetNotAny, Bool.false_or]
  rw [injectionMatches]
  simp [eligible]

/-- A validated path reaches Optional[Path] through its path base. -/
theorem validated_path_injects_optional (identity : ValidationIdentity) :
    assignable (.validated identity .path) .path = true ∧
      baseView (.validated identity .path) (.optional .path) = false ∧
      assignable (.validated identity .path) (.optional .path) = true := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- Optional Path does not widen to Optional Any, and Any does not inject into Optional Any. -/
theorem optional_storage_widening_and_any_injection_are_rejected :
    assignable (.optional .path) (.optional .any) = false ∧
      assignable .any (.optional .any) = false := by
  simp [assignable, baseView, optionalInjectionMatches]

/-- Ordinary collection parameters remain invariant when no validation base is exposed. -/
theorem collection_parameter_invariance_remains (kind : CollectionKind) :
    assignable (.collection kind .path) (.collection kind .any) = false := by
  cases kind <;> simp [assignable, baseView, optionalInjectionMatches]

/-- Every concrete source has the dynamic `Any` view. -/
theorem concrete_type_fits_any (source : XshType) :
    assignable source .any = true := by
  simp [assignable, optionalInjectionMatches]

/-- A dynamic `Any` source does not acquire a concrete type implicitly. -/
theorem any_does_not_fit_path : assignable .any .path = false := by
  rfl

/-- Updating a widened binding leaves a distinct narrower binding in the old store unchanged. -/
theorem persistent_view_update_preserves_narrow_binding
    (store : Store) (sourceId targetId : VariableIdentity) (source : Binding)
    (present : store sourceId = some source) (target : XshType)
    (eligible : assignable source.declared target = true) (replacement : RuntimeValue)
    (different : sourceId ≠ targetId) :
    persistentUpdate
      (persistentView store sourceId targetId source present different target eligible)
      targetId (widenBinding source target eligible)
      (by simp [persistentView, writeBinding]) replacement sourceId =
      some source := by
  simp [persistentUpdate, persistentView, writeBinding, present, different]

/-- A successful sibling-path update commits its payload and leaves the narrow source binding intact. -/
theorem persistent_path_update_commits_and_preserves_source
    (store : Store) (sourceId targetId : VariableIdentity) (source : Binding)
    (present : store sourceId = some source) (different : sourceId ≠ targetId)
    (target : XshType) (eligible : assignable source.declared target = true)
    (path : PathBytes) (replacement : RuntimeValue)
    (pathResult : replacePathAfterResource source.value path = some replacement) :
    ∃ updated, persistentPathUpdate store sourceId targetId source present different
        target eligible path = some updated ∧
      updated targetId = some { widenBinding source target eligible with value := replacement } ∧
      updated sourceId = some source := by
  refine ⟨persistentUpdate
    (persistentView store sourceId targetId source present different target eligible)
    targetId (widenBinding source target eligible)
    (by simp [persistentView, writeBinding]) replacement, ?_, ?_, ?_⟩
  · simp [persistentPathUpdate, pathResult]
  · simp [persistentUpdate, writeBinding]
  · simp [persistentUpdate, persistentView, writeBinding, present, different]

/-- A persistent update writes the supplied payload at the selected binding. -/
theorem persistent_update_commits_selected_payload (store : Store)
    (targetId : VariableIdentity) (current : Binding)
    (present : store targetId = some current) (replacement : RuntimeValue) :
    persistentUpdate store targetId current present replacement targetId =
      some { current with value := replacement } := by
  simp [persistentUpdate, writeBinding]

/-- A base view preserves the exact runtime payload, including embedded resource identities. -/
theorem widened_binding_preserves_payload (source : Binding) (target : XshType)
    (eligible : assignable source.declared target = true) :
    (widenBinding source target eligible).value = source.value := rfl

/-- Replacing a sibling path retains the embedded resource handle's identity. -/
theorem path_update_preserves_resource_identity (identity : ResourceIdentity)
    (oldPath newPath : PathBytes) (rest : List RuntimeElement) :
    replacePathAfterResource (.list (.resource identity :: .path oldPath :: rest)) newPath =
        some (.list (.resource identity :: .path newPath :: rest)) ∧
      leadingResourceIdentity?
        (.list (.resource identity :: .path newPath :: rest)) = some identity := by
  exact ⟨rfl, rfl⟩

def nonEmptyAnyIdentity : ValidationIdentity := 1
def resourceWitnessIdentity : ResourceIdentity := 17
def narrowVariableIdentity : VariableIdentity := 1
def baseVariableIdentity : VariableIdentity := 2
def nonEmptyAnyType : XshType :=
  .validated nonEmptyAnyIdentity (.collection .list .any)
def anyListType : XshType := .collection .list .any
def originalValue : RuntimeValue := .list [
  .resource resourceWitnessIdentity,
  .path [1]
]
def originalBinding : Binding := ⟨nonEmptyAnyType, originalValue⟩
def emptyStore : Store := fun _ => none
def narrowStore : Store := writeBinding emptyStore narrowVariableIdentity originalBinding
def widenedStore : Store := persistentView narrowStore narrowVariableIdentity
  baseVariableIdentity originalBinding (by simp [narrowStore, writeBinding]) (by decide)
  anyListType (by decide)
def updatedValue : RuntimeValue := .list [
  .resource resourceWitnessIdentity,
  .path [2]
]
def baseViewBinding : Binding := ⟨anyListType, originalValue⟩
def updatedBinding : Binding := ⟨anyListType, updatedValue⟩
def updatedStore : Store := (persistentPathUpdate narrowStore narrowVariableIdentity
  baseVariableIdentity originalBinding (by simp [narrowStore, writeBinding]) (by decide)
  anyListType (by decide) [2]).get (by decide)
def changedHandleValue : RuntimeValue :=
  (replacePathAfterResourceChangesHandle originalValue [2]).get (by decide)
def disabledUpdateStore : Store :=
  disablePersistentUpdateMutation widenedStore baseVariableIdentity updatedBinding updatedValue

/-- A concrete path update changes the widened payload and preserves its source binding and handle. -/
theorem persistent_resource_update_witness :
    assignable nonEmptyAnyType anyListType = true ∧
      replacePathAfterResource originalValue [2] = some updatedValue ∧
      widenedStore narrowVariableIdentity = some originalBinding ∧
      updatedStore narrowVariableIdentity = some originalBinding ∧
      updatedStore baseVariableIdentity = some updatedBinding ∧
      originalValue ≠ updatedValue ∧
      leadingResourceIdentity? updatedValue = some resourceWitnessIdentity := by
  decide

/-- Changing a resource identity during a sibling-path update is detected. -/
theorem changed_resource_identity_mutation_is_detected :
    replacePathAfterResourceChangesHandle originalValue [2] = some changedHandleValue ∧
      leadingResourceIdentity? changedHandleValue = some (resourceWitnessIdentity + 1) ∧
      leadingResourceIdentity? changedHandleValue ≠ some resourceWitnessIdentity := by
  decide

/-- Disabling the persistent update is detected by the missing widened payload. -/
theorem disabled_persistent_update_mutation_is_detected :
    disabledUpdateStore baseVariableIdentity = some baseViewBinding ∧
      updatedStore baseVariableIdentity = some updatedBinding ∧
      some baseViewBinding ≠ some updatedBinding := by
  decide

end Xsh.Validation.IdentityStorage
