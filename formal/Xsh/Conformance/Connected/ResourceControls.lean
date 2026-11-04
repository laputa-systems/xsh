import Xsh.Conformance.Connected.ResourceFacts
import Xsh.Conformance.Connected.ActualFacts
import Xsh.Conformance.Connected.ResourceDocuments

namespace Xsh.Conformance.Connected

open Xsh.Typing.Connected
open Xsh.Conformance.Transport

def projectionTransferControl (transfers wrongSite : Bool) (checkedOverride : Option Bool) : Bool :=
  match admitSource 0 projectionSource with
  | .ok unit =>
    match unit.declarations with
    | .function source :: _ =>
      let site := if wrongSite then { source.bodySite with start := source.bodySite.start + 1 } else source.bodySite
      checkFunctionBlockTransfer source projectionBodyFacts.result [⟨.blockValue, site, transfers, checkedOverride⟩]
    | _ => false
  | .error _ => false

set_option maxRecDepth 100000 in
set_option maxHeartbeats 0 in
theorem generic_projection_transfer_controls :
    projectionTransferControl true false none = true ∧
    projectionTransferControl false false none = false ∧
    projectionTransferControl true true none = false ∧
    projectionTransferControl true false (some false) = false ∧
    projectionTransferControl true false (some true) = false := by
  decide +kernel

def groundCallerTransferControl (checkedOverride : Option Bool) : Bool :=
  match admitSource 0 projectionSource with
  | .ok unit =>
    match unit.declarations with
    | _ :: .function source :: _ =>
      checkFunctionBlockTransfer source (.primitive .integer) [⟨.blockValue, source.bodySite, false, checkedOverride⟩]
    | _ => false
  | .error _ => false

set_option maxRecDepth 100000 in
set_option maxHeartbeats 0 in
theorem ground_caller_requires_published_override :
    groundCallerTransferControl (some false) = true ∧
    groundCallerTransferControl none = false ∧
    groundCallerTransferControl (some true) = false := by
  decide +kernel

def actualProjectionTransfers : List TransferFact :=
  [⟨.blockValue, ⟨0, 20, 16⟩, true, none⟩, ⟨.blockValue, ⟨0, 63, 39⟩, false, some false⟩]

def wrongProjectionTransfer : List TransferFact :=
  [⟨.blockValue, ⟨0, 20, 16⟩, false, none⟩, ⟨.blockValue, ⟨0, 63, 39⟩, false, some false⟩]

def wrongProjectionOverride : List TransferFact :=
  [⟨.blockValue, ⟨0, 20, 16⟩, true, some false⟩, ⟨.blockValue, ⟨0, 63, 39⟩, false, some false⟩]

def missingCallerOverride : List TransferFact :=
  [⟨.blockValue, ⟨0, 20, 16⟩, true, none⟩, ⟨.blockValue, ⟨0, 63, 39⟩, false, none⟩]

set_option maxRecDepth 100000 in
set_option maxHeartbeats 0 in
theorem actual_resource_transfer_bytes_decode :
    (parseCodes resourceFactsCodes).bind decodeTransferFacts = some actualProjectionTransfers := by
  decide +kernel

set_option maxRecDepth 100000 in
set_option maxHeartbeats 0 in
theorem wrong_resource_bit_bytes_decode :
    (parseCodes resourceBadBitCodes).bind decodeTransferFacts = some wrongProjectionTransfer := by
  decide +kernel

set_option maxRecDepth 100000 in
set_option maxHeartbeats 0 in
theorem wrong_resource_override_bytes_decode :
    (parseCodes resourceBadOverrideCodes).bind decodeTransferFacts = some wrongProjectionOverride := by
  decide +kernel

set_option maxRecDepth 100000 in
set_option maxHeartbeats 0 in
theorem missing_resource_override_bytes_decode :
    (parseCodes resourceMissingOverrideCodes).bind decodeTransferFacts = some missingCallerOverride := by
  decide +kernel

def projectionResourceChecks (facts : List TransferFact) : Bool :=
  match admitSource 0 projectionSource with
  | .ok unit =>
    match unit.declarations with
    | .function source :: .function caller :: _ =>
      checkFunctionBlockTransfer source projectionBodyFacts.result facts &&
        checkFunctionBlockTransfer caller (.primitive .integer) facts
    | _ => false
  | .error _ => false

def checkProjectionResourceBytes (codes : ExchangeCodes) : Bool :=
  match (parseCodes codes).bind decodeTransferFacts with
  | some facts => projectionResourceChecks facts
  | none => false

set_option maxRecDepth 100000 in
set_option maxHeartbeats 0 in
theorem actual_resource_transfer_bytes_are_checked :
    checkProjectionResourceBytes resourceFactsCodes = true ∧
    checkProjectionResourceBytes resourceBadBitCodes = false ∧
    checkProjectionResourceBytes resourceBadOverrideCodes = false ∧
    checkProjectionResourceBytes resourceMissingOverrideCodes = false := by
  unfold checkProjectionResourceBytes
  rw [actual_resource_transfer_bytes_decode, wrong_resource_bit_bytes_decode, wrong_resource_override_bytes_decode,
    missing_resource_override_bytes_decode]
  decide +kernel

end Xsh.Conformance.Connected
