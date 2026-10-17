# lsipc fixture boundary

`lsipc` compares six live IPC resource classes. A native test must create them
inside private IPC and mount namespaces, then run both commands before cleanup.

The fixture owner must:

1. Create the private namespaces and mount fresh procfs, tmpfs at `/dev/shm`,
   and mqueue at `/dev/mqueue` before creating resources.
2. Run `keeper.py` with one `--resource` per required class. Track each
   returned System V ID and each exact POSIX name as soon as creation succeeds.
3. Keep each class below 16 objects and each object at or below 1 MiB.
4. Run reference and candidate against the same fixture state. Do not change
   ownership, permissions, values, or timestamps between those calls.
5. Remove only the recorded IDs and names when the keeper closes. Remove them
   on success, partial setup, assertion failure, timeout, and cancellation.

`ipcmk` exits after creation and cannot seed activity or keep the creator PID
alive. `keeper.py` uses Python ctypes to create and mutate resources in one
persistent process. It emits one JSON ready line with each exact ID or name,
owner and creator IDs, mode, and size. After readiness, `snapshot` emits one
JSON line with current kernel state. `release` or EOF removes the registered
objects. Snapshots match `/proc/sysvipc` records only by registered IDs and
inspect only registered POSIX names. Cleanup never scans a host table or path.
It removes registered IDs and names on release, EOF, setup failure, `SIGTERM`,
`SIGINT`, and `SIGHUP`.
Accepted classes are `sysv_queue`, `sysv_shmem`, `sysv_semaphore`,
`posix_mqueue`, `posix_shmem`, and `posix_semaphore`. Each selected System V
class has three records: readable and private objects owned by the first UID,
plus a private object created by the observer and then assigned to the owner.
Each selected POSIX class has readable and private objects owned by the first
UID. This stays below 16 records per class.

Use short random names for POSIX objects. Never scan `/proc/sysvipc`,
`/dev/shm`, or `/dev/mqueue` to decide what to remove. The namespace owner
handles namespace teardown and process cancellation; the keeper owns only the
objects in its in-memory registry. The original util-linux creator source is
[`sys-utils/ipcmk.c`](https://github.com/util-linux/util-linux/blob/v2.42.4/sys-utils/ipcmk.c).

Permission cases use two actual non-root UIDs with capabilities dropped. One
UID owns an object that the other UID cannot read. Visibility is collector-
dependent: the System V collectors use procfs records, POSIX shared memory
reads directory metadata, and POSIX semaphores and message queues attempt to
open each object for reading. Compare both identities against the reference;
do not assume a single permission rule across all six classes. A root-only
result does not cover that behavior.

The fixture owner also records each creator UID, owner UID, creator GID, owner
GID, mode, and value. `ipcmk` sets the initial creator and owner to the same
identity and initializes queue counts, shared-memory attachments, and
semaphore values to zero. Its creator process exits after creation, so it
cannot provide the live creator command line shown by `COMMAND`. Create
separate records when a case needs distinct creator and owner identities.
Seed nonzero System V and POSIX message counts and byte use, shared-memory
attach and detach times, and semaphore values and operation times only through
exact fixture-owned IDs. Keep any process needed for PID or command columns
alive through both comparisons.

The keeper runs permission-checked operations, snapshots, and removal with the
resource owner's effective UID. It reads System V status and applies `IPC_SET`
with the creator's effective UID, changing only ownership and mode. This keeps
the original creator authorized and preserves fields such as the System V
queue byte limit. The keeper blocks handled signals while it switches IDs,
restores effective root IDs, then restores the prior signal mask.

For System V semaphore snapshots, `keeper.py` also records each registered
element's value, waiter counts, and last-operation PID. It leaves the two
elements at values 2 and 3 after real `semop` activity. `observe.xsh` calls the
three read-only Linux IPC APIs eight times against the private POSIX semaphore,
POSIX message queue, and both System V semaphore elements. It reports values
or errno plus its descriptor counts before and after the calls. Native tests
run it as the owner and as the second UID with all capabilities dropped,
compare successful values with the keeper snapshot, and verify
missing-resource errors after release.

The pinned Alpine musl `sem_open` stores a named semaphore at
`/dev/shm/<name>`, while util-linux discovers named semaphores by scanning for
`sem.*` and opens the suffix. For each registered semaphore, the keeper adds a
hard link at the exact `sem.<name>` discovery path. Both paths refer to the same
semaphore inode. The keeper records that alias and removes it after unlinking
the POSIX name; it never scans the directory for cleanup.

The foundation lane owns private namespaces, mount setup, and the shared keeper
process API. This lane owns resource setup, identity capture, and cleanup of
registered IPC objects. Native tests use the shared scene wrapper to run the
keeper, reference, and candidate in that namespace. The case map is in
[`dev/util-linux/coverage/lsipc.md`](../../../../../dev/util-linux/coverage/lsipc.md).
