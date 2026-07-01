use crate::xsht::cli::{XshConfig, nearest_config_for_file, resolve_config_path};
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
        self.config
            .module_path
            .iter()
            .cloned()
            .map(|root| resolve_config_path(&self.config_dir, root))
            .collect()
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
