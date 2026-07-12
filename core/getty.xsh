#!/bin/xsh
use lib.auth as auth

type GettyOptions = {
  no_prompt: Bool,
  no_issue: Bool,
  issue_file: Path,
  login: Str,
  external_login: Bool,
  host: Str,
  init_string: Str,
  baud: Str,
  tty: Str,
  term: Str,
}

pure tail_after(arg: Str, count: Int) -> Str {
  return (arg.split("") |> drop(count)).join("")
}

pure parse_getty_args(argv: List[Str]) -> Result[GettyOptions] {
  var no_prompt = false
  var no_issue = false
  var issue_file = /etc/issue
  var login = ""
  var external_login = false
  var host = ""
  var init_string = ""
  var operands: List[Str] = []
  var index = 0

  while index < argv.len() {
    let arg = argv[index]

    if arg == "-" or ! arg.starts_with("-") {
      operands = operands.push(arg)
      index += 1
      continue
    }

    if arg == "--no-prompt" or arg == "-n" {
      no_prompt = true
      index += 1
      continue
    }

    if arg == "--no-issue" or arg == "-i" {
      no_issue = true
      index += 1
      continue
    }

    if arg == "--issue-file" or arg == "-f" {
      if index + 1 >= argv.len() {
        return Err(auth.AuthError.Failed(auth.missing_option_value("getty", "f")))
      }

      issue_file = fp"${argv[index + 1]}"
      index += 2
      continue
    }

    if arg.starts_with("--issue-file=") {
      issue_file = fp"${arg.replace("--issue-file=", "")}"
      index += 1
      continue
    }

    if arg.starts_with("-f") and arg.count_chars() > 2 {
      issue_file = fp"${tail_after(arg, 2)}"
      index += 1
      continue
    }

    if arg == "--login-program" or arg == "-l" {
      if index + 1 >= argv.len() {
        return Err(auth.AuthError.Failed(auth.missing_option_value("getty", "l")))
      }

      login = argv[index + 1]
      external_login = true
      index += 2
      continue
    }

    if arg.starts_with("--login-program=") {
      login = arg.replace("--login-program=", "")
      external_login = true
      index += 1
      continue
    }

    if arg.starts_with("-l") and arg.count_chars() > 2 {
      login = tail_after(arg, 2)
      external_login = true
      index += 1
      continue
    }

    if arg == "--init-string" or arg == "-I" {
      if index + 1 >= argv.len() {
        return Err(auth.AuthError.Failed(auth.missing_option_value("getty", "I")))
      }

      init_string = argv[index + 1]
      index += 2
      continue
    }

    if arg.starts_with("--init-string=") {
      init_string = arg.replace("--init-string=", "")
      index += 1
      continue
    }

    if arg.starts_with("-I") and arg.count_chars() > 2 {
      init_string = tail_after(arg, 2)
      index += 1
      continue
    }

    if arg == "--host" or arg == "-H" {
      if index + 1 >= argv.len() {
        return Err(auth.AuthError.Failed(auth.missing_option_value("getty", "H")))
      }

      host = argv[index + 1]
      index += 2
      continue
    }

    if arg.starts_with("--host=") {
      host = arg.replace("--host=", "")
      index += 1
      continue
    }

    if arg.starts_with("-H") and arg.count_chars() > 2 {
      host = tail_after(arg, 2)
      index += 1
      continue
    }

    if arg == "--timeout" or arg == "-t" {
      if index + 1 >= argv.len() {
        return Err(auth.AuthError.Failed(auth.missing_option_value("getty", "t")))
      }

      index += 2
      continue
    }

    if arg.starts_with("--timeout=") or arg.starts_with("-t") and arg.count_chars() > 2 {
      index += 1
      continue
    }

    if arg == "-h" or arg == "-L" or arg == "-m" or arg == "-w" {
      index += 1
      continue
    }

    if arg.starts_with("--") {
      return Err(auth.AuthError.Failed(auth.unrecognized_option(arg)))
    }

    return Err(auth.AuthError.Failed(auth.invalid_option(arg.split("")[1])))
  }

  if operands.len() != 2 and operands.len() != 3 {
    return Err(auth.AuthError.Failed("missing operand"))
  }

  return {
    no_prompt: no_prompt,
    no_issue: no_issue,
    issue_file: issue_file,
    login: login,
    external_login: external_login,
    host: host,
    init_string: init_string,
    baud: operands[0],
    tty: operands[1],
    term: if operands.len() == 3 { operands[2] } else { "" },
  }
}

proc run_external_login(options: GettyOptions, username: Str) [process, error] -> Result[Int] {
  let login = if options.login == "" { process.which("login")?.display() } else { options.login }
  var argv = [login]

  if options.host != "" {
    argv = argv.push("-h")
    argv = argv.push(options.host)
  }

  if username != "" {
    argv = argv.push(username)
  }

  let env_record = if options.term == "" { {} } else { {TERM: options.term} }
  let status = process.run(process.command_argv(login, argv, env: env_record))?

  if status.exited() {
    return status.exit_code()?
  }

  return 1
}

proc main(...argv: List[Str]) [fs, process, error, io] -> Result[Int] {
  let options = parse_getty_args(argv)?

  if options.init_string != "" {
    io.write_stdout(options.init_string)?
  }

  if ! options.no_issue and options.issue_file.exists()? {
    io.write_stdout(options.issue_file.read_text()?)?
  }

  var username = ""

  if ! options.no_prompt {
    username = tui.read_secret("login: ")?
  }

  return run_external_login(options, username)?
}

main(@args)?
