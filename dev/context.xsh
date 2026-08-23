##! Repository and host context shared by development lifecycle commands.
use targets as target_policy

## Paths and typed policy inherited by each development command.
export type Context = {
  root: Path,
  target_dir: Path,
  coverage_dir: Path,
  artifact_dir: Path,
  host_os: target_policy.HostOs,
  host_arch: target_policy.HostArch,
  target: target_policy.Target,
  profile: Str,
  darwin_deployment_target: Str,
}

## Operational failures rendered at the development command boundary.
export error ContextError = WrongDirectory(root: Path)

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
  let host_os = target_policy.host_os_tag(uname.sysname)?
  let host_arch = target_policy.host_arch_tag(uname.machine)?
  let requested_target = env.get_or("TARGET", "")?.trim()
  let target_name = if requested_target == "" { target_policy.default_triple } else { requested_target }
  let target = target_policy.resolve(target_name)?
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
    target: target,
    profile: if profile == "" { "dist" } else { profile },
    darwin_deployment_target: env.get_or("DARWIN_DEPLOYMENT_TARGET", "26.0")?.trim(),
  }
}
