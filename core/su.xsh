#!/bin/xsh
use lib.auth as auth

type SuOptions = {login: Bool, preserve_env: Bool, shell: Str, command: Str, user: Str, extra_args: List[Str]}

pure parse_su_args(argv: List[Str]) -> Result[SuOptions] {
  var login = false
  var preserve_env = false
  var shell = ""
  var command = ""
  var operands: List[Str] = []
  var index = 0
  var operands_only = false

  while index < argv.len() {
    let arg = argv[index]

    if operands_only or arg == "-" or ! arg.starts_with("-") {
      if arg == "-" and operands.len() == 0 and command == "" {
        login = true
      } else {
        operands = operands.push(arg)
      }

      index += 1
      continue
    }

    if arg == "--" {
      operands_only = true
      index += 1
      continue
    }

    if arg == "-l" or arg == "--login" {
      login = true
      index += 1
      continue
    }

    if arg == "-m" or arg == "-p" or arg == "--preserve-environment" or arg == "--preserve-env" {
      preserve_env = true
      index += 1
      continue
    }

    if arg == "-s" or arg == "--shell" {
      if index + 1 >= argv.len() {
        return Err(auth.AuthError.Failed(auth.missing_option_value("su", "s")))
      }

      shell = argv[index + 1]
      index += 2
      continue
    }

    if arg.starts_with("--shell=") {
      shell = arg.replace("--shell=", "")
      index += 1
      continue
    }

    if arg.starts_with("-s") and arg.count_chars() > 2 {
      shell = (arg.split("") |> drop(2)).join("")
      index += 1
      continue
    }

    if arg == "-c" or arg == "--command" {
      if index + 1 >= argv.len() {
        return Err(auth.AuthError.Failed(auth.missing_option_value("su", "c")))
      }

      command = argv[index + 1]
      index += 2
      continue
    }

    if arg.starts_with("--command=") {
      command = arg.replace("--command=", "")
      index += 1
      continue
    }

    if arg.starts_with("-c") and arg.count_chars() > 2 {
      command = (arg.split("") |> drop(2)).join("")
      index += 1
      continue
    }

    if arg.starts_with("--") {
      return Err(auth.AuthError.Failed(auth.unrecognized_option(arg)))
    }

    return Err(auth.AuthError.Failed(auth.invalid_option(arg.split("")[1])))
  }

  let target = if operands.len() == 0 { "root" } else { operands[0] }
  let empty_rest: List[Str] = []
  let rest = if operands.len() <= 1 { empty_rest } else { operands |> drop(1) }

  return {
    login: login,
    preserve_env: preserve_env,
    shell: shell,
    command: command,
    user: target,
    extra_args: rest,
  }
}

proc main(...argv: List[Str]) [fs, process, env, error, io] -> Result[Int] {
  let options = parse_su_args(argv)?

  match auth.lookup_user(options.user) {
    Ok(entry) => {
      let typed_entry: auth.PasswdEntry = entry
      let current = auth.current_user_name()?

      if applet.current_euid() != 0 and options.user != current {
        match auth.authenticate(typed_entry) {
          Ok(_) => {}
          Err(error) => return auth.fail("su", error.message)
        }
      }

      return applet.su_session(
        typed_entry,
        options.login,
        options.preserve_env,
        options.shell,
        options.command,
        options.extra_args,
      )?
    }
    Err(error) => return auth.fail("su", error.message)
  }
}

main(@args)?
