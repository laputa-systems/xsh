#!/bin/xsh
use lib.auth as auth

type PasswdOptions = {algorithm: Str, action: Str, user: Str}

pure parse_passwd_args(argv: List[Str]) -> Result[PasswdOptions] {
  let opts = cli.applet(
    argv,
    {
      algorithm: {
        form: "-a --algorithm ALGORITHM",
        default: "sha512",
      },
      delete: {
        form: "-d --delete",
        default: false,
        conflicts: [
          "lock",
          "unlock",
        ],
      },
      lock: {
        form: "-l --lock",
        default: false,
        conflicts: [
          "delete",
          "unlock",
        ],
      },
      unlock: {
        form: "-u --unlock",
        default: false,
        conflicts: [
          "delete",
          "lock",
        ],
      },
      operands: {
        form: "...USER",
      },
    },
  )?

  let action = if opts.delete { "delete" } else if opts.lock { "lock" } else if opts.unlock { "unlock" } else { "set" }
  let user_name = if opts.operands.len() == 0 { "" } else { opts.operands[0] }

  if opts.operands.len() > 1 {
    return Err(auth.AuthError.Failed("extra operand"))
  }

  return {algorithm: opts.algorithm, action: action, user: user_name}
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
  let passwd = auth.read_passwd_entries()?
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

abort(main(@args)?)
