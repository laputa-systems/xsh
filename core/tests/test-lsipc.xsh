use support.uu as uu
use support.util_linux as ul
use core.lib.gnu as gnu

type LsipcPair = {reference: uu.Ran, candidate: uu.Ran}

proc run_both(s: uu.Scene, args: List[Str]) [fs, process, env, error] -> Result[LsipcPair, Error] {
  let vars = {LC_ALL: "C", TZ: "UTC"}
  let reference = ul.reference(s, "lsipc", args, timeout: 5s, vars: vars)?
  let candidate = uu.invoke(s, "lsipc", args, vars: vars, timeout: 5s)?
  Ok({reference: reference, candidate: candidate})
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
