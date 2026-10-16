# Procfs exited-state race fix review

**GO** for `3646630bb5d6e0f406b63676cf828091be6dd9b3`, reviewed against `ba14ae8e41251f6e4d8092773a992c435141d86d`. The worktree is clean at the requested commit. I reviewed `src/modules/process.rs` and the `sample_processes` call in `src/modules/linux/sampling.rs`; no tests or builds were run.

`LinuxStatParse` now separates `Exited`, `Valid`, and `Malformed` (`process.rs:1351-1356`). Parsing first requires the existing parenthesized command and a state token. It returns `Exited` only for the exact token `X` (`1360-1375`), before parsing numeric fields. Every other state still goes through the prior field extraction, overflow checks, and nonnegative-counter validation (`1376-1402`); invalid/missing fields remain `Malformed`.

The observed 133-byte row at `.work/tmp/procps-stat-observe/standalone-stat-malformed.bin` exactly matches the test's `EXITED_STAT` constant. It has state `X`, pgrp `-1`, and session `-1`; pgrp and session map to counters 7 and 8, which remain rejected by the existing negative-counter guard. The exact-X early return therefore handles this disappearing-process snapshot without relaxing validation for live records.

Caller behavior is bounded and consistent:

- `sample_processes` skips `Exited`, emits `linux-sample` for `Malformed`, and accepts `Valid` (`process.rs:1441-1451`). Its caller propagates the result with `?` (`modules/linux/sampling.rs:127`). Stat read errors still skip only NotFound/ESRCH and PermissionDenied; other errors retain host-error propagation (`1436-1440`). Metadata still skips only vanished processes (`1446-1449`).
- `LinuxThreadStream` and `linux_process_record` accept only `Valid` and skip the other variants (`274-276`, `1284-1288`), matching their prior `Option`-parse behavior. The inventory and port-owner callers continue to skip a missing record as before.

The focused source tests cover the captured X row returning no records (`2360-2367`), the same negative pgrp/session values with live state `S` remaining an error (`2370-2378`), and a valid live row producing one record (`2381-2389`). Existing tests also cover ESRCH disappearance and unexpected I/O propagation, including EISDIR (`2334-2357`, `2392-2408`). There is no reread/recheck of procfs and no general malformed-record fallback.
