##! Repository and host context shared by development lifecycle commands.
use targets

## Paths and typed policy inherited by each development command.
export type Context = {
  root: Path,
  target_dir: Path,
  coverage_dir: Path,
  artifact_dir: Path,
  host_os: Str,
  host_arch: Str,
  target_triple: Str,
  target_os: Str,
  target_arch: Str,
  docker_platform: Str,
  executable_format: Str,
  elf_machine: Str,
  target_cpu_rustflags: List[Str],
  target_cpu_cflags: List[Str],
  static_musl: Bool,
  profile: Str,
  darwin_deployment_target: Str,
}

## Operational failures rendered at the development command boundary.
export error ContextError = WrongDirectory(root: Path) | MissingTool(tool: Str) | StageFailed(stage: Str, target: Str, detail: Str)

## Resolves a configuration path relative to the repository unless it is absolute.
export pure repo_path(root: Path, value: Str) -> Path {
  if value.starts_with("/") {
    return fp"${value}"
  }

  return fp"${root}/${value}"
}

## Validates that the current directory is the XSH repository root.
export proc require_root() [fs, error] -> Result[Path] {
  let root = fs.cwd()?
  let required = [fp"${root}/Cargo.toml", fp"${root}/rust-toolchain.toml", fp"${root}/xsht-config.ini"]

  for required_path in required {
    if ! required_path.exists()? {
      return Err(ContextError.WrongDirectory(root: root))
    }
  }

  return root
}

## Reads host and environment policy into one lifecycle context.
export proc create() [fs, env, error] -> Result[Context] {
  let root = require_root()?
  let uname = system.uname()?
  let host_os = targets.host_os(uname.sysname)?
  let host_arch = targets.host_arch(uname.machine)?
  let requested_target = env.get_or("TARGET", "")?.trim()
  let target_name = if requested_target == "" { targets.default_triple } else { requested_target }
  let target = targets.resolve(target_name)?
  let target_value = env.get_or("CARGO_TARGET_DIR", "")?.trim()
  let target_dir = if target_value == "" { fp"${root}/target" } else { repo_path(root, target_value) }
  let profile = env.get_or("DIST_PROFILE", "dist")?.trim()

  return {
    root: root,
    target_dir: target_dir,
    coverage_dir: fp"${root}/target/cov",
    artifact_dir: fp"${root}/dist",
    host_os: host_os,
    host_arch: host_arch,
    target_triple: target.triple,
    target_os: target.os,
    target_arch: target.arch,
    docker_platform: target.docker_platform,
    executable_format: target.executable_format,
    elf_machine: target.elf_machine,
    target_cpu_rustflags: target.cpu_rustflags,
    target_cpu_cflags: target.cpu_cflags,
    static_musl: target.static_musl,
    profile: if profile == "" { "dist" } else { profile },
    darwin_deployment_target: env.get_or("DARWIN_DEPLOYMENT_TARGET", "26.0")?.trim(),
  }
}

## Resolves a required external program with a named diagnostic on absence.
export proc require_tool(name: Str) [process, error] -> Result[Path] {
  match process.which(name) {
    Ok(tool_path) => return tool_path
    Err(_) => return Err(ContextError.MissingTool(tool: name))
  }
}

## Renders a direct argv vector for stage reporting.
export pure command_display(argv: List[Str]) -> Str {
  return argv.join(" ")
}

## Executes one visible direct process boundary and classifies a failed status.
export proc run_stage(
  stage: Str,
  target: Str,
  executable: Str,
  argv: List[Str],
  cwd: Path,
  command_env: Record,
) [process, error, io] -> Result[Unit] {
  print f"[${stage} target=${target}] ${command_display(argv)}"
  let status = process.run(process.command_argv(executable, argv, cwd: cwd, env: command_env))?

  if status.ok {
    return
  }

  let detail = if status.exited() {
    f"exit ${status.exit_code()?}"
  } else if status.signaled() {
    f"signal ${status.signal_number()?}"
  } else {
    "unknown process status"
  }
  return Err(ContextError.StageFailed(stage: stage, target: target, detail: detail))
}

## Creates a lifecycle directory when it does not already exist.
export proc ensure_dir(directory: Path) [fs, error] -> Result[Unit] {
  if ! directory.exists()? {
    directory.mkdir()?
  }
}
