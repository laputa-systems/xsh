use super::builtin;
use super::complete;
use super::history::{FuzzyMatch, History};
use super::line::LineBuffer;
use super::render::{self, HistoryPagerCache, RenderOpts, RenderedRegion};
use super::repl;
use super::session::Session;
use super::term::TermWriter;
use std::collections::BTreeMap;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

pub fn synthetic_history_45k() -> Vec<String> {
    const TEMPLATES: &[&str] = &[
        "git commit -m 'fix issue #{}' --no-verify",
        "git checkout -b feature/task-{}",
        "cargo test --package xsh -- test_{}",
        "rg '{}' src/ --type rust",
        "/opt/homebrew/bin/git diff HEAD~{}",
        "cd ~/projects/project-{}/src",
        "make -j{} build",
        "docker compose up -d service-{}",
        "ssh deploy@prod-{}.example.com",
        "curl -s https://api.example.com/v{}/status",
        "python3 scripts/migrate_{}.py --dry-run",
        "npm run build -- --env=staging-{}",
        "kubectl get pods -n namespace-{}",
        "vim src/module_{}/lib.rs",
        "tar czf backup-{}.tar.gz data/",
    ];
    (0..45_000)
        .map(|i| TEMPLATES[i % TEMPLATES.len()].replace("{}", &i.to_string()))
        .collect()
}

/// A session that touches nothing outside the process: its history lives in
/// memory and there is no denv state.
pub struct BenchSession {
    session: Session,
}

impl BenchSession {
    pub fn with_history(history: Vec<String>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut env = BTreeMap::new();
        env.insert(b"PWD".to_vec(), cwd.as_os_str().as_bytes().to_vec());
        for name in ["PATH", "HOME", "USER"] {
            if let Some(value) = std::env::var_os(name) {
                env.insert(name.as_bytes().to_vec(), value.into_vec());
            }
        }
        let session = Session::detached(cwd, env, History::from_entries(history));
        Self { session }
    }

    pub fn set_cwd(&mut self, path: &Path) {
        self.session
            .set_cwd(path.to_path_buf())
            .expect("set bench cwd");
    }

    pub fn complete_len(&self, text: &str, cursor: usize, term_cols: u16) -> usize {
        let mut line = LineBuffer::new();
        line.set_with_cursor(text, cursor);
        repl::start_completion(
            &line,
            term_cols,
            &self.session,
            complete::Completions::default(),
        )
        .comp
        .len()
    }

    pub fn list_len(&self, args: &[String]) -> usize {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let status = builtin::list_directory(args, &mut stdout, &mut stderr);
        stdout.len() + stderr.len() + status as usize
    }

    pub fn workflow_cd_l_completion_len(&mut self, path: &Path) -> usize {
        self.set_cwd(path);
        self.list_len(&[]) + self.complete_len("d", 1, 80)
    }

    pub fn execute_len(&mut self, source: &str) -> usize {
        let output = super::app::execute_line(&mut self.session, source);
        output.output_len()
    }
}

pub struct RenderBench {
    writer: TermWriter,
    line: LineBuffer,
    region: RenderedRegion,
}

impl RenderBench {
    pub fn new(line: &str, _term_cols: u16) -> Self {
        let mut buffer = LineBuffer::new();
        buffer.set(line);
        Self {
            writer: TermWriter::new(),
            line: buffer,
            region: RenderedRegion::default(),
        }
    }

    pub fn render_prompt(&mut self, prompt: &str, term_cols: u16) -> u16 {
        self.writer.clear_buffer();
        self.region = render::render_line(
            &mut self.writer,
            prompt,
            prompt.len(),
            &self.line,
            term_cols,
            self.region,
            &RenderOpts::default(),
        );
        self.region.painted_rows
    }
}

pub struct HistorySearchRenderBench {
    writer: TermWriter,
    history: History,
    query: String,
    matches: Vec<FuzzyMatch>,
    region: RenderedRegion,
    cache: HistoryPagerCache,
    selected: usize,
    term_rows: u16,
    term_cols: u16,
}

impl HistorySearchRenderBench {
    pub fn new(query: &str, term_rows: u16, term_cols: u16) -> Self {
        let history = History::from_entries(synthetic_history_45k());
        let mut candidates = Vec::new();
        let mut scratch = Vec::new();
        let mut matches = Vec::new();
        history.visible_entry_indices_into(&mut candidates);
        history.fuzzy_search_subset_into(query, &candidates, &mut scratch, &mut matches, 200);
        Self {
            writer: TermWriter::new(),
            history,
            query: query.to_owned(),
            matches,
            region: RenderedRegion::default(),
            cache: HistoryPagerCache::default(),
            selected: 0,
            term_rows,
            term_cols,
        }
    }

    pub fn render_navigation(&mut self) -> usize {
        self.writer.clear_buffer();
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
        }
        self.region = render::render_history_pager_cached(
            &mut self.writer,
            &self.query,
            &self.matches,
            &self.history,
            self.selected,
            self.term_rows,
            self.term_cols,
            self.query.len(),
            self.region,
            &mut self.cache,
        );
        self.selected
    }
}

/// A history stored in a private temporary directory, in the state a shell
/// finds it: folded into the cache, or still a raw log of one session.
pub struct HistoryStoreBench {
    dir: PathBuf,
    path: PathBuf,
    history: History,
    added: usize,
}

impl HistoryStoreBench {
    pub fn compacted(entries: usize) -> Self {
        let mut store = Self::with_log(entries);
        store.history.compact();
        store
    }

    pub fn with_log(entries: usize) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "xshi-bench-history-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create bench history dir");
        let path = dir.join("history");
        let mut history = History::load_from(path.clone());
        let cwd = Path::new("/work/project");
        for command in synthetic_history_45k().into_iter().take(entries) {
            history.add_in_dir(&command, Some(cwd));
        }
        Self {
            dir,
            path,
            history,
            added: 0,
        }
    }

    /// A shell starting up.
    pub fn load_len(&self) -> usize {
        History::load_from(self.path.clone()).len()
    }

    /// The per-prompt check when no other shell has written.
    pub fn sync_idle_len(&mut self) -> usize {
        self.history.sync();
        self.history.len()
    }

    /// Recording one more command.
    pub fn add_len(&mut self) -> usize {
        self.added += 1;
        let command = format!("cargo bench --package xshi -- run_{}", self.added);
        self.history
            .add_in_dir(&command, Some(Path::new("/work/project")));
        self.history.len()
    }

    /// A shell exiting.
    pub fn compact_len(&mut self) -> usize {
        self.history.compact();
        self.history.len()
    }
}

impl Drop for HistoryStoreBench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
