# Agent Guide

This repository contains the standalone XSH language implementation, tools,
core command scripts, examples, docs, and tests.

Before editing, read [the tour](docs/user-tour.md), the nearest code and tests,
and the relevant owner below.

## Commands and invariants

- Build from source: `cargo dev build`; Linux verification uses `cargo dev test linux` in `Dockerfile.test`.
- Choose the narrowest gate with `cargo dev gate PATH...`; follow [Testing](docs/TESTING.md) for manual build/test commands.
- Use native XSH tests first; Rust tests own host boundaries the native harness cannot express.
- Process tests require verification or release binaries; release is for final and performance verification.
- Run one full native suite at a time; bound ad-hoc probes and clean up owned processes.
- Linux evidence requires the pinned container and target flags; this campaign accepts x86_64 musl only.
- Edit generated-doc templates/snippets, then `make docs`; change the SPEC with language behavior.
- Do not run formatters or autofixers; verify with scoped builds and tests.
- Keep comments self-contained and changes scoped; preserve useful rationale.
- Report findings and verification briefly; name the failing command and why it matters.

## Topic owners

| Topic | Owner |
|---|---|
| language, OS, streams, JSON, interactive behavior | [SPEC](docs/SPEC.md), [interactive SPEC](docs/SPEC-INTERACTIVE.md) |
| API signatures and details | registry behind `xsht api` |
| architecture, invariants, contribution and content tiers | [Architecture](docs/ARCHITECTURE.md) |
| syntax rationale | [Design](docs/DESIGN.md) |
| builds, test gates, profiles, platforms, generated docs | [Testing](docs/TESTING.md) |
| tooling and configuration | [XSHT](docs/XSHT.md) |
| formatter policy | [XSHT-FMT](docs/XSHT-FMT.md) |
| core commands and system reporting | [Core](core/README.md), [system report](core/SYSTEM-REPORT.md), [btop](core/BTOP.md) |
| substantial example and showcase programs | [Examples](examples/README.md), [Showcase](docs/SHOWCASE.md) |
