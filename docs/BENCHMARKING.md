# Benchmarking

The curated rustybench suite measures user-facing latency and allocation
workflows in the interactive `xshi` crate. It does not benchmark the `xsh` or
`xsht` frontends.

The suite lives in `crates/xshi/benches/bench.rs`. The benchmark-only
`xshi::interactive::bench` helpers are enabled through the `benchmark` feature;
release application builds do not compile them. rustybench's allocation
profiler records latency, allocation count, allocated bytes, and peak live
allocation data using the system allocator.

## Workloads

The suite covers complete interactive operations:

- rendering a long prompt;
- navigating completion over 1,000 directory entries;
- searching and rendering a 45,000-entry history;
- completing a `cd` workflow over 1,000 entries;
- executing dynamic-name session commands.

Generated directory and history fixtures are deterministic. Fixture setup stays
outside the measured operation where possible. A benchmark belongs here only
when making interactive `xshi` behavior faster directly improves the user
experience.

Run the normal latency suite with:

```sh
make bench
```

Run a fast allocation iteration with:

```sh
make bench-fast
```

The direct focused form is:

```sh
cargo bench -p xshi --bench bench --features benchmark xshi_prompt_render_long_command -- \
  --sample-count 1 --sample-size 1
```

`make bench` and `make bench-fast` compare against ignored machine-local
baselines under `crates/xshi/benches/`. Keep the command, profile, host,
allocator, and sample settings paired when comparing changes. Fast runs are
allocation iteration signals, not reliable latency measurements.

Run benchmark processes serially. XSH has process-global interners and caches,
so the first sample can be colder than later samples. Treat small single-run
latency changes as inconclusive and repeat timing measurements with identical
settings before acting on them.

## Syscall diagnostics

Run:

```sh
make bench-syscalls
```

This is a separate diagnostic path for detecting unexpected subprocesses,
filesystem churn, or kernel work. It does not add another benchmark corpus.
