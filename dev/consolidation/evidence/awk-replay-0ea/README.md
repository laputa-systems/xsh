# Bounded replay of the historical AWK source

On 2026-10-15, the original `0ea216732686b7851bfb8ba49b81b5ef6d2608b1`
test source and its freshly desugared form each ran the twelve stream/process
shards. Both used root, eight workers, and the pinned Linux image. The native
suite lock excluded other campaign native suites.

Both runs passed shard 001 and failed shard 008's `io_unreadable_file` case.
Both exited 1. This reproduces the separate root-sensitive observation, not
the historical extra shard-001 failure. Timings are not performance evidence.

The products were built at `2688d52fc037ab36e4f85ff847ad49b900cbf686`
with the verification profile. Relevant runtime, test-runner, AWK applet, and
matrix sources match 0ea. The project-discovery helper differs. This is a
controlled replay, not the historical release binary or environment.

The two `source.txt` files retain exact written and transformed bytes.
The stdout files retain complete results. Stderr was empty. `context.txt`
records the command, statuses, binary hashes, environment, and resource facts.
Paths in that capture identify the replay's disposable directories.

The historical transformed bytes, failing child diagnostics, worker count,
and environment remain unavailable. This replay supplies no discriminator for
the old extra failure. Preserve that failure and keep the AWK acceptance
obligation open under [the campaign's Phase A policy](../../CAMPAIGN.md).
