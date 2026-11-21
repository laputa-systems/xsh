##! Fixture entry.
use pm.catalog as cat
use system.image
use system.remote as remote
use lib.reexport as reexport
use pm.types as pm_types

let target: pm_types.Target = {triple: "x86_64"}

print image.format_size(7)
print ${remote.REMOTE_RETRY_LIMIT}
print ${reexport.relayed_limit}
print ${pm_types.Payload == pm_types.Meta}
print ${target.triple}

proc main() [fs, error] {
  print cat.load(p".")?.name
}
