# Feature-touch compile replay result

Source base: commit `4d7b0d0d103ab2bff68bf0a019e7a172cb9ce6ae`, tree `bf7a566fbf2ca782a4473307746cec9be01d7ff1`.

Runner: pinned Linux AMD64 image `sha256:3b8b01330d40d578781f6391fe25f7b86ef5efdae5628e6eec9bf30487d8852c`; `TARGET=x86_64-unknown-linux-musl`; 8 CPUs; 16 GiB memory with swap disabled; `kache`; `mold`; `dev/musl-linker.sh`; compiler-only jemalloc; rustflags `-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol`. Cargo target and temporary spill directories are isolated under `.work/tmp/feature-touch-4d7b/`.

Commands and results:

- Seed only, `cargo check --message-format=json --target x86_64-unknown-linux-musl -p xsh --lib --all-features`: exit 101; 36 E0004 compiler occurrences at 36 primary spans.
- XSH diagnostic scaffold, same command: exit 0. It adds 34 `todo!()` match arms plus one compact catalog row. Two compiler occurrences originate from catalog-generated matches and are resolved by the single row.
- Seed plus XSH scaffold, `cargo check --message-format=json --target x86_64-unknown-linux-musl -p xsht --lib --all-features`: exit 101; 19 E0004 occurrences at 19 primary spans.
- Combined scaffold, `cargo check --message-format=json --target x86_64-unknown-linux-musl -p xsh -p xsht --lib --all-features`: exit 0. XSHT adds 19 `todo!()` match arms.

Raw JSON, stderr, and exit statuses are under `.work/tmp/feature-touch-4d7b/logs/`. Normalized diagnostics are `stage1-xsh.errors.jsonl` (36 occurrences) and `stage2-xsht.errors.jsonl` (19 occurrences). There are 34 XSH placeholder arms, one XSH catalog row, and 19 XSHT placeholder arms. These are compiler-demanded diagnostic scaffolding, not feature implementations. The scaffold and successful check provide no runtime-reachability evidence. The source candidate index has 300 lexical candidate rows (165 XSH and 135 XSHT); those rows are not semantic touch counts or reachability evidence.

No tests, native suites, release builds, benchmarks, formatters, or feature execution were run. This diagnostic branch must never merge.
