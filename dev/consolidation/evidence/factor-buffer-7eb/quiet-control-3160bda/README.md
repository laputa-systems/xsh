# Factor parallel isolated control

The unchanged integrated candidate passed the exact parallel factor selector in isolation. The prior R3 timeout remains unexplained by this control.

## Run

The candidate was commit `3160bda38f5fb4cdb05e6b279efc57b296d4fcf8`, tree `7bb5fb42a1dd437cf8be1e8d1d49137425d2a0b7`. It ran this exact selection inside the pinned test container:

```sh
/target/xsht test -j 1 --exact core/tests/test-uu-factor.xsh::test_uu_factor_parallel --keep-temp
```

The container used image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`. It ran Linux amd64 as UID 1000 and GID 1000. It used cpuset 0–11, a 12 CPU quota, 24 GiB of memory with no swap, and no network. `TMPDIR` was the run's own scratch directory. The test kept its two-minute timeout. The outer cutoff was 150 seconds.

## Result and resources

The selector matched one test and passed. It reported one passed, zero failed, and zero skipped. The test verified the expected sorted-output hash. Its kept output had 400,008 lines and 6,520,608 bytes. All eight child stderr files were empty. The harness reported 1m34s. The container ran for about 95 seconds. These durations document the functional run and are not performance results. The coordination window held other campaign builds, but external host jobs remained active; this run was not perf-ready.

The cgroup peak was 375,775,232 bytes. `memory.events` recorded zero OOM events. CPU usage was 189.663 seconds with zero throttling. Docker reported `OOMKilled=false`. The original shared lock remained device 66307, inode 62012470, with unchanged metadata. Source, test, and product hashes match before and after in the adjacent files.

## Profile limits

A five-second, 49 Hz user-only sample of one child in this parallel run
produced 241 samples. `perf.data` remains in ignored run scratch at
`.work/tmp/native-factor-repair/quiet-unchanged-candidate-3160-parallel/perf-child-2409.data`.
Its header lacks build-ID data. The pinned `xsh` product has build ID
`08e9c85aa4041bf5211e11e23a9730d4c078f494`; the runner mounted it read-only and
checked its hash before and after. `perf report --symfs` maps the sampled PC
`0x7f00ea6bd19f` to ELF address `0x16bd19f`, inside that product's `memcpy`
instructions (`0x16bd17b`–`0x16bd1ac`). `memcpy` held 27.80% of self samples,
`memcmp` 2.07%, and `LoweredValue::clone` 0.83%. The recorded call chains do not
identify the XSH source caller. The profile itself cannot prove DSO identity
because it lacks a build ID. The byte-identical `perf report --symfs` output is
retained in [`perf-report-symfs.txt`](perf-report-symfs.txt), with its stderr
beside it.

An initial `addr2line` query used the wrong offset, `0x16bc19f`, and appeared to
name `trinkle`. That attribution was incorrect; the recorded address maps to
`memcpy` as described above.

A separate authorized one-child probe exercised the route used by the test
helper. It ran frozen `xsh` through `core/tests/support/uu-launch.xsh` with
`core/factor.xsh --` and `integer_input(50000)`, which supplies integers 0
through 50,000 with a trailing space. The output was 50,001 lines and 815,076
bytes, byte-identical to one baseline child chunk; exit status was zero and
Docker reported no OOM. The exact invocation and input generator are retained
in the probe record summarized here:

```sh
/bin/setpriv --reuid=1000 --regid=1000 --clear-groups \
  /target/xsh \
  /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-r3-repair/core/tests/support/uu-launch.xsh \
  "$metadata" /target/xsh \
  /home/josh/d/laputa-systems/xsh/.work/wt/consolidation-r3-repair/core/factor.xsh \
  -- < /scratch/input
```

The runner generated `$metadata` with `jaq -nc '{caller_keys:[],umask:null}'`
and `/scratch/input` from `''.join(f'{n} ' for n in range(50001))`.

The profile used the pinned R3 products (`xsh` SHA-256
`1d7690114501313ffc4706b7f78ab6154ecd668f79d3735700219a00501dbe33`, `xsht`
SHA-256 `0cd37140bd43ca0ca5104d43d11bc1732084fc2342ee3bb69a73fda38452f1c4`),
and the read-only source checkout at commit
`3160bda38f5fb4cdb05e6b279efc57b296d4fcf8` with `core/factor.xsh` SHA-256
`47b0714bc148ea8bef7f2cf345f0cdde6ca23c1ccd67bdabbd2f2ff84288bbd2`. The
five-second user-only `cycles:u` sample at 49 Hz with DWARF call graphs used
one CPU on cpuset 14, 2 GiB memory, equal memory-swap limit, and no network. It
produced 245 samples. `memcpy` held 35.57% of samples and
`Vec<LoweredValue>::clone` 1.22%. Caller unwinding failed: 32.31% of samples
ended at `memcpy`; remaining frames were unresolved or invalid. The clone
sample therefore does not attribute the `memcpy` work to that clone. This
profile identifies copy activity but not its source caller. The container
exited zero with no OOM and was removed. Source and product guards matched
before and after. The text report, output check, and resource counters are in
[`single-child-profile`](single-child-profile/).

The +30 and +90 second container samples could not run `ps -o pcpu`; BusyBox
`ps` in the image does not support `pcpu`. Both raw errors are retained. A
separate host `docker top` sample near +70 seconds showed `xsht` and four factor
children waiting in `futex_do_wait`. Each visible child had about three seconds
of CPU time at that point. This point sample and the profiles do not establish
the cause of the earlier timeout. No performance result is claimed.
