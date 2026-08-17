use crate::xsht::format::DEFAULT_LINE_WIDTH;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use xsh::process::cancellation_requested_signal;

pub const CONFIG_FILE_NAME: &str = "xsht-config.ini";

pub fn collect_xsh_files(
    root: &Path,
    excludes: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    check_cancellation()?;
    if root.is_file() {
        if root.extension().is_some_and(|extension| extension == "xsh") {
            files.push(root.to_path_buf());
        }
        return Ok(());
    }
    let mut discovered = collect_xsh_files_parallel(root, excludes)?;
    files.append(&mut discovered);
    files.sort_unstable();
    files.dedup();
    Ok(())
}

pub fn collect_configured_xsh_files(
    root: &Path,
    config: &XshConfig,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    collect_xsh_files(root, &config.exclude, files)?;
    for include in &config.include {
        let path = configured_include_path(root, include);
        if !path.exists() {
            return Err(format!(
                "configured include '{}' does not exist",
                path.display()
            ));
        }
        collect_xsh_files(&path, &config.exclude, files)?;
    }
    files.sort_unstable();
    files.dedup();
    Ok(())
}

pub(crate) fn collect_configured_or_explicit_xsh_files(
    root: &Path,
    config: &XshConfig,
    paths: &[String],
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if paths.is_empty() {
        collect_configured_xsh_files(root, config, &mut files)?;
    } else {
        for path in paths {
            let path = Path::new(path);
            if path.is_dir() {
                collect_xsh_files(path, &config.exclude, &mut files)?;
            } else {
                files.push(path.to_path_buf());
            }
        }
        files.sort_unstable();
        files.dedup();
    }
    Ok(files)
}

fn configured_include_path(root: &Path, include: &str) -> PathBuf {
    let path = PathBuf::from(include);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

#[allow(clippy::single_call_fn)]
fn collect_xsh_files_parallel(root: &Path, excludes: &[String]) -> Result<Vec<PathBuf>, String> {
    let root = root.to_path_buf();
    let excludes = excludes.to_vec();
    let workers = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .max(1);
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut builder = ignore::WalkBuilder::new(&root);
    builder
        .hidden(false)
        .ignore(true)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .threads(workers)
        .add_custom_ignore_filename(".fdignore");
    let walker = builder.build_parallel();
    walker.run(|| {
        let tx = tx.clone();
        let root = root.clone();
        let excludes = excludes.clone();
        Box::new(move |result| {
            if let Err(error) = check_cancellation() {
                let _ = tx.send(Err(error));
                return ignore::WalkState::Quit;
            }
            let entry = match result {
                Ok(entry) => entry,
                Err(error) => {
                    let _ = tx.send(Err(error.to_string()));
                    return ignore::WalkState::Quit;
                }
            };
            let path = entry.path();
            if entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
                && path.extension().is_some_and(|extension| extension == "xsh")
                && !is_path_excluded(&root, path, &excludes)
                && tx.send(Ok(path.to_path_buf())).is_err()
            {
                return ignore::WalkState::Quit;
            }
            ignore::WalkState::Continue
        })
    });
    drop(tx);

    let mut results = Vec::new();
    for result in rx {
        {
            let path = result?;
            results.push(path)
        }
    }
    results.sort_unstable();
    results.dedup();
    Ok(results)
}

fn check_cancellation() -> Result<(), String> {
    if cancellation_requested_signal().is_some() {
        Err("interrupted".to_string())
    } else {
        Ok(())
    }
}

#[allow(clippy::single_call_fn)]
pub(crate) fn is_path_excluded(root: &Path, path: &Path, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }
    let path_str = path.to_string_lossy();
    let normalized = path_str.strip_prefix("./").unwrap_or(&path_str);
    if excludes.iter().any(|pat| glob_matches(pat, normalized)) {
        return true;
    }
    let Ok(stripped) = path.strip_prefix(root) else {
        return false;
    };
    let relative = stripped.to_string_lossy();
    excludes.iter().any(|pat| glob_matches(pat, &relative))
}

#[derive(Clone, Debug, Default)]
pub struct LintConfig {
    pub runless_except: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CoverageConfig {
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct DeadCodeConfig {
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CheckConfig {
    pub annotate: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatConfig {
    pub line_width: usize,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            line_width: DEFAULT_LINE_WIDTH,
        }
    }
}

/// Tooling defaults include the current working directory as the implicit
/// project module root. A project may replace this list with `module_path` in
/// `xsht-config.ini` when its modules live elsewhere.
#[derive(Clone, Debug)]
pub struct XshConfig {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub module_path: Vec<String>,
    pub test_roots: Vec<String>,
    pub check: CheckConfig,
    pub format: FormatConfig,
    pub lint: LintConfig,
    pub dead_code: DeadCodeConfig,
    pub coverage: CoverageConfig,
}

fn default_module_path() -> Vec<String> {
    vec![".".to_string()]
}

impl Default for XshConfig {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            module_path: default_module_path(),
            test_roots: Vec::new(),
            check: CheckConfig::default(),
            format: FormatConfig::default(),
            lint: LintConfig::default(),
            dead_code: DeadCodeConfig::default(),
            coverage: CoverageConfig::default(),
        }
    }
}

pub fn load_config() -> Result<XshConfig, String> {
    load_config_from(Path::new(CONFIG_FILE_NAME))
}

pub fn load_config_from(path: &Path) -> Result<XshConfig, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(XshConfig::default()),
        Err(error) => {
            return Err(format!(
                "failed to read {} '{}': {error}",
                CONFIG_FILE_NAME,
                path.display()
            ));
        }
    };
    let span = xsh::frontend::source::Span::new(xsh::frontend::source::SourceId::new(0), 0, 0);
    let value = xsh::host::ini::decode(&text, span).map_err(|error| {
        format!(
            "invalid {} '{}': {}",
            CONFIG_FILE_NAME,
            path.display(),
            error.message
        )
    })?;
    parse_config_ini(&value)
}

pub(crate) fn nearest_config_for_file(file: &Path) -> Result<Option<(PathBuf, XshConfig)>, String> {
    let parent = file.parent().unwrap_or_else(|| Path::new("."));
    for ancestor in parent.ancestors() {
        let dir = if ancestor.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            ancestor.to_path_buf()
        };
        let candidate = dir.join(CONFIG_FILE_NAME);
        if candidate.is_file() {
            return load_config_from(&candidate).map(|config| Some((dir, config)));
        }
    }
    Ok(None)
}

pub(crate) fn resolve_config_path(config_dir: &Path, raw: String) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        config_dir.join(path)
    }
}

fn parse_config_ini(value: &xsh::execution::value::Value) -> Result<XshConfig, String> {
    let fields = match value {
        xsh::execution::value::Value::Record(fields) => fields,
        _ => return Err(format!("{CONFIG_FILE_NAME} must decode to a record")),
    };
    Ok(XshConfig {
        include: ini_string_list(fields, "include").unwrap_or_default(),
        exclude: ini_string_list(fields, "exclude").unwrap_or_default(),
        module_path: ini_string_list(fields, "module_path").unwrap_or_else(default_module_path),
        test_roots: ini_string_list(fields, "test_roots").unwrap_or_default(),
        check: parse_check_ini(fields),
        format: parse_format_ini(fields)?,
        lint: parse_lint_ini(fields),
        dead_code: parse_dead_code_ini(fields),
        coverage: parse_coverage_ini(fields),
    })
}

fn parse_check_ini(fields: &xsh::execution::value::RecordMap) -> CheckConfig {
    let Some(xsh::execution::value::Value::Record(check)) = fields.get("check") else {
        return CheckConfig::default();
    };
    CheckConfig {
        annotate: ini_string_list(check, "annotate"),
    }
}

fn parse_lint_ini(fields: &xsh::execution::value::RecordMap) -> LintConfig {
    let Some(xsh::execution::value::Value::Record(lint)) = fields.get("lint") else {
        return LintConfig::default();
    };
    LintConfig {
        runless_except: ini_string_list(lint, "runless-except").unwrap_or_default(),
    }
}

fn parse_coverage_ini(fields: &xsh::execution::value::RecordMap) -> CoverageConfig {
    let Some(xsh::execution::value::Value::Record(coverage)) = fields.get("coverage") else {
        return CoverageConfig::default();
    };
    CoverageConfig {
        exclude: ini_string_list(coverage, "exclude").unwrap_or_default(),
    }
}

fn parse_dead_code_ini(fields: &xsh::execution::value::RecordMap) -> DeadCodeConfig {
    let Some(xsh::execution::value::Value::Record(dead_code)) = fields.get("dead-code") else {
        return DeadCodeConfig::default();
    };
    DeadCodeConfig {
        exclude: ini_string_list(dead_code, "exclude").unwrap_or_default(),
    }
}

fn parse_format_ini(fields: &xsh::execution::value::RecordMap) -> Result<FormatConfig, String> {
    let Some(value) = fields.get("format") else {
        return Ok(FormatConfig::default());
    };
    let xsh::execution::value::Value::Record(format) = value else {
        return Err(format!("{CONFIG_FILE_NAME} [format] must be a section"));
    };
    let Some(raw_line_width) = ini_string(format, "line-width") else {
        return Ok(FormatConfig::default());
    };
    let trimmed = raw_line_width.trim();
    let Ok(line_width) = trimmed.parse::<usize>() else {
        return Err(format!(
            "invalid {CONFIG_FILE_NAME} format.line-width: expected a positive integer"
        ));
    };
    if line_width == 0 {
        return Err(format!(
            "invalid {CONFIG_FILE_NAME} format.line-width: expected a positive integer"
        ));
    }
    Ok(FormatConfig { line_width })
}

fn ini_string<'a>(fields: &'a xsh::execution::value::RecordMap, key: &str) -> Option<&'a str> {
    let xsh::execution::value::Value::Str(value) = fields.get(key)? else {
        return None;
    };
    Some(value)
}

fn ini_string_list(fields: &xsh::execution::value::RecordMap, key: &str) -> Option<Vec<String>> {
    let value = ini_string(fields, key)?;
    Some(value.split('\n').map(|s| s.to_string()).collect())
}

#[allow(clippy::single_call_fn)]
fn glob_matches(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    glob_match_parts(&pat, &path)
}

fn glob_match_parts(pat: &[&str], path: &[&str]) -> bool {
    match pat {
        [] => path.is_empty(),
        ["**"] => true,
        ["**", rest @ ..] => {
            glob_match_parts(rest, path) || (!path.is_empty() && glob_match_parts(pat, &path[1..]))
        }
        [p, rest_pat @ ..] => match path {
            [] => false,
            [s, rest_path @ ..] => {
                seg_match(p.as_bytes(), s.as_bytes()) && glob_match_parts(rest_pat, rest_path)
            }
        },
    }
}

fn seg_match(pat: &[u8], seg: &[u8]) -> bool {
    match pat {
        [] => seg.is_empty(),
        [b'*', rest @ ..] => seg_match(rest, seg) || (!seg.is_empty() && seg_match(pat, &seg[1..])),
        [p, rest_pat @ ..] => match seg {
            [] => false,
            [s, rest_seg @ ..] => p == s && seg_match(rest_pat, rest_seg),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::xsht::cli::files::{
        XshConfig, collect_configured_xsh_files, collect_xsh_files, load_config_from,
    };
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn discovery_respects_gitignore_by_default() {
        let root = temp_root("gitignore");
        fs::create_dir_all(root.join("ignored")).expect("create ignored dir");
        fs::write(root.join(".gitignore"), "ignored/\n*.tmp.xsh\n").expect("write gitignore");
        fs::write(root.join("visible.xsh"), "let value = 1\n").expect("write visible");
        fs::write(root.join("hidden.tmp.xsh"), "let value = 1\n").expect("write ignored file");
        fs::write(root.join("ignored").join("nested.xsh"), "let value = 1\n")
            .expect("write ignored nested");

        let files = discover(&root, &[]);

        assert_eq!(relative_paths(&root, &files), vec!["visible.xsh"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovery_applies_config_excludes_after_gitignore() {
        let root = temp_root("excludes");
        fs::create_dir_all(root.join("nested")).expect("create nested dir");
        fs::write(root.join("keep.xsh"), "let value = 1\n").expect("write keep");
        fs::write(root.join("skip.xsh"), "let value = 1\n").expect("write skip");
        fs::write(root.join("nested").join("skip.xsh"), "let value = 1\n")
            .expect("write nested skip");

        let excludes = vec!["skip.xsh".to_string(), "nested/**".to_string()];
        let files = discover(&root, &excludes);

        assert_eq!(relative_paths(&root, &files), vec!["keep.xsh"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn config_parses_coverage_excludes_separately() {
        let root = temp_root("coverage-config");
        fs::create_dir_all(&root).expect("create root");
        let config_path = root.join("xsht-config.ini");
        fs::write(
            &config_path,
            "exclude = generated/**\n\n[coverage]\nexclude = evals/**/*.xsh\n  fixtures/**/*.xsh\n",
        )
        .expect("write config");

        let config = load_config_from(&config_path).expect("load config");

        assert_eq!(config.exclude, vec!["generated/**"]);
        assert_eq!(
            config.coverage.exclude,
            vec!["evals/**/*.xsh", "fixtures/**/*.xsh"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn config_parses_dead_code_excludes_separately() {
        let root = temp_root("dead-code-config");
        fs::create_dir_all(&root).expect("create root");
        let config_path = root.join("xsht-config.ini");
        fs::write(
            &config_path,
            "exclude = generated/**\n\n[dead-code]\nexclude = docs/snippets/**/*.xsh\n",
        )
        .expect("write config");

        let config = load_config_from(&config_path).expect("load config");

        assert_eq!(config.exclude, vec!["generated/**"]);
        assert_eq!(
            config.dead_code.exclude,
            vec!["docs/snippets/**/*.xsh"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configured_includes_add_extra_script_roots() {
        let root = temp_root("includes");
        fs::create_dir_all(root.join(".github").join("scripts")).expect("create scripts dir");
        fs::write(root.join("main.xsh"), "let value = 1\n").expect("write main");
        fs::write(
            root.join(".github").join("scripts").join("release.xsh"),
            "let value = 1\n",
        )
        .expect("write included");

        let mut files = Vec::new();
        collect_configured_xsh_files(
            &root,
            &XshConfig {
                include: vec![".github/scripts".to_string()],
                ..XshConfig::default()
            },
            &mut files,
        )
        .expect("collect configured files");

        assert_eq!(
            relative_paths(&root, &files),
            vec![".github/scripts/release.xsh", "main.xsh"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_file_is_accepted_even_when_ignored_by_discovery() {
        let root = temp_root("explicit");
        let ignored = root.join("ignored.xsh");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join(".gitignore"), "*.xsh\n").expect("write gitignore");
        fs::write(&ignored, "let value = 1\n").expect("write ignored");

        assert_eq!(discover(&root, &[]), Vec::<PathBuf>::new());

        let mut files = Vec::new();
        collect_xsh_files(&ignored, &[], &mut files).expect("collect explicit file");
        assert_eq!(files, vec![ignored]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn discovery_output_is_sorted_and_deduplicated() {
        let root = temp_root("sorted");
        fs::create_dir_all(root.join("b")).expect("create b dir");
        fs::create_dir_all(root.join("a")).expect("create a dir");
        fs::write(root.join("z.xsh"), "let value = 1\n").expect("write z");
        fs::write(root.join("a").join("a.xsh"), "let value = 1\n").expect("write a");
        fs::write(root.join("b").join("b.xsh"), "let value = 1\n").expect("write b");

        let mut files = Vec::new();
        collect_xsh_files(&root, &[], &mut files).expect("first collection");
        collect_xsh_files(&root, &[], &mut files).expect("second collection");

        assert_eq!(
            relative_paths(&root, &files),
            vec!["a/a.xsh", "b/b.xsh", "z.xsh"]
        );
        let _ = fs::remove_dir_all(root);
    }

    fn discover(root: &Path, excludes: &[String]) -> Vec<PathBuf> {
        let mut files = Vec::new();
        collect_xsh_files(root, excludes, &mut files).expect("collect xsh files");
        files
    }

    fn relative_paths(root: &Path, files: &[PathBuf]) -> Vec<String> {
        files
            .iter()
            .map(|path| {
                path.strip_prefix(root)
                    .expect("path under root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("xsh-cli-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp root");
        root
    }
}
