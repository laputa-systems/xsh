# xsh-root

`xsh-root` protects rooted file opens from CWE-22/path traversal on Linux 5.6+
and macOS 26+. A `Root` holds a directory descriptor; every later open is
kernel-confined beneath it.

```rust
use xsh_root::Root;

let root = Root::open("/srv/data")?;
let file = root.open_file("users/alice/avatar.png")?;
# Ok::<(), std::io::Error>(())
```

`../etc/passwd` and `escape-symlink/etc/passwd` cannot escape `root`. Relative
symlinks that resolve inside the root work normally.

`Root::open_readable_dir` opens a directory for entry iteration under the same
confined resolution rules. It requires a directory during the open, so an
unexpected regular file or FIFO cannot be mistaken for an iterable directory.
`Root::open_dir` instead creates another confined root descriptor; on Linux
that descriptor is `O_PATH` and cannot be used to read entries.

Linux calls `openat2` directly with `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`.
macOS calls `openat` with `O_RESOLVE_BENEATH`. An existing file needs exactly
one pathname-opening syscall after `Root::open`; there is no component walker
and no older-kernel fallback.

This is path-resolution protection, not a sandbox. It confines only opens made
through `Root`; it cannot stop other code from using `std::fs`, and it does not
restrict mount points or device nodes beneath the root or enforce higher-level
authorization or mutation policy.

To inspect the pathname-opening calls in the example, build it and trace it:

```sh
cargo build -p xsh-root --example open_bench
```

On Linux, trace it with:

```sh
strace -e trace=openat2 ./target/debug/examples/open_bench
```

On macOS, trace it with:

```sh
sudo dtruss -t openat ./target/debug/examples/open_bench
```

Each rooted label makes 100,000 `Root::open_file` calls, each through one
pathname-opening syscall: `openat2` on Linux or `openat(...,
O_RESOLVE_BENEATH, ...)` on macOS. The example compares path depths rather than
making one sample call.
