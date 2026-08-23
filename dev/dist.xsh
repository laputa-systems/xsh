##! Distribution build selection, target environment, normalization, and verification.
use context
use docker
use stage as stages
use targets
use verify

# Closed Docker execution policy.
type DockerPolicy = Auto | Always | Never

## Decodes the CLI Docker policy before distribution dispatch.
export pure parse_docker_policy(value: Str) -> Result[DockerPolicy] {
  match value {
    "auto" => return Auto
    "always" => return Always
    "never" => return Never
    _ => return Err(
      stages.StageError.Failed(
        stage: "dist",
        target: "",
        detail: f"unsupported Docker policy ${value}",
      ),
    )
  }
}

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
export proc normalize(ctx: context.Context) [fs, error] -> Result[Unit] {
  let profile_dir = targets.profile_directory(ctx.profile)
  let dist_dir = fp"${ctx.target_dir}/${ctx.target.triple}/dist"

  if profile_dir == "dist" {
    return
  }

  stages.ensure_dir(dist_dir)?

  for product in targets.products {
    let source = profile_product_path(ctx.target_dir, ctx.target.triple, profile_dir, product)
    let destination = distribution_product_path(ctx.target_dir, ctx.target.triple, product)
    fs.install(source, destination, 0o755, parents: true, overwrite: true)?
  }
}

## Executes the native Cargo distribution build with scoped target-specific environment.
export proc native_dist(ctx: context.Context, build_std_variable: Str) [fs, process, env, error, io] -> Result[Unit] {
  let inherited_rustflags = env.get_or("RUSTFLAGS", "")?
  let cflags_name = targets.cflags_variable(ctx.target.triple)?
  let inherited_cflags = env.get_or(cflags_name, "")?
  let environment = targets.distribution_env(
    ctx.target.triple,
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
        ctx.target.triple,
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
  stages.execute(
    stages.command(
      "dist-build",
      ctx.target.triple,
      "cargo",
      argv,
      ctx.root,
      environment,
    ),
  )?
  normalize(ctx)?
  verify.verify_all(ctx, targets.native_execution(ctx.target, ctx.host_os, ctx.host_arch))?
}

## Selects native or Docker distribution execution from the public policy value.
export proc build_distribution(
  ctx: context.Context,
  docker_policy: DockerPolicy,
  ci: Bool,
) [fs, process, env, error, io] -> Result[Unit] {
  let native_possible = targets.native_execution(ctx.target, ctx.host_os, ctx.host_arch)
  let use_docker = match docker_policy {
    Always => true,
    Never => false,
    Auto => ! native_possible,
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
