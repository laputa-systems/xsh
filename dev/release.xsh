##! Release artifact names, packaging, core script staging, checksums, and validation.
use context
use targets
use verify

## Returns the exact SHA-256 sidecar content using the repository-relative artifact path.
export proc checksum_line(artifact_path: Path, root: Path) [fs, error] -> Result[Str] {
  return f"""${hash.sha256(artifact_path)?.hex()}  ${artifact_path.relative_to(root).display()}
"""
}

## Packages all products for the selected target into stable release artifact names.
export proc package_binaries(ctx: Context, tag: Str) [fs, process, error, io] -> Result[Unit] {
  if tag.trim() == "" {
    return Err(
      context.ContextError.StageFailed(stage: "release-package", target: ctx.target.triple, detail: "missing release tag"),
    )
  }

  verify.verify_all(ctx, false)?
  let suffix = targets.release_suffix(ctx.target.triple)?
  context.ensure_dir(ctx.artifact_dir)?

  for product in targets.products {
    let source = fp"${ctx.target_dir}/${ctx.target.triple}/dist/${product}"
    let artifact = fp"${ctx.artifact_dir}/${product}-${tag}-${suffix}"
    fs.install(source, artifact, 0o755, parents: true, overwrite: true)?
    fp"${artifact.display()}.sha256".write(checksum_line(artifact, ctx.root)?)?
  }
}

## Runs the release product smoke contract after the distribution build is complete.
export proc smoke(ctx: Context) [fs, process, error, io] -> Result[Unit] {
  verify.verify_all(ctx, true)?
  let xsh = fp"${ctx.target_dir}/${ctx.target.triple}/dist/xsh"
  let xshi = fp"${ctx.target_dir}/${ctx.target.triple}/dist/xshi"
  context.run_stage("release-xsh-startup", ctx.target.triple, xsh.display(), [xsh.display(), "--startup"], ctx.root, {})?
  context.run_stage(
    "release-xshi-smoke",
    ctx.target.triple,
    xshi.display(),
    [xshi.display(), "--no-config", "-c", "print \"ok\""],
    ctx.root,
    {XSHI_ALLOW_NON_TTY_FOR_TESTS: "1"},
  )?
}

## Computes one installed core command path from a source path below `core/`.
export pure core_install_path(relative_source: Path) -> Path {
  return fp"core/${relative_source.display().replace(".xsh", "")}"
}

## Collects core script sources deterministically while excluding the native test subtree.
export proc core_sources(ctx: Context) [fs, error] -> Result[List[Path]] {
  let core = fp"${ctx.root}/core"
  var sources: List[Path] = []

  for entry in fs.walk(core, hidden: true)? {
    if entry.kind == "file" and entry.path.ext() == "xsh" {
      let relative = entry.path.relative_to(core)

      if ! relative.display().starts_with("tests/") {
        sources = sources.push(relative)
      }
    }
  }

  return sources |> sort
}

## Stages, archives, and checksums the core scripts package with stable source ordering.
export proc package_core(ctx: Context, tag: Str) [fs, error] -> Result[Unit] {
  if tag.trim() == "" {
    return Err(
      context.ContextError.StageFailed(stage: "release-core", target: ctx.target.triple, detail: "missing release tag"),
    )
  }

  context.ensure_dir(ctx.artifact_dir)?
  let root_handle = fs.tempdir()?
  defer fs.close_root(root_handle)?
  let stage = fs.root_path(root_handle)?
  let core = fp"${ctx.root}/core"
  let sources = core_sources(ctx)?
  var archive_entries: List[Path] = []

  for relative in sources {
    let installed = core_install_path(relative)
    fs.install(
      fp"${core}/${relative.display()}",
      fp"${stage}/${installed.display()}",
      0o755,
      parents: true,
      overwrite: true,
    )?
    archive_entries = archive_entries.push(installed)
  }

  let core_archive = fp"${ctx.artifact_dir}/core-${tag}.tar.xz"
  archive.tar_create(core_archive, stage, archive_entries, compression: "xz", overwrite: true)?

  if core_archive.metadata()?.size == 0 {
    return Err(
      context.ContextError.StageFailed(stage: "release-core", target: ctx.target.triple, detail: "core archive is empty"),
    )
  }

  fp"${ctx.artifact_dir}/core-${tag}.sha256".write(checksum_line(core_archive, ctx.root)?)?

  for entry in fs.files(ctx.artifact_dir, hidden: true)? {
    if entry.ext == "xz" and entry.path != core_archive {
      return Err(
        context.ContextError.StageFailed(
          stage: "release-core",
          target: ctx.target.triple,
          detail: f"unexpected compressed artifact ${entry.path.display()}",
        ),
      )
    }
  }
}

## Validates the full nine-product release artifact set and checksum sidecars.
export proc validate_artifacts(ctx: Context, tag: Str) [fs, error] -> Result[Unit] {
  if tag.trim() == "" {
    return Err(
      context.ContextError.StageFailed(stage: "release-validate", target: ctx.target.triple, detail: "missing release tag"),
    )
  }

  var expected_files: List[Str] = []

  for triple in ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl", "aarch64-apple-darwin"] {
    let suffix = targets.release_suffix(triple)?

    for product in targets.products {
      let artifact = fp"${ctx.artifact_dir}/${product}-${tag}-${suffix}"
      expected_files = expected_files.extend([artifact.name, f"${artifact.name}.sha256"])
      if artifact.exists()? {
        fs.chmod(artifact, 0o755)?
      }

      if ! artifact.exists()? or ! artifact.executable()? or artifact.metadata()?.size == 0 {
        return Err(
          context.ContextError.StageFailed(
            stage: "release-validate",
            target: triple,
            detail: f"missing artifact ${artifact.display()}",
          ),
        )
      }

      let checksum = fp"${artifact.display()}.sha256"
      if ! checksum.exists()? {
        return Err(
          context.ContextError.StageFailed(
            stage: "release-validate",
            target: triple,
            detail: f"missing checksum ${artifact.display()}.sha256",
          ),
        )
      }

      if checksum.read_text()? != checksum_line(artifact, ctx.root)? {
        return Err(
          context.ContextError.StageFailed(
            stage: "release-validate",
            target: triple,
            detail: f"invalid checksum ${checksum.display()}",
          ),
        )
      }
    }
  }

  for entry in fs.files(ctx.artifact_dir, hidden: true)? {
    if entry.kind == "file" and entry.name not in expected_files {
      return Err(
        context.ContextError.StageFailed(
          stage: "release-validate",
          target: ctx.target.triple,
          detail: f"unexpected artifact ${entry.path.display()}",
        ),
      )
    }
  }
}
