#!/bin/xsh
use lib.system_report as system_report
use lib.system_report_live as live_collector

error SystemReportError = Usage(message: Str) : Usage | InvalidInput(message: Str) : InvalidInput | Unsupported(message: Str) : Unsupported

type SystemReportOptions = {
  from_report: Str,
  json: Bool,
  full: Bool,
  section: Str,
  sensitive: Bool,
  version: Bool,
}

proc main(...argv: List[Str]) [env, fs, time, system, io, error] {
  let options: SystemReportOptions = cli.applet(
    argv,
    {
      from_report: {form: "--from FILE", default: "", help: "read and render a saved v1 report"},
      json: {form: "--json", default: false, help: "emit one v1 JSON document"},
      full: {form: "--full", default: false, help: "expand the text report"},
      section: {form: "--section NAME", default: "", help: "collect and render one report section"},
      sensitive: {form: "--sensitive", default: false, help: "include supported identifying values"},
      version: {form: "-V --version", default: false, help: "print the report schema version"},
    },
  )?

  if options.version {
    print "system-report schema v1"
    return
  }

  if options.section != "" {
    let _validated_section = match system_report.parse_report_section(options.section) {
      Ok(value) => value
      Err(error) => return Err(SystemReportError.Usage(error.message))
    }
  }

  if options.from_report == "" {
    let host = system.uname()?
    if host.sysname != "Linux" {
      return Err(SystemReportError.Unsupported(
        "system-report: live collection is supported on Linux only",
      ))
    }
    let report = live_collector.collect_live(options.section, options.sensitive)?
    if options.json {
      let output = system_report.encode_report_json(report, options.sensitive, false)?
      io.write_stdout(f"${output}\n")?
    } else {
      io.write_stdout(system_report.render_text(report, options.full, options.sensitive)?)?
    }
    return
  }

  let input_path = fp"${options.from_report}"
  let input_root = match fs.open_root(input_path.parent()) {
    Ok(value) => value
    Err(error) => return Err(SystemReportError.InvalidInput(
      f"system-report: cannot open replay file parent: ${error.message}",
    ))
  }
  defer fs.close_root(input_root)?
  let input = match fs.root_read_result(
    input_root,
    fp"${input_path.name()}",
    max_bytes: 16777216,
  ) {
    Ok(value) => value
    Err(error) => return Err(SystemReportError.InvalidInput(
      f"system-report: cannot read replay file: ${error.message}",
    ))
  }

  if input.state != "observed" {
    let errno = if input.errno == null { "unknown" } else { f"${input.errno}" }
    return Err(SystemReportError.InvalidInput(
      f"system-report: cannot read replay file (${input.state}, errno=${errno})",
    ))
  }
  if input.truncated {
    return Err(SystemReportError.InvalidInput(
      "system-report: replay file exceeds the 16 MiB input limit",
    ))
  }

  if input.data == null {
    return Err(SystemReportError.InvalidInput("system-report: replay read produced no bytes"))
  }

  let source = match input.data.utf8() {
    Ok(value) => value
    Err(error) => return Err(SystemReportError.InvalidInput(
      f"system-report: replay file is not valid UTF-8: ${error.message}",
    ))
  }
  let report = match system_report.decode_report_json(source) {
    Ok(value) => value
    Err(error) => return Err(SystemReportError.InvalidInput(
      f"system-report: invalid replay report: ${error.message}",
    ))
  }
  let selected: Record = if options.section == "" {
    report
  } else {
    match system_report.select_report_section(report, options.section) {
      Ok(value) => value
      Err(error) => return Err(SystemReportError.Usage(error.message))
    }
  }

  if options.json {
    let output = system_report.encode_report_json(selected, options.sensitive, false)?
    io.write_stdout(f"${output}\n")?
  } else {
    io.write_stdout(system_report.render_text(selected, options.full, options.sensitive)?)?
  }
}
