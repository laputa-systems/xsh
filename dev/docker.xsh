##! Docker image, mount, platform, and direct internal-XSH invocation policy.
use context

## Computes the Docker image name from the supported environment override.
export proc image_name() [env, error] -> Result[Str] {
  let image = env.get_or("XSH_TEST_IMAGE", "xsh-test")?.trim()
  return if image == "" { "xsh-test" } else { image }
}

## Computes the Docker platform while allowing the explicit environment override.
export proc platform(ctx: Context) [env, error] -> Result[Str] {
  let override_value = env.get_or("DOCKER_PLATFORM", "")?.trim()
  return if override_value == "" { ctx.target.docker_platform } else { override_value }
}

## Builds or verifies the configured test image according to `XSH_TEST_IMAGE_BUILD`.
export proc ensure_image(ctx: Context) [process, env, error, io] -> Result[Str] {
  let image = image_name()?
  let selected_platform = platform(ctx)?
  let build_image = env.get_or("XSH_TEST_IMAGE_BUILD", "1")?.trim()

  if build_image == "0" {
    context.run_stage(
      "docker-image-inspect",
      ctx.target.triple,
      "docker",
      ["docker", "image", "inspect", image],
      ctx.root,
      {},
    )?
  } else {
    context.run_stage(
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
    )?
  }

  return image
}

## Constructs a direct `docker run` argv ending in a Cargo-to-XSH internal command.
export pure internal_argv(
  ctx: Context,
  image: Str,
  selected_platform: Str,
  operation: Str,
  privileged: Bool,
  host_uid: Int,
  host_gid: Int,
  stress_repeat: Str,
  extra: List[Str],
) -> List[Str] {
  var argv = ["docker", "run", "--rm", "--platform", selected_platform]

  if privileged {
    argv = argv.push("--privileged")
  }

  if stress_repeat.trim() != "" {
    argv = argv.extend(["-e", f"XSH_OS_STRESS_REPEAT=${stress_repeat}"])
  }

  argv = argv.extend(
    [
      "-v",
      f"${ctx.root.display()}:/work",
      "-v",
      f"${ctx.target_dir.display()}:/work/target",
      "-v",
      "xsh-cargo-registry:/root/.cargo/registry",
      "-w",
      "/work",
      "-e",
      f"TARGET=${ctx.target.triple}",
      "-e",
      "CARGO_TARGET_DIR=/work/target",
      "-e",
      "CARGO_BUILD_WARNINGS=deny",
      "-e",
      f"HOST_UID=${host_uid}",
      "-e",
      f"HOST_GID=${host_gid}",
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
    ],
  )
  return argv.extend(extra)
}

## Runs one container-internal lifecycle operation without a shell intermediary.
export proc run_internal(
  ctx: Context,
  operation: Str,
  privileged: Bool,
  extra: List[Str],
) [process, env, error, io] -> Result[Unit] {
  let image = ensure_image(ctx)?
  let selected_platform = platform(ctx)?
  let identity = unix.id()?
  let stress_repeat = if operation == "test-linux" { env.get_or("XSH_OS_STRESS_REPEAT", "")? } else { "" }
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
  )
  context.run_stage(f"docker-${operation}", ctx.target.triple, "docker", argv, ctx.root, {})?
}
