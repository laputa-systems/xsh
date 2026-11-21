import Xsh.Semantics.Connected.Machine
import Xsh.Semantics.Connected.CoreEmbedding
import Std.Data.String.ToNat

namespace Xsh.Semantics.Connected.CoreBridge

open Xsh.Foundation
open Xsh.Values
open Xsh.Syntax.Foundation
open Xsh.Validation.Nominal
open Xsh.Semantics.Environments

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
  have coreTyped := typed.core _ sameType
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
