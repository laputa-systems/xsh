##! Docker image, mount, platform, and direct internal-XSH invocation policy.
use context
use stage as stages
use stage_contract as contract
use targets

## Host-side tool paths mounted read-only into the pinned Linux test container.
export type HostLinkTools = {mold: Path, kache: Path}

## Docker could not identify the immutable image for gate execution.
export error ImageError = InvalidIdentity(detail: Str) | Unavailable(raw_status: Int) | Cancelled(raw_status: Int)

## A container the gate started could not be shown to be gone.
export error ContainerError = InvalidIdentity(detail: Str) | Survived(id: Str) | Unverified(detail: Str)

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

pure image_build(ctx: context.Context, image: Str, selected_platform: Str, extra: List[Str]) -> contract.CommandSpec {
  stages.command(
    "docker-image-build",
    ctx.target.triple,
    "docker",
    ["docker", "build", "--provenance=false", @extra, "--platform", selected_platform, "-t", image, "-f", "Dockerfile.test", "."],
    ctx.root,
    {},
  )
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
    stages.execute(image_build(ctx, image, selected_platform, []))
  }

  image
}

pure valid_image_identity(identity: Str) -> Bool {
  return false when ! identity.starts_with("sha256:") or identity.byte_len() != 71
  for digit in identity[7..].split("") {
    return false when digit not in "0123456789abcdef"
  }
  true
}

## Resolves the immutable ID of the test image. Unless XSH_TEST_IMAGE_BUILD is 0 the image is built first and its ID
## is read from the build, so a retag between the build and a lookup cannot change it.
export proc resolve_image(ctx: context.Context) [fs, process, env, error, io] -> Result[Str, Error] {
  let image = image_name()?
  if env.get_or("XSH_TEST_IMAGE_BUILD", "1")?.trim() == "0" {
    let inspection = run.capture --text docker image inspect --format "{{.Id}}" $image
    let raw = inspection.status.shell_code()?
    return Err(ImageError.Cancelled(raw_status: raw)) when raw == 130
    return Err(ImageError.Unavailable(raw_status: raw)) when raw != 0
    let identity = inspection.stdout.trim()
    return Err(ImageError.InvalidIdentity(detail: "Docker returned an invalid image ID")) when ! valid_image_identity(identity)
    return Err(ImageError.InvalidIdentity(detail: "Docker resolved a different image ID")) when image.starts_with("sha256:") and identity != image
    return identity
  }
  stages.ensure_dir(fp"{ctx.root}/.work/tmp")
  let scratch = fs.tempdir_in(fp"{ctx.root}/.work/tmp")?
  let iidfile = fp"{scratch.host_path()?}/image.iid"
  stages.execute(image_build(ctx, image, platform(ctx)?, ["--iidfile", iidfile.display()]))
  let identity = iidfile.read_text()?.trim()
  return Err(ImageError.InvalidIdentity(detail: "Docker returned an invalid image ID")) when ! valid_image_identity(identity)
  identity
}

## The container confinement a lifecycle operation requests. Namespaces replaces Docker's default seccomp profile
## with dev/gate-namespaces.seccomp.json, which allows the namespace and mount syscalls without CAP_SYS_ADMIN so that
## a process can create user namespaces. It adds no capability, leaves /proc masking and read-only paths in place and
## grants no device access. Isolated keeps the default profile and removes the network.
export enum Confinement { Default, Namespaces, Isolated }

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
  confinement: Confinement = Default,
  cidfile: Path? = null,
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

  if let file = cidfile {
    argv += ["--cidfile", file.display()]
  }

  if privileged {
    argv += ["--privileged"]
  }

  match confinement {
    Default => {}
    Namespaces => { argv += ["--security-opt", f"seccomp={ctx.root}/dev/gate-namespaces.seccomp.json"] }
    Isolated => { argv += ["--network", "none"] }
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
  confinement: Confinement = Default,
  cidfile: Path? = null,
) [fs, process, env, error, io] -> Result[contract.CommandSpec, Error] {
  if linux_test_operation(operation) {
    internal_target_dir(ctx, operation).mkdir(parents: true)?
  }
  let image = if operation in ["gate-lane", "gate-full"] { resolve_image(ctx)? } else { ensure_image(ctx)? }
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
    confinement: confinement,
    cidfile: cidfile,
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

## Removes a container the gate started, by the ID its cidfile recorded, and verifies it is gone. A client that is
## cancelled can leave the container running; this never touches a container the gate did not start.
export proc remove_owned_container(ctx: context.Context, cidfile: Path) [fs, process, error, io, time] -> Result[Unit, Error] {
  return when ! cidfile.exists()?
  let id = cidfile.read_text()?.trim()
  guard id.byte_len() == 64 else { return Err(ContainerError.InvalidIdentity(detail: "the cidfile does not hold a container ID")) }
  for digit in id.split("") {
    return Err(ContainerError.InvalidIdentity(detail: "the cidfile does not hold a container ID")) when digit not in "0123456789abcdef"
  }
  var attempt = 0
  while attempt < 8 {
    let inspection = run.capture --text docker container inspect --format "{{.Id}}" $id
    return when inspection.status.ok == false and "No such container" in inspection.stderr
    return Err(ContainerError.Unverified(detail: inspection.stderr.trim())) when ! inspection.status.ok
    if attempt == 0 {
      let removal = run.capture --text docker rm -f $id
      print f"[docker-container-remove target={ctx.target.triple}] docker rm -f {id}"
      print removal.stderr.trim() when ! removal.status.ok and "No such container" not in removal.stderr and "already in progress" not in removal.stderr
    }
    time.sleep(time.millis(250))
    attempt += 1
  }
  Err(ContainerError.Survived(id: id))
}

## The directory where this run records the cidfile of every container it starts, so one cancellation hook can
## find them all. It belongs to the running process.
export proc owned_container_dir(ctx: context.Context) [process, error] -> Result[Path, Error] {
  fp"{ctx.root}/.work/tmp/gate-containers-{process.current_pid()?}"
}

## Removes every container this run started and verifies each removal, then forgets the registry.
export proc remove_owned_containers(ctx: context.Context) [fs, process, error, io, time] -> Result[Unit, Error] {
  let registry = owned_container_dir(ctx)?
  return when ! registry.exists()?
  var failure: Error? = null
  for entry in fs.children(registry)? {
    if entry.name.ends_with(".cid") {
      match remove_owned_container(ctx, entry.path) {
        Ok(_) => entry.path.remove(missing_ok: true)?
        Err(cause) => { failure = cause when failure == null }
      }
    }
  }
  if let cause = failure { return Err(cause) }
  registry.remove(missing_ok: true)?
}

## Returns the owned container launch status without replacing its raw child code. The container is recorded by a
## cidfile in the run's registry. Whatever way the client ends, a container that is still present is removed and its
## removal verified. A cancellation hook removes the registered containers before the client can be killed.
export proc run_internal_status(ctx: context.Context, operation: Str, privileged: Bool, extra: List[Str], confinement: Confinement = Default) [fs, process, env, error, io, time] -> Result[Status, Error] {
  let registry = owned_container_dir(ctx)?
  registry.mkdir(parents: true)?
  let cidfile = fp"{registry}/{operation}-{time.monotonic()?}.cid"
  let spec = internal_command(ctx, operation, privileged, extra, confinement: confinement, cidfile: cidfile)?
  print f"[{spec.stage} target={spec.target}] {spec.argv.join(" ")}"
  let outcome = try { process.run(process.command_argv(spec.executable, spec.argv, cwd: spec.cwd, env: spec.environment))? }
  remove_owned_container(ctx, cidfile)?
  cidfile.remove(missing_ok: true)?
  registry.remove(missing_ok: true)? when (fs.children(registry)? |> count()) == 0
  outcome
}
