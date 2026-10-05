use crate::xsht::cli::{XshConfig, nearest_config_for_file};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct FileToolConfig {
    pub config_dir: PathBuf,
    pub config: XshConfig,
    // The project module roots of the file. They come only from a config
    // above the file, never from the fallback: `xsh` finds roots the same
    // way, so a file checks with exactly the modules it would load.
    module_roots: Vec<PathBuf>,
}

impl FileToolConfig {
    /// The tool configuration of a file whose nearest project config is
    /// `nearest`. Without one, tool settings come from `fallback_config` and
    /// the file has no project module roots.
    pub fn new(nearest: Option<(PathBuf, XshConfig)>, fallback_config: &XshConfig) -> Self {
        match nearest {
            Some((config_dir, config)) => Self {
                module_roots: xsh::frontend::load::module_roots(&config_dir, &config.module_path),
                config_dir,
                config,
            },
            None => Self {
                config_dir: PathBuf::from("."),
                config: fallback_config.clone(),
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

pub(crate) fn config_for_file(
    file: &str,
    fallback_config: &XshConfig,
) -> Result<FileToolConfig, String> {
    Ok(FileToolConfig::new(
        nearest_config_for_file(Path::new(file))?,
        fallback_config,
    ))
}

pub(crate) fn config_for_dir(
    dir: &Path,
    fallback_config: &XshConfig,
) -> Result<FileToolConfig, String> {
    let probe = dir.join(".xsht-config-probe.xsh");
    Ok(FileToolConfig::new(
        nearest_config_for_file(&probe)?,
        fallback_config,
    ))
}
