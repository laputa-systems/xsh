import Xsh.Core.Store

namespace Xsh.Core.ReferenceControls

open Xsh.Values
open Xsh.Semantics.Environments

private abbrev TestValue := Value Nat Nat Unit

private def membership : MembershipWorld Nat Nat Unit where
  entryFields _ := []
  stream _ _ := False
  callable _ _ := False
  scheme _ _ := False
  generic _ _ := False

private def world (policy : ReferencePolicy Nat) : Store.World Nat Nat Unit :=
  ⟨membership, policy⟩

private def decisions (policy : ReferencePolicy Nat) (choice : ∀ reference, Decidable (policy reference)) :
    Store.Decisions (world policy) where
  contracts := {
    stream := fun _ _ => isFalse id
    callable := fun _ _ => isFalse id
    scheme := fun _ _ => isFalse id
    generic := fun _ _ => isFalse id
  }
  references := choice

private def zero : ReferencePolicy Nat := fun reference => reference.2.identity = 0
private def zeroDecisions : Store.Decisions (world zero) := decisions zero fun _ => by unfold zero; infer_instance
private def processOnly : ReferencePolicy Nat := fun reference => reference.1 = .process ∧ zero reference
private def processDecisions : Store.Decisions (world processOnly) := decisions processOnly fun _ => by unfold processOnly zero; infer_instance

private def unit : TestValue := .scalar .unit
private def bad : TestValue := .processHandle ⟨1⟩
private def status (identity : Nat) : ProcessStatus Nat :=
  ⟨false, .exit, some intOne,
    [⟨0, [], some ⟨identity⟩, false, .exit, some intOne, none, none⟩]⟩

private def error (payload : List (String × TestValue)) (cause : Option TestValue) : TestValue :=
  .error ⟨0, "Failure", "failure", "", payload, [], none, [], cause, none⟩

private def processError (state : Option (ProcessStatus Nat)) (cause : Option TestValue) : TestValue :=
  .processError ⟨"process", "", none, state, [], cause⟩

private def direct : List TestValue := [
  .stream ⟨0⟩, .filesystemEntry ⟨0⟩, .module ⟨0⟩ [], .callable ⟨0⟩ .pure,
  .command ⟨0⟩, .processHandle ⟨0⟩, .networkJob ⟨0⟩, .filesystemRoot ⟨0⟩,
  .filesystemLock ⟨0⟩, .sshSession ⟨0⟩
]

private def nested : List TestValue := [
  .list [bad], .map [(.string "x", bad)], .record [("x", bad)], .ok bad, .err bad,
  .tagged 0 "Value" [bad], .nominal 0 bad, error [("x", bad)] none,
  error [] (some bad), error [] (some (error [] (some bad))),
  processError none (some bad), processError (some (status 1)) none,
  .status (status 1), .module ⟨0⟩ [("x", bad)]
]

theorem reference_controls :
    (direct.map (Store.check (world zero) zeroDecisions (.primitive .any)) = List.replicate 10 true) ∧
    (direct.map (Store.check (world processOnly) processDecisions (.primitive .any)) =
      [false, false, false, false, false, true, false, false, false, false]) ∧
    (nested.map (Store.check (world zero) zeroDecisions (.primitive .any)) = List.replicate 14 false) ∧
    (Store.check (world zero) zeroDecisions (.primitive .any)
      (processError (some (status 0)) (some (error [("x", .processHandle ⟨0⟩)] none))) = true) := by
  decide +kernel

private def cell : CellId := ⟨0, ⟨⟨0, by decide⟩⟩⟩

private def initial : Store.State (world zero) := fun candidate =>
  if candidate = cell then
    some ⟨⟨.primitive .any, unit,
      (Store.check_iff (world zero) zeroDecisions (.primitive .any) unit).mp (by decide +kernel)⟩, .mutable⟩
  else none

private def accepted : StoreWriteResult (Store.State (world zero)) → Bool
  | .accepted _ => true
  | .rejected _ => false

theorem store_controls :
    accepted (Store.write (world zero) zeroDecisions initial cell (.processHandle ⟨0⟩)) = true ∧
    accepted (Store.write (world zero) zeroDecisions initial cell (error [] (some bad))) = false ∧
    accepted (Store.write (world zero) zeroDecisions initial cell (.status (status 1))) = false := by
  decide +kernel

end Xsh.Core.ReferenceControls
