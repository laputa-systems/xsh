#!/bin/xsh
use lib.auth as auth

type SuOptions = {login: Bool, preserve_env: Bool, shell: Str, command: Str, user: Str, extra_args: List[Str]}

pure parse_su_args(argv: List[Str]) -> Result[SuOptions] {
  let options = cli.applet(
    argv,
    {
      login: {
        form: "-l --login",
        default: false,
      },
      preserve_env: {
        form: "-m -p --preserve-environment --preserve-env",
        default: false,
      },
      shell: {
        form: "-s --shell SHELL",
        default: "",
      },
      command: {
        form: "-c --command COMMAND",
        default: "",
      },
      operands: {
        form: "...USER",
      },
    },
  )?
  var login = options.login
  var operands = options.operands

  if operands.len() > 0 and operands[0] == "-" {
    login = true
    operands = operands |> drop(1)
  }

  let target = if operands.len() == 0 { "root" } else { operands[0] }
  let empty_rest: List[Str] = []
  let rest = if operands.len() <= 1 { empty_rest } else { operands |> drop(1) }

  return {
    login: login,
    preserve_env: options.preserve_env,
    shell: options.shell,
    command: options.command,
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

abort(main(@args)?)
