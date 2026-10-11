##! Support for the native tests transcribed from the uutils coreutils
##! integration suite. A transcribed test keeps the shape of the original:
##! build a scene, run one applet, then assert on its status and streams.
##!
##!     let s = uu.scene(ctx)?
##!     uu.write(s, "f", "a\n")?
##!     let r = uu.invoke(s, "cat", ["f"])?
##!     uu.succeeds(r)
##!     uu.stdout_only(r, "a\n")
##!
##! Applets run by their real script path with `LC_ALL=C` and `TZ=UTC`, the
##! environment used by the suite. Only PATH, LD_PRELOAD, and LLVM_PROFILE_FILE
##! are inherited before caller overrides. Runs use a private scratch directory
##! that is also the working directory. Each assertion names what it checked so
##! a failure points at the transcribed line.

## A scratch directory and the test context it belongs to.
export type Scene = {ctx: TestContext, root: Path}

## What one applet run produced.
export type Ran = {util: Str, args: List[Str], status: Int, stdout: Bytes, stderr: Bytes}

## A scratch directory for one test. It is the working directory of every run.
export proc scene(ctx: TestContext) [fs, error] -> Result[Scene, Error] {
  let root = test.temp_dir(ctx, name: "uu")?
  Ok({ctx: ctx, root: root})
}

## The path of `name` inside the scene.
export pure at(s: Scene, name: Str) -> Path {
  fp"{s.root}/{name}"
}

## Creates an empty file.
export proc touch(s: Scene, name: Str) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".write("")?
  Ok()
}

## Writes text to a file.
export proc write(s: Scene, name: Str, content: Str) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".write(content)?
  Ok()
}

## Writes bytes to a file.
export proc write_bytes(s: Scene, name: Str, content: Bytes) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".write(content)?
  Ok()
}

## Creates a directory (and parents).
export proc mkdir(s: Scene, name: Str) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".mkdir()?
  Ok()
}

## Creates a symbolic link `name` pointing at `target`.
export proc symlink(s: Scene, target: Str, name: Str) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".symlink(to: fp"{target}")
  Ok()
}

## Creates a hard link `name` to `target`.
export proc hard_link(s: Scene, target: Str, name: Str) [fs, error] -> Result[Unit, Error] {
  fs.link(fp"{s.root}/{target}", fp"{s.root}/{name}")?
  Ok()
}

## Sets permission bits.
export proc set_mode(s: Scene, name: Str, mode: Int) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".chmod(mode)?
  Ok()
}

## Reads a file as bytes.
export proc read(s: Scene, name: Str) [fs, error] -> Result[Bytes, Error] {
  Ok(fp"{s.root}/{name}".read_bytes()?)
}

## Reads a file as text.
export proc read_text(s: Scene, name: Str) [fs, error] -> Result[Str, Error] {
  Ok(fp"{s.root}/{name}".read_text()?)
}

## Whether the path exists.
export proc exists(s: Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  Ok(fp"{s.root}/{name}".exists()?)
}

# The launcher receives only environment names and an optional permission mask;
# values keep their native transport through the command environment.
proc launch_metadata(vars: Record, umask: Int?) [error] -> Result[Str, Error] {
  if let mask = umask { assert mask >= 0 and mask <= 0o777, "umask must contain only permission bits" }
  json.encode({caller_keys: vars.keys(), umask: umask})
}

## Full lossless launcher argv for callers that wrap an applet in another process.
## Run in the scene directory and pass the same vars to the outer command;
## metadata carries override names while values travel through its environment.
export proc argv(
  s: Scene,
  util: Str,
  args: List[Path],
  vars: Record = {},
  umask: Int? = null,
) [error] -> Result[List[Path], Error] {
  let script = fp"{s.ctx.core_dir}/{util}.xsh"
  let words = [s.ctx.xsh_bin, script, p"--"].extend(args)
  let launcher = fp"{s.ctx.core_dir}/tests/support/uu-launch.xsh"
  Ok([s.ctx.xsh_bin, launcher, Path(launch_metadata(vars, umask)?)].extend(words))
}

## Creates an isolated applet command for callers that own spawning and waiting.
## Parallel callers must supply distinct capture paths or explicitly shared append paths.
## A zero timeout leaves the process unbounded; positive timeouts bound its lifetime.
## An explicit umask affects only this child; omitted masks inherit the runner mask.
export proc command(
  s: Scene,
  util: Str,
  args: List[Str],
  stdin: Bytes = b"",
  vars: Record = {},
  stdout: Path? = null,
  stderr: Path? = null,
  stdout_append: Bool = false,
  stderr_append: Bool = false,
  timeout: Duration = 0ms,
  umask: Int? = null,
) [process, env, error] -> Result[Command, Error] {
  let out = stdout ?? fp"{s.root}/.uu-stdout"
  let err = stderr ?? fp"{s.root}/.uu-stderr"
  let argv = argv(s, util, [Path(word) for word in args], vars, umask)?
  let plan = if timeout == 0ms {
    process.command_argv(s.ctx.xsh_bin, argv, s.root, vars, stdin, out, err, stdout_append, stderr_append)
  } else {
    process.command_argv(s.ctx.xsh_bin, argv, s.root, vars, stdin, out, err, stdout_append, stderr_append, timeout: timeout)
  }
  Ok(plan)
}


## Runs `util` with `args`, feeding `stdin`. `vars` overrides the isolated test environment.
## Explicit output paths belong to the caller and are neither captured nor removed.
export proc invoke(
  s: Scene,
  util: Str,
  args: List[Str],
  stdin: Bytes = b"",
  vars: Record = {},
  stdout: Path? = null,
  stderr: Path? = null,
  stdout_append: Bool = false,
  stderr_append: Bool = false,
  timeout: Duration = 0ms,
  umask: Int? = null,
) [fs, process, env, error] -> Result[Ran, Error] {
  let plan = command(s, util, args, stdin, vars, stdout, stderr, stdout_append, stderr_append, timeout, umask)?
  finish(s, util, args, plan, stdout == null, stderr == null)
}

proc finish(s: Scene, util: Str, args: List[Str], plan: Command, capture_stdout: Bool = true, capture_stderr: Bool = true) [fs, process, error] -> Result[Ran, Error] {
  let out = fp"{s.root}/.uu-stdout"
  let err = fp"{s.root}/.uu-stderr"
  let status = process.run(plan)?
  let code = status.exit_code()?
  let stdout = if capture_stdout { out.read_bytes()? } else { b"" }
  let stderr = if capture_stderr { err.read_bytes()? } else { b"" }
  if capture_stdout { out.remove()? }
  if capture_stderr { err.remove()? }
  Ok({util: util, args: args, status: code, stdout: stdout, stderr: stderr})
}

## The lossless path of a byte name inside the scene.
export proc at_bytes(s: Scene, name: Bytes) [error] -> Result[Path, Error] {
  Path.parse_bytes(bytes.concat([s.root.bytes(), b"/", name]))
}

## Runs an applet with lossless native path words, including non-UTF-8 arguments.
export proc invoke_paths(
  s: Scene,
  util: Str,
  args: List[Path],
  stdin: Bytes = b"",
  vars: Record = {},
  timeout: Duration = 0ms,
  umask: Int? = null,
) [fs, process, env, error] -> Result[Ran, Error] {
  let argv = argv(s, util, args, vars, umask)?
  let out = fp"{s.root}/.uu-stdout"
  let err = fp"{s.root}/.uu-stderr"
  let plan = if timeout == 0ms {
    process.command_argv(s.ctx.xsh_bin, argv, s.root, vars, stdin, out, err)
  } else {
    process.command_argv(s.ctx.xsh_bin, argv, s.root, vars, stdin, out, err, timeout: timeout)
  }
  finish(s, util, [word.display() for word in args], plan)
}

## Opens a path as standard input, preserving directory and device read errors.
## Explicit output paths belong to the caller and are neither captured nor removed.
export proc invoke_from_path(
  s: Scene,
  util: Str,
  args: List[Str],
  stdin: Path,
  vars: Record = {},
  stdout: Path? = null,
  stderr: Path? = null,
  stdout_append: Bool = false,
  stderr_append: Bool = false,
  timeout: Duration = 0ms,
  umask: Int? = null,
) [fs, process, env, error] -> Result[Ran, Error] {
  let out = stdout ?? fp"{s.root}/.uu-stdout"
  let err = stderr ?? fp"{s.root}/.uu-stderr"
  let argv = argv(s, util, [Path(word) for word in args], vars, umask)?
  let plan = if timeout == 0ms {
    process.command_argv(s.ctx.xsh_bin, argv, s.root, vars, stdin, out, err, stdout_append, stderr_append)
  } else {
    process.command_argv(s.ctx.xsh_bin, argv, s.root, vars, stdin, out, err, stdout_append, stderr_append, timeout: timeout)
  }
  finish(s, util, args, plan, stdout == null, stderr == null)
}

## Creates a directory and every missing parent (`mkdir_all`).
export proc mkdir_all(s: Scene, name: Str) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".mkdir()?
  Ok()
}

## Whether `name` exists and is a regular file (`file_exists`).
export proc file_exists(s: Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  Ok(fp"{s.root}/{name}".is_file()?)
}

## Whether `name` exists and is a directory (`dir_exists`).
export proc dir_exists(s: Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  Ok(fp"{s.root}/{name}".is_dir()?)
}

## Whether `name` is a symbolic link, dangling or not (`is_symlink`).
export proc is_symlink(s: Scene, name: Str) [fs, error] -> Result[Bool, Error] {
  Ok(fp"{s.root}/{name}".is_symlink()?)
}

## The text a symbolic link holds (`read_symlink`).
export proc read_link(s: Scene, name: Str) [fs, error] -> Result[Str, Error] {
  Ok(fp"{s.root}/{name}".readlink()?.display())
}

## Appends text to a file (`append`).
export proc append(s: Scene, name: Str, content: Str) [fs, error] -> Result[Unit, Error] {
  let current = if fp"{s.root}/{name}".exists()? { fp"{s.root}/{name}".read_bytes()? } else { b"" }
  fp"{s.root}/{name}".write(bytes.concat([current, bytes.from_text(content)]))?
  Ok()
}

## Removes a file, link or directory tree (`remove`).
export proc remove(s: Scene, name: Str) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".remove(missing_ok: true)?
  Ok()
}

## Renames `from` to `to` inside the scene (`rename`).
export proc rename(s: Scene, from: Str, to: Str) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{from}".rename(to: fp"{s.root}/{to}")?
  Ok()
}

## Truncates or extends a file to `size` bytes (`truncate`).
export proc truncate(s: Scene, name: Str, size: Int) [fs, error] -> Result[Unit, Error] {
  fp"{s.root}/{name}".truncate(size)?
  Ok()
}

## Creates a named pipe (`mkfifo`).
export proc mkfifo(s: Scene, name: Str) [fs, error] -> Result[Unit, Error] {
  fs.mkfifo(fp"{s.root}/{name}", 0o644)?
  Ok()
}

## Copies a recorded upstream fixture `core/tests/data/uutils/<util>/<name>` to `as_name` in the scene.
export proc fixture(s: Scene, util: Str, name: Str, as_name: Str) [fs, error] -> Result[Unit, Error] {
  fp"{s.ctx.core_dir}/tests/data/uutils/{util}/{name}".copy(to: fp"{s.root}/{as_name}")?
  Ok()
}

## The permission bits of `name`, without file-type bits (`metadata().permissions().mode() & 0o7777`).
export proc mode(s: Scene, name: Str) [fs, error] -> Result[Int, Error] {
  Ok(fs.stat(fp"{s.root}/{name}")?.mode % 4096)
}

## The size of `name` in bytes.
export proc size(s: Scene, name: Str) [fs, error] -> Result[Int, Error] {
  Ok(fs.stat(fp"{s.root}/{name}")?.size)
}

## Asserts that `name` holds exactly `expected` (`assert_eq!(at.read(name), expected)`).
export proc file_is(s: Scene, name: Str, expected: Str) [fs, error] {
  let actual = fp"{s.root}/{name}".read_bytes()?
  assert actual == bytes.from_text(expected), f"{name}: {text(actual)} expected {expected}"
}

pure shown(r: Ran) -> Str {
  f"{r.util} {r.args.join(" ")}"
}

# A diagnostic rendering only; assertions compare bytes or require valid UTF-8.
pure text(data: Bytes) -> Str {
  data.utf8() ?? "(non-UTF-8 output)"
}

## Asserts exit status 0.
export proc succeeds(r: Ran) {
  assert r.status == 0, f"{shown(r)}: expected success, status {r.status}, stderr {text(r.stderr)}"
}

## Asserts a non-zero exit status.
export proc fails(r: Ran) {
  assert r.status != 0, f"{shown(r)}: expected failure, got success; stdout {text(r.stdout)}"
}

## Asserts a specific exit status.
export proc fails_with_code(r: Ran, code: Int) {
  assert r.status == code, f"{shown(r)}: expected status {code}, got {r.status}; stderr {text(r.stderr)}"
}

## Asserts standard output equals `expected`.
export proc stdout_is(r: Ran, expected: Str) {
  assert r.stdout == bytes.from_text(expected), f"{shown(r)}: stdout {text(r.stdout)} expected {expected}"
}

## Asserts standard output bytes equal `expected`.
export proc stdout_is_bytes(r: Ran, expected: Bytes) {
  assert r.stdout == expected, f"{shown(r)}: stdout bytes differ: {r.stdout.len()} bytes, expected {expected.len()}"
}

## Asserts standard error equals `expected`.
export proc stderr_is(r: Ran, expected: Str) {
  assert r.stderr == bytes.from_text(expected), f"{shown(r)}: stderr {text(r.stderr)} expected {expected}"
}

## Asserts standard output contains `needle`.
export proc stdout_contains(r: Ran, needle: Str) {
  match r.stdout.utf8() {
    Ok(actual) => assert needle in actual, f"{shown(r)}: stdout {actual} lacks {needle}",
    Err(_) => assert false, f"{shown(r)}: stdout is not valid UTF-8",
  }
}

## Asserts standard error contains `needle`.
export proc stderr_contains(r: Ran, needle: Str) {
  match r.stderr.utf8() {
    Ok(actual) => assert needle in actual, f"{shown(r)}: stderr {actual} lacks {needle}",
    Err(_) => assert false, f"{shown(r)}: stderr is not valid UTF-8",
  }
}

## Asserts standard output starts with `prefix`.
export proc stdout_str_starts_with(r: Ran, prefix: Str) {
  match r.stdout.utf8() {
    Ok(actual) => assert actual.starts_with(prefix), f"{shown(r)}: stdout {actual} does not start with {prefix}",
    Err(_) => assert false, f"{shown(r)}: stdout is not valid UTF-8",
  }
}

## Asserts empty standard output.
export proc no_stdout(r: Ran) {
  assert r.stdout.len() == 0, f"{shown(r)}: unexpected stdout {text(r.stdout)}"
}

## Asserts empty standard error.
export proc no_stderr(r: Ran) {
  assert r.stderr.len() == 0, f"{shown(r)}: unexpected stderr {text(r.stderr)}"
}

## Asserts both streams are empty.
export proc no_output(r: Ran) {
  no_stdout(r)
  no_stderr(r)
}

## Asserts standard output equals `expected` and standard error is empty.
export proc stdout_only(r: Ran, expected: Str) {
  stdout_is(r, expected)
  no_stderr(r)
}

## Asserts standard error equals `expected` and standard output is empty.
export proc stderr_only(r: Ran, expected: Str) {
  stderr_is(r, expected)
  no_stdout(r)
}

## Asserts standard output bytes equal `expected` and standard error is empty.
export proc stdout_only_bytes(r: Ran, expected: Bytes) {
  stdout_is_bytes(r, expected)
  no_stderr(r)
}

## Asserts standard error bytes equal `expected`.
export proc stderr_is_bytes(r: Ran, expected: Bytes) {
  assert r.stderr == expected, f"{shown(r)}: stderr bytes differ: {r.stderr.len()} bytes, expected {expected.len()}"
}
