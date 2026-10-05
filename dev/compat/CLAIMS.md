# Lane claims (two integrators share this branch)

Two Claude sessions are integrating into `campaign-utils` at the same time
(session `01Tqp2F3CMCPs4C3bnavNQyE` and session `01C8j4BapbLtdJ5sWr6nPfin`), and
`ls` and `native-proc-tty` were each built twice. Before starting a lane,
`git fetch origin campaign-utils`, read this file, and claim the lane here by
committing and pushing the claim first. Do not start a claimed lane. A lane
already merged on `campaign-utils` is done.

| Lane | Claimed by | State |
|---|---|---|
| text-b2, text-b1 | 01Tqp2 | merged |
| ls, native-proc-tty | 01C8j4 | merged |
| text-a2, cp, sort, fs-basic, printf-env | 01Tqp2 | running |
| native-bytes-hash, bytes-enc, checksums, perm, stat-du-df | 01Tqp2 | next |

Free for the other session: mv-ln, fs-misc, proc-a, tty-misc, legacy-buckets,
date, text-... any lane not listed above. Claim by editing this table.
