use crate::xsht::cli::{XshConfig, nearest_config_for_file};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct FileToolConfig {
    pub config_dir: PathBuf,
    pub config: XshConfig,
}

impl FileToolConfig {
    pub fn line_width(&self) -> usize {
        self.config.format.line_width
    }

    pub fn module_roots(&self) -> Vec<PathBuf> {
        xsh::frontend::load::module_roots(&self.config_dir, &self.config.module_path)
    }
}

pub(crate) fn config_for_file(
    file: &str,
    fallback_config: &XshConfig,
) -> Result<FileToolConfig, String> {
    let (config_dir, config) = nearest_config_for_file(Path::new(file))?
        .unwrap_or_else(|| (PathBuf::from("."), fallback_config.clone()));
    Ok(FileToolConfig { config_dir, config })
}

pub(crate) fn config_for_dir(
    dir: &Path,
    fallback_config: &XshConfig,
) -> Result<FileToolConfig, String> {
    let probe = dir.join(".xsht-config-probe.xsh");
    let (config_dir, config) = nearest_config_for_file(&probe)?
        .unwrap_or_else(|| (PathBuf::from("."), fallback_config.clone()));
    Ok(FileToolConfig { config_dir, config })
}
