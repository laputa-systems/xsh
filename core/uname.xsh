#!/bin/xsh
proc main(...argv: List[Str]) [process, env, error] {
  let parsed = cli.parse(
    argv,
    {
      all: {
        form: "-a --all",
        default: false,
      },
      sys: {
        form: "-s --sys",
        default: false,
      },
      node: {
        form: "-n --node",
        default: false,
      },
      release: {
        form: "-r --release",
        default: false,
      },
      version: {
        form: "-v --version",
        default: false,
      },
      machine: {
        form: "-m --machine",
        default: false,
      },
      operating_system: {
        form: "-o --operating-system",
        default: false,
      },
    },
  )?

  let all = argv.is_empty() or parsed.all
  let u = system.uname()?
  let cols = collect {
    yield u.sysname when all or parsed.sys

    yield u.nodename when all or parsed.node

    yield u.release when all or parsed.release

    yield u.version when all or parsed.version

    yield u.machine when all or parsed.machine
  }

  print cols.join(" ")
}
