# Code mode report

## Sessions

Reported separately from the static trials. Tokens are summed over the session's model turns; `xsht api` bytes are the reference text the session's queries returned.

| Session | Verdict | Turns | Stopped by | api | edit | check | run | api bytes | Input | Output | Model s | Tool s | Effects on the public fixture |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| session-du-summary/xsh-baseline/0 | wrong | 8 | turns | 12 | 5 | 5 | 8 | 43,682 | 260,557 | 32,231 | 267 | 28 | 0 paths, 0 service calls |
| session-du-summary/xsh-selected/0 | rejected | 6 | time | 39 | 10 | 16 | 42 | 262,671 | 253,779 | 47,335 | 325 | 402 | 0 paths, 0 service calls |
| session-encode-batch/xsh-baseline/0 | correct | 8 | turns | 77 | 2 | 2 | 2 | 46,437 | 214,022 | 20,373 | 171 | 23 | 7 paths, 0 service calls |
| session-encode-batch/xsh-selected/0 | correct | 3 | model | 49 | 5 | 39 | 5 | 512,396 | 115,035 | 22,055 | 161 | 79 | 9 paths, 0 service calls |
| session-mirror-sync/xsh-baseline/0 | no-program | 8 | turns | 9 | 0 | 2 | 1 | 102,213 | 1,140,959 | 135,630 | 521 | 6 | 0 paths, 0 service calls |
| session-mirror-sync/xsh-selected/0 | correct | 8 | turns | 24 | 3 | 3 | 2 | 232,032 | 121,157 | 19,228 | 146 | 12 | 0 paths, 10 service calls |
| session-oncall-report/xsh-baseline/0 | rejected | 8 | turns | 17 | 5 | 2 | 5 | 81,816 | 253,349 | 20,223 | 165 | 8 | 0 paths, 0 service calls |
| session-oncall-report/xsh-selected/0 | rejected | 8 | turns | 30 | 12 | 8 | 10 | 172,286 | 241,851 | 43,794 | 287 | 17 | 0 paths, 0 service calls |

| Condition | Sessions | Correct | Mean turns | Mean api queries | Mean checks | Mean runs | Mean tokens |
|---|---|---|---|---|---|---|---|
| xsh-baseline | 4 | 1 | 8.0 | 28.8 | 2.8 | 4.0 | 519,336 |
| xsh-selected | 4 | 2 | 6.2 | 35.5 | 16.5 | 14.8 | 216,058 |

