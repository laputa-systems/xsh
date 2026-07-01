#!/usr/bin/env -S xsh --
proc main(...argv: List[Str]) [process, env, error] {
  let parsed = cli.parse(
    argv,
    {
      all: {form: "-a --all", default: false},
      sys: {form: "-s --sys", default: false},
      node: {form: "-n --node", default: false},
      release: {form: "-r --release", default: false},
      version: {form: "-v --version", default: false},
      machine: {form: "-m --machine", default: false},
      operating_system: {form: "-o --operating-system", default: false},
    },
  )?

  let all = argv.len() == 0 or parsed.all
  let u = system.uname()?
  var cols: List[Str] = []

  if all or parsed.sys {
    cols = cols.push(u.sysname)
  }

  if all or parsed.node {
    cols = cols.push(u.nodename)
  }

  if all or parsed.release {
    cols = cols.push(u.release)
  }

  if all or parsed.version {
    cols = cols.push(u.version)
  }

  if all or parsed.machine {
    cols = cols.push(u.machine)
  }

  print cols.join(" ")
}
