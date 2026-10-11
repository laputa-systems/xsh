##! Docker image, mount, platform, and direct internal-XSH invocation policy.
use context
use targets
use stage as stages

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

## Makes the pinned target flags visible before Cargo bootstraps the inner
## development script; the script cannot supply flags until it is running.
export pure cargo_environment_argv(ctx: context.Context) -> Result[List[Str], Error] {
  let environment = targets.docker_test_env(ctx.target.triple)?
  collect {
    for name in environment.keys() |> sort {
      yield "-e"
      yield f"{name}={environment.get(name)?.require(Str)?}"
    }
  }
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
) -> Result[List[Str], Error] {
  let cargo_environment = cargo_environment_argv(ctx)?
  let git_mount = if git_common_dir == null { [] } else { ["-v", f"{git_common_dir}:{git_common_dir}:ro"] }
  # PID 1 must reap orphaned jobs after an interactive shell exits.
  var argv = ["docker", "run", "--rm", "--init", "--platform", selected_platform]

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
    @cargo_environment,
    @git_mount,
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
  var common_git: Path? = null
  if operation in ["gate-lane", "gate-batch"] {
    cd ctx.root {
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
