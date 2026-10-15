use system
use unix
proc machine_and_features() [env, process, error] {
  let machine = system.uname()?.machine
  let features = unix.cpu_features()
  assert machine != ""
  for feature in features { assert feature != "" }
  print ${machine}
  print ${features.len()}
}
machine_and_features()?
