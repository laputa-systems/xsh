##! Exact comparisons against the qualified util-linux reference in native scenes.
use uu
use core.lib.gnu

## Requires the coordinator's private IPC, mount and PID namespaces before privileged fixtures.
export proc require_private_namespace() [fs, env, error] {
  assert e"XSH_UTIL_LINUX_PRIVATE"? == "1"
  for name in ["ipc", "mnt", "pid"] {
    let outer = env.get(f"XSH_UTIL_LINUX_PARENT_{name}")?
    assert fp"/proc/self/ns/{name}".readlink()?.display() != outer
  }
}

## Runs the reference with lossless arguments in the candidate's scene and environment.
export proc reference_paths(
  s: uu.Scene,
  command: Str,
  args: List[Path],
  timeout = 5s,
  vars: Record = {},
) [fs, process, error] -> Result[uu.Ran, Error] {
  assert command in ["lslocks", "lsipc", "lsmem", "mountpoint", "namei"]
  assert timeout > 0ms and timeout <= 30s
  let executable = if command == "mountpoint" { p"/bin/mountpoint" } else { fp"/usr/bin/{command}" }
  let stdout = uu.at(s, ".util-linux-stdout")
  let stderr = uu.at(s, ".util-linux-stderr")
  let metadata = json.encode({caller_keys: vars.keys(), umask: null})?
  let launcher = fp"{s.ctx.core_dir}/tests/support/uu-launch.xsh"
  let argv = [s.ctx.xsh_bin, launcher, fp"{metadata}", executable].extend(args)
  let plan = process.command_argv(s.ctx.xsh_bin, argv, s.root, vars, b"", stdout, stderr, timeout:)
  let status = process.run(plan)?.exit_code()?
  let out = stdout.read_bytes()?
  let err = stderr.read_bytes()?
  stdout.remove()
  stderr.remove()
  Ok({util: command, args: [word.display() for word in args], status: status, stdout: out, stderr: err})
}

## Runs the reference with text arguments and the same capture contract as uu.invoke.
export proc reference(
  s: uu.Scene,
  command: Str,
  args: List[Str],
  timeout = 5s,
  vars: Record = {},
) [fs, process, error] -> Result[uu.Ran, Error] {
  reference_paths(s, command, [fp"{word}" for word in args], timeout:, vars:)
}

## Compares every captured byte, status, argument and explicit fixture-state snapshot.
export pure equal(reference: uu.Ran, candidate: uu.Ran, reference_state = b"", candidate_state = b"") -> Bool {
  reference.util == candidate.util and reference.args == candidate.args and
    reference.status == candidate.status and reference.stdout == candidate.stdout and
    reference.stderr == candidate.stderr and reference_state == candidate_state
}

## Asserts exact equality without normalizing identifiers, ordering or whitespace.
export proc compare(reference: uu.Ran, candidate: uu.Ran, reference_state = b"", candidate_state = b"") {
  assert equal(reference, candidate, reference_state:, candidate_state:), f"{reference.util}: reference and candidate differ"
}

## Asserts each implementation's exact provenance for the sole version exception.
export proc version_identity(reference: uu.Ran, candidate: uu.Ran) {
  assert reference.util == candidate.util
  assert reference.args == ["--version"] and candidate.args == ["--version"]
  assert reference.status == 0 and candidate.status == 0
  assert reference.stderr == b"" and candidate.stderr == b""
  assert reference.stdout == bytes.from_text(f"{reference.util} from util-linux 2.42.4\n")
  assert candidate.stdout == bytes.from_text(gnu.version_text(candidate.util) + "\n")
}
