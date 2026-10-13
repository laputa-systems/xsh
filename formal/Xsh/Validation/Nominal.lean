import Xsh.Values.Composite

/-!
A finite fragment for nominal declaration identity, introduction provenance,
erasure, and union membership. A nominal value pairs a represented value with
type-level identity and provenance; erasure keeps only the represented value.

The fragment does not validate record schemas or recursive `.require` inputs,
model stores or resource identity, prove union exhaustiveness, or establish
source or Rust conformance. It is not a full G2 model.
-/

namespace Xsh.Validation.Nominal

open Xsh.Values

/-- A declaration identity includes its module, even when names are equal. -/
structure NominalId where
  moduleName : String
  declarationName : String
  deriving DecidableEq, Repr

/-- The input category retained when `.require` introduces a nominal type. -/
inductive RequireInput where
  | nonNominal
  | nominal (source : NominalId)
  deriving DecidableEq, Repr

/-- Provenance is indexed by the declaration being introduced. -/
inductive IntroductionProvenance (target : NominalId) where
  | constructor
  | require (input : RequireInput)
  deriving DecidableEq, Repr

/-- Type-level nominal evidence paired with an otherwise ordinary value. -/
structure NominalValue where
  nominalId : NominalId
  value : CompositeValue
  provenance : IntroductionProvenance nominalId

def introduceByConstructor (target : NominalId) (value : CompositeValue) :
    NominalValue :=
  ⟨target, value, .constructor⟩

def introduceByRequire (target : NominalId) (input : RequireInput)
    (value : CompositeValue) : NominalValue :=
  ⟨target, value, .require input⟩

/-- Erasure returns the base value and carries no nominal identity. -/
def erase (value : NominalValue) : CompositeValue := value.value

theorem constructor_introduction_retains_exact_provenance
    (target : NominalId) (value : CompositeValue) :
    (introduceByConstructor target value).nominalId = target ∧
      (introduceByConstructor target value).provenance = .constructor ∧
      (introduceByConstructor target value).value = value := by
  exact ⟨rfl, rfl, rfl⟩

theorem require_introduction_retains_exact_provenance
    (target : NominalId) (input : RequireInput) (value : CompositeValue) :
    (introduceByRequire target input value).nominalId = target ∧
      (introduceByRequire target input value).provenance = .require input ∧
      (introduceByRequire target input value).value = value := by
  exact ⟨rfl, rfl, rfl⟩

theorem erasure_preserves_underlying_value (value : NominalValue) :
    erase value = value.value := rfl

/-- A union alternative is selected by its nominal declaration identity. -/
structure NominalUnion where
  alternatives : List NominalId

def NominalUnion.accepts (union : NominalUnion) (value : NominalValue) : Prop :=
  value.nominalId ∈ union.alternatives

def packageFromNorth : NominalId := ⟨"north", "Package"⟩
def packageFromSouth : NominalId := ⟨"south", "Package"⟩

def sharedPackageRepresentation : CompositeValue :=
  .record [("id", .string "xsh"), ("version", .string "1")]

def constructedPackage : NominalValue :=
  introduceByConstructor packageFromNorth sharedPackageRepresentation

def requiredPackage : NominalValue :=
  introduceByRequire packageFromSouth (.nominal packageFromNorth)
    sharedPackageRepresentation

def overlappingPackageUnion : NominalUnion :=
  ⟨[packageFromNorth, packageFromSouth]⟩

def northPackageUnion : NominalUnion := ⟨[packageFromNorth]⟩
def southPackageUnion : NominalUnion := ⟨[packageFromSouth]⟩

theorem package_declaration_ids_are_distinct :
    packageFromNorth ≠ packageFromSouth := by
  decide

theorem nominal_witnesses_keep_identity_before_erasure :
    constructedPackage.nominalId ≠ requiredPackage.nominalId ∧
      constructedPackage.value = requiredPackage.value ∧
      constructedPackage ≠ requiredPackage := by
  refine ⟨package_declaration_ids_are_distinct, rfl, ?_⟩
  intro same
  have sameId := congrArg NominalValue.nominalId same
  exact package_declaration_ids_are_distinct sameId

theorem nominal_witnesses_erase_to_the_same_value :
    erase constructedPackage = erase requiredPackage ∧
      constructedPackage.nominalId ≠ requiredPackage.nominalId := by
  exact ⟨rfl, package_declaration_ids_are_distinct⟩

theorem erasure_is_not_injective :
    ∃ left right : NominalValue, left ≠ right ∧ erase left = erase right := by
  exact ⟨constructedPackage, requiredPackage,
    nominal_witnesses_keep_identity_before_erasure.2.2, rfl⟩

theorem overlapping_nominal_union_keeps_alternatives_distinct :
    overlappingPackageUnion.accepts constructedPackage ∧
      overlappingPackageUnion.accepts requiredPackage ∧
      ¬ southPackageUnion.accepts constructedPackage ∧
      ¬ northPackageUnion.accepts requiredPackage ∧
      constructedPackage.value = requiredPackage.value ∧
      constructedPackage.nominalId ≠ requiredPackage.nominalId := by
  simp [NominalUnion.accepts, overlappingPackageUnion, northPackageUnion,
    southPackageUnion, constructedPackage, requiredPackage,
    sharedPackageRepresentation, introduceByConstructor, introduceByRequire,
    packageFromNorth, packageFromSouth]

/-- A deliberately faulty union check that maps every identity to one ID. -/
def collapseNominalIdMutation (_ : NominalId) : NominalId := packageFromNorth

def acceptsAfterIdCollapse (union : NominalUnion) (value : NominalValue) : Prop :=
  collapseNominalIdMutation value.nominalId ∈ union.alternatives

theorem id_collapse_mutation_is_detected :
    collapseNominalIdMutation packageFromNorth =
        collapseNominalIdMutation packageFromSouth ∧
      acceptsAfterIdCollapse northPackageUnion requiredPackage ∧
      ¬ northPackageUnion.accepts requiredPackage := by
  refine ⟨rfl, ?_, ?_⟩
  · simp [acceptsAfterIdCollapse, northPackageUnion, requiredPackage,
      collapseNominalIdMutation, introduceByRequire,
      packageFromNorth, packageFromSouth]
  · simp [NominalUnion.accepts, northPackageUnion, requiredPackage,
      introduceByRequire, packageFromNorth, packageFromSouth]

/-- A faulty introduction step that overwrites the recorded origin. -/
def forgetProvenanceMutation {target : NominalId}
    (_ : IntroductionProvenance target) : IntroductionProvenance target :=
  .constructor

theorem provenance_erasure_mutation_is_detected :
    forgetProvenanceMutation requiredPackage.provenance = .constructor ∧
      forgetProvenanceMutation requiredPackage.provenance ≠
        requiredPackage.provenance := by
  decide

end Xsh.Validation.Nominal
