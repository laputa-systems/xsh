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
5. Remove only the recorded IDs and names when the fixture owner closes. Close
   on success, partial setup, assertion failure, timeout, and cancellation.

The pinned util-linux `ipcmk` 2.42.4 creates all six resource classes, but it
exits after creation and cannot seed the activity, ownership, and live PID
values used by these cases. The lane-owned `keeper.py` uses Python ctypes to
create and mutate the resources in one persistent process. It emits one JSON
ready line containing each exact ID or name, owner and creator IDs, mode, and
size. Its stdin accepts `release` or EOF. It removes only registered IDs and
names on release, EOF, setup failure, and `SIGTERM`, `SIGINT`, or `SIGHUP`.
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

The foundation lane owns private namespaces, mount setup, and process
lifecycle. This lane owns the keeper's resource-specific setup, identity
capture, and exact cleanup. Native tests will use the shared scene wrapper to
run the keeper, reference, and candidate in that namespace. The case map is in
[`dev/util-linux/coverage/lsipc.md`](../../../../../dev/util-linux/coverage/lsipc.md).
