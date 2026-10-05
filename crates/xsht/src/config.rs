use crate::xsht::cli::{XshConfig, nearest_config_for_file};
use std::path::{Path, PathBuf};

/// What governs one file: the nearest project config above it, and nothing
/// else. The directory a command is started in never changes it, so a file
/// lints, checks, and formats the same from anywhere.
#[derive(Clone, Debug)]
pub(crate) struct FileToolConfig {
    pub config_dir: PathBuf,
    pub config: XshConfig,
    // The project module roots of the file. `xsh` finds roots the same way,
    // so a file checks with exactly the modules it would load.
    module_roots: Vec<PathBuf>,
}

impl FileToolConfig {
    /// The tool configuration of a file whose nearest project config is
    /// `nearest`. Without one the file takes every default and has no
    /// project module roots.
    pub fn new(nearest: Option<(PathBuf, XshConfig)>) -> Self {
        match nearest {
            Some((config_dir, config)) => Self {
                module_roots: xsh::frontend::load::module_roots(&config_dir, &config.module_path),
                config_dir,
                config,
            },
            None => Self {
                config_dir: PathBuf::from("."),
                config: XshConfig::default(),
                module_roots: Vec::new(),
            },
        }
    }

    pub fn line_width(&self) -> usize {
        self.config.format.line_width
    }

    pub fn module_roots(&self) -> Vec<PathBuf> {
        self.module_roots.clone()
    }
}

pub(crate) fn config_for_file(file: &str) -> Result<FileToolConfig, String> {
    Ok(FileToolConfig::new(nearest_config_for_file(Path::new(
        file,
    ))?))
}
