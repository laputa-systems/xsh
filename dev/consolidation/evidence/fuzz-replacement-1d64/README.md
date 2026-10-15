# Replacement-label regression repair

Test-only commit `1d64104e6f2ede6c707a74e516d5ac5b1b0b34f4` repairs an
incorrect seed-shape assumption. Later generator changes consume different
random draws, so seed 4 no longer emits `.replace(`. The checker accepts its
program. The original assertion fails with status 101.

The replacement constructs one `Expr::Method` through existing public AST
types and calls `Program::print`. It checks the exact `with:` argument label
and checker acceptance. The test stays in the canonical soundness target.
Production code, RNG behavior, and the language contract are unchanged.

The committed test source SHA-256 is
`fb990ed3e48b1aa4e0ebdde55c6944f33a0099a16fbcdae12bba1606f885058a`.
The new harness SHA-256 is
`770cd5148b9644b30a94fd812e8dd2e0ac6e938000f9c3fb3ac7afe38faf3864`.

- [Original exact failure](red/stdout.log): status 101.
- [Focused build](build/exit-status.txt): status 0.
- [Exact repaired test](exact-green/stdout.log): one pass.
- [First full-suite attempt](failed-mount/stdout.log): thirteen pass, one
  corpus-path failure. Its source mount was `/work`, while the harness embeds
  `/src`.
- [Corrected full suite](suite-green/stdout.log): fourteen pass, status 0.
- [Integrated exact test](integrated/stdout.log): one pass on main `8805a8ece`.

The lane build preloads jemalloc into Cargo and its build children. This
differs from the prescribed rustc-only preload. Direct runtime checks have no
preload. Final canonical verification still requires the prescribed build
environment. No timing or performance result is claimed from these checks.
