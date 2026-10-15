# Isolated row-solver checkpoints

These direct debug-harness runs belong to `campaign/typing-rank1-integration`.
Main contains no inference implementation. Each directory retains raw test
output, statuses, source and harness hashes, invocation, and runtime limits.
The recorded artifact paths are historical and may contain newer builds now.

| Checkpoint | Observation |
|---|---|
| `dd254015e379` | Fresh flexible-to-rigid binding rejects; open-row alias occurs probe aborts with stack overflow. |
| `89bdd1ab7c3b` | Rigid identity and open-row occurs/alias cases pass. Module has 28 passes and one contradictory alias expectation. |
| `3545a21d6656` | Corrected expectation uses the established canonical variable. All 29 module tests pass. |
| `02a8efd2d503` | Two directional wrappers and flexible-to-rigid row binding fail. The compression probe fails during fixture setup. |
| `d03fdc2b78e1` | Corrected parent-chain fixture reaches the rejected constraint and observes compression after failure. All four new probes fail; 29 existing tests pass. |

The alias expectation correction retains the failed-operation and parent-link
assertions. The compression fixture correction allocates the target variable
last so rank tie-breaking leaves the intended path. Neither correction changes
solver behavior. Independent review still holds the implementation for the
four reproduced defects and immutable rigid-row lacks validation.
