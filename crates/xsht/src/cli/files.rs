use crate::xsht::format::DEFAULT_LINE_WIDTH;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use xsh::process::cancellation_requested_signal;

pub const CONFIG_FILE_NAME: &str = xsh::frontend::load::PROJECT_CONFIG_FILE_NAME;

pub fn collect_xsh_files(
    root: &Path,
    excludes: &[String],
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    collect_xsh_files_below(root, root, excludes, files)
}

/// Collects the scripts under `root`. `excludes` are the patterns of the
/// config in `config_dir` and name paths below that directory, so a walk
/// that starts deeper in the project excludes exactly what a walk from the
/// config's directory excludes there.
pub fn collect_xsh_files_below(
    root: &Path,
    config_dir: &Path,
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
    let mut discovered = collect_xsh_files_parallel(root, config_dir, excludes)?;
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
                collect_xsh_files_below(path, root, &config.exclude, &mut files)?;
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
fn collect_xsh_files_parallel(
    root: &Path,
    config_dir: &Path,
    excludes: &[String],
) -> Result<Vec<PathBuf>, String> {
    let root = root.to_path_buf();
    let config_dir = config_dir.to_path_buf();
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
        let config_dir = config_dir.clone();
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
                && !is_path_excluded(&config_dir, path, &excludes)
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
    // A config found above the current directory has an absolute root; a
    // relative path is then made absolute the same way before the two meet.
    let absolute;
    let path = if root.is_absolute() && path.is_relative() {
        absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        absolute.as_path()
    } else {
        path
    };
    let Ok(stripped) = path.strip_prefix(root) else {
        return false;
    };
    let relative = stripped.to_string_lossy();
    excludes.iter().any(|pat| glob_matches(pat, &relative))
}

#[derive(Clone, Debug)]
pub struct LintConfig {
    pub prefer_inferred_pure_returns: bool,
    /// On unless `prefer-inferred-private-effects = false`.
    pub prefer_inferred_private_effects: bool,
    /// On unless `prefer-env-string = false`.
    pub prefer_env_string: bool,
    /// On unless `prefer-item-shorthand = false`.
    pub prefer_item_shorthand: bool,
    /// On unless `prefer-tempdir-scope = false`.
    pub prefer_tempdir_scope: bool,
    pub prefer_inferred_variants: bool,
    pub prefer_positional_constructors: bool,
    pub prefer_implicit_messages: bool,
    pub prefer_inferred_proc_returns: bool,
    pub prefer_typed_callables: bool,
    /// On only when `explicit-missing-ok = true`.
    pub explicit_missing_ok: bool,
    pub prefer_non_empty_argv: bool,
    /// On only when `prefer-text-pattern = true`.
    pub prefer_text_pattern: bool,
    pub prefer_rel_path: bool,
    pub runless_except: Vec<String>,
}

impl Default for LintConfig {
    fn default() -> Self {
        Self {
            prefer_inferred_pure_returns: false,
            prefer_inferred_private_effects: true,
            prefer_env_string: true,
            prefer_item_shorthand: true,
            prefer_tempdir_scope: true,
            prefer_inferred_variants: false,
            prefer_positional_constructors: false,
            prefer_implicit_messages: false,
            prefer_inferred_proc_returns: false,
            prefer_typed_callables: false,
            explicit_missing_ok: false,
            prefer_non_empty_argv: false,
            prefer_text_pattern: false,
            prefer_rel_path: false,
            runless_except: Vec::new(),
        }
    }
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
    /// Glob patterns that discovery-driven `xsht fmt` leaves alone; files
    /// named explicitly are still formatted.
    pub exclude: Vec<String>,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            line_width: DEFAULT_LINE_WIDTH,
            exclude: Vec::new(),
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

impl Default for XshConfig {
    fn default() -> Self {
        Self {
            include: Vec::new(),
            exclude: Vec::new(),
            module_path: xsh::frontend::load::default_module_path(),
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
    // The library locates and decodes the file and owns `module_path`, so
    // the tools and the runner agree on a project's module roots.
    match xsh::frontend::load::read_project_config(path)? {
        Some(fields) => parse_config_ini(&fields),
        None => Ok(XshConfig::default()),
    }
}

pub(crate) fn nearest_config_for_file(file: &Path) -> Result<Option<(PathBuf, XshConfig)>, String> {
    let Some(dir) = xsh::frontend::load::nearest_project_config_dir(file) else {
        return Ok(None);
    };
    load_config_from(&dir.join(CONFIG_FILE_NAME)).map(|config| Some((dir, config)))
}

fn parse_config_ini(fields: &xsh::execution::value::RecordMap) -> Result<XshConfig, String> {
    Ok(XshConfig {
        include: ini_string_list(fields, "include").unwrap_or_default(),
        exclude: ini_string_list(fields, "exclude").unwrap_or_default(),
        module_path: xsh::frontend::load::configured_module_path(fields)?,
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
        prefer_inferred_pure_returns: ini_string(lint, "prefer-inferred-pure-returns")
            .is_some_and(|value| value == "true"),
        prefer_inferred_private_effects: ini_string(lint, "prefer-inferred-private-effects")
            .is_none_or(|value| value != "false"),
        prefer_env_string: ini_string(lint, "prefer-env-string")
            .is_none_or(|value| value != "false"),
        prefer_item_shorthand: ini_string(lint, "prefer-item-shorthand")
            .is_none_or(|value| value != "false"),
        prefer_tempdir_scope: ini_string(lint, "prefer-tempdir-scope")
            .is_none_or(|value| value != "false"),
        prefer_inferred_variants: ini_string(lint, "prefer-inferred-variants")
            .is_some_and(|value| value == "true"),
        prefer_positional_constructors: ini_string(lint, "prefer-positional-constructors")
            .is_some_and(|value| value == "true"),
        prefer_implicit_messages: ini_string(lint, "prefer-implicit-messages")
            .is_some_and(|value| value == "true"),
        prefer_inferred_proc_returns: ini_string(lint, "prefer-inferred-proc-returns")
            .is_some_and(|value| value == "true"),
        prefer_typed_callables: ini_string(lint, "prefer-typed-callables")
            .is_some_and(|value| value == "true"),
        explicit_missing_ok: ini_string(lint, "explicit-missing-ok")
            .is_some_and(|value| value == "true"),
        prefer_non_empty_argv: ini_string(lint, "prefer-non-empty-argv")
            .is_some_and(|value| value == "true"),
        prefer_text_pattern: ini_string(lint, "prefer-text-pattern")
            .is_some_and(|value| value == "true"),
        prefer_rel_path: ini_string(lint, "prefer-rel-path").is_some_and(|value| value == "true"),
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
    let exclude = ini_string_list(format, "exclude").unwrap_or_default();
    let Some(raw_line_width) = ini_string(format, "line-width") else {
        return Ok(FormatConfig {
            exclude,
            ..FormatConfig::default()
        });
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
    Ok(FormatConfig {
        line_width,
        exclude,
    })
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
        assert_eq!(config.dead_code.exclude, vec!["docs/snippets/**/*.xsh"]);
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

    #[test]
    fn inferred_variant_and_positional_constructor_lints_are_explicit_opt_in() {
        let root = temp_root("constructor-lints-config");
        let path = root.join("xsht-config.ini");
        fs::write(
            &path,
            "[lint]\nprefer-inferred-variants = true\nprefer-positional-constructors = true\n",
        )
        .unwrap();
        let lint = load_config_from(&path).unwrap().lint;
        assert!(lint.prefer_inferred_variants && lint.prefer_positional_constructors);
        fs::write(&path, "[lint]\n").unwrap();
        let lint = load_config_from(&path).unwrap().lint;
        assert!(!lint.prefer_inferred_variants && !lint.prefer_positional_constructors);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prefer_text_pattern_lint_is_explicit_opt_in() {
        let root = temp_root("prefer-text-pattern-lint-config");
        let path = root.join("xsht-config.ini");
        fs::write(&path, "[lint]\nprefer-text-pattern = true\n").unwrap();
        assert!(load_config_from(&path).unwrap().lint.prefer_text_pattern);
        fs::write(&path, "[lint]\n").unwrap();
        assert!(!load_config_from(&path).unwrap().lint.prefer_text_pattern);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn explicit_missing_ok_lint_is_explicit_opt_in() {
        let root = temp_root("explicit-missing-ok-lint-config");
        let path = root.join("xsht-config.ini");
        fs::write(&path, "[lint]\nexplicit-missing-ok = true\n").unwrap();
        assert!(load_config_from(&path).unwrap().lint.explicit_missing_ok);
        fs::write(&path, "[lint]\n").unwrap();
        assert!(!load_config_from(&path).unwrap().lint.explicit_missing_ok);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn implicit_message_lint_is_explicit_opt_in() {
        let root = temp_root("implicit-message-lint-config");
        let path = root.join("xsht-config.ini");
        fs::write(&path, "[lint]\nprefer-implicit-messages = true\n").unwrap();
        assert!(load_config_from(&path).unwrap().lint.prefer_implicit_messages);
        fs::write(&path, "[lint]\n").unwrap();
        assert!(!load_config_from(&path).unwrap().lint.prefer_implicit_messages);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn private_proc_effects_config_is_on_unless_disabled() {
        let root = temp_root("private-proc-effects-config");
        let path = root.join("xsht-config.ini");
        fs::write(&path, "[lint]\nprefer-inferred-private-effects = true\n").unwrap();
        assert!(
            load_config_from(&path)
                .unwrap()
                .lint
                .prefer_inferred_private_effects
        );
        fs::write(&path, "[lint]\nprefer-inferred-private-effects = false\n").unwrap();
        assert!(
            !load_config_from(&path)
                .unwrap()
                .lint
                .prefer_inferred_private_effects
        );
        fs::write(&path, "[lint]\n").unwrap();
        assert!(
            load_config_from(&path)
                .unwrap()
                .lint
                .prefer_inferred_private_effects
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn migration_lints_are_on_unless_disabled() {
        let root = temp_root("migration-lints-config");
        let path = root.join("xsht-config.ini");
        fs::write(
            &path,
            "[lint]\nprefer-env-string = false\nprefer-item-shorthand = false\nprefer-tempdir-scope = false\n",
        )
        .unwrap();
        let lint = load_config_from(&path).unwrap().lint;
        assert!(
            !lint.prefer_env_string
                && !lint.prefer_item_shorthand
                && !lint.prefer_tempdir_scope
        );
        fs::write(&path, "[lint]\n").unwrap();
        let lint = load_config_from(&path).unwrap().lint;
        assert!(
            lint.prefer_env_string && lint.prefer_item_shorthand && lint.prefer_tempdir_scope
        );
        fs::remove_dir_all(root).unwrap();
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
