# XSH Core Compatibility Campaign

Status: Wave 0 in progress on branch `campaign-utils`. Lane strategy and the
integrator protocol are in [`LANES.md`](LANES.md); harness usage is in
[`README.md`](README.md).

## Mission

Make XSH core a credible native Unix/Linux userspace compatibility substrate,
so a Laputa system with XSH installed has an unusually complete standard
command environment without shipping GNU coreutils, util-linux, procps-ng,
kmod, pciutils, usbutils, findutils or grep.

1. Match the complete practical Linux command surface of `uutils/coreutils`
   with comparable behavioral compatibility.
2. Measure that with uutils' own integration, GNU and BusyBox suites rather
   than a bespoke suite.
3. Extend to the practical util-linux, procps-ng, kmod, pciutils, usbutils,
   findutils and grep surface.
4. Implement commands as thin XSH applets over typed native APIs; add Rust
   primitives where syscalls, byte semantics, performance or kernel
   interfaces demand it.
5. Never fake a command: an accepted option works, and unsupported behavior
   fails explicitly.

Two constraints govern every decision:

> Laputa should not need dozens of tiny Unix packages merely because
> conventional software expects familiar executable names.

> Do not reproduce BusyBox's worst failure mode: a command name present while
> important semantics quietly do nothing.

## Architecture

### Two interfaces

- **Native XSH APIs** (`fs`, `process`, `unix`, `linux`, `system`, `hash`,
  `compression`, `archive`, `dns`, `net`, `cli`, ...) expose semantics as typed
  records and operations. They live in `src/modules/` and are documented
  through the registry behind `xsht api`.
- **Compatibility CLIs** (`core/*.xsh`) translate conventional argv into those
  APIs and print conventional output.

One compatibility command never parses another's text output. Shared data
flows through typed collectors:

```text
BAD:   lsusb -> run system-report -> parse human output
GOOD:  typed USB inventory -> { system-report, lsusb }
```

The same holds for PCI, block devices, processes, network state, mounts and
modules. `system-report`'s collectors (`core/lib/system_report_*.xsh`, about
11.7k lines) are extracted into reusable typed modules without weakening its
bounded reads, redaction, safety, or capture/replay guarantees. Presentation
commands may show local detail by default where convention expects it;
`system-report`'s share-safe redaction stays its own policy.

### Shared applet foundation (revised: Wave 0, before any fan-out)

`core/README.md` currently keeps applets self-contained and usage errors
local. The compatibility campaign needs one exception, because uutils and GNU
tests assert exact option grammar, diagnostics and exit codes, and ninety
applets must not each reinvent them:

- **GNU option grammar in `cli`** (native, `src/modules/cli.rs`). Today
  `cli.applet`/`cli.parse` reject repeated options (`duplicate argument at
  argv[N]`; GNU `ls -l -l` is valid, last value wins), do not resolve
  unambiguous long-option abbreviations (`--all` from `--al`), and report
  errors in XSH style. Add a GNU mode: short-option bundling, attached and
  separate values, optional arguments (`--color[=WHEN]`), `--opt=value`,
  abbreviation with ambiguity errors, argument permutation with
  `POSIXLY_CORRECT` and a leading `+` disabling it, `--`, repeated options,
  numeric-option shapes (`head -5`, `nice -n -5`), and an explicit
  `unsupported` declaration that fails with a clear diagnostic.
- **GNU diagnostics and exit codes** (`core/lib/gnu.xsh` plus any native
  support). `PROG: message`, `PROG: cannot access 'X': No such file or
  directory` (with GNU quoting of names), `Try 'PROG --help' for more
  information.`, per-utility usage-error exit status (1 for most, 2 for `ls`,
  `cmp`, `diff`, `grep`; 125 for `env`, `nice`, `nohup`, `timeout`,
  `stdbuf`, `chroot`), `--help`/`--version` handling, and SIGPIPE/EPIPE
  behavior on stdout.
- **Invoked-name access** for applets. Aliases (`dir`, `vdir`, `[`, `egrep`,
  `gunzip`, `zcat`, ...) are installed as symlinks; the applet reads the name
  it was invoked as (the script path the kernel passes) to choose defaults,
  help and diagnostics. Expose this cleanly in the runtime if it is not
  already available.
- **Byte-stream helpers** so binary data never passes through `Str`.

`core/README.md` is updated to describe this shared layer as an audited
command family, matching its existing rule for `lib/auth.xsh` and
`lib/text_input.xsh`.

## Phase 0: upstream baseline (done)

- Pinned uutils/coreutils `e7c9f3194280835c4487c2945c68d5f01ccacc8d`
  (`0.12.0-289-ge7c9f3194`, 2026-10-04) in
  [`upstream.lock.json`](upstream.lock.json). Checkout lives outside the tree
  via `UUTILS_ROOT`.
- GNU coreutils 9.12, pinned transitively by uutils' `util/fetch-gnu.sh` at
  that commit; fetched from GitHub releases into `GNU_ROOT`, never vendored.
- [`parity.py`](parity.py) derives the utility list from the pinned
  `Cargo.toml` (`feat_os_unix`, expanded recursively, filtered to `src/uu/*`
  crates) and writes [`../coreutils-parity.json`](../coreutils-parity.json).
  `parity.py --check` fails when the manifest is stale.
- Denominator: **108 upstream utilities = 106 in scope + 2 SELinux-gated**
  (`chcon`, `runcon`, tracked with `capability: "selinux"` and still counted).
  Every `src/uu` crate is accounted for except the `checksum_common` library.
- Baseline presence: **41 of 106** in-scope utilities have an XSH applet; 65
  are missing. XSH has 58 applets in total (17 are outside the uutils set:
  `fd`, `getty`, `host`, `ifdown`, `ifup`, `ip`, `mdev`, `passwd`, `pstree`,
  `rev`, `rg`, `strings`, `su`, `system-report`, `tar`, `tree`, `which`).
- The denominator never shrinks. Exclusions are per test ID, categorized and
  explained, never per module.

Manifest fields per utility: upstream commit (top level), utility, capability
gate, XSH implementation path, present, native test file, uutils test count,
uutils integration pass/fail/skip/excluded, GNU differential, BusyBox
results, excluded test IDs, known semantic gaps.

## Phase 1: harness (Wave 0)

### A. uutils integration suite (Gate 3)

- [`xsh-uutests`](xsh-uutests) implements the multicall contract
  `BINARY UTILITY ARGS...` by exec'ing the staged applet. It is a dispatcher,
  never a second implementation; argv, stdio bytes, exit status, cwd,
  environment and signals pass through untouched (verified with a stand-in
  interpreter, including empty arguments, `--`, and NUL bytes on stdin). A
  missing applet exits 127 with a diagnostic.
- [`stage.py`](stage.py) installs `core/` in its released shape (suffix
  dropped, `lib/` adjacent, aliases as symlinks, shebang pointing at the built
  `xsh`) and writes the stage's `applets.json`. Suites test the installed
  shape, not the source tree.
- [`run-uutils.sh`](run-uutils.sh) builds the uutils `tests` target with
  `--features feat_os_unix` (the test crate requires
  `CARGO_BIN_EXE_coreutils` and per-utility features, so uutils
  implementations are compiled but never exercised), makes the uutils
  `coreutils` binary non-executable as a tripwire for anything that bypasses
  `UUTESTS_BINARY_PATH`, runs `cargo nextest` with
  `UUTESTS_BINARY_PATH=xsh-uutests`, `LC_ALL=C`, `TZ=UTC`, and converts the
  JUnit report with [`results.py`](results.py) into
  `results/uutils-integration.json`. Partial runs (named utilities) update
  only their entries.
- Cross-utility calls such as `scene.ccmd("touch")` also go to XSH. Fix
  widely used cheap applets first; their failures cascade.
- [`host-deps.json`](host-deps.json) lists the 28 utilities whose tests spawn
  host programs directly (`sh`, `strace`, `setfacl`, `unshare`, `locale`,
  ...) or compare against a host GNU tool. These are explicit, not
  accidental: in the clean environment they come from XSH or are excluded
  with category `host-oracle`.

### B. GNU suite differential (Gate 4)

[`run-gnu.sh`](run-gnu.sh) prepares one GNU 9.12 tree with uutils'
`util/build-gnu.sh`, then runs the identical tests twice, changing only the
`PATH` entry that `tests/local.mk` prepends: the pinned uutils multicall
directory, then the XSH stage's `gnu-bin/`. `gnu-bin/` has every GNU program
name; names XSH lacks are copies of `false`, so nothing falls through to host
GNU binaries (uutils uses the same trick). Because both sides share one
patched harness, uutils' patches cannot bias the differential.

Patch policy (revised): the brief asks to separate plumbing, accepted
semantic divergences and uutils wording patches. That classification
matters for absolute GNU conformance, not for the uutils-parity milestone,
so it is a tracked task in [`gnu-patches.json`](gnu-patches.json) (15 patches
plus `build-gnu.sh`'s sed rewrites, all `unclassified` today) rather than a
precondition for running the differential.

Results: `results/gnu-uutils.json` (cached per uutils commit),
`results/gnu-xsh.json`, and `results/gnu-differential.json` with the four
cells globally and per test directory. `uutils pass / xsh fail` is the
blocker list. GNU groups some tests under `misc/`; the per-utility rollup
maps those by test name when the manifest ingests them.

### C. BusyBox suite

After A and B are green enough to be useful, adapt uutils' BusyBox route
(`GNUmakefile` `busytest`). BusyBox behavior never overrides the uutils/GNU
contract where the projects intentionally differ.

### D. Native tests remain

`core/tests/test-<applet>.xsh` stay the fast regression suite and cover
XSH-specific boundaries. Upstream suites supplement them.

### E. Ratchets

- [`check_ignored_options.py`](check_ignored_options.py) (Gate 6): fails on any
  new discard bucket (`ignored`, `unused`, `compat`, `noop`), on a bucket that
  gains options, and on a stale baseline. Baseline: 16 buckets in chgrp,
  chmod, chown, cp, df, du, getty, hostname, ifdown, ifup, ln, ls, mv, pstree,
  touch, which ([`ignored-options-baseline.json`](ignored-options-baseline.json)).
  It can only shrink.
- Test ratchet: the integrator rejects a merge that turns any previously
  passing uutils test into a failure (compared against the committed
  `results/uutils-integration.json`).

## Phase 2: complete uutils Linux surface

The authoritative list is `totals.missing_in_scope` in the manifest, never a
handwritten list. Notes:

- Names that are xshi builtins (`echo`, `kill`, `true`, `false`, `test`) still
  need external applets for `env CMD`, `execve`, `PATH` lookups and programs
  that bypass the shell. Provide `[` for `test`.
- Share implementations through aliases in [`aliases.json`](aliases.json):
  `arch` over `uname` machinery, `dir`/`vdir` over `ls` with different
  defaults, the checksum family (`md5sum`, `sha*sum`, `b2sum`; `cksum` and
  `sum` share the native hash layer), `true`/`false`. Preserve each command's
  argv, help and output.
- `stdbuf` needs an LD_PRELOAD-style buffering shim in uutils; on musl that is
  unavailable (uutils drops it from `feat_require_unix_musl`). Decide with
  evidence: implement over a native mechanism or record a capability
  exclusion; never ship a no-op.
- `pinky`, `users`, `who`, `uptime` (user count) read utmp; Laputa's utmp
  story decides between real support and explicit failure.

Quality bar per command: options and their interactions, stdout, stderr, exit
status, binary safety, symlink follow/no-follow, permissions, ownership,
timestamps, hardlinks, sparse files, recursion, filesystem boundaries, broken
symlinks, material races, TTY behavior, signals, numeric parsing and
overflow, empty input, `-` operands, multiple operands and error continuation.
When POSIX/GNU is unclear, the pinned uutils implementation and tests are the
specification. uutils is a reference and oracle, never a cargo dependency;
reusing a small MIT-licensed algorithm is acceptable with attribution.

The no-ignored-options rule applies to every existing applet before it counts
as passing: implement or reject each discarded option.

## Native API growth

Add primitives instead of contorting scripts. Expected areas:

- **fs**: complete nofollow metadata, ownership/mode preservation, ns
  timestamps (`utimensat`), hardlinks, FIFOs and device nodes, sparse files
  (`SEEK_DATA`/`SEEK_HOLE`), efficient copy (`copy_file_range`, reflink),
  xattrs if `cp -a` requires them, statvfs and mount identity, atomic and
  no-clobber operations (`RENAME_NOREPLACE`, `O_EXCL`).
- **process**: signals, process groups and sessions, priority, terminal
  association, identity, wait with timeout, rlimits, stdio and environment
  control (for `nice`, `nohup`, `timeout`, `setsid`, `ps`, `pgrep`, `pkill`).
- **unix**: termios, PTYs and sessions (`stty`, `tty`, `getty`, `login`).
- **bytes**: streaming byte I/O for base encodings, checksums, `dd`, `od`.
- **hash**: one native layer for the checksum family; add missing algorithms
  (BLAKE2b with variable length, CRC for `cksum`, BSD/SysV `sum`).
- **compression**: existing gzip/bzip2/xz/lzma codecs back Phase 5.

## Phase 3: practical Linux surface beyond coreutils

Required, built over typed collectors and existing `linux.*` APIs:

- **3A hardware/inventory**: `lscpu`, `lspci` (`-n -nn -k -v -vv -D -t -s -d`;
  numeric output without a database, names from `/usr/share/hwdata/pci.ids`
  when present, never embedded), `lsusb` (`-t -v -d -s -D`, `usb.ids` when
  present), `lsblk` (`-a -b -d -f -J -l -n -o -p -r`, relationships from typed
  identity, not names), `blkid`, `findmnt` (mountinfo first: `TARGET`,
  `--json`, `--types`, `--source`, `--target`; `--fstab` after), `rfkill`,
  `sensors`.
- **3B util-linux control**: `mount`, `umount`, `swapon`, `swapoff`, `mkswap`,
  `losetup`, `dmesg`, `sysctl`, `hwclock`, `flock`, `setsid`, `pivot_root`,
  `switch_root`, `reboot`, `poweroff`, `halt` over `linux.mount`,
  `linux.swapon`, `linux.loop_*`, `linux.dmesg`, `linux.sysctl_*`,
  `linux.hwclock`, `linux.pivot_root`, `linux.switch_root` and friends.
- **3C kmod**: `lsmod`, `modinfo`, `modprobe`, `insmod`, `rmmod`, `depmod` over
  `linux.modules`, `linux.modinfo`, `linux.modprobe`, `linux.depmod`,
  `linux.insmod`, `linux.rmmod`, with kmod-compatible argv and output.
- **3D procps**: `ps` (`-e -f -ef aux -o -p --ppid --sort`, mainstream Linux
  forms tested comprehensively), `pgrep`/`pkill`/`pidof`/`killall` over one
  typed selector, `free` (`-h -m -g -s`), `lsof` over `linux.open_files()`
  grown to files, pipes, sockets, devices, cwd/root/exe; then `watch`.
- **3E networking**: `ip` grows to `link show/set up/down`, `address
  show/add/del`, `route show/add/del`, `rule show/add/del`, `-4 -6 -j -o`,
  via native rtnetlink mutation APIs next to `linux.network_dump()`; `ss`
  (TCP/UDP/listening/process); `ping`/`ping6` with real ICMP. Legacy
  `ifconfig`/`route`/`arp` only if cheap. Not `tc` or `bridge`.
- **3F udevadm**: honest `info`, `trigger`, `settle`, `monitor` over sysfs and
  XSH's uevent machinery; no operation reports success without effect.
- **3G accounts**: `login` sharing `lib/auth.xsh` with `passwd`, `su`, `getty`
  (which already execs `login`); `getent` for passwd, group, hosts, and
  services/protocols where present. No NSS empire.

## Phase 4: script-ecosystem commands

`grep`/`egrep`/`fgrep` (shared lower-level search primitives, not `rg`; `-E
-F -G -i -v -w -x -n -H -h -l -L -c -q -r -R -e -f --color`, `-P` only if
truly supported), `find` (`-name -iname -type -path -regex -size -mtime
-newer -user -group -perm -maxdepth -mindepth -prune -print -print0 -exec
-execdir -delete` with boolean composition), `xargs` (byte/NUL-safe `-0 -n -L
-P -I -r`, correct exit codes), `sed` (a real parser and addressing model;
uutils/sed and its GNU shim as reference and tests), `awk` (credible POSIX
awk, Rust parser/runtime), `cmp`, `diff`, `patch` over the native diff/patch
modules.

Revised: `sed` and `awk` share nothing with coreutils and are each multi-week
efforts, so they start in Wave 1 as long-running lanes.

## Phase 5: compression CLIs

`gzip`/`gunzip`/`zcat`, `bzip2`/`bunzip2`/`bzcat`, `xz`/`unxz`/`xzcat`,
`lzma`/`unlzma`/`lzcat` as one family over native codecs: stdin/stdout,
replacement semantics, levels, `-k -f -t -c -d`. `zstd` only if native
support stays lean; otherwise it remains a package.

## Phase 6: small high-value commands

`file` (deterministic subset: ELF, shebangs, archives and compression, text
encodings, common image/container formats; no libmagic database), `ldd`
(inspect the ELF interpreter and dependencies on musl without executing
untrusted binaries), `clear`, `reset`.

## Non-goals

`git`, SSH, `curl` parity, browsers, databases, nftables/iptables semantics,
Mesa, PipeWire, ALSA playback, `wpa_supplicant`, Tailscale. Rule: command
veneers and kernel/userspace inspection/control belong in XSH when XSH owns
the structured capability; independent application or protocol semantics
stay packages.

## Semantics and style rules

- Never fake success. No accepted semantic flag is discarded; unsupported
  behavior fails clearly.
- Output is conventional; typed records belong in the native API. Support
  established JSON options (`ip -j`, `lsblk -J`, `findmnt --json`).
- `LC_ALL=C` and `TZ=UTC` are the deterministic baseline. UTF-8 is still
  handled correctly; full i18n does not block the milestone.
- Linux first. Coreutils applets stay cross-platform where the API already
  is; Linux-only commands report unsupported cleanly elsewhere.

## Performance

Cold start, measured on the 4-core reference host with the release `xsh`
(2026-10-04, 50 sequential invocations, wall time per invocation): empty
script 9.5 ms, `print "x"` 10.6 ms, staged `core/cat` 13.6 ms, staged
`core/basename` 19 ms, host `/bin/cat` 3 ms. A suite of 6,000 invocations
therefore spends about 1-2 minutes in startup; cold start does not bound suite
runtime or make per-applet scripts non-viable, so no native multicall applet
entry is needed for startup. Throughput on large inputs is the real risk: the
uutils `cat` tests drove `core/cat` to 100% CPU and over 1 GB RSS and timed out.

Measure hot utilities (`cat`, `cp`, `dd`, `grep`, `sort`, `wc`, `head`,
`tail`, `find`, `xargs`, `ls`, `du`, checksums, `base64`, compression)
against uutils with the existing `bench/` infrastructure. Revised: measure
interpreter cold start first. Every suite invocation and every script call
pays it; if it dominates, the fix is a native multicall applet entry, not
per-script tuning. Prefer a small applet plus a fast native primitive over
rewriting applets in Rust.

## Packaging

`dev/release.xsh::package_core` already ships every `core/*.xsh`. Add a
deterministic applet manifest (applets, aliases, libraries; the shape
`stage.py` writes as `applets.json`) to the release artifact so Laputa can
materialize `/usr/bin` links from it instead of its hand-maintained list.
Aliases (`[`, `dir`, `vdir`, `egrep`, `fgrep`, `gunzip`, `zcat`, ...) come
from [`aliases.json`](aliases.json) with no duplicated sources. Laputa itself
changes only if needed to prove the interface.

## Verification gates

1. **Native**: project Rust tests, `xsht check`, `core/tests` pass.
2. **Inventory**: every in-scope utility implemented or carrying an accepted
   capability/platform rationale in the manifest.
3. **uutils integration**: all applicable `tests/by-util` tests pass through
   `UUTESTS_BINARY_PATH`; exclusions are per test ID, categorized, explained.
4. **GNU differential**: zero `uutils pass / xsh fail`, reported globally and
   per utility with all four cells.
5. **Options**: automated comparison of each uutils command's option surface
   with XSH's (help-text parsing is one input, not proof of semantics); gaps
   appear in the manifest.
6. **No ignored semantic options**: ratchet at zero buckets.
7. **Practical Linux suite**: deterministic tests for every Phase 3–5
   command; privileged behavior runs in namespaces, temporary loop devices,
   QEMU or containers, never against the developer host.
8. **Clean smoke**: a Linux image with XSH first on `PATH` and no GNU
   coreutils, util-linux, procps, findutils, grep, sed, gawk, diffutils or
   kmod, running real workflows: file manipulation, archives, checksum
   verification, find/xargs, grep/sed/awk pipelines, process, mount/block,
   network, module and hardware inspection, boot-adjacent commands.

## Deliverables

Implementation; pinned reference manifest; uutils adapter; GNU differential
runner; per-command parity report; native tests for new primitives and
applets; documentation (what "XSH core compatibility" means, baseline
revision, rerunning the suites, unsupported capability areas, downstream
discovery and installation); and a final report: applets before/after,
uutils coverage, uutils integration pass rate, GNU differential, intentional
gaps, extra Linux utilities, new native APIs.

## Sequence (revised)

Wave 0 is serial and must land before fan-out; Waves 1 and 2 are parallel
lanes defined in [`LANES.md`](LANES.md).

| Wave | Work |
|---|---|
| 0 | parity manifest and adapter (done); toolchain and suite build; baseline uutils run of current XSH; `cli` GNU mode; `lib/gnu` diagnostics; invoked-name access; byte helpers; ratchets wired into `make check`; applet manifest in release |
| 1 | existing-applet repair (ignored buckets, GNU diagnostics); missing cheap coreutils; native fs and process/tty primitives; text, bytes/checksum families; `system-report` collector extraction; `sed` and `awk` start |
| 2 | difficult coreutils finish; GNU differential to zero blockers; hardware/storage/kmod/util-linux wrappers; procps; networking; grep/find/xargs; diff/cmp/patch; compression; login/getent/udevadm; Phase 6 |
| 3 | BusyBox route; Gate 5 option comparison; Gate 8 clean smoke image; performance pass; final report |

## Environment notes

- The repository pins `nightly-2026-09-15`; XSH uses no `#![feature]`, so
  stable 1.97 builds it when the pinned toolchain is unreachable
  (`RUSTUP_TOOLCHAIN=stable`). Do not commit a toolchain change for this.
- Cloud sessions need network access to crates.io (`index.crates.io`,
  `static.crates.io`), `static.rust-lang.org`, GitHub release downloads, and,
  for the `xsh-test` image, Docker Hub and `dl-cdn.alpinelinux.org`. The
  default **Trusted** environment level covers crates.io, the Rust
  distribution host and Docker Hub; add anything else as a custom domain.
- AGENTS.md makes `Dockerfile.test` (`xsh-test`, musl) the authority for Linux
  evidence. Host-glibc runs are fast iteration only; gate results that count
  come from the image.
