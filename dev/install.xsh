##! Host-dispatched installation with explicit Darwin signing and Linux linker setup.
use context
use targets

## Returns the user's installation bin directory without mutating parent environment state.
export proc bin_dir() [fs, env, error] -> Result[Path] {
  let configured_home = env.get_or("HOME", "")?.trim()
  let home = if configured_home == "" { user.current()?.home } else { fp"${configured_home}" }
  let destination = fp"${home}/usr/bin"
  context.ensure_dir(destination)?
  return destination
}

## Installs signed Darwin release products and tolerates only a missing quarantine attribute.
export proc darwin(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  let inherited_rustflags = env.get_or("RUSTFLAGS", "")?
  let inherited_cflags = env.get_or("CFLAGS_aarch64_apple_darwin", "")?
  let command_env = targets.distribution_env(
    ctx.target.triple,
    inherited_rustflags,
    inherited_cflags,
    ctx.darwin_deployment_target,
  )?
  context.run_stage(
    "install-darwin-build",
    ctx.target.triple,
    "cargo",
    [
      "cargo",
      "build",
      "--release",
      "--target",
      ctx.target.triple,
      "-p",
      "xsh",
      "-p",
      "xsht",
      "-p",
      "xshi",
      "--bin",
      "xsh",
      "--bin",
      "xsht",
      "--bin",
      "xshi",
      "--no-default-features",
      "--features",
      targets.distribution_features,
    ],
    ctx.root,
    command_env,
  )?
  let destination_dir = bin_dir()?
  let supplied_flags = env.get_or("DARWIN_CODESIGN_FLAGS", "")?
  var signing_flags = process.argv_words(supplied_flags)?
  let entitlements = env.get_or("DARWIN_CODESIGN_ENTITLEMENTS", "")?.trim()

  if entitlements != "" {
    signing_flags = signing_flags.extend(["--entitlements", entitlements])
  }

  for product in targets.products {
    let source = fp"${ctx.target_dir}/${ctx.target.triple}/release/${product}"
    let destination = fp"${destination_dir}/${product}"
    fs.install(source, destination, 0o755, parents: true, overwrite: true)?
    let codesign_argv = ["codesign", "-fs", "-"].extend(signing_flags).push(destination.display())
    context.run_stage("install-darwin-codesign", ctx.target.triple, "codesign", codesign_argv, ctx.root, {})?
    let xattr = run.capture --text xattr -d com.apple.quarantine $destination ?

    if ! xattr.status.ok and "No such xattr" not in xattr.stderr {
      return Err(
        context.ContextError.StageFailed(
          stage: "install-darwin-xattr",
          target: ctx.target.triple,
          detail: f"failed to remove quarantine from ${destination.display()}",
        ),
      )
    }
  }
}

## Stages one Linux CRT object after stripping debug metadata with LLVM tooling.
export proc linux_crt_object(ctx: context.Context, name: Str) [fs, process, error, io] -> Result[Unit] {
  let crt_dir = fp"${ctx.target_dir}/llvm-crt"
  context.ensure_dir(crt_dir)?
  context.run_stage(
    "install-linux-crt",
    ctx.target.triple,
    "llvm-objcopy",
    ["llvm-objcopy", "--strip-debug", f"/usr/lib/${name}", fp"${crt_dir}/${name}".display()],
    ctx.root,
    {},
  )?
}

## Installs Linux products with the existing clang, llvm-ar, and lld contract.
export proc linux_install(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  if ctx.target.triple != "x86_64-unknown-linux-musl" {
    return Err(
      context.ContextError.StageFailed(
        stage: "install-linux",
        target: ctx.target.triple,
        detail: "Linux installation supports x86_64-unknown-linux-musl",
      ),
    )
  }

  for object in ["Scrt1.o", "crti.o", "crtn.o"] {
    linux_crt_object(ctx, object)?
  }

  let path_value = env.get_or("PATH", "")?
  let rustflags = env.get_or(
    "LINUX_INSTALL_RUSTFLAGS",
    f"-C linker=clang -C link-arg=-B${ctx.root.display()}/target/llvm-crt -C link-arg=-B${ctx.root.display()}/tools -C link-arg=-fuse-ld=lld",
  )?
  context.run_stage(
    "install-linux-build",
    ctx.target.triple,
    "cargo",
    [
      "cargo",
      "build",
      "--release",
      "-p",
      "xsh",
      "-p",
      "xsht",
      "-p",
      "xshi",
      "--bin",
      "xsh",
      "--bin",
      "xsht",
      "--bin",
      "xshi",
      "--no-default-features",
      "--features",
      targets.distribution_features,
    ],
    ctx.root,
    {
      PATH: f"${env.get_or("HOME", "")?}/.cargo/bin:${path_value}",
      CC: "clang",
      AR: "llvm-ar",
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER: "clang",
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: rustflags,
    },
  )?
  let destination_dir = bin_dir()?

  for product in targets.products {
    fs.install(
      fp"${ctx.target_dir}/release/${product}",
      fp"${destination_dir}/${product}",
      0o755,
      parents: true,
      overwrite: true,
    )?
  }
}

## Dispatches installation to the current supported host family.
export proc install(ctx: context.Context) [fs, process, env, error, io] -> Result[Unit] {
  if ctx.host_os == "darwin" {
    return darwin(ctx)
  }

  if ctx.host_os == "linux" {
    return linux_install(ctx)
  }

  return Err(
    context.ContextError.StageFailed(
      stage: "install",
      target: ctx.target.triple,
      detail: f"unsupported host ${ctx.host_os}",
    ),
  )
}
