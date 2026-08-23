##! Artifact proofs shared by distribution and release packaging workflows.
use context
use stage as stages

## Verifies one final distribution binary at its typed output location.
export proc binary(ctx: context.Context, name: Str, run_help: Bool) [fs, process, error, io] -> Result[Unit] {
  let product = fp"${ctx.target_dir}/${ctx.target.triple}/dist/${name}"

  if ! product.exists()? {
    return Err(
      stages.StageError.Failed(
        stage: "verify-binary",
        target: ctx.target.triple,
        detail: f"missing artifact ${product.display()}",
      ),
    )
  }

  let metadata = product.metadata()?

  if metadata.size < 1024 {
    return Err(
      stages.StageError.Failed(
        stage: "verify-binary",
        target: ctx.target.triple,
        detail: f"implausibly small artifact ${product.display()}",
      ),
    )
  }

  if ! product.executable()? {
    return Err(
      stages.StageError.Failed(
        stage: "verify-binary",
        target: ctx.target.triple,
        detail: f"artifact is not executable ${product.display()}",
      ),
    )
  }

  if ctx.target.os == "linux" {
    let data = product.read_bytes()?

    if data.len() < 4 or data.slice(0, length: 4) != b"\x7fELF" {
      return Err(
        stages.StageError.Failed(
          stage: "verify-elf",
          target: ctx.target.triple,
          detail: f"not an ELF executable ${product.display()}",
        ),
      )
    }

    let header = run.capture --text readelf -h $product ?

    if ! header.status.ok or ctx.target.elf_machine not in header.stdout {
      return Err(
        stages.StageError.Failed(
          stage: "verify-elf-machine",
          target: ctx.target.triple,
          detail: f"wrong ELF machine for ${product.display()}",
        ),
      )
    }

    let dynamic = run.capture --text readelf -d $product ?

    if ! dynamic.status.ok or "NEEDED" in dynamic.stdout {
      return Err(
        stages.StageError.Failed(
          stage: "verify-static",
          target: ctx.target.triple,
          detail: f"dynamic dependency found in ${product.display()}",
        ),
      )
    }
  } else {
    let description = run.capture --text file -b $product ?

    if ! description.status.ok or ctx.target.executable_format not in description.stdout or "arm64" not in description.stdout {
      return Err(
        stages.StageError.Failed(
          stage: "verify-format",
          target: ctx.target.triple,
          detail: f"wrong executable format for ${product.display()}",
        ),
      )
    }
  }

  if run_help {
    stages.execute(
      stages.command(
        "verify-help",
        ctx.target.triple,
        product.display(),
        [product.display(), "--help"],
        ctx.root,
        {},
      ),
    )?
  }
}

## Verifies every distribution product, executing help only when the host can run the target.
export proc verify_all(ctx: context.Context, run_help: Bool) [fs, process, error, io] -> Result[Unit] {
  for product in ["xsh", "xsht", "xshi"] {
    binary(ctx, product, run_help)?
  }
}
