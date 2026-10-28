##! Docker image, mount, platform, and direct internal-XSH invocation policy.
use context
use stage as stages
use stage_contract as contract
use targets

## Host-side tool paths mounted read-only into the pinned Linux test container.
export type HostLinkTools = {mold: Path, kache: Path}

## Docker could not identify the immutable image for gate execution.
export error ImageError = InvalidIdentity(detail: Str) | Unavailable(raw_status: Int) | Cancelled(raw_status: Int)

proc host_link_tools(ctx: context.Context) [env, fs, error] -> Result[HostLinkTools?, Error] {
  return null when ctx.host_os != .Linux or ctx.host_arch != .X86_64
  return null when ctx.target.triple != "x86_64-unknown-linux-musl"

  let mold = /usr/local/bin/mold
  let cargo_home = env.get_or("CARGO_HOME", "")?.trim()
  let kache = if cargo_home == "" {
    fp"{env.get_or("HOME", "")?}/.cargo/bin/kache"
  } else {
    fp"{cargo_home}/bin/kache"
  }
  let linker_adapter = fp"{ctx.root}/dev/musl-linker.sh"
  guard mold.exists() and kache.exists() and linker_adapter.exists() else {
    return Err(stages.StageError.MissingTool(tool: "mold, kache, or dev/musl-linker.sh"))
  }

  {mold: mold, kache: kache}
}

## Computes the Docker image name from the supported environment override.
export proc image_name() [env, error] -> Result[Str, Error] {
  let image = env.get_or("XSH_TEST_IMAGE", "xsh-test")?.trim()
  if image == "" {
    "xsh-test"
  } else {
    image
  }
}

## Computes the Docker platform while allowing the explicit environment override.
export proc platform(ctx: context.Context) [env, error] -> Result[Str, Error] {
  let override_value = env.get_or("DOCKER_PLATFORM", "")?.trim()
  if override_value == "" {
    ctx.target.docker_platform
  } else {
    override_value
  }
}

pure linux_test_operation(operation: Str) -> Bool {
  operation in ["test-linux", "test-linux-ci", "test-linux-split", "test-linux-ci-split"]
}

pure internal_target_dir(ctx: context.Context, operation: Str) -> Path {
  if linux_test_operation(operation) {
    fp"{ctx.target_dir}/linux-test"
  } else {
    ctx.target_dir
  }
}

## Builds or verifies the configured test image according to `XSH_TEST_IMAGE_BUILD`.
export proc ensure_image(ctx: context.Context) [process, env, error, io] -> Result[Str, Error] {
  let image = image_name()?
  let selected_platform = platform(ctx)?
  let build_image = env.get_or("XSH_TEST_IMAGE_BUILD", "1")?.trim()

  if build_image == "0" {
    stages.execute(
      stages.command(
        "docker-image-inspect",
        ctx.target.triple,
        "docker",
        ["docker", "image", "inspect", image],
        ctx.root,
        {},
      ),
    )
  } else {
    stages.execute(
      stages.command(
        "docker-image-build",
        ctx.target.triple,
        "docker",
        [
          "docker",
          "build",
          "--platform",
          selected_platform,
          "-t",
          image,
          "-f",
          "Dockerfile.test",
          ".",
        ],
        ctx.root,
        {},
      ),
    )
  }

  image
}

## Constructs a direct `docker run` argv ending in a Cargo-to-XSH internal command.
## The inner lifecycle creates a new context, so target and profile travel in
## the container environment rather than relying on host process inheritance.
export pure internal_argv(
  ctx: context.Context,
  image: Str,
  selected_platform: Str,
  operation: Str,
  privileged: Bool,
  host_uid: Int,
  host_gid: Int,
  stress_repeat: Str,
  extra: List[Str],
  git_common_dir: Path? = null,
  shared_lease_dir: Path? = null,
  host_link_tools: HostLinkTools? = null,
) -> Result[List[Str], Error] {
  let git_mount = if git_common_dir == null { [] } else { ["-v", f"{git_common_dir}:{git_common_dir}:ro"] }
  let lease_mount = if shared_lease_dir == null { [] } else { ["-v", f"{shared_lease_dir}:{shared_lease_dir}"] }
  guard host_link_tools == null or ctx.target.triple == "x86_64-unknown-linux-musl" else {
    return Err(targets.TargetError.Unsupported(target: ctx.target.triple))
  }
  let linker_mounts = if host_link_tools == null {
    []
  } else {
    [
      "-v",
      f"{ctx.root}/dev/musl-linker.sh:/usr/local/bin/xsh-mold-linker:ro",
      "-v",
      f"{host_link_tools.mold}:/usr/local/bin/mold:ro",
      "-v",
      f"{host_link_tools.kache}:/usr/local/bin/kache:ro",
    ]
  }
  guard ctx.target.triple == "x86_64-unknown-linux-musl" or ctx.target.triple == "aarch64-unknown-linux-musl" else {
    return Err(targets.TargetError.Unsupported(target: ctx.target.triple))
  }
  let target_flags = targets.docker_test_env(ctx.target.triple)?
  let target_flags_variable = if ctx.target.triple == "x86_64-unknown-linux-musl" {
    "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS"
  } else {
    "CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUSTFLAGS"
  }
  let target_rustflags = target_flags.get(target_flags_variable)?.require(Str)?
  let linker_environment = if host_link_tools == null {
    []
  } else {
    [
      "-e",
      "RUSTC_WRAPPER=/usr/local/bin/kache",
      "-e",
      "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=/usr/local/bin/xsh-mold-linker",
    ]
  }
  let target_environment = ["-e", f"{target_flags_variable}={target_rustflags}"]
  let target_dir = internal_target_dir(ctx, operation)
  # PID 1 must reap orphaned jobs after an interactive shell exits.
  var argv = ["docker", "run", "--rm", "--init", "--platform", selected_platform]
  if operation == "test-linux-split" or operation == "test-linux-ci-split" {
    argv += ["--cpus=0.01", "--memory=1g", "--memory-swap=1g"]
  }

  # Temporary test fixtures need executable scripts and metadata-only device
  # nodes, so /tmp must allow execution while denying device access.
  if operation == "test-linux" or operation == "test-linux-ci" or operation == "test-linux-split" or operation == "test-linux-ci-split" {
    argv += ["--tmpfs", "/tmp:rw,nodev,exec,mode=1777"]
  }

  if privileged {
    argv += ["--privileged"]
  }

  if stress_repeat.trim() != "" {
    argv += ["-e", f"XSH_OS_STRESS_REPEAT={stress_repeat}"]
  }

  argv += [
    "-v",
    f"{ctx.root}:/work",
    "-v",
    f"{target_dir}:/work/target",
    "-v",
    "xsh-cargo-registry:/root/.cargo/registry",
    "-w",
    "/work",
    "-e",
    f"TARGET={ctx.target.triple}",
    "-e",
    f"DIST_PROFILE={ctx.profile}",
    "-e",
    "CARGO_TARGET_DIR=/work/target",
    "-e",
    "CARGO_BUILD_WARNINGS=deny",
    "-e",
    f"HOST_UID={host_uid}",
    "-e",
    f"HOST_GID={host_gid}",
    @linker_mounts,
    @linker_environment,
    @target_environment,
    @git_mount,
    @lease_mount,
    image,
    "cargo",
    "run",
    "--quiet",
    "--target",
    ctx.target.triple,
    "-p",
    "xsh",
    "--bin",
    "xsh",
    "--",
    "dev/main.xsh",
    "--",
    "internal",
    operation,
  ]
  argv.extend(extra)
}

proc internal_command(
  ctx: context.Context,
  operation: Str,
  privileged: Bool,
  extra: List[Str],
) [fs, process, env, error, io] -> Result[contract.CommandSpec, Error] {
  if linux_test_operation(operation) {
    internal_target_dir(ctx, operation).mkdir(parents: true)?
  }
  let gate_operation = operation in ["gate-lane", "gate-full"]
  let configured_image = if gate_operation and env.get_or("XSH_TEST_IMAGE_BUILD", "1")?.trim() == "0" {
    image_name()?
  } else {
    ensure_image(ctx)?
  }
  let image = if gate_operation {
    let inspection = run.capture --text docker image inspect --format "{{.Id}}" $configured_image
    let raw = inspection.status.shell_code()?
    return Err(ImageError.Cancelled(raw_status: raw)) when raw == 130
    return Err(ImageError.Unavailable(raw_status: raw)) when raw != 0
    let identity = inspection.stdout.trim()
    return Err(ImageError.InvalidIdentity(detail: "Docker returned an invalid image ID")) when ! identity.starts_with("sha256:") or identity.byte_len() != 71
    for digit in identity[7..].split("") {
      return Err(ImageError.InvalidIdentity(detail: "Docker returned an invalid image ID")) when digit not in "0123456789abcdef"
    }
    return Err(ImageError.InvalidIdentity(detail: "Docker resolved a different image ID")) when configured_image.starts_with("sha256:") and identity != configured_image
    identity
  } else { configured_image }
  let selected_platform = platform(ctx)?
  let identity = unix.id()?
  let stress_repeat = if operation == "test-linux" { env.get_or("XSH_OS_STRESS_REPEAT", "")? } else { "" }
  let available_link_tools = host_link_tools(ctx)?
  var common_git = null
  if operation in ["gate-lane", "gate-full"] {
    cd (ctx.root) {
      let output = run.text python3 -B fp"{ctx.root}/dev/gate_source.py" --common-dir
      common_git = fp"{output.trim()}"
    }
  }
  var shared_lease_dir = null
  if common_git != null {
    let lease_dir = fp"{common_git.parent()}/.work/consolidation/infra"
    lease_dir.mkdir(parents: true)?
    shared_lease_dir = lease_dir
  }
  let argv = internal_argv(
    ctx,
    image,
    selected_platform,
    operation,
    privileged,
    identity.uid,
    identity.gid,
    stress_repeat,
    extra,
    git_common_dir: common_git,
    shared_lease_dir: shared_lease_dir,
    host_link_tools: available_link_tools,
  )?
  stages.command(
      f"docker-{operation}",
      ctx.target.triple,
      "docker",
      argv,
      ctx.root,
      {},
  )
}

## Runs one container-internal lifecycle operation through the stage diagnostics.
export proc run_internal(ctx: context.Context, operation: Str, privileged: Bool, extra: List[Str]) [fs, process, env, error, io] -> Result[Unit, Error] {
  stages.execute(internal_command(ctx, operation, privileged, extra)?)
}

## Returns the owned container launch status without replacing its raw child code.
export proc run_internal_status(ctx: context.Context, operation: Str, privileged: Bool, extra: List[Str]) [fs, process, env, error, io] -> Result[Status, Error] {
  let spec = internal_command(ctx, operation, privileged, extra)?
  print f"[{spec.stage} target={spec.target}] {spec.argv.join(" ")}"
  process.run(process.command_argv(spec.executable, spec.argv, cwd: spec.cwd, env: spec.environment))
}
