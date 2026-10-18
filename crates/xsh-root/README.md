# xsh-root

`Root` confines pathname resolution to a trusted directory. Linux 5.6 and
later use `openat2` with `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`. macOS 26
and later use `openat` with `O_RESOLVE_BENEATH`. Linux retries transient
`EAGAIN` responses. Other platforms are unsupported.

Open the trusted root before using paths from untrusted input:

```rust
use xsh_root::Root;

let root = Root::open("/srv/data")?;
let file = root.open_file("users/alice/avatar.png")?;
# Ok::<(), std::io::Error>(())
```

`../etc/passwd` and symlinks that leave the root cannot escape it. Relative
symlinks that resolve inside the root work. `open_readable_dir` requires a
directory during the open, so a regular file or FIFO cannot be mistaken for a
directory. `open_stat_target` opens a confined target for metadata queries
without reading its contents. `open_with` applies its limited options; `create`
creates or truncates for writing. `open_dir` creates a narrower `Root`.

`Root::open` establishes the trusted anchor through the ambient filesystem.
Calls through `Root` protect pathname resolution only. They do not constrain
other code that uses `std::fs`, mount points or special files below the root,
or higher-level authorization and mutation policy.
