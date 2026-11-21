import Xsh.Conformance.Vertical.SourceAdmissionBase
import Xsh.Conformance.Vertical.RecordLocalLowering

namespace Xsh.Conformance.Vertical.SourceAdmission

set_option maxRecDepth 10000 in
theorem record_bytes_admitted :
    check (witnessBytes recordLocalSourceDocument.text) [] recordLocalFunctions
      [[("value", 0), ("box", 2)]] = true := by
  decide +kernel

theorem parse_refusal_rejects_candidates (text : String)
    (refused : parseSourceCodes ((witnessBytes text).map UInt8.toNat) = none)
    (functions : List SourceFunction) (scopes : List (List (String × Nat))) :
    check (witnessBytes text) [] functions scopes = false := by
  simp [check, refused]

set_option maxRecDepth 10000 in
theorem record_update_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {...value, low.x: value}\n  return 0\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_spread_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {...value, low: value}\n  return 0\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_nested_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {low: {inside: value}}\n  return box.low\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_empty_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {}\n  return 0\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_punned_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {value}\n  return 0\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_quoted_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {\"low\": value}\n  return 0\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_keyword_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {while: value}\n  return 0\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_repeated_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {low: value, low: value}\n  return box.low\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_unknown_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {low: value}\n  return box.high\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_return_record_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {low: value}\n  return box\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_pass_record_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {low: value}\n  return g(box)\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_parameter_field_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {low: value}\n  return value.low\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_chained_field_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {low: value}\n  return box.low.x\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_var_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  var box = {low: value}\n  return box.low\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_annotation_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box: T = {low: value}\n  return box.low\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_computed_neighbor_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let extra = value + 1\n  let box = {low: value}\n  return box.low\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_two_parameters_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int, other: Int) -> Int {\n  let box = {low: value}\n  return box.low\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_caller_refused :
    parseSourceCodes ((witnessBytes "pure f(value: Int) -> Int {\n  let box = {low: value}\n  return box.low\n}\n\npure g(value: Int) -> Int {\n  return f(value)\n}\n").map UInt8.toNat) = none := by
  decide +kernel

set_option maxRecDepth 10000 in
theorem record_forward_caller_refused :
    parseSourceCodes ((witnessBytes ("pure g(value: Int) -> Int {\n  return f(value)\n}\n\n" ++
      "pure f(value: Int) -> Int {\n  let box = {low: value}\n  return box.low\n}\n")).map UInt8.toNat) = none := by
  decide +kernel

end Xsh.Conformance.Vertical.SourceAdmission
