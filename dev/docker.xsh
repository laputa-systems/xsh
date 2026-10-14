##! Docker image, mount, platform, and direct internal-XSH invocation policy.
use context
use stage as stages
use targets

## Host-side tool paths mounted read-only into the pinned Linux test container.
export type HostLinkTools = {mold: Path, kache: Path}

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
  host_link_tools: HostLinkTools? = null,
) -> Result[List[Str], Error] {
  let git_mount = if git_common_dir == null { [] } else { ["-v", f"{git_common_dir}:{git_common_dir}:ro"] }
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
    f"{ctx.target_dir}:/work/target",
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

## Runs one container-internal lifecycle operation without a shell intermediary.
export proc run_internal(
  ctx: context.Context,
  operation: Str,
  privileged: Bool,
  extra: List[Str],
) [fs, process, env, error, io] -> Result[Unit, Error] {
  let image = ensure_image(ctx)?
  let selected_platform = platform(ctx)?
  let identity = unix.id()?
  let stress_repeat = if operation == "test-linux" { env.get_or("XSH_OS_STRESS_REPEAT", "")? } else { "" }
  let available_link_tools = host_link_tools(ctx)?
  var common_git = null
  if operation in ["gate-lane", "gate-batch"] {
    cd (ctx.root) {
      let output = run.text git rev-parse --path-format=absolute --git-common-dir
      common_git = fp"{output.trim()}"
    }
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
    host_link_tools: available_link_tools,
  )?
  stages.execute(
    stages.command(
      f"docker-{operation}",
      ctx.target.triple,
      "docker",
      argv,
      ctx.root,
      {},
    ),
  )
}
