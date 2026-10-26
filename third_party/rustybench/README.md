# Rustybench library

This is the library source from the existing `rustybench` 0.1.0 dependency.
XSH's interactive benchmarks use its timing, allocation and report contracts.
The report derives and JSON rendering use the in-tree Deser packages so the
workspace no longer needs miniserde. The standalone CLI and its fixtures are
omitted. The existing external benchmark driver remains the command owner.

When updating, preserve the schema-1 report fields and run the report round-trip
test and the interactive benchmark build. Retain the MIT license.
