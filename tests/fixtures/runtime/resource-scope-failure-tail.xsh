proc primary_failure(lock: FsLock) [fs, error] -> Result[Unit] {
  with held = lock { fail "body stays primary" }
}
