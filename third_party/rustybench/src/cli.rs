use std::{env, ffi::OsString, str::FromStr};

use lexopt::{Parser, prelude::*};

use crate::{
    config::{ParsedSeconds, SortingAttr},
    counter::{BytesFormat, MaxCountUInt},
};

/// The output format requested from the benchmark runner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    #[default]
    Human,
    Terse,
    Json,
}

/// Arguments accepted by the benchmark executable.
#[derive(Debug, Default)]
pub(crate) struct Options {
    pub filters: Vec<String>,
    pub skips: Vec<String>,
    pub exact: bool,
    pub test: bool,
    pub list: bool,
    pub bench: bool,
    pub ignored: bool,
    pub include_ignored: bool,
    pub sort: Option<SortingAttr>,
    pub sortr: Option<SortingAttr>,
    pub sample_count: Option<u32>,
    pub sample_size: Option<u32>,
    pub threads: Option<Vec<usize>>,
    pub min_time: Option<ParsedSeconds>,
    pub max_time: Option<ParsedSeconds>,
    pub skip_ext_time: Option<bool>,
    pub items_count: Option<MaxCountUInt>,
    pub bytes_count: Option<MaxCountUInt>,
    pub bytes_format: Option<BytesFormat>,
    pub chars_count: Option<MaxCountUInt>,
    pub cycles_count: Option<MaxCountUInt>,
    pub format: OutputFormat,
}

fn env_parse_from<T, F>(name: &str, get: &F) -> Result<Option<T>, String>
where
    T: FromStr,
    T::Err: std::fmt::Display,
    F: Fn(&str) -> Option<OsString>,
{
    let Some(value) = get(name) else {
        return Ok(None);
    };

    let value = value
        .into_string()
        .map_err(|_| format!("{name} is not valid Unicode"))?;
    value
        .parse()
        .map(Some)
        .map_err(|error| format!("invalid {name} value {value:?}: {error}"))
}

fn parse_format(value: &str) -> Result<OutputFormat, String> {
    match value {
        "terse" => Ok(OutputFormat::Terse),
        "json" => Ok(OutputFormat::Json),
        "pretty" | "human" => Ok(OutputFormat::Human),
        _ => Err(format!(
            "invalid format value {value:?}; expected pretty, terse, or json"
        )),
    }
}

fn parse_sorting(value: &str) -> Result<SortingAttr, String> {
    match value {
        "kind" => Ok(SortingAttr::Kind),
        "name" => Ok(SortingAttr::Name),
        "location" => Ok(SortingAttr::Location),
        _ => Err(format!(
            "invalid sorting attribute {value:?}; expected kind, name, or location"
        )),
    }
}

fn parse_bytes_format(value: &str) -> Result<BytesFormat, String> {
    match value {
        "decimal" => Ok(BytesFormat::Decimal),
        "binary" => Ok(BytesFormat::Binary),
        _ => Err(format!(
            "invalid bytes format {value:?}; expected decimal or binary"
        )),
    }
}

fn value_string(value: OsString) -> Result<String, lexopt::Error> {
    value.into_string().map_err(lexopt::Error::from)
}

fn parse_threads(value: &str) -> Result<Vec<usize>, String> {
    value
        .split(',')
        .map(|value| {
            value
                .parse()
                .map_err(|error| format!("invalid thread count {value:?}: {error}"))
        })
        .collect()
}

fn parse_env_from<F>(options: &mut Options, get: &F) -> Result<(), String>
where
    F: Fn(&str) -> Option<OsString>,
{
    options.sort = env_parse_from::<String, _>("RUSTYBENCH_SORT", get)?
        .map(|value| parse_sorting(&value))
        .transpose()?;
    options.sortr = env_parse_from::<String, _>("RUSTYBENCH_SORTR", get)?
        .map(|value| parse_sorting(&value))
        .transpose()?;
    options.sample_count = env_parse_from("RUSTYBENCH_SAMPLE_COUNT", get)?;
    options.sample_size = env_parse_from("RUSTYBENCH_SAMPLE_SIZE", get)?;
    options.threads = env_parse_from::<String, _>("RUSTYBENCH_THREADS", get)?
        .map(|value| parse_threads(&value))
        .transpose()?;
    options.min_time = env_parse_from("RUSTYBENCH_MIN_TIME", get)?;
    options.max_time = env_parse_from("RUSTYBENCH_MAX_TIME", get)?;
    options.skip_ext_time = env_parse_from("RUSTYBENCH_SKIP_EXT_TIME", get)?;
    options.items_count = env_parse_from("RUSTYBENCH_ITEMS_COUNT", get)?;
    options.bytes_count = env_parse_from("RUSTYBENCH_BYTES_COUNT", get)?;
    options.bytes_format = env_parse_from::<String, _>("RUSTYBENCH_BYTES_FORMAT", get)?
        .map(|value| parse_bytes_format(&value))
        .transpose()?;
    options.chars_count = env_parse_from("RUSTYBENCH_CHARS_COUNT", get)?;
    options.cycles_count = env_parse_from("RUSTYBENCH_CYCLES_COUNT", get)?;
    Ok(())
}

fn help() {
    eprintln!(
        "Usage: rustybench [FILTER ...] [OPTIONS]\n\n\
         Options:\n\
           --test                         Run each benchmark once\n\
           --list                         List benchmarks\n\
           --skip <FILTER>                Skip matching benchmarks\n\
           --exact                        Use exact filters\n\
           --ignored                      Run only ignored benchmarks\n\
           --include-ignored              Run ignored and normal benchmarks\n\
           --sort <kind|name|location>    Sort ascending\n\
           --sortr <kind|name|location>   Sort descending\n\
           --sample-count <N>             Number of samples\n\
           --sample-size <N>              Iterations per sample\n\
           --threads <N[,N...]>           Benchmark thread counts\n\
           --min-time <SECONDS>           Minimum benchmark duration\n\
           --max-time <SECONDS>           Maximum benchmark duration\n\
           --skip-ext-time[=<BOOL>]       Exclude external time\n\
           --items-count <N>              Throughput item count\n\
           --bytes-count <N>              Throughput byte count\n\
           --bytes-format <decimal|binary>\n\
           --chars-count <N>              Throughput character count\n\
           --cycles-count <N>             Throughput cycle count\n\
           --format <pretty|terse|json>   Select output format\n\
           -h, --help                    Show this help"
    );
}

/// Parses benchmark arguments without allocating a command-definition graph.
pub(crate) fn parse() -> Result<Options, lexopt::Error> {
    parse_with_env(Parser::from_env(), &|name| env::var_os(name))
}

fn parse_with_env<F>(mut parser: Parser, get: &F) -> Result<Options, lexopt::Error>
where
    F: Fn(&str) -> Option<OsString>,
{
    let mut options = Options::default();
    parse_env_from(&mut options, get).map_err(|error| lexopt::Error::Custom(error.into()))?;
    let mut threads_from_cli = false;

    while let Some(arg) = parser.next()? {
        match arg {
            Short('h') | Long("help") => {
                help();
                std::process::exit(0);
            }
            Short('V') | Long("version") => {
                println!("rustybench {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            Long("test") => options.test = true,
            Long("list") => options.list = true,
            Long("bench") => options.bench = true,
            Long("nocapture") | Long("show-output") => {}
            Long("skip") => options.skips.push(value_string(parser.value()?)?),
            Long("exact") => options.exact = true,
            Long("ignored") => options.ignored = true,
            Long("include-ignored") => options.include_ignored = true,
            Long("sort") => {
                options.sort = Some(
                    parse_sorting(&value_string(parser.value()?)?)
                        .map_err(|error| lexopt::Error::Custom(error.into()))?,
                );
            }
            Long("sortr") => {
                options.sortr = Some(
                    parse_sorting(&value_string(parser.value()?)?)
                        .map_err(|error| lexopt::Error::Custom(error.into()))?,
                );
            }
            Long("sample-count") => options.sample_count = Some(parser.value()?.parse()?),
            Long("sample-size") => options.sample_size = Some(parser.value()?.parse()?),
            Long("threads") => {
                let mut threads = parse_threads(&value_string(parser.value()?)?)
                    .map_err(|error| lexopt::Error::Custom(error.into()))?;
                threads.sort_unstable();
                threads.dedup();
                if !threads_from_cli {
                    options.threads = Some(Vec::new());
                    threads_from_cli = true;
                }
                options.threads.get_or_insert_with(Vec::new).extend(threads);
            }
            Long("min-time") => options.min_time = Some(parser.value()?.parse()?),
            Long("max-time") => options.max_time = Some(parser.value()?.parse()?),
            Long("skip-ext-time") => {
                options.skip_ext_time = Some(match parser.optional_value() {
                    Some(value) => value_string(value)?.parse().map_err(|error| {
                        lexopt::Error::Custom(
                            format!("invalid skip-ext-time value: {error}").into(),
                        )
                    })?,
                    None => true,
                });
            }
            Long("items-count") => options.items_count = Some(parser.value()?.parse()?),
            Long("bytes-count") => options.bytes_count = Some(parser.value()?.parse()?),
            Long("bytes-format") => {
                options.bytes_format = Some(
                    parse_bytes_format(&value_string(parser.value()?)?)
                        .map_err(|error| lexopt::Error::Custom(error.into()))?,
                );
            }
            Long("chars-count") => options.chars_count = Some(parser.value()?.parse()?),
            Long("cycles-count") => options.cycles_count = Some(parser.value()?.parse()?),
            Long("format") => {
                options.format = parse_format(&value_string(parser.value()?)?)
                    .map_err(|error| lexopt::Error::Custom(error.into()))?;
            }
            Value(value) => options.filters.push(value_string(value)?),
            _ => return Err(arg.unexpected()),
        }
    }

    if options.ignored && options.include_ignored {
        return Err(lexopt::Error::Custom(
            "--ignored conflicts with --include-ignored"
                .to_owned()
                .into(),
        ));
    }
    if options.list && options.test {
        return Err(lexopt::Error::Custom(
            "--list conflicts with --test".to_owned().into(),
        ));
    }

    Ok(options)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn parse(arguments: &[&str], values: &[(&str, &str)]) -> Result<Options, lexopt::Error> {
        let values: BTreeMap<_, _> = values
            .iter()
            .map(|&(name, value)| (name.to_owned(), OsString::from(value)))
            .collect();
        parse_with_env(Parser::from_args(arguments), &|name| {
            values.get(name).cloned()
        })
    }

    #[test]
    fn command_line_values_override_environment_values() {
        let options = parse(
            &[
                "--sort",
                "name",
                "--sample-count",
                "7",
                "--threads",
                "2,4",
                "input",
            ],
            &[
                ("RUSTYBENCH_SORT", "kind"),
                ("RUSTYBENCH_SAMPLE_COUNT", "3"),
                ("RUSTYBENCH_THREADS", "1,8"),
            ],
        )
        .unwrap();

        assert!(matches!(options.sort, Some(SortingAttr::Name)));
        assert_eq!(options.sample_count, Some(7));
        assert_eq!(options.threads, Some(vec![2, 4]));
        assert_eq!(options.filters, vec!["input"]);
    }

    #[test]
    fn repeated_options_are_accepted_and_last_scalar_value_wins() {
        let options = parse(
            &[
                "--sample-count",
                "3",
                "--sample-count",
                "9",
                "--threads",
                "8,2",
                "--threads",
                "2,4",
            ],
            &[],
        )
        .unwrap();

        assert_eq!(options.sample_count, Some(9));
        assert_eq!(options.threads, Some(vec![2, 8, 2, 4]));
    }

    #[test]
    fn cargo_runner_flags_are_ignored() {
        let options = parse(&["--bench", "--nocapture", "--show-output", "--list"], &[]).unwrap();

        assert!(options.bench);
        assert!(options.list);
    }

    #[test]
    fn malformed_environment_values_are_reported() {
        let error = parse(&[], &[("RUSTYBENCH_SAMPLE_COUNT", "many")])
            .unwrap_err()
            .to_string();

        assert!(error.contains("invalid RUSTYBENCH_SAMPLE_COUNT value"));
    }

    #[test]
    fn conflicting_options_are_rejected() {
        let error = parse(&["--ignored", "--include-ignored"], &[])
            .unwrap_err()
            .to_string();
        assert!(error.contains("--ignored conflicts with --include-ignored"));

        let error = parse(&["--list", "--test"], &[]).unwrap_err().to_string();
        assert!(error.contains("--list conflicts with --test"));
    }

    #[test]
    fn format_values_have_explicit_modes() {
        let options = parse(&["--format", "json"], &[]).unwrap();

        assert_eq!(options.format, OutputFormat::Json);
        assert!(parse_format("sometimes").is_err());
    }
}
