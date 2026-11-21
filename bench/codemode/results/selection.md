# Guide selection

Applied `selection-rule.md` to the development runs `dev-initial` and `dev-revision`
(12 tasks × 2 samples per contender):

| Contender | Correct within 3 | Qualifies (≥ 95.8 − 5) | Tokens per solved task | First-attempt correct |
|---|---|---|---|---|
| `xsh-baseline` (current docs, 22,766 tokens) | 95.8% | yes | 50,538 | 70.8% |
| `xsh-candidate` (2,107 tokens) | 91.7% | yes | 29,107 | 54.2% |
| `xsh-revised` (2,451 tokens) | 95.8% | yes | 26,156 | 70.8% |

All three qualify; `xsh-revised` has the lowest total tokens per solved task, 10% below
the candidate (outside the 5% tie band) and 48% below the current documentation. The
selected guide is `revised.md`, frozen byte for byte as `contexts/selected.md`:

- SHA-256 `b000c759c7ce6523a27405b345923187b1b33a7d8762ebed3992d9a9e1f6d357`
- 8,178 bytes; 2,451 input tokens measured with `codemode measure-contexts`
  (2,538 with the one-line probe, minus 87 for the probe alone)
- 50 of 50 claim checks hold (`codemode verify-guide selected.md`)

Development evidence only: the revision ran after the initial matrix, on the same tasks,
toolchain and model; matched first-attempt difference against the current docs is 0.0
points [−29.2, 29.2] and against the candidate +16.7 [0.0, 33.3].
