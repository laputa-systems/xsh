# Etthool namespace fixture effect

Canonical 787 reports ten device-backed ethtool failures. Each fails while
checking the generated namespace setup: `main` calls `make_devices`, which
requires `net`, without declaring that effect.

Commit `7d01429ba` adds `net` to that private fixture entry point.
The existing assertions are unchanged. One exact device test passes, then all
20 tests in `core/tests/test-ethtool.xsh` pass. After integration, all 20 pass again.
No tests skip in these three runs.

The runs use the pinned Linux image from canonical 787, the existing verification
products, root, a private network namespace, a two-CPU cap, 4 GiB memory with
no swap, and no runtime preload. The product digests before and after match.
No compilation, full native suite, or performance measurement is involved.
