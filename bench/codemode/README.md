# Code mode experiment

Measures how well a model writes XSH from documentation, compares contexts, and compares
XSH with TypeScript on the same tasks. [DESIGN.md](DESIGN.md) explains the harness;
[contexts/README.md](contexts/README.md) the contexts and the baseline rule;
[defects/README.md](defects/README.md) the product defects found.

## Setup

```sh
mkdir -p .work/codemode-runs/toolchain
flock .work/locks/rust-slot cargo build --profile verification -p xsh --bin xsh -p xsht --bin xsht
cp target/verification/xsh target/verification/xsht .work/codemode-runs/toolchain/
docker pull oci.17166969.xyz/xsh/test@sha256:fed05abfa97c947798fd88621278a2ecd9c7334a840ac638a2c7719b461bb0c6
docker build -t xsh-codemode-ts:local bench/codemode/ts
```

Every command refuses a toolchain or image whose digest differs from
`harness/config.py`. Records go under `.work/codemode-runs/` (`CODEMODE_RUNS` overrides).

## Commands

```sh
bench/codemode/codemode references [--split dev] [--task ID]   # qualify both references on every variant
bench/codemode/codemode judges [--split dev] [--task ID]       # each judge must reject its wrong programs
bench/codemode/codemode isolation                              # what a program can see in the boundary
bench/codemode/codemode context                                # rebuild-and-compare the baseline, print digests
bench/codemode/codemode verify-guide candidate.md              # check every guide claim and example
bench/codemode/codemode prompt --task ID --condition xsh-candidate
bench/codemode/codemode trials --out .work/codemode-runs/NAME --phase dev \
  --condition xsh-baseline --condition xsh-candidate --condition ts --samples 2 --jobs 2
bench/codemode/codemode trials --out .work/codemode-runs/NAME --resume
bench/codemode/codemode report .work/codemode-runs/NAME [MORE_RUNS...] [--taxonomy FILE]
```

`trials` writes `manifest.json` (schema `manifest.schema.json`) with the shuffled
schedule, toolchain, image, context and task digests, then one directory per cell under
`trials/TASK/CONDITION-SAMPLE/`: prompts, raw pi events, replies, extracted programs,
checker and run records per variant, service ledgers, tree effects, verdicts and usage.
`--resume` runs only cells with no finished record. Exit status 3 means the provider
stayed unavailable past the backoff budget; the manifest is kept for a later resume.

## Tasks

A task is a directory `tasks/SPLIT/ID/` with `task.json`, `fixture.py` (`setup` and
`judge`), `reference.xsh`, `reference.mts`, and `wrong/` programs its judge must reject.
Splits: `dev` (12 development tasks), `held` (held-out), `session` (session workflows).
