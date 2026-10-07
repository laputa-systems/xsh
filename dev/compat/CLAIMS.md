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
