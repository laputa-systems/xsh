test pure_native_mode_and_duration_queries_require_no_host_permissions [error] {
  fs.executable(0o755)
  !fs.executable(0o644)
  fs.owner_executable(0o700)
  fs.group_executable(0o070)
  fs.other_executable(0o007)
  fs.world_writable(0o002)
  fs.sticky(0o1000)
  fs.setuid(0o4000)
  fs.setgid(0o2000)
  time.duration_compact(69) == "    1:09"
}
