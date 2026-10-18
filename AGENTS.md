# Agent Guide

XSH is a standalone scripting language, toolchain, and core command collection.
Before editing, read the [tour](docs/user-tour.md), nearest code and tests, and
the topic owner below.

## Commands and invariants

- Build: `cargo dev build`; run the narrowest applicable check with `cargo dev gate PATH...`.
- Run Linux verification with `cargo dev test linux` through `Dockerfile.test`; see [Testing](docs/TESTING.md) for manual commands and profiles.
- Native XSH tests own language behavior; Rust tests cover host boundaries the native harness cannot express.
- Process tests use verification or release binaries; reserve release for final and performance checks.
- Run one full native suite at a time; bound ad-hoc probes and clean up owned processes.
- Linux verification uses the pinned container and target flags.
- Edit generated-doc sources before `make docs`; update the SPEC when language behavior changes.
- Do not run formatters or autofixers.

## Topic owners

| Topic | Owner |
|---|---|
| language, OS, streams, JSON, interactive behavior | [SPEC](docs/SPEC.md), [interactive SPEC](docs/SPEC-INTERACTIVE.md) |
| API signatures and generated references | `xsht api`, [stdlib](docs/reference/stdlib.md), [CLI](docs/reference/cli.md), [grammar](docs/reference/grammar.md), [lints](docs/reference/lints.md) |
| architecture, invariants, contribution and content tiers | [Architecture](docs/ARCHITECTURE.md) |
| syntax rationale | [Design](docs/DESIGN.md) |
| builds, test gates, profiles, platforms, generated docs | [Testing](docs/TESTING.md) |
| tooling and configuration | [XSHT](docs/XSHT.md) |
| formatter policy | [XSHT-FMT](docs/XSHT-FMT.md) |
| core commands and system reporting | [Core](docs/CORE.md), [system report](docs/SYSTEM-REPORT.md), [btop](docs/BTOP.md) |
| substantial example and showcase programs | [Examples](examples/README.md), [Showcase](docs/SHOWCASE.md) |
| formal models and proof coverage | [Formal verification](formal/README.md) |
| current performance and deployment work | [Campaign](docs/CAMPAIGN-PERFORMANCE-DEPLOYMENT.md), [inference evidence](docs/PERFORMANCE-INFERENCE.md) |
| open work and design drafts | [TODO](TODO.md), [handle ownership](docs/rfc/XSH-4-affine-runtime-handles.md), [record validation](docs/rfc/XSH-7-record-json-exactness.md) |
| generated documentation sources | [Templates](docs/templates/), [generation commands](docs/TESTING.md) |
