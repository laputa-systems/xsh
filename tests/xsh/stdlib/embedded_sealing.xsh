# The embedded standard library is sealed: user source, module search roots,
# and same-spelled user declarations can neither replace nor reach into a
# standard implementation, and user source is still held to its own rules.

type Captured = {status: Status, stdout: Str, stderr: Str}

# Writes each `files` entry, a path below the fixture root and its text, into
# a fresh directory.
proc fixture(ctx: TestContext, name: Str, files: Map[Str, Str]) [fs, error] -> Result[Path] {
  let root = test.temp_dir(ctx, name:)?
  for file_name in files.keys() {
    let file = fp"{root}/{file_name}"
    file.parent().mkdir()
    file.write(files[file_name])
  }

  Ok(root)
}

# Runs `script`, a path below `root`, with `root` as the current directory.
proc run_in(root: Path, script: Str) [process, env, error] -> Result[Captured] {
  cd (root) {
    run.capture --text "xsh" $script
  }
}

const hostile_quote = """##! A hostile stand-in.

## Quote a value.
export pure quote(value: Str) -> Str {
  return "COMPROMISED"
}
"""

# User source, module search roots, and a hostile module named after a
# standard module cannot replace a standard implementation.
test standard_implementations_cannot_be_replaced { |ctx|
  # A user module named after a native standard module and files named after
  # internal implementation labels cannot replace standard bindings.
  let root = fixture(
    ctx,
    "sealed",
    {
  "hostile/shlex.xsh": hostile_quote,
  "hostile/stdlib.xsh": hostile_quote,
  "hostile/target.xsh": hostile_quote,
  "hostile/tui.xsh": """##! A hostile stand-in.

## Return a color.
export pure red() -> Str {
  return "COMPROMISED"
}
""",
  "sealed.xsh": """proc main() [io] {
  print shlex.quote("a b")
  print shlex.join(["a b"])
  print "a b".fields().join("|")
  print tui.red()
  print bytes.human(2048)
}
""",
  "shadow.xsh": """type Hostile = module {
  export pure quote(value: Str) -> Str
}

proc main() [io, error] {
  let loaded = module.load(p"hostile/shlex.xsh")?.require(Hostile)?
  print loaded.quote("plain")
}
""",
},
  )?
  let hostile = fp"{root}/hostile"

  cd (root) {
    let sealed = run.capture --text XSH_MODULE_PATH=$hostile "xsh" sealed.xsh
    assert sealed.status.exited_with(0), sealed.stderr
    assert sealed.stdout == "'a b'\n'a b'\na|b\n\x1b[31m\n2.0K\n"

    # Loading the hostile module explicitly still yields ordinary user
    # privileges: it is reachable, but it is not the standard implementation.
    let shadowed = run.capture --text XSH_MODULE_PATH=$hostile "xsh" shadow.xsh
    assert shadowed.status.exited_with(0), shadowed.stderr
    assert shadowed.stdout == "COMPROMISED\n"
  }
}

# A private implementation helper cannot be named from user source.
test private_implementation_helpers_are_not_callable { |ctx|
  let root = fixture(
    ctx,
    "private-helper",
    {
  "private.xsh": """proc main() [io] {
  print lines_error("nope")
}
""",
},
  )?
  let output = run_in(root, "private.xsh")?
  assert ! output.status.exited_with(0), output.stdout
  assert "lines_error" in output.stderr, output.stderr
}

# A user declaration with the same spelling as an embedded helper cannot
# capture, or be captured by, the implementation module.
test same_spelled_user_helpers_cannot_capture_implementation_helpers { |ctx|
  let root = fixture(
    ctx,
    "capture",
    {
  "data.txt": "abc",
  "capture.xsh": """pure is_hex_text(value: Str) -> Bool {
  return false
}

proc main() [io, fs, error] {
  let checksum = hash.sha256(p"data.txt")?.hex()
  hash.verify_file(p"data.txt", sha256: checksum)?
  print is_hex_text("abc")
}
""",
},
  )?
  let output = run_in(root, "capture.xsh")?
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "false\n"
}

# A user file whose contents are a copy of an embedded implementation remains
# ordinary user source, and the standard entry is unaffected.
test copied_embedded_source_grants_no_private_access { |ctx|
  let root = fixture(
    ctx,
    "copied-source",
    {
  "copy.xsh": p"stdlib/tui.xsh".read_text()?,
  "copied.xsh": """proc main() [io, error] {
  print tui.red()
  let loaded = module.load(p"copy.xsh")?
  let _ = loaded
  print "loaded"
}
""",
},
  )?
  let output = run_in(root, "copied.xsh")?
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "\u{1b}[31m\nloaded\n"
}

# The implementation namespace is not spellable, and the existing
# reserved-name rules keep working.
test implementation_namespace_is_unspellable_and_reserved_names_still_work { |ctx|
  let root = fixture(
    ctx,
    "unspellable",
    {
  "namespace.xsh": """proc main() [io] {
  print <xsh-stdlib:tui>.red()
}
""",
  "reserved.xsh": r"""proc main() [io] {
  let shlex = 1
  print "$shlex"
}
""",
  "error-binding.xsh": r"""proc main() [io] {
  let error = "bound"
  print "$error"
}
""",
},
  )?

  let cases = [
    {script: "namespace.xsh", runs: false},
    {script: "reserved.xsh", runs: false},
    {script: "error-binding.xsh", runs: true},
  ]

  for case in cases {
    let output = run_in(root, case.script)?
    assert output.status.exited_with(0) == case.runs, f"{case.script}: {output.stderr}"
  }
}

# Preparation executes no library initialization: context-sensitive values are
# read at each invocation, not when the module is prepared.
test prepared_implementations_read_context_at_invocation_time { |ctx|
  let root = fixture(
    ctx,
    "context",
    {
  "context.xsh": """proc main() [io, error] {
  print env.get_or("XSH_PORT_PROBE", "absent")?
  env XSH_PORT_PROBE="scoped" {
    print env.get_or("XSH_PORT_PROBE", "absent")?
  }
  print env.get_or("XSH_PORT_PROBE", "absent")?
}
""",
},
  )?

  cd (root) {
    let output = run.capture --text env -u XSH_PORT_PROBE "xsh" context.xsh
    assert output.status.exited_with(0), output.stderr
    assert output.stdout == "absent\nscoped\nabsent\n"
  }
}

# A user function spelled like a private representation bridge stays an
# ordinary user function; the bridge rewrite is confined to its own module.
test user_functions_cannot_impersonate_a_representation_bridge { |ctx|
  let root = fixture(
    ctx,
    "bridge-impersonation",
    {
  "bridge.xsh": r"""pure record_with_field(value_record: Record, field: Str, value: Any) -> Record {
  return {impersonated: true}
}

pure type_name(value: Any) -> Str {
  return "USER"
}

proc main() [io] {
  let updated = record_with_field({a: 1}, "b", 2)
  print "${"impersonated" in updated}"
  print type_name(1)
}
""",
},
  )?
  let output = run_in(root, "bridge.xsh")?
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "true\nUSER\n"
}

# Pure/effect checking still applies to user source.
test pure_user_functions_still_cannot_perform_io { |ctx|
  let root = fixture(
    ctx,
    "purity",
    {
  "impure.xsh": """pure read_it(target: Path) -> Str {
  return fs.read_text(target)?
}

proc main() [io] {
  print read_it(p"x")
}
""",
},
  )?
  let output = run_in(root, "impure.xsh")?
  assert ! output.status.exited_with(0), output.stdout
}

# Module-level dependencies work across script and native modules, and an
# import cycle is diagnosed rather than looping.
test module_dependencies_resolve_and_cycles_are_diagnosed { |ctx|
  let root = fixture(
    ctx,
    "cycles",
    {
  "left.xsh": """##! Left half of a two-module dependency.

use right

## Tag a value through the other module.
export pure from_left(value: Str) -> Str {
  return right.tag(value)
}
""",
  "right.xsh": """##! Right half of a two-module dependency.

## Normalize a value.
export pure tag(value: Str) -> Str {
  return value.fields().join(" ")
}
""",
  "entry.xsh": """use left

proc main() [io] {
  print shlex.quote(left.from_left("a b"))
}
""",
  "alpha.xsh": """##! First half of an import cycle.

use beta

## Answer.
export pure a() -> Int {
  return 1
}
""",
  "beta.xsh": """##! Second half of an import cycle.

use alpha

## Answer.
export pure b() -> Int {
  return 2
}
""",
  "cyclic.xsh": """use alpha

proc main() [io] {
  print alpha.a()
}
""",
},
  )?

  let resolved = run_in(root, "entry.xsh")?
  assert resolved.status.exited_with(0), resolved.stderr
  assert resolved.stdout == "'a b'\n"

  let cyclic = run_in(root, "cyclic.xsh")?
  assert ! cyclic.status.exited_with(0), cyclic.stdout
  assert "cycle" in cyclic.stderr, cyclic.stderr
}

# Linux text entries read their host text at call time, whichever binding
# (native or embedded) each one selects.
test linux_text_entries_answer_from_the_host { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("the Linux text entries exist only on Linux")
    return
  }

  let root = fixture(
    ctx,
    "linux-embedded-entries",
    {
  "entries.xsh": """use system
use unix
use linux

proc main() [io, env, fs, error] {
  let memory = system.memory()?
  if memory.total <= 0 {
    return error.fail("system.memory reported no total")
  }
  print "memory"
  let release = system.os_release()?
  if release.name == "" {
    return error.fail("system.os_release reported no name")
  }
  print "release"
  let uptime = unix.uptime_seconds()?
  if uptime < 0 {
    return error.fail("unix.uptime_seconds reported a negative uptime")
  }
  print "uptime"
  let meminfo = linux.meminfo()?
  if meminfo.total <= 0 {
    return error.fail("linux.meminfo reported no total")
  }
  print "meminfo"
  let modules = linux.modules()?.collect()
  for row in modules {
    if row.name == "" {
      return error.fail("linux.modules yielded an unnamed module")
    }
  }
  print "modules"
}
""",
},
  )?
  let output = run_in(root, "entries.xsh")?
  assert output.status.exited_with(0), output.stderr
  assert output.stdout == "memory\nrelease\nuptime\nmeminfo\nmodules\n"
}
