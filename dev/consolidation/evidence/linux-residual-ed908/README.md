# Targeted Linux residual checks at ed908

Both checks bind clean source `ed90857bdd825439159b7d788ea5d57cd9652920`, tree
`efc56a4a81c7993e35b2fd76a2549742eb8d8847`, and unchanged products before and after.
The pinned Linux musl image runs without a runtime allocator preload. These are
targeted correctness checks with no canonical-batch or timing credit.

The non-root traceroute file selects 20 cases: 18 pass and two skip. The expected
source-route case observes its EPERM assertion and reports the CAP_NET_RAW skip.

The root block-device filter passes both cases under a container-private scratch
mount verified with `statvfs` to have `ST_NODEV`. Its dedicated privileged harness
comes from the retained successful Cargo JSON artifact event, with SHA-256
`21f4d5c06c07bfbd957c757e179e6d63a146c1eb099014882e328ef740bac88a`.

The first block-device execution also passes both tests, but its driver rejects
their actual `core_compat_extra::` name prefix. That failed driver receipt is
retained. The corrected driver requires the full names; a fresh execution passes.
An earlier CLI type error occurred before a container launch; the driver now
converts the target-directory argument to Path consistently.

Building the privileged feature replaces the plain xsh product. The retained
default-feature Cargo no-run event then restores its original hash before the
targeted checks. The receipts verify that actual restored hash throughout.
