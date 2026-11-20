use context
use docker
use targets

test test_image_lock_rejects_changed_dockerfile { |ctx|
  let root = test.temp_dir(ctx, name: "image-lock")?
  fp"{root}/dev".mkdir()
  fp"{root}/Dockerfile.test".write("changed\n")
  fp"{root}/dev/images.lock".write("oci.17166969.xyz/xsh/test@sha256:0000000000000000000000000000000000000000000000000000000000000000\nwrong\n")
  let target = targets.resolve("x86_64-unknown-linux-musl")?
  let image_ctx: context.Context = {root: root, target: target, host_os: .Linux, host_arch: .X86_64, target_dir: fp"{root}/target", coverage_dir: fp"{root}/cov", artifact_dir: fp"{root}/dist", profile: "verification", darwin_deployment_target: "26.0"}
  let tools = fp"{root}/tools"
  tools.mkdir()
  fp"{tools}/docker".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  env ({PATH: tools, XSH_TEST_IMAGE: "", XSH_TEST_IMAGE_BUILD: ""}) {
    match docker.ensure_image(image_ctx) {
      Ok(_) => test.fail("stale image lock accepted")
      Err(error) => assert "cargo dev image build" in error.message
    }
  }
}
