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

Linux calls `openat2` directly with `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`.
macOS calls `openat` with `O_RESOLVE_BENEATH`. An existing file needs exactly
one pathname-opening syscall after `Root::open`; there is no component walker
and no older-kernel fallback.

This is not a sandbox: it does not stop code using `std::fs` directly, restrict
mounts or device nodes below the root, or provide directory traversal,
mutation, or other filesystem policy.

To verify syscall count manually, trace a small program that calls
`root.open_file("a/b/c")` after constructing the root:

```text
Linux: strace -e trace=openat2 ./target/debug/examples/open_bench
macOS: sudo dtruss -t openat ./target/debug/examples/open_bench
```

The traced rooted shallow and deep opens should each show one `openat2` on
Linux, or one `openat(..., O_RESOLVE_BENEATH, ...)` on macOS.
