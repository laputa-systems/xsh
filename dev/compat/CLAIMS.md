# Lane claims

The campaign resumed on 2026-10-07. The integrator owns shared APIs, generated
reports, docs, and merges. The following 16 disjoint lanes completed their
assigned applet and test-file scopes; their feature commits are integrated at
`fabe52f6`. This table records that closed wave's ownership; during the wave,
each lane stayed within its scope and did not delegate.

| Lane | Branch | Worktree | Owned scope |
|---|---|---|---|
| `printf-env` | `campaign/resume/printf-env` | `../xsh-resume-lanes/printf-env` | `core/printf.xsh`, `core/env.xsh`, matching tests |
| `text-b1` | `campaign/resume/text-b1` | `../xsh-resume-lanes/text-b1` | `uniq`, `comm`, `join`, `split`, `csplit`, `tsort`, `ptx`, `shuf` and matching tests |
| `text-a1` | `campaign/resume/text-a1` | `../xsh-resume-lanes/text-a1` | `cat`, `tac`, `tee`, `head`, `tail`, `rev` and matching tests |
| `cp` | `campaign/resume/cp` | `../xsh-resume-lanes/cp` | `core/cp.xsh` and its tests |
| `fs-basic` | `campaign/resume/fs-basic` | `../xsh-resume-lanes/fs-basic` | `rm`, `rmdir`, `mkdir`, `touch` and matching tests |
| `fs-misc` | `campaign/resume/fs-misc` | `../xsh-resume-lanes/fs-misc` | `mknod`, `mkfifo`, `mktemp`, `truncate`, `shred`, `sync`, `readlink`, `realpath`, `pwd`, `dirname`, `pathchk`, `basename` and matching tests |
| `ls` | `campaign/resume/ls` | `../xsh-resume-lanes/ls` | `ls`, `dir`, `vdir` and matching tests |
| `mv-ln` | `campaign/resume/mv-ln` | `../xsh-resume-lanes/mv-ln` | `mv`, `ln`, `link`, `unlink`, `install` and matching tests |
| `perm` | `campaign/resume/perm` | `../xsh-resume-lanes/perm` | `chmod`, `chown`, `chgrp`, `chroot` and matching tests |
| `stat-du-df` | `campaign/resume/stat-du-df` | `../xsh-resume-lanes/stat-du-df` | `stat`, `du`, `df` and matching tests |
| `proc-a` | `campaign/resume/proc-a` | `../xsh-resume-lanes/proc-a` | `kill`, `nice`, `nohup`, `timeout`, `stdbuf` and matching tests |
| `date` | `campaign/resume/date` | `../xsh-resume-lanes/date` | `date`, `dircolors` and matching tests |
| `bytes-enc` | `campaign/resume/bytes-enc` | `../xsh-resume-lanes/bytes-enc` | `base32`, `base64`, `basenc`, `od`, `dd`, `core/lib/bytes_enc.xsh`, `core/lib/bytes_enc_dd.xsh` and matching tests |
| `checksums` | `campaign/resume/checksums` | `../xsh-resume-lanes/checksums` | checksum applets, `core/lib/checksums.xsh` and matching tests |
| `awk` | `campaign/resume/awk` | `../xsh-resume-lanes/awk` | `core/awk.xsh`, `core/lib/awk.xsh`, and tests |
| `sed` | `campaign/resume/sed` | `../xsh-resume-lanes/sed` | `core/sed.xsh`, `core/lib/sed.xsh`, and tests |

Every lane uses `gpt-6-luna` at `xhigh`. Command semantics, parsers, and
formatting stay in XSH. Rust changes require an exact reusable OS or byte
boundary request to the integrator. Dependency additions are approved for this
campaign when a reusable primitive has no local equivalent.
Lanes report counts from their own uutils slices and focused native tests,
commit locally on their assigned branch, and never push or merge.

## Longer follow-up wave (2026-10-07)

The same 16 lanes completed disjoint follow-up tasks from `4273cc46`. Their
commits were integrated on `master` after the sort merge work; this records the
source commits for that wave.

| Lane | Commit | Lane | Commit |
|---|---|---|---|
| `awk` | `13fd852a` | `sed` | `630e8467` |
| `date` | `944dc86b` | `fs-basic` | `a724156a` |
| `fs-misc` | `dfcdbd59` | `bytes-enc` | `fbd253c6` |
| `text-a1` | `0c0694bc` | `text-b1` | `5cb5de6c` |
| `cp` | `9ca90c8f` | `proc-a` | `858eeb11` |
| `perm` | `365c4d65` | `stat-du-df` | `2133dd89` |
| `mv-ln` | `7c711270` | `ls` | `cd7d5e10` |
| `printf-env` | `b491bc05` | `checksums` | `0eb21496` |

## Focused continuation (2026-10-07)

Owners with remaining work continued from `20560ca4` within their existing
disjoint module scopes. These mainline commits integrate that continuation;
the integrator owns the runtime cleanup, signal, and stdin corrections.

| Lane | Integrated commits |
|---|---|
| `sort` | `d0fefec5`, `8ec420e0` |
| `integrator` | `89c4e143` |
| `date` | `8c386769` |
| `bytes-enc` | `e673e911` |
| `fs-basic` | `4ffb5f90` |
| `fs-misc` | `4693e0db`, `d2a922c1` |
| `cp` | `dcd04e2e` |
| `stat-du-df` | `78d5225c`, `d7e9b2c3` |
| `mv-ln` | `56cc795c` |
| `text-a1` | `88c41063` |
| `text-a2` | `077cda3e`, `ee72efda`, `05b457c6`, `4819a6c2`, `dfab7454`, `4a5210df` |
| `text-b1` | `1233171b` |
| `text-b2` | `97171c3f` |
| `printf-env` | `a211cee4`, `deff787c` |

## Native port continuation (2026-10-11, Codex)

Integration is isolated on `codex/compat-native` in `../xsh-codex-compat`,
starting at `96bf2dc3`. The primary checkout and existing Claude worktrees
are read-only. Every subagent uses `gpt-6-luna` with `xhigh` reasoning.
The owner confirmed original GNU and BusyBox behavioral tests with origin
IDs, without copying GPL script text. No changes are pushed.

| Lane | Owned scope |
|---|---|
| `port_tooling` | `dev/compat/port/lanes.py` and focused generator tests |
| `retirement_audit` | `dev/compat/port/suite_sources.py`, after coordinating the generator interface |
| `pilot_review` | pilot/helper audit; `dev/compat/port/oracle_port.py` and its focused tests |
| `port_ratchet` | `dev/compat/port/check_port.py`, focused tests; read-only imported-helper lowering investigation |
| integrator | shared test helper, exceptions, claims, campaign status and integration |

The full native baseline runs first in `xsh-test`, UID 1000, tmpfs `/tmp`,
with three jobs. Pilot arch, cat and tr ports precede the full transcription
fan-out. Script lanes share the existing release binaries read-only.
