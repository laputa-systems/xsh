#!/usr/bin/env -S xsh --
use lib.auth as auth

type PasswdOptions = {algorithm: Str, action: Str, user: Str}

pure parse_passwd_args(argv: List[Str]) -> Result[PasswdOptions] {
  var algorithm = "sha512"
  var action = "set"
  var user_name = ""
  var index = 0
  var operands_only = false

  while index < argv.len() {
    let arg = argv[index]

    if operands_only or arg == "-" or ! arg.starts_with("-") {
      if user_name != "" {
        return Err(auth.AuthError.Failed("extra operand"))
      }

      user_name = arg
      index += 1
      continue
    }

    if arg == "--" {
      operands_only = true
      index += 1
      continue
    }

    if arg == "-a" or arg == "--algorithm" {
      if index + 1 >= argv.len() {
        return Err(auth.AuthError.Failed(auth.missing_option_value("passwd", "a")))
      }

      algorithm = argv[index + 1]
      index += 2
      continue
    }

    if arg.starts_with("--algorithm=") {
      algorithm = arg.replace("--algorithm=", "")
      index += 1
      continue
    }

    if arg.starts_with("-a") and arg.count_chars() > 2 {
      algorithm = (arg.split("") |> drop(2)).join("")
      index += 1
      continue
    }

    if arg == "-d" or arg == "--delete" {
      action = "delete"
      index += 1
      continue
    }

    if arg == "-l" or arg == "--lock" {
      action = "lock"
      index += 1
      continue
    }

    if arg == "-u" or arg == "--unlock" {
      action = "unlock"
      index += 1
      continue
    }

    if arg.starts_with("--") {
      return Err(auth.AuthError.Failed(auth.unrecognized_option(arg)))
    }

    return Err(auth.AuthError.Failed(auth.invalid_option(arg.split("")[1])))
  }

  return {algorithm: algorithm, action: action, user: user_name}
}

proc read_new_password(user_name: Str, algorithm: Str) [process, error, io] -> Result[Str] {
  print f"Changing password for ${user_name}"
  let first = tui.read_secret("New password: ")?
  let second = tui.read_secret("Retype password: ")?

  if first != second {
    print "Passwords don't match"
    return Err(auth.AuthError.Failed(f"password for ${user_name} is unchanged"))
  }

  return applet.hash_password(first, algorithm)?
}

proc target_user(options: PasswdOptions, passwd: List[auth.PasswdEntry]) [fs, process, env, error] -> Result[Str] {
  let name = if options.user == "" { auth.current_user_name()? } else { options.user }

  for entry in passwd {
    if entry.name == name {
      return name
    }
  }

  return Err(auth.AuthError.Failed(f"unknown user ${name}"))
}

proc main(...argv: List[Str]) [fs, process, env, time, error, io] -> Result[Int] {
  let options = parse_passwd_args(argv)?
  let passwd: List[auth.PasswdEntry] = auth.read_passwd_entries()?
  let records = auth.read_shadow_records()?

  match target_user(options, passwd) {
    Ok(name) => {
      let current = auth.current_password(records, passwd, name)
      var password = ""

      if options.action == "set" {
        password = read_new_password(name, options.algorithm)?
      } else if options.action == "delete" {
        password = ""
      } else if options.action == "lock" {
        password = auth.lock_password(current.password)
      } else if options.action == "unlock" {
        password = auth.unlock_password(current.password)
      } else {
        return auth.fail("passwd", f"invalid action ${options.action}")
      }

      auth.write_shadow_records(auth.upsert_shadow(records, name, password, auth.days_since_epoch()))?
      return 0
    }
    Err(error) => return auth.fail("passwd", error.message)
  }
}

main(@args)?
