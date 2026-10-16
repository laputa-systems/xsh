# Targeted Linux checks at 449

Clean source is `449320b1844cb5c7cc361df2fe27e387b37b12d1`, tree
`b1f2632d9fc3b48ceb5747843a6631db6e0aeb1c`. Cargo JSON artifact events bind the
ordinary and dedicated privileged harnesses; the product build explicitly selects
the xsh, xsht, and xshi packages. Source and product identities remain unchanged.

The non-root traceroute file passes 18 cases and skips two, including its expected
CAP_NET_RAW boundary. The dedicated root block-device harness passes both cases
on a container-private scratch mount verified with ST_NODEV. Runtime allocator
preload is empty. Receipts retain resource limits, argv, statuses, and products.
These are focused correctness checks, with no canonical or timing credit.

The integrated adapter-runner module passes all 68 tests. An earlier invocation
had 67 passes and one lock-acquisition error while the real W9 compare held its
global lock. The retained successful run follows that compare's completion and
lock release; no test or lock check was bypassed.

The first build wrapper returned status 2 after successful Cargo commands because
it demanded an xshi artifact without selecting the separate xshi package. Its
raw outputs are retained in `build-missing-xshi`. The corrected build selects
all three packages and exits zero. No application change was needed.
