# Canonical run on 7b148

The pinned Linux musl run used main source `7b1485310abdf2190ce4532d840114e74ff7c2cd`, tree `0f3394902504cbe5271edd8916f504bd493f091a`.

The receipt records a clean source before and after the run. Cleanup resolved. The run ended `failed-or-incomplete` with exit 3.

Native tests reported 13,902 passed, 3 failed, and 78 skipped. Root Rust passed 747 tests with 3 ignored. Root integration passed 223. XSHT passed 116 with 5 filtered. Soundness passed 14.

Three native tests failed:

- GNU rm ext3perf exited 3 with empty stderr.
- uu-sort `buffer_sizes` timed out after 4 minutes and was canceled with signal 15.
- `test_system_report_pci_capture_replays_raw_identity_links_and_rejects_tampering` failed with EAGAIN, errno 11.

The logs preserve these failures. They do not establish the GNU rm or PCI failure causes.

A separate default-product restoration passed on the same source. It rebuilt all three products fresh. The restored `xsh` matches the bootstrap artifact, whose Cargo record reports `fresh=false`. This restoration does not change the failed canonical result. The receipt claims no timing credit.

The canonical receipt, full stdout and stderr, and container resources are copied byte-for-byte from the run directory. The bootstrap and default-product receipts and Cargo logs are also copied byte-for-byte.
