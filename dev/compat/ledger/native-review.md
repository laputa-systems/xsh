# Native primitive review (read-only audit, 2026-10-10)
Classification of natives added since fork 95e35002 and verdicts on pending requests.

## Keep (OS or codec boundary)
io.stdin_read/write_stderr/flush_stderr; unix.read_fd/write_fd/seek_fd/poll_fd/dup_fd/redirect_fd; unix.exec_env/set_uid/set_gid/set_groups;
bytes.unpack_float (no Float bit reinterpret in XSH); regex.captures_bytes (POSIX engine); hash.digest_* algorithms and crc32;
fs.xattr_*, sync_path, rename_exchange, access, path_limits, tempdir_in, temp_sibling; linux ioctl wrappers; time.to_calendar/from_calendar (tz database); user.groups.

## XSH could do it (candidates to move back to XSH once parity is reached; not now)
- speed-only: bytes.squeeze, bytes.repeat_prefix_count, hash.checksum bsd/sysv, linux.sample
- thin conveniences: unix.set_credentials, process.which (Path), fs.copy_file `force`, regex.find_bytes (loop over captures_bytes), bytes.resize (duplicates Path.truncate), unix.cpu_features (cksum --debug shaped)
- carry policy in Rust: compression.transform (~80% applet policy: tempfile, noclobber, perms/times, level ranges; should shrink to a streaming codec session), linux.module_plan (modprobe.d resolution), linux.block_signatures table, time.format (strftime flags/padding policy; date.xsh re-splits formats around it)

## Pending requests: verdicts
1. dd: justified. `flags: List[Str]` on unix.open_fd (closed set: direct, noatime, nofollow, directory, dsync, sync, append, nonblock, noctty); unix.fadvise(fd, offset, length, advice) returning the errno; read_fd/write_fd bounce through a page-aligned buffer under O_DIRECT. core/dd.xsh:116 rejects these today and test-dd.xsh:124 asserts the rejection (change it).
2. SIGPIPE: no new API; startup fix. Rust std ignores SIGPIPE before main (install_interactive_signal_handlers is never called). Record the inherited disposition (-Zon-broken-pipe=inherit on the pinned nightly), report it through process.signal_action, apply it in exec/spawn instead of blanket SIG_DFL (runtime/process.rs:2069).
3. timeout --foreground: justified. One Bool on process.command_argv/spawn selecting ProcessGroupConfig::Inherit (runtime/process.rs:616); signal the pid, not killpg.
4. getrandom EAGAIN panic: not an API. regex-lite `std` feature uses HashMap/RandomState; set `default-features = false, features = ["string"]` (Cargo.toml:91). Audit std HashSet at src/modules/linux/block.rs:563,640,785.
5. env: justified. `block_signals: List[Str]` on unix.exec_env applied in pre_exec + a getter for the blocked set; env.entries() returning Bytes name/value and exec_env taking byte maps (printenv's /proc/self/environ read goes away).
6. FICLONE visibility: reject for now. Every applet runs on a spawned 12 MiB worker (runtime/eval.rs:8173); evaluating on the main thread is a runtime change with stack-size risk for one strace-quirk test.
7. Deep rm: no new API. Make Path.remove (fs.rs:1829, std remove_dir_all, fd per level) iterative and fd-bounded (openat/unlinkat, verified ascend).
8. date large years: XSH (done, merged).
9. Bytes ordering: XSH. Bytes.compare exists (methods.rs:937); use it in testexpr.xsh.
