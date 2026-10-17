use crate::xsht::format::DEFAULT_LINE_WIDTH;
use rustc_hash::FxHashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;
use xsh::process::cancellation_requested_signal;

pub const CONFIG_FILE_NAME: &str = xsh::frontend::load::PROJECT_CONFIG_FILE_NAME;

/// Which exclusion lists of a config keep a file out of discovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiscoveryFor {
    /// `exclude` alone.
    Scripts,
    /// `exclude` and `[format] exclude`.
    Formatting,
}

/// The scripts `xsht` processes under `root`: every `.xsh` file below it
/// that the ignore files do not hide and that no config from the file up to
/// the one governing `root` excludes. A `root` that is a file is taken as
/// written, whatever any config says about it.
pub(crate) fn collect_xsh_files(
    root: &Path,
    configs: &ConfigCache,
    purpose: DiscoveryFor,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    check_cancellation()?;
    if root.is_file() {
        if root.extension().is_some_and(|extension| extension == "xsh") {
            files.push(root.to_path_buf());
        }
        return Ok(());
    }
    for path in collect_xsh_files_parallel(root)? {
        if !configs.excludes_from_discovery(root, &path, purpose)? {
            files.push(path);
        }
    }
    files.sort_unstable();
    files.dedup();
    Ok(())
}

/// The scripts a path-oriented command processes. Without `paths` they are
/// the scripts under the current directory and under each `include` entry of
/// `cwd_config`, the config in the current directory: `include` lists the
/// extra roots of one project, so it is read from the project the command
/// was started in and from no other. A directory in `paths` contributes the
/// scripts under it, and a file in `paths` is taken as written.
pub(crate) fn discover_scripts(
    paths: &[String],
    cwd_config: &XshConfig,
    configs: &ConfigCache,
    purpose: DiscoveryFor,
) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    if paths.is_empty() {
        collect_xsh_files(Path::new("."), configs, purpose, &mut files)?;
        for include in &cwd_config.include {
            // Spelled like the files found under `.`, so a root that lies
            // below the current directory adds no second name for a file.
            let path = Path::new(".").join(include);
            if !path.exists() {
                return Err(format!(
                    "configured include '{}' does not exist",
                    path.display()
                ));
            }
            collect_xsh_files(&path, configs, purpose, &mut files)?;
        }
    } else {
        for path in paths {
            let path = Path::new(path);
            if path.is_dir() {
                collect_xsh_files(path, configs, purpose, &mut files)?;
            } else {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort_unstable();
    files.dedup();
    Ok(files)
}

type NearestConfig = Result<Option<(PathBuf, XshConfig)>, String>;

/// The nearest project config of every directory a command asks about, read
/// and decoded once per directory.
#[derive(Default)]
pub(crate) struct ConfigCache {
    nearest: Mutex<FxHashMap<PathBuf, NearestConfig>>,
}

impl ConfigCache {
    /// The config that governs `file`: the nearest one in its directory or
    /// above, with the directory it was found in.
    pub(crate) fn nearest_config_for_file(&self, file: &Path) -> NearestConfig {
        // Keyed by the directory the search starts in, spelled one way, so
        // `a.xsh`, `./a.xsh`, and a directory probed from below share an entry
        // only when they really start in the same place.
        let key = lexically_absolute(file).and_then(|file| file.parent().map(Path::to_path_buf));
        let Some(key) = key else {
            return nearest_config_for_file(file);
        };
        if let Some(cached) = self
            .nearest
            .lock()
            .expect("config cache mutex poisoned")
            .get(&key)
            .cloned()
        {
            return cached;
        }
        let resolved = nearest_config_for_file(file);
        self.nearest
            .lock()
            .expect("config cache mutex poisoned")
            .insert(key, resolved.clone());
        resolved
    }

    /// Whether discovery under `root` skips `file`.
    ///
    /// The settings of a file come from its nearest config alone, but an
    /// exclusion also holds for the projects nested below the config that
    /// states it: a project that excludes `vendor/**` or its worktrees means
    /// the directory, whether or not something in it carries a config of its
    /// own. So every config from the file up to the one that governs `root`
    /// is asked. Configs above that one are not: a command about a nested
    /// project sees the project as it sees itself.
    pub(crate) fn excludes_from_discovery(
        &self,
        root: &Path,
        file: &Path,
        purpose: DiscoveryFor,
    ) -> Result<bool, String> {
        let root = lexically_absolute(root);
        let mut governed = file.to_path_buf();
        while let Some((config_dir, config)) = self.nearest_config_for_file(&governed)? {
            if is_path_excluded(&config_dir, file, &config.exclude)
                || (purpose == DiscoveryFor::Formatting
                    && is_path_excluded(&config_dir, file, &config.format.exclude))
            {
                return Ok(true);
            }
            let below_root = match (&root, lexically_absolute(&config_dir)) {
                (Some(root), Some(config_dir)) => {
                    config_dir != *root && config_dir.starts_with(root)
                }
                _ => false,
            };
            if !below_root {
                break;
            }
            // The next config is the one governing this config's directory.
            governed = config_dir;
        }
        Ok(false)
    }
}

enum DiscoveredPath {
    Script(PathBuf),
    Repository(PathBuf),
}

fn has_repository_marker(directory: &Path) -> Result<bool, String> {
    for marker in [".git", ".jj"] {
        let path = directory.join(marker);
        if path.try_exists().map_err(|error| format!("{}: {error}", path.display()))? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn inside_repository(root: &Path) -> Result<bool, String> {
    let absolute = root.canonicalize().map_err(|error| format!("{}: {error}", root.display()))?;
    for directory in absolute.ancestors() {
        if has_repository_marker(directory)? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[allow(clippy::single_call_fn)]
fn collect_xsh_files_parallel(root: &Path) -> Result<Vec<PathBuf>, String> {
    let workers = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .max(1);
    let mut roots = vec![root.to_path_buf()];
    let mut results = Vec::new();
    while let Some(root) = roots.pop() {
        check_cancellation()?;
        let require_git = inside_repository(&root)?;
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut builder = ignore::WalkBuilder::new(&root);
        builder
            .hidden(false)
            .ignore(true)
            .parents(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .require_git(require_git)
            .threads(workers)
            .add_custom_ignore_filename(".fdignore");
        let walker = builder.build_parallel();
        walker.run(|| {
            let tx = tx.clone();
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
                if !require_git && entry.depth() > 0
                    && entry.file_type().is_some_and(|file_type| file_type.is_dir())
                {
                    match has_repository_marker(path) {
                        Ok(true) => {
                            let _ = tx.send(Ok(DiscoveredPath::Repository(path.to_path_buf())));
                            return ignore::WalkState::Skip;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            let _ = tx.send(Err(error));
                            return ignore::WalkState::Quit;
                        }
                    }
                }
                if entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_file())
                    && path.extension().is_some_and(|extension| extension == "xsh")
                    && tx.send(Ok(DiscoveredPath::Script(path.to_path_buf()))).is_err()
                {
                    return ignore::WalkState::Quit;
                }
                ignore::WalkState::Continue
            })
        });
        drop(tx);

        for result in rx {
            match result? {
                DiscoveredPath::Script(path) => results.push(path),
                DiscoveredPath::Repository(path) => roots.push(path),
            }
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

/// Whether one of `excludes`, the glob patterns of the config in
/// `config_dir`, names `path`. A pattern is matched against the path of the
/// file below the config's directory, so the answer is the same from every
/// directory a command is started in and for every spelling of the path. A
/// file outside the config's directory is never named.
pub(crate) fn is_path_excluded(config_dir: &Path, path: &Path, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }
    let (Some(config_dir), Some(path)) = (lexically_absolute(config_dir), lexically_absolute(path))
    else {
        return false;
    };
    let Ok(relative) = path.strip_prefix(&config_dir) else {
        return false;
    };
    let relative = relative.to_string_lossy();
    excludes.iter().any(|pat| glob_matches(pat, &relative))
}

/// `path` from the filesystem root with `.` dropped and each `..` cancelling
/// the component before it, without consulting symbolic links: the same
/// reading of a path that finds its nearest config. `None` when a relative
/// path has no current directory to start from.
fn lexically_absolute(path: &Path) -> Option<PathBuf> {
    use std::path::Component;
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other),
        }
    }
    Some(normalized)
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
    pub prefer_inferred_proc_returns: bool,
    /// On only when `prefer-set = true`.
    pub prefer_set: bool,
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
            prefer_inferred_proc_returns: false,
            prefer_set: false,
            prefer_text_pattern: false,
            prefer_rel_path: false,
            runless_except: Vec::new(),
        }
    }
}

/// One `[lint.RULE]` section: the rule and the globs, relative to the
/// configuration file, of the files exempt from it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LintRuleExclude {
    pub rule: xsh::diagnostic::DiagnosticCode,
    pub exclude: Vec<String>,
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
    /// `[lint.RULE] exclude`: for each rule, the files `xsht lint` does not
    /// report it in. A corpus that shows an older spelling on purpose, such
    /// as documentation examples, is named here instead of turning the rule
    /// off for the whole project.
    pub lint_rule_excludes: Vec<LintRuleExclude>,
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
            lint_rule_excludes: Vec::new(),
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
        lint_rule_excludes: parse_lint_rule_excludes_ini(fields)?,
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
        prefer_inferred_proc_returns: ini_string(lint, "prefer-inferred-proc-returns")
            .is_some_and(|value| value == "true"),
        prefer_set: ini_string(lint, "prefer-set").is_some_and(|value| value == "true"),
        prefer_text_pattern: ini_string(lint, "prefer-text-pattern")
            .is_some_and(|value| value == "true"),
        prefer_rel_path: ini_string(lint, "prefer-rel-path").is_some_and(|value| value == "true"),
        runless_except: ini_string_list(lint, "runless-except").unwrap_or_default(),
    }
}

/// Reads every `[lint.RULE]` section. The section name is the rule's
/// diagnostic code, so a misspelled or removed rule is an error instead of an
/// exclusion that silently stops applying.
fn parse_lint_rule_excludes_ini(
    fields: &xsh::execution::value::RecordMap,
) -> Result<Vec<LintRuleExclude>, String> {
    let mut excludes = Vec::new();
    for (section, value) in fields {
        if !section.starts_with("lint.") {
            continue;
        }
        let xsh::execution::value::Value::Record(rule_fields) = value else {
            return Err(format!("{CONFIG_FILE_NAME} [{section}] must be a section"));
        };
        let Some(rule) = xsh::diagnostic::DiagnosticCode::from_name(section) else {
            return Err(format!(
                "{CONFIG_FILE_NAME} [{section}] does not name a lint rule"
            ));
        };
        excludes.push(LintRuleExclude {
            rule,
            exclude: ini_string_list(rule_fields, "exclude").unwrap_or_default(),
        });
    }
    Ok(excludes)
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
        CONFIG_FILE_NAME, ConfigCache, DiscoveryFor, LintRuleExclude, collect_xsh_files,
        load_config_from,
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
    #[cfg(unix)]
    fn discovery_skips_linked_worktree_shared_excludes_before_reading_directories() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("linked-worktree-excludes");
        let worktree = root.join("worktree");
        let git_dir = root.join("common.git/worktrees/linked");
        let excluded = worktree.join("private");
        fs::create_dir_all(&git_dir).unwrap();
        fs::create_dir_all(root.join("common.git/info")).unwrap();
        fs::create_dir_all(&excluded).unwrap();
        fs::write(worktree.join(".git"), format!("gitdir: {}\n", git_dir.display())).unwrap();
        fs::write(git_dir.join("commondir"), "../..\n").unwrap();
        fs::write(root.join("common.git/info/exclude"), "/private/\n").unwrap();
        fs::write(worktree.join("visible.xsh"), "let value = 1\n").unwrap();
        fs::write(excluded.join("hidden.xsh"), "let value = 1\n").unwrap();
        fs::set_permissions(&excluded, fs::Permissions::from_mode(0)).unwrap();

        let mut files = Vec::new();
        let result = collect_xsh_files(
            &worktree,
            &ConfigCache::default(),
            DiscoveryFor::Scripts,
            &mut files,
        );
        fs::set_permissions(&excluded, fs::Permissions::from_mode(0o755)).unwrap();
        let actual = relative_paths(&worktree, &files);
        fs::remove_dir_all(root).unwrap();

        result.expect("discover without entering the excluded directory");
        assert_eq!(actual, vec!["visible.xsh"]);
    }

    #[test]
    fn discovery_enters_visible_nested_linked_worktrees_and_keeps_ignore_precedence() {
        let root = temp_root("nested-linked-worktrees");
        for name in ["linked", "hidden", "whitelisted"] {
            linked_worktree(&root, name);
        }
        fs::write(root.join(".gitignore"), "hidden/\nwhitelisted/\n*.outside.xsh\n").unwrap();
        fs::write(root.join(".fdignore"), "!whitelisted/\n").unwrap();
        fs::write(root.join(".ignore"), "!kept.outside.xsh\n").unwrap();
        fs::write(root.join("kept.outside.xsh"), "let value = 1\n").unwrap();
        fs::write(root.join("hidden.outside.xsh"), "let value = 1\n").unwrap();

        let files = discover(&root, &[]);

        assert_eq!(relative_paths(&root, &files), vec![
            "kept.outside.xsh", "linked/visible.xsh", "whitelisted/visible.xsh",
        ]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_keeps_linked_worktree_ignore_whitelists_above_shared_excludes() {
        let root = temp_root("linked-worktree-whitelists");
        let worktree = linked_worktree(&root, "linked");
        fs::write(worktree.join(".gitignore"), "*.blocked.xsh\n").unwrap();
        fs::write(worktree.join(".ignore"), "!kept.blocked.xsh\n!discard.xsh\n").unwrap();
        fs::write(worktree.join(".fdignore"), "!private/\n!private/hidden.xsh\ndiscard.xsh\n").unwrap();
        fs::write(worktree.join("kept.blocked.xsh"), "let value = 1\n").unwrap();
        fs::write(worktree.join("discard.xsh"), "let value = 1\n").unwrap();

        let files = discover(&worktree, &[]);

        assert_eq!(relative_paths(&worktree, &files), vec![
            "kept.blocked.xsh", "private/hidden.xsh", "visible.xsh",
        ]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discovery_respects_ordinary_repository_excludes_from_a_subdirectory() {
        let root = temp_root("ordinary-git-excludes");
        fs::create_dir_all(root.join(".git/info")).unwrap();
        fs::create_dir_all(root.join("sources/private")).unwrap();
        fs::write(root.join(".git/info/exclude"), "sources/private/\n").unwrap();
        fs::write(root.join("sources/visible.xsh"), "let value = 1\n").unwrap();
        fs::write(root.join("sources/private/hidden.xsh"), "let value = 1\n").unwrap();

        let files = discover(&root.join("sources"), &[]);

        assert_eq!(relative_paths(&root, &files), vec!["sources/visible.xsh"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn discovery_resolves_linked_worktree_excludes_through_an_explicit_directory_symlink() {
        let root = temp_root("linked-worktree-symlink");
        let worktree = linked_worktree(&root, "linked");
        let alias = root.join("alias");
        std::os::unix::fs::symlink(&worktree, &alias).unwrap();
        std::os::unix::fs::symlink(&worktree, worktree.join("cycle")).unwrap();

        let files = discover(&alias, &[]);

        assert_eq!(relative_paths(&alias, &files), vec!["visible.xsh"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn discovery_reports_unexcluded_unreadable_directories() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_root("unreadable-directory");
        let private = root.join("private");
        fs::create_dir_all(&private).unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0)).unwrap();
        let permission_denied = fs::read_dir(&private).is_err_and(|error| {
            error.kind() == std::io::ErrorKind::PermissionDenied
        });
        let mut files = Vec::new();
        let result = collect_xsh_files(
            &root,
            &ConfigCache::default(),
            DiscoveryFor::Scripts,
            &mut files,
        );
        fs::set_permissions(&private, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(root).unwrap();

        if permission_denied {
            let error = result.expect_err("report the unreadable directory");
            assert!(error.contains("private"), "{error}");
            assert!(error.contains("Permission denied"), "{error}");
        } else {
            result.expect("privileged directory traversal");
        }
    }

    #[test]
    fn discovery_stops_gitignore_at_repository_boundaries_and_keeps_parent_ignore_rules() {
        let root = temp_root("repository-ignore-boundary");
        let repository = root.join("nested");
        fs::create_dir_all(repository.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "*.outside.xsh\n").unwrap();
        fs::write(root.join(".ignore"), "parent-hidden.xsh\nrestored.xsh\n").unwrap();
        fs::write(root.join(".fdignore"), "parent-fd-hidden.xsh\n!restored.xsh\n").unwrap();
        for file in [
            root.join("hidden.outside.xsh"),
            repository.join("visible.outside.xsh"),
            repository.join("parent-hidden.xsh"),
            repository.join("parent-fd-hidden.xsh"),
            repository.join("restored.xsh"),
        ] {
            fs::write(file, "let value = 1\n").unwrap();
        }

        let files = discover(&root, &[]);

        assert_eq!(relative_paths(&root, &files), vec![
            "nested/restored.xsh", "nested/visible.outside.xsh",
        ]);
        fs::remove_dir_all(root).unwrap();
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
    fn config_reads_rule_excludes_and_rejects_an_unknown_rule() {
        let root = temp_root("lint-rule-exclude-config");
        fs::create_dir_all(&root).unwrap();
        let path = root.join(CONFIG_FILE_NAME);
        fs::write(
            &path,
            "[lint]\nprefer-env-string = false\n\n[lint.prefer-inferred-variant]\nexclude = docs/snippets/**/*.xsh\n  examples/old/*.xsh\n",
        )
        .unwrap();
        let config = load_config_from(&path).unwrap();
        assert!(!config.lint.prefer_env_string);
        assert_eq!(
            config.lint_rule_excludes,
            vec![LintRuleExclude {
                rule: xsh::diagnostic::DiagnosticCode::LintPreferInferredVariant,
                exclude: vec![
                    "docs/snippets/**/*.xsh".to_string(),
                    "examples/old/*.xsh".to_string()
                ],
            }]
        );

        fs::write(
            &path,
            "[lint.prefer-inferred-variants]\nexclude = docs/**\n",
        )
        .unwrap();
        assert_eq!(
            load_config_from(&path).unwrap_err(),
            "xsht-config.ini [lint.prefer-inferred-variants] does not name a lint rule"
        );
        fs::remove_dir_all(root).unwrap();
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
    fn discovery_applies_the_exclusions_of_each_config_from_the_file_up_to_the_root() {
        let root = temp_root("nearest-excludes");
        let inner = root.join("vendor").join("inner");
        fs::create_dir_all(inner.join("generated")).expect("create inner project");
        fs::write(
            root.join(CONFIG_FILE_NAME),
            "exclude = vendor/**\n  skip.xsh\n",
        )
        .expect("write outer config");
        fs::write(inner.join(CONFIG_FILE_NAME), "exclude = generated/**\n")
            .expect("write inner config");
        for file in [
            root.join("keep.xsh"),
            root.join("skip.xsh"),
            root.join("vendor").join("loose.xsh"),
            inner.join("main.xsh"),
            inner.join("skip.xsh"),
            inner.join("generated").join("out.xsh"),
        ] {
            fs::write(file, "let value = 1\n").expect("write script");
        }

        // From the outer project, its `vendor/**` drops the whole directory,
        // the project nested in it included. From the nested project only
        // its own config speaks: it drops `generated/**` and knows nothing of
        // the outer `vendor/**` or `skip.xsh`.
        let files = discover(&root, &[]);
        assert_eq!(relative_paths(&root, &files), vec!["keep.xsh"]);
        assert_eq!(
            relative_paths(&root, &discover(&root.join("vendor"), &[])),
            Vec::<String>::new()
        );
        assert_eq!(
            relative_paths(&root, &discover(&inner, &[])),
            vec!["vendor/inner/main.xsh", "vendor/inner/skip.xsh"]
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
        collect_xsh_files(
            &ignored,
            &ConfigCache::default(),
            DiscoveryFor::Scripts,
            &mut files,
        )
        .expect("collect explicit file");
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
        let configs = ConfigCache::default();
        collect_xsh_files(&root, &configs, DiscoveryFor::Scripts, &mut files)
            .expect("first collection");
        collect_xsh_files(&root, &configs, DiscoveryFor::Scripts, &mut files)
            .expect("second collection");

        assert_eq!(
            relative_paths(&root, &files),
            vec!["a/a.xsh", "b/b.xsh", "z.xsh"]
        );
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
            !lint.prefer_env_string && !lint.prefer_item_shorthand && !lint.prefer_tempdir_scope
        );
        fs::write(&path, "[lint]\n").unwrap();
        let lint = load_config_from(&path).unwrap().lint;
        assert!(lint.prefer_env_string && lint.prefer_item_shorthand && lint.prefer_tempdir_scope);
        fs::remove_dir_all(root).unwrap();
    }

    /// The scripts discovery finds under `root`, after writing `excludes`,
    /// when there are any, as the `exclude` of a config in `root`.
    fn discover(root: &Path, excludes: &[String]) -> Vec<PathBuf> {
        if !excludes.is_empty() {
            fs::write(
                root.join(CONFIG_FILE_NAME),
                format!("exclude = {}\n", excludes.join("\n  ")),
            )
            .expect("write config");
        }
        let mut files = Vec::new();
        collect_xsh_files(
            root,
            &ConfigCache::default(),
            DiscoveryFor::Scripts,
            &mut files,
        )
        .expect("collect xsh files");
        files
    }

    fn linked_worktree(root: &Path, name: &str) -> PathBuf {
        let worktree = root.join(name);
        let git_dir = root.join("common.git/worktrees").join(name);
        fs::create_dir_all(&git_dir).unwrap();
        fs::create_dir_all(root.join("common.git/info")).unwrap();
        fs::create_dir_all(worktree.join("private")).unwrap();
        fs::write(worktree.join(".git"), format!("gitdir: {}\n", git_dir.display())).unwrap();
        fs::write(git_dir.join("commondir"), "../..\n").unwrap();
        fs::write(root.join("common.git/info/exclude"), "/private/\n").unwrap();
        fs::write(worktree.join("visible.xsh"), "let value = 1\n").unwrap();
        fs::write(worktree.join("private/hidden.xsh"), "let value = 1\n").unwrap();
        worktree
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
