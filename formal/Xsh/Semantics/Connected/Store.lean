import Xsh.Semantics.Connected.CoreEmbedding

namespace Xsh.Semantics.Connected

open Xsh.Semantics.Environments

structure StoreTyped (type : RuntimeType) (value : Value) : Prop where
  connected : HasType type value
  core : ∀ coreType, CoreBridge.embedType type = some coreType →
    Xsh.Semantics.CoreWorld.HasType CoreBridge.world coreType (CoreBridge.embedValue value)

theorem StoreTyped.of_hasType {type : RuntimeType} {value : Value} (typed : HasType type value) :
    StoreTyped type value :=
  ⟨typed, fun _ embedded => CoreBridge.has_type_embeds typed embedded⟩

theorem storeTyped_iff (type : RuntimeType) (value : Value) : StoreTyped type value ↔ HasType type value :=
  ⟨StoreTyped.connected, StoreTyped.of_hasType⟩

instance (type : RuntimeType) (value : Value) : Decidable (StoreTyped type value) :=
  decidable_of_iff _ (storeTyped_iff type value).symm

abbrev Store := TypedStore RuntimeType Value StoreTyped

def write (store : Store) (cell : CellId) (candidate : Value) : StoreWriteResult Store :=
  writeCell (fun _ _ => inferInstance) store cell candidate

theorem write_rejects_wrong_type_atomically (store : Store) (cell : CellId)
    (candidate : Value) (current : StoreCell RuntimeType Value StoreTyped)
    (present : store cell = some current) (mutable : current.mutability = .mutable)
    (wrong : ¬ HasType current.contents.typeTag candidate) :
    (write store cell candidate).storeAfter = store := by
  exact Xsh.Semantics.Environments.write_rejects_nonmember_atomically (fun _ _ => inferInstance) store cell candidate
    current present mutable (fun typed => wrong typed.connected)

end Xsh.Semantics.Connected
