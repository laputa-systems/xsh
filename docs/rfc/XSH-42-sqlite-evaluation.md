# XSH-42: Optional SQLite boundary

## Decision

Reject the boundary for now. No examined consumer demonstrates that SQLite
would remove enough bespoke persistence machinery to earn a new dependency and
host API. Revisit with a consumer and a measured removal. Early boot and
immutable package receipts remain independent of SQLite.

## Evidence

Searches from the XSH root and `/home/josh/repos/laputa-systems/laputa`,
respectively (the latter also has `system/timers.xsh`, not `timers/`):

```sh
rg -n -i 'sqlite|catalog|index|receipt|history|journal|lock|write_atomic|read_text|\.write\(|rename|flock' core/ src/modules/
rg -n -i 'sqlite|catalog|index|receipt|history|journal|lock|write_atomic|read_text|\.write\(|rename|flock' pm/ xinit/ timers/ trust/ packages/laputa-*/
rg -n -i 'sqlite|catalog|receipt|history|journal|lock|write_atomic|read_text|rename|flock' system/timers.xsh
```

The second search reported the absent `timers/` directory and searched the
other named paths. Laputa paths below are relative to its repository root.

| Candidate | Mechanism and assessment |
|---|---|
| XSH user and group databases | `src/modules/user.rs:108`, `:128`, `:129`, `:137` and `src/modules/group.rs:49`, `:62` read and rewrite system files by atomic rename (`user.rs:345`, `group.rs:214`). Concurrent writers and the separate passwd and shadow writes can require coordination, but these externally consumed file formats cannot be replaced by SQLite transactions. |
| XSH interface and DHCP state | `core/ifup.xsh:249`, `:259` and `core/ifdown.xsh:296`, `:310` rewrite interface state. DHCP validates and atomically writes a bounded per-interface record (`core/lib/dhcp_store.xsh:282`, `:359`, `:376`) under an interface lock (`core/lib/dhcp_live.xsh:302`, `:371`). DNS contributions are read and composed under a lock into the resolver file (`core/lib/dhcp_dns.xsh:264`, `:328`, `:335`, `:345`). Writers need coordination, but SQLite cannot transact the kernel or resolver-file effects; these bounded records do not show a query need. |
| PM catalog and receipts | `pm/catalog.xsh:54`, `:121` builds an in-memory catalog. Immutable artifacts use per-key locks and staged directory publication (`pm/store.xsh:1`, `:364`, `:398`, `:415`); root receipts are written and verified with the root (`pm/root.xsh:785`, `:808`, `:856`). Concurrent publication is already coordinated and filesystem artifacts must still be published. No database query need is shown. |
| PM accepted repository snapshot | `pm/repo_snapshot.xsh:220`, `:262`, `:269`, `:286` reads one high-water record under a lock and replaces it by rename. Concurrent writers need serialization and rollback checks; a single record does not require database queries or a multi-record transaction. |
| Trust store | `trust/store.xsh:156`, `:181`, `:239`, `:264`, `:397`, `:447` locks, atomically replaces a journal and high-water files, publishes generation directories and flips a symlink. `trust/update.xsh:402`, `:614`, `:634`, `:637`, `:641` recovers phases around those filesystem effects. Concurrent updates need serialization and crash recovery; SQLite cannot atomically commit those external effects, so the journal and recovery have not been shown removable. |
| Timer scheduler | `system/timers.xsh:1183`, `:1292`, `:1677`, `:1714` keeps per-attempt writer records, settles live processes, writes a status snapshot and holds one scheduler lock. The lock excludes concurrent schedulers; process-liveness recovery remains outside a database transaction. No relational query need is shown. |
| xinit | `xinit/xinit.xsh:1124`, `:1175`, `:1420`, `:1444` reads and atomically replaces service status. Control claims use filesystem rename (`:2290`). State coordinates processes and early boot, so a database would retain those boundaries and add a boot dependency. No query need is shown. |

These assessments do not prove that ordinary files solve every concurrency or
crash issue. They establish no consumer whose persistence machinery is shown to
be removed by the proposed boundary.
