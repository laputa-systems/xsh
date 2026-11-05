import Xsh.Conformance.Connected.RawResultLiterals

namespace Xsh.Conformance.Connected
open Xsh.Foundation Xsh.Syntax Xsh.Typing.Connected Xsh.Conformance.Transport

set_option maxRecDepth 100000 in
theorem result_control_rows_decode_to_the_fixture :
    (parseCodes resultRawInstructionCodes).bind decodeRawInstructionRows = some resultRows := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem result_control_functions_decode_to_the_fixture :
    (parseCodes resultRawFunctionCodes).bind decodeRawFunctionRows = some resultFunctions := by
  decide +kernel

set_option maxRecDepth 100000 in
theorem result_control_signatures_decode_to_the_fixture :
    decodeResultSignatures resultSignatureCodes = some resultSignatures := by
  decide +kernel

end Xsh.Conformance.Connected
