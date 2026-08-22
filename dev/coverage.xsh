##! Native-or-Docker selection for the existing combined coverage programs.

use context
use docker

## Selects the automatic coverage backend from the preserved Alpine Linux contract.
export pure automatic_backend_for(ctx: Context, alpine_linux: Bool, cargo_available: Bool, linker_available: Bool) -> Str {
  if ctx.host_os == "linux" and ctx.host_arch == "x86_64" and alpine_linux and cargo_available and linker_available {
    return "native"
  }

  return "docker"
}

## Selects the automatic coverage backend from the preserved Alpine Linux contract.
export proc automatic_backend(ctx: Context) [fs, process, error] -> Result[Str] {
  let alpine_linux = p"/etc/alpine-release".exists()?
  let cargo_available = match process.which("cargo") {
    Ok(_) => true
    Err(_) => false
  }
  var linker_available = false

  for name in ["cc", "clang", "gcc"] {
    if ! linker_available {
      match process.which(name) {
        Ok(_) => linker_available = true
        Err(_) => {}
      }
    }
  }

  return automatic_backend_for(ctx, alpine_linux, cargo_available, linker_available)
}

## Resolves the native linker without hiding an unavailable tool.
export proc native_linker() [env, process, error] -> Result[Path] {
  let configured = env.get_or("COV_NATIVE_LINKER", "")?.trim()

  if configured != "" {
    return fp"${configured}"
  }

  for name in ["cc", "clang", "gcc"] {
    match process.which(name) {
      Ok(found) => return found
      Err(_) => {}
    }
  }

  return Err(context.ContextError.MissingTool(tool: "cc, clang, or gcc"))
}

## Runs the retained native combined Rust LLVM and XSH API coverage program.
export proc native_coverage(ctx: Context) [fs, env, process, error, io] -> Result[Unit] {
  context.ensure_dir(ctx.coverage_dir)?
  let cargo_value = env.get_or("COV_CARGO", "")?.trim()
  let cargo = if cargo_value == "" { process.which("cargo")?.display() } else { cargo_value }
  let configured_bin = env.get_or("COV_CARGO_BIN", "")?.trim()
  let cargo_bin = if configured_bin == "" { fp"${cargo}".parent().display() } else { configured_bin }
  let linker = native_linker()?
  context.run_stage(
    "coverage-native",
    ctx.target_triple,
    cargo,
    [cargo, "run", "-p", "xsh", "--bin", "xsh", "--", "tools/cov-linux.xsh"],
    ctx.root,
    {
      XSH_COV_CARGO_BIN: cargo_bin,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER: linker.display(),
      CC_x86_64_unknown_linux_musl: linker.display(),
    },
  )?
}

## Runs retained coverage logic inside a privileged Docker boundary.
export proc docker_backend(ctx: Context) [fs, env, process, error, io] -> Result[Unit] {
  context.ensure_dir(ctx.coverage_dir)?
  let image = docker.ensure_image(ctx)?
  let identity = unix.id()?
  let argv = [
    "docker",
    "run",
    "--rm",
    "--privileged",
    "-v",
    f"${ctx.root.display()}:/work",
    "-v",
    "xsh-test-cov-target:/work/target",
    "-v",
    f"${ctx.coverage_dir.display()}:/work/target/cov",
    "-w",
    "/work",
    "-e",
    f"TARGET=${ctx.target_triple}",
    "-e",
    "CARGO_TARGET_DIR=/work/target",
    "-e",
    f"HOST_UID=${identity.uid}",
    "-e",
    f"HOST_GID=${identity.gid}",
    image,
    "cargo",
    "run",
    "--quiet",
    "-p",
    "xsh",
    "--bin",
    "xsh",
    "--",
    "dev/main.xsh",
    "--",
    "internal",
    "coverage",
  ]
  context.run_stage("coverage-docker", ctx.target_triple, "docker", argv, ctx.root, {})?
}

## Dispatches the public coverage backend policy.
export proc coverage(ctx: Context, requested_backend: Str) [fs, env, process, error, io] -> Result[Unit] {
  let backend = if requested_backend == "" { automatic_backend(ctx)? } else { requested_backend }

  if backend == "native" {
    return native_coverage(ctx)
  }

  if backend == "docker" {
    return docker_backend(ctx)
  }

  return Err(context.ContextError.StageFailed(stage: "coverage", target: ctx.target_triple, detail: f"unsupported backend ${backend}"))
}
