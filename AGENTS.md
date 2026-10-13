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
| API signatures and details | registry queried by `xsht api` |
| architecture, invariants, contribution and content tiers | [Architecture](docs/ARCHITECTURE.md) |
| syntax rationale | [Design](docs/DESIGN.md) |
| builds, test gates, profiles, platforms, generated docs | [Testing](docs/TESTING.md) |
| tooling and configuration | [XSHT](docs/XSHT.md) |
| formatter policy | [XSHT-FMT](docs/XSHT-FMT.md) |
| core commands and system reporting | [Core](core/README.md), [system report](core/SYSTEM-REPORT.md), [btop](core/BTOP.md) |
| substantial example and showcase programs | [Examples](examples/README.md), [Showcase](docs/SHOWCASE.md) |
