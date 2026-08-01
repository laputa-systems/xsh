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

pure parse_getty_args(argv: List[Str]) -> Result[GettyOptions] {
  let opts = cli.applet(
    argv,
    {
      no_prompt: {
        form: "-n --no-prompt",
        default: false,
      },
      no_issue: {
        form: "-i --no-issue",
        default: false,
      },
      issue_file: {
        form: "-f --issue-file FILE",
        kind: "Path",
        default: /etc/issue,
      },
      login: {
        form: "-l --login-program PROGRAM",
        default: "",
      },
      host: {
        form: "-H --host HOST",
        default: "",
      },
      init_string: {
        form: "-I --init-string STRING",
        default: "",
      },
      timeout: {
        form: "-t --timeout SECONDS",
        default: "",
      },
      ignored: {
        form: "-h -L -m -w",
        default: false,
      },
      operands: {
        form: "...ARG",
      },
    },
  )?
  let operands = opts.operands

  if operands.len() != 2 and operands.len() != 3 {
    return Err(auth.AuthError.Failed("missing operand"))
  }

  return {
    no_prompt: opts.no_prompt,
    no_issue: opts.no_issue,
    issue_file: opts.issue_file,
    login: opts.login,
    external_login: opts.login != "",
    host: opts.host,
    init_string: opts.init_string,
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

abort(main(@args)?)
