use support.uu as uu
use support.util_linux as ul
use core.lib.gnu as gnu

type LsipcPair = {reference: uu.Ran, candidate: uu.Ran}
type IpcResource = {
  kind: Str,
  id: Int?,
  name: Str?,
  lookup_name: Str?,
  owner_uid: Int,
  owner_gid: Int,
  creator_uid: Int,
  creator_gid: Int,
  creator_pid: Int,
  mode: Int,
  size: Int,
}
type KeeperReady = {pid: Int, owner_uid: Int, observer_uid: Int, resources: List[IpcResource]}
type IpcKeeper = {handle: ul.Keeper, ready: KeeperReady}
type ResourceMode = {kind: Str, selector: Str, short_selector: Str, all_columns: Str}
type GlobalMode = {args: List[Str], resource: Str?}
type OptionAlias = {prefix: List[Str], long: List[Str], short: List[Str]}

const RESOURCE_MODES: List[ResourceMode] = [
  {kind: "sysv_queue", selector: "--queues", short_selector: "-q", all_columns: "KEY,ID,OWNER,PERMS,CUID,CUSER,CGID,CGROUP,UID,USER,GID,GROUP,CTIME,USEDBYTES,MSGS,SEND,RECV,LSPID,LRPID"},
  {kind: "sysv_shmem", selector: "--shmems", short_selector: "-m", all_columns: "KEY,ID,OWNER,PERMS,CUID,CUSER,CGID,CGROUP,UID,USER,GID,GROUP,CTIME,SIZE,NATTCH,STATUS,ATTACH,DETACH,COMMAND,CPID,LPID"},
  {kind: "sysv_semaphore", selector: "--semaphores", short_selector: "-s", all_columns: "KEY,ID,OWNER,PERMS,CUID,CUSER,CGID,CGROUP,UID,USER,GID,GROUP,CTIME,NSEMS,OTIME"},
  {kind: "posix_mqueue", selector: "--posix-mqueues", short_selector: "-Q", all_columns: "NAME,OWNER,PERMS,CUID,CUSER,CGID,CGROUP,MTIME"},
  {kind: "posix_shmem", selector: "--posix-shmems", short_selector: "-M", all_columns: "NAME,OWNER,PERMS,CUID,CUSER,CGID,CGROUP,MTIME"},
  {kind: "posix_semaphore", selector: "--posix-semaphores", short_selector: "-S", all_columns: "NAME,OWNER,PERMS,CUID,CUSER,CGID,CGROUP,MTIME"},
]

const RESOURCE_KINDS: List[Str] = [
  "sysv_queue",
  "sysv_shmem",
  "sysv_semaphore",
  "posix_mqueue",
  "posix_shmem",
  "posix_semaphore",
]

const GLOBAL_MODES: List[GlobalMode] = [
  {args: ["--global", "--queues"], resource: "MSGMNI"},
  {args: ["--global", "--posix-mqueues"], resource: "MQUMNI"},
  {args: ["--global", "--shmems"], resource: "SHMMNI"},
  {args: ["--global", "--semaphores"], resource: "SEMMNI"},
  {args: ["--global", "--posix-shmems"], resource: null},
  {args: ["--global", "--posix-semaphores"], resource: null},
]

const GLOBAL_COLUMNS = "RESOURCE,DESCRIPTION,LIMIT,USED,USE%"

proc start_keeper(s: uu.Scene, kinds: List[Str]) [fs, process, env, net, time, error] -> Result[IpcKeeper, Error] {
  ul.require_private_namespace()
  var args = [
    "--owner-uid", "10001",
    "--owner-gid", "10001",
    "--observer-uid", "10002",
    "--observer-gid", "10002",
  ]
  for kind in kinds {
    args = args.extend(["--resource", kind])
  }
  let script = fp"{s.ctx.core_dir}/tests/fixtures/util-linux/ipc/keeper.py"
  let handle = ul.start_keeper(s, script: script, args: args)?
  let ready = json.decode(handle.ready)?.require(KeeperReady)?
  assert ready.owner_uid == 10001 and ready.observer_uid == 10002
  var expected_count = 0
  for kind in kinds {
    if kind.starts_with("sysv_") { expected_count += 3 } else { expected_count += 2 }
    let _ = resource_of(ready, kind)?
  }
  assert ready.resources.len() == expected_count
  for resource in ready.resources {
    assert resource.kind in kinds
    assert resource.creator_pid == ready.pid
    assert resource.owner_uid == ready.owner_uid
    assert resource.owner_gid == 10001
    assert resource.creator_uid in [ready.owner_uid, ready.observer_uid]
    assert resource.creator_gid in [10001, 10002]
    assert resource.size <= 1048576
  }
  Ok({handle: handle, ready: ready})
}

proc resource_of(ready: KeeperReady, kind: Str) [error] -> Result[IpcResource, Error] {
  for resource in ready.resources {
    if resource.kind == kind { return Ok(resource) }
  }
  Err(error.failure(f"fixture did not create {kind}"))
}

proc resource_args(mode: ResourceMode, resource: IpcResource) -> List[Str] {
  if resource.kind.starts_with("sysv_") {
    [mode.selector, "--id", f"{resource.id ?? -1}", "--output", "ID"]
  } else {
    [mode.selector, "--name", resource.lookup_name ?? "", "--output", "NAME"]
  }
}

proc keeper_snapshot(handle: ul.Keeper) [net, process, time, error] -> Result[Bytes, Error] {
  ul.keeper_send(handle, "snapshot")
  Ok(bytes.from_text(ul.keeper_receive(handle)?))
}

proc assert_seeded_activity(ready: KeeperReady, snapshot: Bytes) [error] {
  let document = json.decode(snapshot.utf8() ?? "")?
  var index = 0
  for resource in ready.resources {
    assert json.get(document, ["resources", index, "kind"])?.require(Str)? == resource.kind
    if resource.kind == "sysv_queue" {
      assert json.get(document, ["resources", index, "state", "qnum"])?.require(Str)? == "1"
      assert json.get(document, ["resources", index, "state", "cbytes"])?.require(Str)? == "14"
      assert json.get(document, ["resources", index, "state", "lspid"])?.require(Str)? == f"{ready.pid}"
      assert json.get(document, ["resources", index, "state", "lrpid"])?.require(Str)? == f"{ready.pid}"
      assert json.get(document, ["resources", index, "state", "stime"])?.require(Str)?.parse_int()? > 0
      assert json.get(document, ["resources", index, "state", "rtime"])?.require(Str)?.parse_int()? > 0
      assert json.get(document, ["resources", index, "state", "ctime"])?.require(Str)?.parse_int()? > 0
    } else if resource.kind == "sysv_shmem" {
      assert json.get(document, ["resources", index, "state", "size"])?.require(Str)? == "4096"
      assert json.get(document, ["resources", index, "state", "nattch"])?.require(Str)? == "1"
      assert json.get(document, ["resources", index, "state", "cpid"])?.require(Str)? == f"{ready.pid}"
      assert json.get(document, ["resources", index, "state", "lpid"])?.require(Str)? == f"{ready.pid}"
      assert json.get(document, ["resources", index, "state", "atime"])?.require(Str)?.parse_int()? > 0
      assert json.get(document, ["resources", index, "state", "dtime"])?.require(Str)?.parse_int()? > 0
      assert json.get(document, ["resources", index, "state", "ctime"])?.require(Str)?.parse_int()? > 0
    } else if resource.kind == "sysv_semaphore" {
      assert json.get(document, ["resources", index, "state", "nsems"])?.require(Str)? == "2"
      assert json.get(document, ["resources", index, "state", "values"])?.require(List[Int])? == [0, 0]
      assert json.get(document, ["resources", index, "state", "otime"])?.require(Str)?.parse_int()? > 0
      assert json.get(document, ["resources", index, "state", "ctime"])?.require(Str)?.parse_int()? > 0
    } else if resource.kind == "posix_mqueue" {
      assert json.get(document, ["resources", index, "state", "uid"])?.require(Int)? == resource.owner_uid
      assert json.get(document, ["resources", index, "state", "gid"])?.require(Int)? == resource.owner_gid
      assert json.get(document, ["resources", index, "state", "mode"])?.require(Int)? == resource.mode
      assert json.get(document, ["resources", index, "state", "attributes", "curmsgs"])?.require(Int)? == 1
      assert json.get(document, ["resources", index, "state", "mtime_ns"])?.require(Int)? > 0
      assert json.get(document, ["resources", index, "state", "ctime_ns"])?.require(Int)? > 0
    } else if resource.kind == "posix_shmem" {
      assert json.get(document, ["resources", index, "state", "uid"])?.require(Int)? == resource.owner_uid
      assert json.get(document, ["resources", index, "state", "gid"])?.require(Int)? == resource.owner_gid
      assert json.get(document, ["resources", index, "state", "mode"])?.require(Int)? == resource.mode
      assert json.get(document, ["resources", index, "state", "size"])?.require(Int)? == 4096
      assert json.get(document, ["resources", index, "state", "mtime_ns"])?.require(Int)? > 0
      assert json.get(document, ["resources", index, "state", "ctime_ns"])?.require(Int)? > 0
    } else if resource.kind == "posix_semaphore" {
      assert json.get(document, ["resources", index, "state", "uid"])?.require(Int)? == resource.owner_uid
      assert json.get(document, ["resources", index, "state", "gid"])?.require(Int)? == resource.owner_gid
      assert json.get(document, ["resources", index, "state", "mode"])?.require(Int)? == resource.mode
      assert json.get(document, ["resources", index, "state", "value"])?.require(Int)? == 1
      assert json.get(document, ["resources", index, "state", "mtime_ns"])?.require(Int)? > 0
      assert json.get(document, ["resources", index, "state", "ctime_ns"])?.require(Int)? > 0
    }
    index += 1
  }
}

proc release_keeper(handle: ul.Keeper) [fs, net, process, error] -> Result[Unit, Error] {
  ul.keeper_send(handle, "release")
  ul.finish_keeper(handle)
  Ok()
}

proc run_both(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[LsipcPair, Error] {
  run_both_with_vars(s, args, vars: {LC_ALL: "C", TZ: "UTC"})
}

proc run_both_with_vars(s: uu.Scene, args: List[Str], vars: Record) [fs, process, env, error] -> Result[LsipcPair, Error] {
  let reference = ul.reference(s, "lsipc", args, timeout: 5s, vars: vars)?
  let candidate = uu.invoke(s, "lsipc", args, vars: vars, timeout: 5s)?
  Ok({reference: reference, candidate: candidate})
}

proc run_both_as_user(s: uu.Scene, args: List[Str], uid: Int) [fs, process, env, error] -> Result[LsipcPair, Error] {
  let vars = {LC_ALL: "C", TZ: "UTC"}
  let reference = ul.reference_as_user(s, "lsipc", args, uid:, vars:, timeout: 5s)?
  let candidate = ul.candidate_as_user(s, "lsipc", args, uid:, vars:, timeout: 5s)?
  Ok({reference: reference, candidate: candidate})
}

proc compare_option_alias(s: uu.Scene, prefix: List[Str], long: List[Str], short: List[Str]) [fs, process, env, error] -> Result[Unit, Error] {
  let long_pair = run_both(s, prefix.extend(long))?
  let short_pair = run_both(s, prefix.extend(short))?
  ul.compare(long_pair.reference, long_pair.candidate)
  ul.compare(short_pair.reference, short_pair.candidate)
  assert long_pair.candidate.status == short_pair.candidate.status
  assert long_pair.candidate.stdout == short_pair.candidate.stdout
  assert long_pair.candidate.stderr == short_pair.candidate.stderr
  Ok()
}

test test_lsipc_help_lists_resource_modes_and_columns { |ctx|
  let s = uu.scene(ctx)?
  let pair = run_both(s, ["--help"])?
  ul.compare(pair.reference, pair.candidate)
  assert pair.candidate.status == 0
  assert pair.candidate.stderr == b""
  let stdout = pair.candidate.stdout.utf8() ?? ""
  assert "Usage: lsipc [options]" in stdout
  for name in [
    "--shmems",
    "--posix-shmems",
    "--queues",
    "--posix-mqueues",
    "--semaphores",
    "--posix-semaphores",
    "--global",
    "--id",
    "--name",
    "--output",
    "--noheadings",
    "--notruncate",
    "--export",
    "--json",
    "--list",
    "--newline",
    "--raw",
    "--shell",
    "--bytes",
    "--creator",
    "--numeric-perms",
    "--time",
    "--time-format",
    "--help",
    "--version",
    "KEY",
    "ID",
    "OWNER",
    "PERMS",
    "CUID",
    "CUSER",
    "CGID",
    "CGROUP",
    "UID",
    "USER",
    "GID",
    "GROUP",
    "CTIME",
    "NAME",
    "MTIME",
    "USEDBYTES",
    "MSGS",
    "SEND",
    "RECV",
    "LSPID",
    "LRPID",
    "SIZE",
    "NATTCH",
    "STATUS",
    "ATTACH",
    "DETACH",
    "COMMAND",
    "CPID",
    "LPID",
    "NSEMS",
    "OTIME",
    "RESOURCE",
    "DESCRIPTION",
    "LIMIT",
    "USED",
    "USE%",
    "SVAL",
  ] {
    assert name in stdout, f"missing {name} from help"
  }

  let short = run_both(s, ["-h"])?
  ul.compare(short.reference, short.candidate)
  assert stdout == (short.candidate.stdout.utf8() ?? "")
}

test test_lsipc_version_preserves_xsh_provenance { |ctx|
  let s = uu.scene(ctx)?
  let pair = run_both(s, ["--version"])?
  ul.version_identity(pair.reference, pair.candidate)
  assert pair.candidate.status == 0
  assert pair.candidate.stderr == b""
  let stdout = pair.candidate.stdout.utf8() ?? ""
  let expected = gnu.version_text("lsipc") + "\n"
  assert stdout == expected, stdout

  let short = run_both(s, ["-V"])?
  ul.version_identity(short.reference, short.candidate)
  assert short.candidate.stdout == pair.candidate.stdout
}

test test_lsipc_short_option_aliases_match_long_options { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)
  let queue_mode = RESOURCE_MODES[0]
  let queue = resource_of(fixture.ready, queue_mode.kind)?
  let queue_id = f"{queue.id ?? -1}"
  let mqueue_mode = RESOURCE_MODES[3]
  let mqueue = resource_of(fixture.ready, mqueue_mode.kind)?
  let mqueue_name = mqueue.lookup_name ?? ""
  let aliases: List[OptionAlias] = [
    {prefix: [], long: ["--global"], short: ["-g"]},
    {prefix: [queue_mode.selector], long: ["--id", queue_id], short: ["-i", queue_id]},
    {prefix: [mqueue_mode.selector], long: ["--name", mqueue_name], short: ["-N", mqueue_name]},
    {prefix: [queue_mode.selector], long: ["--output", queue_mode.all_columns], short: ["-o", queue_mode.all_columns]},
    {prefix: [queue_mode.selector], long: ["--export"], short: ["-e"]},
    {prefix: [queue_mode.selector], long: ["--json"], short: ["-J"]},
    {prefix: [queue_mode.selector], long: ["--list"], short: ["-l"]},
    {prefix: [queue_mode.selector], long: ["--newline"], short: ["-n"]},
    {prefix: [queue_mode.selector], long: ["--raw"], short: ["-r"]},
    {prefix: [queue_mode.selector, "--export"], long: ["--shell"], short: ["-y"]},
    {prefix: [queue_mode.selector], long: ["--bytes"], short: ["-b"]},
    {prefix: [queue_mode.selector], long: ["--creator"], short: ["-c"]},
    {prefix: [queue_mode.selector], long: ["--numeric-perms"], short: ["-P"]},
    {prefix: [queue_mode.selector], long: ["--time"], short: ["-t"]},
  ]
  for alias in aliases {
    compare_option_alias(s, alias.prefix, alias.long, alias.short)?
  }
  release_keeper(fixture.handle)?
}

test test_lsipc_rejects_unknown_time_format { |ctx|
  let s = uu.scene(ctx)?
  let pair = run_both(s, ["--time-format=invalid"])?
  ul.compare(pair.reference, pair.candidate)
  assert pair.candidate.status == 1
  assert pair.candidate.stdout == b""
  assert "unknown time format: invalid" in (pair.candidate.stderr.utf8() ?? "")
}

test test_lsipc_rejects_unknown_and_inapplicable_columns { |ctx|
  let s = uu.scene(ctx)?
  let unknown = run_both(s, ["--queues", "--output", "NOT_A_COLUMN"])?
  ul.compare(unknown.reference, unknown.candidate)
  assert unknown.candidate.status == 1
  assert unknown.candidate.stdout == b""
  assert "unknown column: NOT_A_COLUMN" in (unknown.candidate.stderr.utf8() ?? "")

  let inapplicable = run_both(s, ["--queues", "--output", "SVAL"])?
  ul.compare(inapplicable.reference, inapplicable.candidate)
  assert inapplicable.candidate.status == 1
  assert inapplicable.candidate.stdout == b""
  assert "column SVAL does not apply to the specified IPC" in (inapplicable.candidate.stderr.utf8() ?? "")

  let posix_semaphore = run_both(s, ["--posix-semaphores", "--output", "SVAL"])?
  ul.compare(posix_semaphore.reference, posix_semaphore.candidate)
  assert posix_semaphore.candidate.status == 1
  assert posix_semaphore.candidate.stdout == b""
  assert "column SVAL does not apply to the specified IPC" in (posix_semaphore.candidate.stderr.utf8() ?? "")

  let mqueue_default_columns = run_both(s, ["--posix-mqueues", "--output", "USEDBYTES,MSGS"])?
  ul.compare(mqueue_default_columns.reference, mqueue_default_columns.candidate)
  assert mqueue_default_columns.candidate.status == 1
  assert mqueue_default_columns.candidate.stdout == b""
  assert mqueue_default_columns.candidate.stderr == b"lsipc: column USEDBYTES,MSGS does not apply to the specified IPC\n"

  for entry in [
    {selector: "--posix-mqueues", column: "MSGS"},
    {selector: "--posix-shmems", column: "SIZE"},
  ] {
    let pair = run_both(s, [entry.selector, "--output", entry.column])?
    ul.compare(pair.reference, pair.candidate)
    assert pair.candidate.status == 1, f"{entry.selector} {entry.column}"
    assert pair.candidate.stdout == b"", f"{entry.selector} {entry.column}"
    assert pair.candidate.stderr != b"", f"{entry.selector} {entry.column}"
  }

  for columns in ["", ",ID", "ID,,OWNER"] {
    let pair = run_both(s, ["--queues", "--output", columns])?
    ul.compare(pair.reference, pair.candidate)
    assert pair.candidate.status == 1, columns
    assert pair.candidate.stdout == b"", columns
  }

  let global_column = run_both(s, ["--global", "--queues", "--output", "USEDBYTES"])?
  ul.compare(global_column.reference, global_column.candidate)
  assert global_column.candidate.status == 1
  assert global_column.candidate.stdout == b""
  assert global_column.candidate.stderr != b""
}

test test_lsipc_rejects_invalid_options_and_missing_arguments { |ctx|
  let s = uu.scene(ctx)?
  let cases = [
    ["--not-a-real-option"],
    ["--id"],
    ["--name"],
    ["--output"],
    ["--time-format"],
    ["--queues", "--id", "not-an-id"],
  ]
  for args in cases {
    let pair = run_both(s, args)?
    ul.compare(pair.reference, pair.candidate)
    assert pair.candidate.status == 1, f"accepted malformed arguments {args.join(" ")}"
    assert pair.candidate.stdout == b"", f"wrote stdout for malformed arguments {args.join(" ")}"
    assert pair.candidate.stderr != b"", f"omitted diagnostics for malformed arguments {args.join(" ")}"
  }
}

test test_lsipc_rejects_each_exclusive_option_pair { |ctx|
  let s = uu.scene(ctx)?
  let pairs = [
    ["--json", "--export"],
    ["--json", "--list"],
    ["--json", "--newline"],
    ["--json", "--raw"],
    ["--export", "--list"],
    ["--export", "--newline"],
    ["--export", "--raw"],
    ["--list", "--newline"],
    ["--list", "--raw"],
    ["--newline", "--raw"],
    ["--name", "/owned", "--global"],
    ["--name", "/owned", "--id", "1"],
    ["--global", "--id", "1"],
    ["--creator", "--output", "ID"],
    ["--creator", "--time"],
    ["--output", "ID", "--time"],
    ["--shmems", "--posix-shmems"],
    ["--shmems", "--queues"],
    ["--shmems", "--posix-mqueues"],
    ["--shmems", "--semaphores"],
    ["--shmems", "--posix-semaphores"],
    ["--queues", "--posix-shmems"],
    ["--queues", "--posix-mqueues"],
    ["--queues", "--semaphores"],
    ["--queues", "--posix-semaphores"],
    ["--posix-shmems", "--posix-mqueues"],
    ["--posix-shmems", "--semaphores"],
    ["--posix-shmems", "--posix-semaphores"],
    ["--posix-mqueues", "--semaphores"],
    ["--posix-mqueues", "--posix-semaphores"],
    ["--semaphores", "--posix-semaphores"],
  ]
  for pair in pairs {
    let both = run_both(s, pair)?
    ul.compare(both.reference, both.candidate)
    assert both.candidate.status == 1, f"accepted exclusive options {pair.join(" ")}"
    assert both.candidate.stdout == b"", f"wrote stdout for {pair.join(" ")}"
    assert both.candidate.stderr != b"", f"omitted diagnostics for {pair.join(" ")}"
  }
}

test test_lsipc_rejects_id_and_name_without_matching_modes { |ctx|
  let s = uu.scene(ctx)?
  let id = run_both(s, ["--id", "1"])?
  ul.compare(id.reference, id.candidate)
  assert id.candidate.status == 1
  assert id.candidate.stdout == b""
  assert "--global is mutually exclusive" in (id.candidate.stderr.utf8() ?? "")

  let name = run_both(s, ["--name", "/owned"])?
  ul.compare(name.reference, name.candidate)
  assert name.candidate.status == 1
  assert name.candidate.stdout == b""
  assert "--global is mutually exclusive" in (name.candidate.stderr.utf8() ?? "")
}

test test_lsipc_default_and_global_summary_outputs_match_reference { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  let default = run_both(s, [])?
  ul.compare(default.reference, default.candidate)
  assert default.candidate.status == 0
  assert default.candidate.stdout != b""
  let default_text = default.candidate.stdout.utf8() ?? ""
  for resource in ["MSGMNI", "MQUMNI", "SHMMNI", "SEMMNI"] {
    assert resource in default_text, f"default summary omitted {resource}"
  }

  let explicit_global = run_both(s, ["--global"])?
  ul.compare(explicit_global.reference, explicit_global.candidate)
  assert explicit_global.candidate.stdout == default.candidate.stdout

  let explicit_columns = run_both(s, ["--global", "--output", GLOBAL_COLUMNS])?
  ul.compare(explicit_columns.reference, explicit_columns.candidate)
  assert explicit_columns.candidate.status == 0

  for mode in GLOBAL_MODES {
    let pair = run_both(s, mode.args)?
    ul.compare(pair.reference, pair.candidate)
    assert pair.candidate.status == 0, mode.args.join(" ")
    if let resource = mode.resource {
      assert resource in (pair.candidate.stdout.utf8() ?? ""), mode.args.join(" ")
    }

    let columns = run_both(s, mode.args.extend(["--output", GLOBAL_COLUMNS]))?
    ul.compare(columns.reference, columns.candidate)
    assert columns.candidate.status == 0, mode.args.join(" ")
  }
  release_keeper(fixture.handle)?
}

test test_lsipc_keeper_reference_observes_all_classes_and_release_removes_them { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  let before = keeper_snapshot(fixture.handle)?
  assert_seeded_activity(fixture.ready, before)
  for mode in RESOURCE_MODES {
    let resource = resource_of(fixture.ready, mode.kind)?
    let args = resource_args(mode, resource)
    let reference = ul.reference(s, "lsipc", args, timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
    let identifier = if resource.kind.starts_with("sysv_") {
      f"{resource.id ?? -1}"
    } else {
      resource.lookup_name ?? ""
    }
    assert reference.status == 0, args.join(" ")
    assert reference.stderr == b"", args.join(" ")
    assert identifier in (reference.stdout.utf8() ?? ""), args.join(" ")
  }
  let after = keeper_snapshot(fixture.handle)?
  assert before == after, "reference collection changed registered IPC state"

  release_keeper(fixture.handle)?
  for mode in RESOURCE_MODES {
    let resource = resource_of(fixture.ready, mode.kind)?
    let args = resource_args(mode, resource)
    let reference = ul.reference(s, "lsipc", args, timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
    let identifier = if resource.kind.starts_with("sysv_") {
      f"{resource.id ?? -1}"
    } else {
      resource.lookup_name ?? ""
    }
    assert reference.status == 0, args.join(" ")
    assert reference.stderr != b"", args.join(" ")
    assert identifier not in (reference.stdout.utf8() ?? ""), args.join(" ")
  }
}

test test_lsipc_keeper_cancellation_removes_registered_resources { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  let selected = [
    {mode: RESOURCE_MODES[0], resource: resource_of(fixture.ready, "sysv_queue")?},
    {mode: RESOURCE_MODES[1], resource: resource_of(fixture.ready, "sysv_shmem")?},
    {mode: RESOURCE_MODES[2], resource: resource_of(fixture.ready, "sysv_semaphore")?},
    {mode: RESOURCE_MODES[3], resource: resource_of(fixture.ready, "posix_mqueue")?},
    {mode: RESOURCE_MODES[4], resource: resource_of(fixture.ready, "posix_shmem")?},
    {mode: RESOURCE_MODES[5], resource: resource_of(fixture.ready, "posix_semaphore")?},
  ]
  fixture.handle.child.cancel(signal: "TERM", kill_after: 2s)

  for entry in selected {
    let args = resource_args(entry.mode, entry.resource)
    let reference = ul.reference(s, "lsipc", args, timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
    let identifier = if entry.resource.kind.starts_with("sysv_") {
      f"{entry.resource.id ?? -1}"
    } else {
      entry.resource.lookup_name ?? ""
    }
    assert reference.status == 0, args.join(" ")
    assert reference.stderr != b"", args.join(" ")
    assert identifier not in (reference.stdout.utf8() ?? ""), args.join(" ")
  }
}

test test_lsipc_default_and_explicit_columns_cover_all_six_resource_modes { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  for mode in RESOURCE_MODES {
    let defaults = run_both(s, [mode.selector])?
    ul.compare(defaults.reference, defaults.candidate)
    assert defaults.candidate.status == 0, mode.selector
    assert defaults.candidate.stdout != b"", mode.selector

    let short = run_both(s, [mode.short_selector])?
    ul.compare(short.reference, short.candidate)
    assert short.candidate.stdout == defaults.candidate.stdout, mode.short_selector

    let columns = run_both(s, [mode.selector, "--output", mode.all_columns])?
    ul.compare(columns.reference, columns.candidate)
    assert columns.candidate.status == 0, f"{mode.selector}: {columns.candidate.stderr.utf8() ?? ""}"
    assert columns.candidate.stdout != b"", mode.selector
  }

  let trailing_comma = run_both(s, ["--queues", "--output", "ID,"])?
  ul.compare(trailing_comma.reference, trailing_comma.candidate)
  assert trailing_comma.candidate.status == 0
  release_keeper(fixture.handle)?
}

test test_lsipc_id_and_name_select_registered_objects { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  for mode in RESOURCE_MODES {
    let resource = resource_of(fixture.ready, mode.kind)?
    if mode.kind.starts_with("sysv_") {
      let id = resource.id ?? -1
      assert id >= 0, mode.kind
      let pair = run_both(s, [mode.selector, "--id", f"{id}"])?
      ul.compare(pair.reference, pair.candidate)
      assert pair.candidate.status == 0, mode.kind
      assert pair.candidate.stdout != b"", mode.kind
    } else {
      let name = resource.lookup_name ?? ""
      assert name != "", mode.kind
      let pair = run_both(s, [mode.selector, "--name", name])?
      ul.compare(pair.reference, pair.candidate)
      assert pair.candidate.status == 0, mode.kind
      assert pair.candidate.stdout != b"", mode.kind
    }
  }
  for mode in [RESOURCE_MODES[0], RESOURCE_MODES[2]] {
    let pair = run_both(s, [mode.selector, "--id", "-2", "--raw"])?
    ul.compare(pair.reference, pair.candidate)
    assert pair.candidate.status == 0, mode.selector
    let output = pair.candidate.stdout.utf8() ?? ""
    for resource in fixture.ready.resources {
      if resource.kind == mode.kind {
        assert f"{resource.id ?? -1}" in output, mode.selector
      }
    }
  }
  release_keeper(fixture.handle)?
}

test test_lsipc_missing_ids_and_names_preserve_reference_status_and_diagnostics { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  for mode in RESOURCE_MODES {
    let resource = resource_of(fixture.ready, mode.kind)?
    let args = if mode.kind.starts_with("sysv_") {
      [mode.selector, "--id", "2147483647"]
    } else {
      let name = resource.lookup_name ?? ""
      assert name != "", mode.kind
      [mode.selector, "--name", f"{name}-missing"]
    }
    let pair = run_both(s, args)?
    ul.compare(pair.reference, pair.candidate)
    assert pair.candidate.status == 0, args.join(" ")
    assert pair.candidate.stderr != b"", args.join(" ")
  }
  release_keeper(fixture.handle)?
}

test test_lsipc_creator_and_time_options_report_live_metadata { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)
  var distinct_creator = false
  for resource in fixture.ready.resources {
    if resource.kind.starts_with("sysv_") and resource.creator_uid != resource.owner_uid {
      distinct_creator = true
    }
  }
  assert distinct_creator

  for mode in RESOURCE_MODES {
    let pair = run_both(s, [mode.selector, "--creator"])?
    ul.compare(pair.reference, pair.candidate)
    assert pair.candidate.status == 0, mode.selector
    assert pair.candidate.stdout != b"", mode.selector

    let raw = run_both(s, [mode.selector, "--creator", "--raw"])?
    ul.compare(raw.reference, raw.candidate)
    assert raw.candidate.status == 0, mode.selector
    if mode.kind.starts_with("sysv_") {
      let output = raw.candidate.stdout.utf8() ?? ""
      assert "10001" in output, mode.selector
      assert "10002" in output, mode.selector
    }
  }

  for mode in RESOURCE_MODES[0..3] {
    let timed = run_both(s, [mode.selector, "--time"])?
    ul.compare(timed.reference, timed.candidate)
    assert timed.candidate.status == 0, mode.selector
    for format in ["short", "full", "iso"] {
      let pair = run_both(s, [mode.selector, "--time", "--time-format", format])?
      ul.compare(pair.reference, pair.candidate)
      assert pair.candidate.status == 0, f"{mode.selector} {format}"
      assert pair.candidate.stdout != b"", f"{mode.selector} {format}"
    }
  }
  release_keeper(fixture.handle)?
}

test test_lsipc_bytes_and_numeric_permissions_match_reference { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  for mode in RESOURCE_MODES {
    for option in ["--bytes", "--numeric-perms"] {
      let pair = run_both(s, [mode.selector, option])?
      ul.compare(pair.reference, pair.candidate)
      assert pair.candidate.status == 0, f"{mode.selector} {option}"
      assert pair.candidate.stdout != b"", f"{mode.selector} {option}"
    }
  }
  let global = run_both(s, ["--global", "--bytes"])?
  ul.compare(global.reference, global.candidate)
  assert global.candidate.status == 0
  release_keeper(fixture.handle)?
}

test test_lsipc_output_modes_and_table_controls_match_reference { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  for mode in RESOURCE_MODES {
    for args in [
      [mode.selector, "--json"],
      [mode.selector, "--export"],
      [mode.selector, "--list"],
      [mode.selector, "--newline"],
      [mode.selector, "--raw"],
      [mode.selector, "--export", "--shell"],
      [mode.selector, "--noheadings"],
      [mode.selector, "--notruncate"],
    ] {
      let pair = run_both(s, args)?
      ul.compare(pair.reference, pair.candidate)
      assert pair.candidate.status == 0, args.join(" ")
      assert pair.candidate.stdout != b"", args.join(" ")
      if "--json" in args { let _ = json.decode(pair.candidate.stdout.utf8() ?? "")? }
    }
  }

  let width = {LC_ALL: "C", TZ: "UTC", COLUMNS: "24"}
  let truncated = run_both_with_vars(s, ["--shmems", "--output", "ID,COMMAND"], vars: width)?
  ul.compare(truncated.reference, truncated.candidate)
  let notruncated = run_both_with_vars(s, ["--shmems", "--output", "ID,COMMAND", "--notruncate"], vars: width)?
  ul.compare(notruncated.reference, notruncated.candidate)
  let keeper_path = f"{s.ctx.core_dir}/tests/fixtures/util-linux/ipc/keeper.py"
  assert truncated.candidate.stdout != notruncated.candidate.stdout
  assert keeper_path in (notruncated.candidate.stdout.utf8() ?? "")
  assert keeper_path not in (truncated.candidate.stdout.utf8() ?? "")

  let process_columns = run_both(s, ["--shmems", "--notruncate", "--output", "CPID,LPID,COMMAND"])?
  ul.compare(process_columns.reference, process_columns.candidate)
  assert process_columns.candidate.status == 0
  let process_output = process_columns.candidate.stdout.utf8() ?? ""
  assert f"{fixture.ready.pid}" in process_output
  assert keeper_path in process_output

  let ordered = run_both(s, ["--queues", "--output", "LRPID,ID,KEY,LRPID"])?
  ul.compare(ordered.reference, ordered.candidate)
  assert ordered.candidate.status == 0
  assert ordered.candidate.stdout != b""
  for mode in ["--json", "--export", "--list", "--raw"] {
    let args = ["--global", mode]
    let pair = run_both(s, args)?
    ul.compare(pair.reference, pair.candidate)
    assert pair.candidate.status == 0, args.join(" ")
    assert pair.candidate.stdout != b"", args.join(" ")
    if "--json" in args { let _ = json.decode(pair.candidate.stdout.utf8() ?? "")? }
  }
  release_keeper(fixture.handle)?
}

test test_lsipc_visibility_matches_for_owner_and_unprivileged_observer { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  for mode in RESOURCE_MODES {
    let owner = run_both_as_user(s, [mode.selector], 10001)?
    ul.compare(owner.reference, owner.candidate)
    assert owner.candidate.status == 0, f"owner {mode.selector}"
    assert owner.candidate.stdout != b"", f"owner {mode.selector}"

    let observer = run_both_as_user(s, [mode.selector], 10002)?
    ul.compare(observer.reference, observer.candidate)
    assert observer.candidate.status == 0, f"observer {mode.selector}"
    assert observer.candidate.stdout != b"", f"observer {mode.selector}"
    if mode.kind in ["posix_mqueue", "posix_semaphore"] {
      assert owner.candidate.stdout != observer.candidate.stdout, mode.selector
    }
  }
  release_keeper(fixture.handle)?
}

test test_lsipc_collectors_leave_registered_resources_unchanged { |ctx|
  let s = uu.scene(ctx)?
  let fixture = start_keeper(s, RESOURCE_KINDS)?
  defer ul.stop_keeper(fixture.handle)

  for mode in RESOURCE_MODES {
    let before = keeper_snapshot(fixture.handle)?
    let reference = ul.reference(s, "lsipc", [mode.selector], timeout: 5s, vars: {LC_ALL: "C", TZ: "UTC"})?
    let after_reference = keeper_snapshot(fixture.handle)?
    let candidate = uu.invoke(s, "lsipc", [mode.selector], vars: {LC_ALL: "C", TZ: "UTC"}, timeout: 5s)?
    let after_candidate = keeper_snapshot(fixture.handle)?
    ul.compare(reference, candidate, reference_state: after_reference, candidate_state: after_candidate)
    assert before == after_reference, f"reference changed {mode.kind} fixture state"
    assert before == after_candidate, f"candidate changed {mode.kind} fixture state"
  }
  release_keeper(fixture.handle)?
}
