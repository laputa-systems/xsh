##! Native-or-Docker selection for the existing combined coverage programs.
use context
use docker
use stage as stages
use targets
use test_workflows as tests

## Concrete coverage execution backend.
export enum CoverageBackend { NativeBackend, DockerBackend }

# Parsed coverage request, including automatic selection.
enum CoverageRequest {
    Automatic,
    NativeRequest,
    DockerRequest,
}

## Decodes the CLI coverage request before workflow dispatch.
export pure parse_request(value: Str) -> Result[CoverageRequest, Error] {
  match value {
    "" => Automatic
    "native" => NativeRequest
    "docker" => DockerRequest
    else => Err(
      stages.StageError.Failed(
        stage: "coverage",
        target: "",
        detail: f"unsupported backend {value}",
      ),
    )
  }
}

## Renders a backend only at the test and display boundary.
export pure backend_name(backend: CoverageBackend) -> Str {
  match backend {
    NativeBackend => "native"
    DockerBackend => "docker"
  }
}

## Chooses a native-architecture Linux container unless the caller selected a target.
export pure docker_target_triple(host_arch: targets.HostArch, selected: Str) -> Str {
  guard selected == "" else {
    return selected
  }

  return "aarch64-unknown-linux-musl" when host_arch == .Aarch64

  "x86_64-unknown-linux-musl"
}

## Automatic coverage enters the pinned image; callers already inside it can
## explicitly select the native backend.
export pure automatic_backend() -> CoverageBackend {
  DockerBackend
}

## Runs the retained native combined Rust LLVM and XSH API coverage program.
export proc native_coverage(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.ensure_dir(ctx.coverage_dir)
  let cargo_value = env.get_or("COV_CARGO", "")?.trim()
  let cargo = if cargo_value == "" { process.which("cargo")?.display() } else { cargo_value }
  let configured_bin = env.get_or("COV_CARGO_BIN", "")?.trim()
  let cargo_bin = if configured_bin == "" {
    fp"{cargo}".parent().display()
  } else {
    configured_bin
  }
  let flags = tests.cargo_environment(ctx)?
  let environment: Record = match ctx.target.triple {
    "x86_64-unknown-linux-musl" => {
      XSH_COV_CARGO_BIN: cargo_bin,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: flags.get(
        "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS",
      )?.require(Str)?,
    }
    "aarch64-unknown-linux-musl" => {
      XSH_COV_CARGO_BIN: cargo_bin,
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: flags.get(
        "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS",
      )?.require(Str)?,
    }
    else => { return Err(targets.TargetError.Unsupported(target: ctx.target.triple)) }
  }
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
      environment,
    ),
  )
}

## Runs retained coverage logic inside a privileged Docker boundary.
export proc docker_backend(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.ensure_dir(ctx.coverage_dir)
  let image = docker.ensure_image(ctx)?
  let identity = unix.id()?
  let cargo_environment = docker.cargo_environment_argv(ctx)?
  let argv = [
    "docker",
    "run",
    "--rm",
    "--init",
    "--platform",
    docker.platform(ctx)?,
    "--privileged",
    "-v",
    f"{ctx.root}:/work",
    "-v",
    "xsh-test-cov-target:/work/target",
    "-v",
    f"{ctx.coverage_dir}:/work/target/cov",
    "-w",
    "/work",
    "-e",
    f"TARGET={ctx.target.triple}",
    "-e",
    "CARGO_TARGET_DIR=/work/target",
    "-e",
    f"DIST_PROFILE={tests.profile_name(tests.execution_profile()?)}",
    "-e",
    f"HOST_UID={identity.uid}",
    "-e",
    f"HOST_GID={identity.gid}",
    @cargo_environment,
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
  )
}

## Dispatches the public coverage backend policy.
export proc coverage(
  ctx: context.Context,
  request: CoverageRequest,
) [fs, process, env, error, io] -> Result[Unit, Error] {
  let backend = match request {
    Automatic => automatic_backend(),
    NativeRequest => NativeBackend,
    DockerRequest => DockerBackend,
  }
  match backend {
    NativeBackend => return native_coverage(ctx)
    DockerBackend => {
      let selected = env.get_or("TARGET", "")?.trim()
      let triple = docker_target_triple(ctx.host_arch, selected)

      return docker_backend(ctx) when triple == ctx.target.triple

      env TARGET=$triple {
        docker_backend(context.create()?)
      }
      return
    }
  }
}
