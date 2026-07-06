proc main(src: Path, dest: Path) [fs, env, error] {
  let pkg = module.load(/Users/josh/d/laputa-systems/packages/repo/baselayout/PKGBUILD.xsh)?
  let build_fn: Proc = pkg.get("build")?
  fs.remove(dest, missing_ok: true)?
  fs.mkdir(dest)?

  cd src {
    build_fn.call(dest)?
  } ?
}

main(@args)?
