##! Inspect and repair package repositories.

proc default_repo() [env] -> Str {
  env.get("REPO_ROOT") ?? "/srv/repo"
}

## Verify the index of a repository.
cli main repo check(repo = default_repo(), deep_scan = false) [env] {
  print f"checking {repo} (deep scan: {deep_scan})"
}

## Copy a repository to its mirrors.
cli main repo sync_all(target: Str, jobs: UInt = 4) {
  print f"syncing to {target} with {jobs} jobs"
}

## Print the version.
cli main version() {
  print "1.0"
}
