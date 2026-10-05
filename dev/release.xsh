##! Release artifact names, packaging, core script staging, checksums, and validation.
use context
use stage as stages
use targets
use verify

## Returns the exact SHA-256 sidecar content using the repository-relative artifact path.
export proc checksum_line(artifact_path: Path, root: Path) [fs, error] -> Result[Str, Error] {
  f"""{hash.sha256(artifact_path)?.hex()}  {artifact_path.relative_to(root)}
"""
}

## Packages all products for the selected target into stable release artifact names.
export proc package_binaries(ctx: context.Context, tag: Str) [fs, process, error, io] -> Result[Unit, Error] {
  if tag.trim() == "" {
    return Err(
      stages.StageError.Failed(stage: "release-package", target: ctx.target.triple, detail: "missing release tag"),
    )
  }

  verify.verify_all(ctx, false)
  let suffix = targets.release_suffix(ctx.target.triple)?
  stages.ensure_dir(ctx.artifact_dir)

  for product in targets.products {
    let source = fp"{ctx.target_dir}/{ctx.target.triple}/dist/{product}"
    let artifact = fp"{ctx.artifact_dir}/{product}-{tag}-{suffix}"
    fs.install(source, artifact, 0o755, parents: true, overwrite: true)
    fp"{artifact}.sha256".write(checksum_line(artifact, ctx.root)?)
  }
}

## Runs the release product smoke contract after the distribution build is complete.
export proc smoke(ctx: context.Context) [fs, process, error, io] -> Result[Unit, Error] {
  verify.verify_all(ctx, true)
  let xsh = fp"{ctx.target_dir}/{ctx.target.triple}/dist/xsh"
  let xshi = fp"{ctx.target_dir}/{ctx.target.triple}/dist/xshi"
  stages.execute(
    stages.command(
      "release-xsh-startup",
      ctx.target.triple,
      xsh.display(),
      [xsh.display(), "--startup"],
      ctx.root,
      {},
    ),
  )
  stages.execute(
    stages.command(
      "release-xshi-smoke",
      ctx.target.triple,
      xshi.display(),
      [xshi.display(), "--no-config", "-c", "print \"ok\""],
      ctx.root,
      {XSHI_ALLOW_NON_TTY_FOR_TESTS: "1"},
    ),
  )
}

## Commands install without `.xsh`; library modules keep it so `use lib.*`
## resolves beside packaged commands through the normal module loader.
export pure core_install_path(relative_source: Path) -> Path {
  return fp"core/{relative_source}" when relative_source.display().starts_with("lib/")

  let relative = relative_source.display()
  let command = if relative.ends_with(".xsh") {
    relative.byte_slice(0, length: relative.byte_len() - 4)
  } else {
    relative
  }
  fp"core/{command}"
}

type AliasFile = {aliases: List[AliasEntry]}

type AppletEntry = {name: Str, source: Str}

type AliasEntry = {name: Str, target: Str}

type AppletManifest = {
  aliases: List[AliasEntry],
  applets: List[AppletEntry],
  generated_by: Str,
  libraries: List[Str],
}

## Builds the deterministic applet manifest from core sources and the alias table
## `dev/compat/aliases.json`. Each applet is installed as `core/NAME`; each alias
## is an extra executable name for one applet, so a consumer can materialize
## links from this file instead of a hand-maintained list. An alias that names a
## missing applet, or collides with an applet, fails the release.
export proc applet_manifest(ctx: context.Context) [fs, error] -> Result[Str, Error] {
  let sources = core_sources(ctx)?
  var applets: List[AppletEntry] = []
  var libraries: List[Str] = []

  for relative in sources {
    let text = relative.display()
    if text.starts_with("lib/") {
      libraries += [text]
    } else if "/" not in text {
      applets += [AppletEntry(name: text.byte_slice(0, length: text.byte_len() - 4), source: f"core/{text}")]
    }
  }

  let names = applets |> map .name
  let alias_file = fp"{ctx.root}/dev/compat/aliases.json"
  var aliases: List[AliasEntry] = []

  if alias_file.exists()? {
    let table = json.read(alias_file)?.require(AliasFile)?
    for {name: key, target: value} in table.aliases {
      if value not in names {
        return Err(
          stages.StageError.Failed(
            stage: "release-core",
            target: ctx.target.triple,
            detail: f"alias {key} targets missing applet {value}",
          ),
        )
      }

      if key in names {
        return Err(
          stages.StageError.Failed(
            stage: "release-core",
            target: ctx.target.triple,
            detail: f"alias {key} collides with applet core/{key}.xsh",
          ),
        )
      }

      aliases += [AliasEntry(name: key, target: value)]
    }
  }

  let manifest = AppletManifest(
    aliases: aliases |> sort-by .name,
    applets: applets |> sort-by .name,
    generated_by: "dev/release.xsh",
    libraries: libraries |> sort,
  )
  json.encode(manifest, pretty: true)? + "\n"
}

## Collects core script sources deterministically while excluding the native test subtree.
export proc core_sources(ctx: context.Context) [fs, error] -> Result[List[Path], Error] {
  let core = fp"{ctx.root}/core"
  let sources: List[Path] = collect {
    for entry in fs.walk(core, hidden: true)? {
      if entry.kind == "file" and entry.path.ext() == "xsh" {
        let relative = entry.path.relative_to(core)

        yield relative unless relative.display().starts_with("tests/")
      }
    }
  }

  sources |> sort
}

## Stages, archives, and checksums the core scripts package with stable source ordering.
export proc package_core(ctx: context.Context, tag: Str) [fs, error] -> Result[Unit, Error] {
  if tag.trim() == "" {
    return Err(
      stages.StageError.Failed(stage: "release-core", target: ctx.target.triple, detail: "missing release tag"),
    )
  }

  stages.ensure_dir(ctx.artifact_dir)
  let core_archive = fp"{ctx.artifact_dir}/core-{tag}.tar.xz"
  for entry in fs.files(ctx.artifact_dir, hidden: true)? {
    if entry.ext == "xz" and entry.path != core_archive {
      return Err(
        stages.StageError.Failed(
          stage: "release-core",
          target: ctx.target.triple,
          detail: f"unexpected compressed artifact {entry.path}",
        ),
      )
    }
  }

  let root_handle = fs.tempdir()?
  defer root_handle.close()
  let stage = root_handle.host_path()?
  let core = fp"{ctx.root}/core"
  let sources = core_sources(ctx)?
  let archive_entries: List[Path] = collect {
    for relative in sources {
      let installed = core_install_path(relative)
      let mode = if relative.display().starts_with("lib/") { 0o644 } else { 0o755 }
      fs.install(
        fp"{core}/{relative}",
        fp"{stage}/{installed}",
        mode,
        parents: true,
        overwrite: true,
      )
      yield installed
    }
  }

  stages.ensure_dir(fp"{stage}/core")
  fp"{stage}/core/applets.json".write(applet_manifest(ctx)?)
  archive_entries += [p"core/applets.json"]

  archive.tar_create(core_archive, stage, archive_entries, compression: "xz", overwrite: true)

  if core_archive.metadata()?.size == 0 {
    return Err(
      stages.StageError.Failed(stage: "release-core", target: ctx.target.triple, detail: "core archive is empty"),
    )
  }

  fp"{ctx.artifact_dir}/core-{tag}.sha256".write(checksum_line(core_archive, ctx.root)?)
}

## Validates the full nine-product release artifact set and checksum sidecars.
export proc validate_artifacts(ctx: context.Context, tag: Str) [fs, error] -> Result[Unit, Error] {
  if tag.trim() == "" {
    return Err(
      stages.StageError.Failed(stage: "release-validate", target: ctx.target.triple, detail: "missing release tag"),
    )
  }

  let expected_files: List[Str] = collect {
    for triple in ["x86_64-unknown-linux-musl", "aarch64-unknown-linux-musl", "aarch64-apple-darwin"] {
      let suffix = targets.release_suffix(triple)?

      for product in targets.products {
        let artifact = fp"{ctx.artifact_dir}/{product}-{tag}-{suffix}"
        yield @[artifact.name, f"{artifact.name}.sha256"]
        if artifact.exists() {
          artifact.chmod(0o755)
        }

        if ! artifact.exists() or ! artifact.executable() or artifact.metadata()?.size == 0 {
          return Err(
            stages.StageError.Failed(
              stage: "release-validate",
              target: triple,
              detail: f"missing artifact {artifact}",
            ),
          )
        }

        let checksum = fp"{artifact}.sha256"
        if ! checksum.exists() {
          return Err(
            stages.StageError.Failed(
              stage: "release-validate",
              target: triple,
              detail: f"missing checksum {artifact}.sha256",
            ),
          )
        }

        if checksum.read_text()? != checksum_line(artifact, ctx.root)? {
          return Err(
            stages.StageError.Failed(
              stage: "release-validate",
              target: triple,
              detail: f"invalid checksum {checksum}",
            ),
          )
        }
      }
    }
  }

  for entry in fs.files(ctx.artifact_dir, hidden: true)? {
    if entry.kind == "file" and entry.name not in expected_files {
      return Err(
        stages.StageError.Failed(
          stage: "release-validate",
          target: ctx.target.triple,
          detail: f"unexpected artifact {entry.path}",
        ),
      )
    }
  }
}
