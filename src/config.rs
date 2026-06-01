use fs_err as fs;
use hashbrown::HashMap;
use std::path::{Path, PathBuf};

use exn::{Exn, ResultExt};

use crate::shared;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct StrixConfig {
    core: ConfigCore,
    corpusconf: CorpusConf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ConfigCore {
    #[serde(default)]
    base_dir: PathBuf,
    settings_dir: PathBuf,
    texts_dir: PathBuf,
    transformers_postprocess_dir: Option<PathBuf>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CorpusConf {
    settings_dir: PathBuf,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct ModeConfig {
    name: String,
    pub(crate) translation_name: HashMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Failed to load config")]
    Load,
}
impl StrixConfig {
    pub fn from_path(path: &Path) -> Result<Self, Exn<ConfigError>> {
        let src = fs::read_to_string(path).or_raise(|| ConfigError::Load)?;
        let core: ConfigCore = serde_saphyr::from_str(&src).or_raise(|| ConfigError::Load)?;
        let corpusconf =
            CorpusConf::from_path(&core.settings_dir).or_raise(|| ConfigError::Load)?;
        Ok(Self { core, corpusconf })
    }

    pub fn base_dir(&self) -> &Path {
        &self.core.base_dir
    }
    pub(crate) fn settings_dir(&self) -> &Path {
        &self.core.settings_dir
    }

    pub fn texts_dir(&self) -> &Path {
        &self.core.texts_dir
    }
    pub fn transformers_postprocess_dir(&self) -> Option<&Path> {
        self.core.transformers_postprocess_dir.as_deref()
    }

    pub fn corpusconf(&self) -> &CorpusConf {
        &self.corpusconf
    }
}

impl CorpusConf {
    pub fn from_path(path: &Path) -> Result<Self, Exn<ConfigError>> {
        Ok(Self {
            settings_dir: path.to_path_buf(),
        })
    }

    pub fn get_mode(&self, mode_name: &str) -> Result<ModeConfig, Exn<ConfigError>> {
        let mode_path = self.settings_dir.join(format!("modes/{mode_name}.yaml"));
        let mode_map: HashMap<String, ModeConfig> =
            shared::load_yaml_from_path(&mode_path).or_raise(|| ConfigError::Load)?;
        Ok(mode_map.into_values().next().unwrap())
    }
}
