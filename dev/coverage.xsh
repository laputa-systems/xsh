##! Native-or-Docker selection for the existing combined coverage programs.
use context
use docker
use stage as stages
use targets

## Concrete coverage execution backend.
export type CoverageBackend = NativeBackend | DockerBackend

# Parsed coverage request, including automatic selection.
type CoverageRequest = Automatic | NativeRequest | DockerRequest

## Decodes the CLI coverage request before workflow dispatch.
export pure parse_request(value: Str) -> Result[CoverageRequest] {
  match value {
    "" => return Automatic
    "native" => return NativeRequest
    "docker" => return DockerRequest
    _ => return Err(
      stages.StageError.Failed(
        stage: "coverage",
        target: "",
        detail: f"unsupported backend ${value}",
      ),
    )
  }
}

## Selects the automatic coverage backend from the preserved Alpine Linux contract.
export pure automatic_backend_for(
  ctx: context.Context,
  alpine_linux: Bool,
  cargo_available: Bool,
  linker_available: Bool,
) -> CoverageBackend {
  let native_host = ctx.host_os == targets.Linux and ctx.host_arch == targets.X86_64
  let prerequisites = alpine_linux and cargo_available and linker_available

  if native_host and prerequisites {
    return NativeBackend
  }

  return DockerBackend
}

## Renders a backend only at the test and display boundary.
export pure backend_name(backend: CoverageBackend) -> Str {
  match backend {
    NativeBackend => return "native"
    DockerBackend => return "docker"
  }
}

## Selects the automatic coverage backend from the preserved Alpine Linux contract.
export proc automatic_backend(ctx: context.Context) [fs, process, error] -> Result[CoverageBackend] {
  let alpine_linux = p"/etc/alpine-release".exists()?
  let cargo_available = match process.which("cargo") {
    Ok(_) => true,
    Err(_) => false,
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
export proc native_linker() [process, env, error] -> Result[Path] {
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

  return Err(stages.StageError.MissingTool(tool: "cc, clang, or gcc"))
}

## Runs the retained native combined Rust LLVM and XSH API coverage program.
export proc native_coverage(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  stages.ensure_dir(ctx.coverage_dir)?
  let cargo_value = env.get_or("COV_CARGO", "")?.trim()
  let cargo = if cargo_value == "" { process.which("cargo")?.display() } else { cargo_value }
  let configured_bin = env.get_or("COV_CARGO_BIN", "")?.trim()
  let cargo_bin = if configured_bin == "" {
    fp"${cargo}".parent().display()
  } else {
    configured_bin
  }
  let linker = native_linker()?
  stages.execute(
    stages.command(
      "coverage-native",
      ctx.target.triple,
      cargo,
      [
        cargo,
        "run",
        "-p",
        "xsh",
        "--bin",
        "xsh",
        "--",
        "tools/cov-linux.xsh",
      ],
      ctx.root,
      {
        XSH_COV_CARGO_BIN: cargo_bin,
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER: linker.display(),
        CC_x86_64_unknown_linux_musl: linker.display(),
      },
    ),
  )?
}

## Runs retained coverage logic inside a privileged Docker boundary.
export proc docker_backend(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  stages.ensure_dir(ctx.coverage_dir)?
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
    f"TARGET=${ctx.target.triple}",
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
  stages.execute(
    stages.command(
      "coverage-docker",
      ctx.target.triple,
      "docker",
      argv,
      ctx.root,
      {},
    ),
  )?
}

## Dispatches the public coverage backend policy.
export proc coverage(ctx: context.Context, request: CoverageRequest) [fs, process, env, error, io] -> Result[Unit] {
  let backend = match request {
    Automatic => automatic_backend(ctx)?,
    NativeRequest => NativeBackend,
    DockerRequest => DockerBackend,
  }
  match backend {
    NativeBackend => return native_coverage(ctx)
    DockerBackend => return docker_backend(ctx)
  }
}
