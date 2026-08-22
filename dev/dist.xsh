##! Distribution build selection, target environment, normalization, and verification.
use context
use docker
use targets
use verify

## Parses a whitespace-delimited Cargo override without using a shell boundary.
export proc cargo_words(value: Str) [process, error] -> Result[List[Str]] {
  if value.trim() == "" {
    return []
  }

  return process.argv_words(value)?
}

## Resolves one Cargo profile product path before normalization.
export pure profile_product_path(target_dir: Path, triple: Str, profile: Str, product: Str) -> Path {
  return fp"${target_dir}/${triple}/${targets.profile_directory(profile)}/${product}"
}

## Resolves one stable final distribution product path.
export pure distribution_product_path(target_dir: Path, triple: Str, product: Str) -> Path {
  return fp"${target_dir}/${triple}/dist/${product}"
}

## Copies non-dist profile output into the stable distribution artifact directory.
export proc normalize(ctx: Context) [fs, error] -> Result[Unit] {
  let profile_dir = targets.profile_directory(ctx.profile)
  let dist_dir = fp"${ctx.target_dir}/${ctx.target_triple}/dist"

  if profile_dir == "dist" {
    return
  }

  context.ensure_dir(dist_dir)?

  for product in targets.products {
    let source = profile_product_path(ctx.target_dir, ctx.target_triple, profile_dir, product)
    let destination = distribution_product_path(ctx.target_dir, ctx.target_triple, product)
    fs.install(source, destination, 0o755, parents: true, overwrite: true)?
  }
}

## Executes the native Cargo distribution build with scoped target-specific environment.
export proc native_dist(ctx: Context, build_std_variable: Str) [fs, process, env, error, io] -> Result[Unit] {
  let inherited_rustflags = env.get_or("RUSTFLAGS", "")?
  let cflags_name = targets.cflags_variable(ctx.target_triple)?
  let inherited_cflags = env.get_or(cflags_name, "")?
  let command_env = targets.distribution_env(
    ctx.target_triple,
    inherited_rustflags,
    inherited_cflags,
    ctx.darwin_deployment_target,
  )?
  let build_std = cargo_words(env.get_or(build_std_variable, "-Z build-std=std")?)?
  let argv = [
    "cargo",
    "build",
    "--locked",
    "--profile",
    ctx.profile,
  ].extend(build_std)
    .extend(
      [
        "--target",
        ctx.target_triple,
        "-p",
        "xsh",
        "-p",
        "xsht",
        "-p",
        "xshi",
        "--no-default-features",
        "--features",
        targets.distribution_features,
        "--bin",
        "xsh",
        "--bin",
        "xsht",
        "--bin",
        "xshi",
      ],
    )
  context.run_stage("dist-build", ctx.target_triple, "cargo", argv, ctx.root, command_env)?
  normalize(ctx)?
  verify.verify_all(ctx, targets.native_execution(ctx.target_triple, ctx.host_os, ctx.host_arch)?)?
}

## Selects native or Docker distribution execution from the public policy value.
export proc build_distribution(
  ctx: Context,
  docker_policy: Str,
  ci: Bool,
) [fs, process, env, error, io] -> Result[Unit] {
  let native_possible = targets.native_execution(ctx.target_triple, ctx.host_os, ctx.host_arch)?
  if docker_policy != "always" and docker_policy != "never" and docker_policy != "auto" {
    return Err(
      context.ContextError.StageFailed(
        stage: "dist",
        target: ctx.target_triple,
        detail: f"unsupported Docker policy ${docker_policy}",
      ),
    )
  }

  let use_docker = if docker_policy == "always" {
    true
  } else if docker_policy == "never" {
    false
  } else {
    ! native_possible
  }

  if use_docker {
    docker.run_internal(ctx, "dist", false, [])?
  } else {
    native_dist(ctx, "DIST_BUILD_STD_FLAGS")?
  }

  if ci {
    verify.verify_all(ctx, native_possible)?
  }
}
