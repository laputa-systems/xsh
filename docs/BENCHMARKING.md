# Benchmarking

The curated rustybench suite measures user-facing latency and allocation
workflows in the interactive `xshi` crate. It does not benchmark the `xsh` or
`xsht` frontends.

The paired end-to-end `xsh` and `xsht` CLI latency corpus already lives in
`bench/stdlib-port/run.py` and `bench/stdlib-port/tooling.py`. Its cold script,
`xsht check`, and `xsht api` rows have fixed B0 comparisons, raw samples,
output parity, and macOS/Linux results documented in
`bench/stdlib-port/README.md`. Use those rows when frontend preparation or
tooling latency changes.

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
cargo dev bench
```

Run a fast allocation iteration with:

```sh
cargo dev bench --fast
```

The direct focused form is:

```sh
cargo bench -p xshi --bench bench --features benchmark xshi_prompt_render_long_command -- \
  --sample-count 1 --sample-size 1
```

`cargo dev bench` and `cargo dev bench --fast` compare against ignored machine-local
baselines under `crates/xshi/benches/`. Keep the command, profile, host,
allocator, and sample settings paired when comparing changes. Fast runs are
allocation iteration signals, not reliable latency measurements.

Run benchmark processes serially. XSH has process-global interners and caches,
so the first sample can be colder than later samples. Treat small single-run
latency changes as inconclusive and repeat timing measurements with identical
settings before acting on them.

## Call tracing probe

`bench/trace-call-overhead.xsh` is a fixed 10,000-call script for separating
plain-runner latency from `xsht trace` summary and raw-output costs. The 12
rotating-order macOS ARM64 release rounds in
`bench/trace-call-overhead-c09-2026-09-24.json` measured median wall times of
30.3 ms plain, 38.1 ms summary, and 42.8 ms raw. Median process peak RSS was
12.9, 23.2, and 25.9 MB respectively. Sending raw trace output to `/dev/null`
left a similar 41.9 ms median, since `xsht` still builds and renders the events.
These are end-to-end CLI comparisons across two binaries, not an isolated cost
for trace collection. The failure fixture retained its `result.propagate` call
path, and raw mode emitted `proc.enter`, `proc.exit`, and `script.exit` events.
No runtime change follows from this probe alone.

## Syscall diagnostics

Run:

```sh
cargo dev bench --syscalls
```

This is a separate diagnostic path for detecting unexpected subprocesses,
filesystem churn, or kernel work. It does not add another benchmark corpus.
