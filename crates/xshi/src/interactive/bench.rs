use super::complete::{self, CompletionRequest, CompletionState};
use super::denv::DenvState;
use super::edit::{self, LineBuffer};
use super::history::{self, History};
use super::listing;
use super::prompt;
use super::render::{self, RenderOpts, RenderedRegion};
use super::session::Session;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::path::PathBuf;

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

pub fn parse_history(text: &str) -> Vec<String> {
    history::parse_history(text)
}

pub struct BenchSession {
    session: Session,
}

impl BenchSession {
    pub fn with_history(history: Vec<String>) -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut env = BTreeMap::new();
        env.insert(b"PWD".to_vec(), cwd.as_os_str().as_bytes().to_vec());
        if let Some(path) = std::env::var_os("PATH") {
            env.insert(b"PATH".to_vec(), path.into_vec());
        }
        let mut uid_names = BTreeMap::new();
        if let Some(user) = std::env::var_os("USER").and_then(|user| user.into_string().ok()) {
            uid_names.insert(rustix::process::getuid().as_raw(), user);
        }
        let session = Session {
            cwd,
            env,
            aliases: BTreeMap::new(),
            last_status: 0,
            last_process_status: None,
            home,
            history: History::from_entries(history),
            denv: DenvState::default(),
            user: Some("bench".to_string()),
            host: Some("host".to_string()),
            colors: false,
            uid_names,
            cwd_snapshot: None,
            denv_git_root_snapshot: None,
            path_commands: Vec::new(),
            git_prompt: None,
            job: None,
            completion_dir_cache: RefCell::new(BTreeMap::new()),
        };
        Self { session }
    }

    pub fn set_cwd(&mut self, path: &Path) {
        self.session
            .set_cwd(path.to_path_buf())
            .expect("set bench cwd");
    }

    pub fn prefix_search(&self, prefix: &str) -> Option<&str> {
        edit::history_prefix_match(&self.session, prefix)
    }

    pub fn fuzzy_search<'a>(&'a self, needle: &str) -> Option<&'a str> {
        edit::fuzzy_history_match(&self.session, needle)
    }

    pub fn autosuggestion<'a>(&'a self, line: &BenchLine) -> &'a str {
        edit::autosuggestion(&self.session, &line.line)
    }

    pub fn complete_len(&self, text: &str, cursor: usize, term_cols: u16) -> usize {
        complete::start_completion(
            &self.session,
            CompletionRequest {
                text,
                cursor,
                term_cols,
            },
        )
        .comp
        .len()
    }

    pub fn prompt_len(&self) -> usize {
        prompt::prompt(&self.session).len()
    }

    pub fn list_len(&self, args: &[String]) -> usize {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let status = listing::run(&self.session, args, &mut stdout, &mut stderr);
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

pub struct BenchLine {
    line: LineBuffer,
}

impl BenchLine {
    pub fn new(text: &str) -> Self {
        Self {
            line: LineBuffer::from_text(text),
        }
    }
}

pub fn completion_grid(entries: usize, term_cols: u16) -> (usize, usize) {
    let mut comp = complete::Completions::new();
    for index in 0..entries {
        comp.push(
            &format!("file_{index:03}.rs"),
            index % 5 == 0,
            false,
            index % 10 == 0,
        );
    }
    complete::compute_grid(&comp.entries, term_cols)
}

pub struct RenderBench {
    output: Vec<u8>,
    line: LineBuffer,
    region: RenderedRegion,
    completion: CompletionState,
    completion_rows: usize,
}

impl RenderBench {
    pub fn new(line: &str, term_cols: u16) -> Self {
        let mut comp = complete::Completions::new();
        for entry in [
            "alpha-one",
            "alpha-two",
            "arc-three",
            "arc-four",
            "arrow-five",
            "archive-six",
            "atlas-seven",
            "atom-eight",
        ] {
            comp.push(entry, false, false, false);
        }
        let (cols, rows) = complete::compute_grid(&comp.entries, term_cols);
        Self {
            output: Vec::with_capacity(4096),
            line: LineBuffer::from_text(line),
            region: RenderedRegion::default(),
            completion: CompletionState {
                comp,
                selected: 0,
                cols,
                rows,
                scroll: 0,
                term_cols,
                dir_prefix: String::new(),
                in_quote: false,
            },
            completion_rows: 0,
        }
    }

    pub fn render_prompt(&mut self, prompt: &str, term_cols: u16) -> u16 {
        self.output.clear();
        self.region = render::render_line(
            &mut self.output,
            prompt,
            &self.line,
            term_cols,
            self.region,
            &RenderOpts { suggestion: "" },
        )
        .expect("render line");
        self.region.painted_rows
    }

    pub fn render_completion_nav(&mut self, prompt: &str, term_cols: u16) -> usize {
        self.output.clear();
        self.completion.move_down();
        self.region = render::render_line(
            &mut self.output,
            prompt,
            &self.line,
            term_cols,
            self.region,
            &RenderOpts { suggestion: "" },
        )
        .expect("render line");
        self.completion_rows = render::render_completions(
            &mut self.output,
            &self.completion,
            self.region,
            self.completion_rows == 0,
            self.completion_rows,
        )
        .expect("render completions");
        self.completion.selected
    }
}

pub struct HistorySearchRenderBench {
    output: Vec<u8>,
    history: History,
    query: LineBuffer,
    matches: Vec<history::FuzzyMatch>,
    region: RenderedRegion,
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
            output: Vec::with_capacity(8192),
            history,
            query: LineBuffer::from_text(query),
            matches,
            region: RenderedRegion::default(),
            selected: 0,
            term_rows,
            term_cols,
        }
    }

    pub fn render_navigation(&mut self) -> usize {
        self.output.clear();
        if !self.matches.is_empty() {
            self.selected = (self.selected + 1) % self.matches.len();
        }
        self.region = render::render_history_search(
            &mut self.output,
            &self.query,
            &self.matches,
            &self.history,
            self.selected,
            (self.term_rows, self.term_cols),
            self.region,
        )
        .expect("render history search");
        self.selected
    }

    pub fn render_wrapped_query(&mut self) -> u16 {
        self.output.clear();
        self.region = render::render_history_search(
            &mut self.output,
            &self.query,
            &self.matches,
            &self.history,
            self.selected,
            (self.term_rows, self.term_cols),
            self.region,
        )
        .expect("render history search");
        self.region.painted_rows
    }
}
