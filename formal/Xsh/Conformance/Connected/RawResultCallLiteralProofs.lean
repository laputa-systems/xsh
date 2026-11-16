import Xsh.Conformance.Connected.RawResultCall

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

set_option maxRecDepth 100000 in
theorem result_call_rows_decode_to_the_fixture :
    (parseCodes resultCallRawInstructionCodes).bind decodeRawInstructionRows = some resultCallRows := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem result_call_functions_decode_to_the_fixture :
    (parseCodes resultCallRawFunctionCodes).bind decodeRawFunctionRows = some resultCallFunctions := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem result_call_signatures_decode_to_the_fixture :
    decodeResultSignatures resultCallSignatureCodes = some resultCallSignatures := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem result_call_source_names_are_the_fixture_names :
    (match admitSource 0 resultCallSource with | .ok unit => decide (unit.names = resultCallSourceNames) | .error _ => false) = true := by
  decide +kernel

end Xsh.Conformance.Connected
