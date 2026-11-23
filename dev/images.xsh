##! Multi-platform test image construction.
## Build the unpublished multi-platform Linux test image.
export proc build(root: Path) [fs, process, error, io] -> Result[Unit, Error] {
  let output = fp"{root}/.work/test-image.oci.tar"
  let layout = fp"{root}/.work/test-image.oci"
  cd (root) {
    run docker buildx build --platform linux/amd64,linux/arm64 --provenance=false --output f"type=oci,dest={output}" -f Dockerfile.test .
    layout.mkdir(parents: true)
    run tar -xf $output -C $layout
  }
}
