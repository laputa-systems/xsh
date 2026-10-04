#!/usr/bin/env -S xsh --
# preflight: verify a host is ready to take traffic.
#
#   xsh preflight.xsh -- /etc/preflight.json
#   xsh preflight.xsh -- /etc/preflight.json --emit-json
#
# {"services": [{"name": "api", "port": 8080, "health": "http://127.0.0.1:8080/healthz"}],
#  "disk_threshold": 90, "required_files": ["/etc/app/app.conf"]}

type Service = {name: Str, port: Int, health: Str?}
type Config = {services: List[Service], disk_threshold: Int, required_files: List[Str]}
type Check = {check: Str, ok: Bool, detail: Str}

const pseudo_filesystems = ["devfs", "devtmpfs", "tmpfs", "overlay", "squashfs", "proc", "sysfs"]

proc load_config(file: Path) [fs, error] -> Result[Config] {
  ctx f"loading {file}" {
    # The return type supplies the schema: this is `.require(Config)`.
    json.read(file)?.require()?
  }
}

proc listening(svc: Service) [process, error] -> Result[Check] {
  let owners = process.port(svc.port)? |> where .state == "LISTEN" |> map .command
  let detail = if owners.len() > 0 { owners[0] } else { "nothing listening" }
  Check(check: f"port {svc.port} ({svc.name})", ok: owners.len() > 0, detail:)
}

proc healthy(svc: Service, url: Str) [net] -> Check {
  let label = f"health {svc.name}"
  match net.request({method: "GET", url, timeout: 3s, fail_status: false}) {
    Ok(response) => Check(check: label, ok: response.status == 200, detail: f"HTTP {response.status}")
    Err(error) => Check(check: label, ok: false, detail: error.message)
  }
}

proc disks(threshold: Int) [fs, error] -> Result[List[Check]] {
  fs.mounts()?
    |> where { |m| ! m.readonly and m.blocks_1k > 0 and m.fstype not in pseudo_filesystems }
    |> map { |m|
      Check(
        check: f"disk {m.mounted_on}",
        ok: m.capacity_percent < threshold,
        detail: f"{m.capacity_percent}% used",
      )
    }
}

proc files(names: List[Str]) [fs, error] -> Result[List[Check]] {
  var checks: List[Check] = []
  for name in names {
    let present = fp"{name}".exists()?
    checks += [Check(check: f"file {name}", ok: present, detail: if present { "present" } else { "missing" })]
  }

  checks
}

cli main(config: Path, emit_json = false) {
  let cfg = load_config(config)?

  let probes = cfg.services |> par-map { |svc|
    var found = [listening(svc)?]
    if svc.health != null {
      found += [healthy(svc, svc.health)]
    }

    found
  }

  let service_checks = probes |> flat-map { |found| found }
  let all = service_checks + disks(cfg.disk_threshold)? + files(cfg.required_files)?
  let failed = all |> where { |c| ! c.ok } |> count()

  if emit_json {
    print json.encode({ok: failed == 0, checks: all}, pretty: true)?
  } else {
    for c in all {
      let mark = if c.ok { "ok  " } else { "FAIL" }
      print f"{mark} {c.check:<32} {c.detail}"
    }
  }

  if failed > 0 {
    eprint f"{failed} check(s) failed"
    abort(1)
  }
}
