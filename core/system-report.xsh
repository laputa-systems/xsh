#!/bin/xsh
use lib.system_report as system_report
use lib.system_report_live as live_collector

error SystemReportCliError = Usage(message: Str) : Usage | InvalidInput(message: Str) : InvalidInput | Unsupported(message: Str) : Unsupported

type SystemReportOptions = {
  from_report: Str,
  json: Bool,
  full: Bool,
  section: Str,
  sensitive: Bool,
  version: Bool,
}

proc main(...argv: List[Str]) [fs, process, env, time, error, io] {
  let options: SystemReportOptions = cli.applet(
    argv,
    {
      from_report: {
        form: "--from FILE",
        default: "",
        help: "read and render a saved v1 report",
      },
      json: {
        form: "--json",
        default: false,
        help: "emit one v1 JSON document",
      },
      full: {
        form: "--full",
        default: false,
        help: "expand the text report",
      },
      section: {
        form: "--section NAME",
        default: "",
        help: "collect and render one report section",
      },
      sensitive: {
        form: "--sensitive",
        default: false,
        help: "include supported identifying values",
      },
      version: {
        form: "-V --version",
        default: false,
        help: "print the report schema version",
      },
    },
  )?

  if options.version {
    print "system-report schema v1"
    return
  }

  if options.section != "" {
    match system_report.parse_report_section(options.section) {
      Ok(_) => {}
      Err(error) => return Err(SystemReportCliError.Usage(error.message))
    }
  }

  if options.from_report == "" {
    let host = system.uname()?
    if host.sysname != "Linux" {
      return Err(
        SystemReportCliError.Unsupported(
          "system-report: live collection is supported on Linux only",
        ),
      )
    }

    let report = live_collector.collect_live(options.section, options.sensitive)?
    if options.json {
      let output = system_report.encode_report_json(report, options.sensitive, false)?
      io.write_stdout(f"""${output}
""")?
    } else {
      io.write_stdout(system_report.render_text(report, options.full, options.sensitive)?)?
    }

    return
  }

  let input_path = fp"${options.from_report}"
  guard let input_root = fs.open_root(input_path.parent()) else { |error|
    return Err(SystemReportCliError.InvalidInput(f"system-report: cannot open replay file parent: ${error.message}"))
  }

  defer input_root.close()?
  guard let input = input_root.read_result(
    fp"${input_path.name()}",
    max_bytes: 16777216,
  ) else { |error|
    return Err(SystemReportCliError.InvalidInput(f"system-report: cannot read replay file: ${error.message}"))
  }

  if input.state != "observed" {
    let errno = if input.errno == null { "unknown" } else { f"${input.errno ?? -1}" }
    return Err(
      SystemReportCliError.InvalidInput(
        f"system-report: cannot read replay file (${input.state}, errno=${errno})",
      ),
    )
  }

  if input.truncated {
    return Err(
      SystemReportCliError.InvalidInput(
        "system-report: replay file exceeds the 16 MiB input limit",
      ),
    )
  }

  if input.data == null {
    return Err(SystemReportCliError.InvalidInput("system-report: replay read produced no bytes"))
  }

  guard let source = input.data.utf8() else { |error|
    return Err(SystemReportCliError.InvalidInput(f"system-report: replay file is not valid UTF-8: ${error.message}"))
  }

  guard let report = system_report.decode_report_json(source) else { |error|
    return Err(SystemReportCliError.InvalidInput(f"system-report: invalid replay report: ${error.message}"))
  }

  var selected: Record = report
  if options.section != "" {
    guard let projected = system_report.select_report_section(report, options.section) else { |error|
      return Err(SystemReportCliError.Usage(error.message))
    }

    selected = projected
  }

  if options.json {
    let output = system_report.encode_report_json(selected, options.sensitive, false)?
    io.write_stdout(f"""${output}
""")?
  } else {
    io.write_stdout(system_report.render_text(selected, options.full, options.sensitive)?)?
  }
}
