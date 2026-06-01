use fs_err as fs;
use std::path::PathBuf;

use exn::{Exn, ResultExt};

use crate::StrixConfig;
use crate::sparv_decoder::CorpusConfig;

#[derive(Debug, Clone)]
pub struct InsertData<'a> {
    index: &'a str,
    corpus_conf: &'a CorpusConfig,
    id_generator: u64,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub task_type: &'static str,
    pub task_id: String,
    pub task_size: u64,
    pub task_data: TaskData,
}
#[derive(Debug, Clone)]
pub struct TaskData {
    pub text: PathBuf,
}

impl<'a> InsertData<'a> {
    pub fn new(index: &'a str, corpus_conf: &'a CorpusConfig) -> Self {
        Self {
            index,
            corpus_conf,
            id_generator: 1,
        }
    }

    pub fn prepare_urls(
        &mut self,
        config: &StrixConfig,
    ) -> Result<(Vec<Task>, u64), Exn<PrepareUrlsError>> {
        let mut urls = Vec::new();
        let mut tot_size = 0;

        let paths = get_paths_for_corpus(self.corpus_conf, config).or_raise(|| PrepareUrlsError)?;

        dbg!(&paths);

        for text in paths {
            if text.is_file() {
                let text_id = text
                    .file_stem()
                    .map(|s| s.to_str())
                    .flatten()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| {
                        let id = format!("text-{}", self.id_generator);
                        self.id_generator += 1;
                        id
                    });

                let metadata = text.metadata().or_raise(|| PrepareUrlsError)?;
                let size = metadata.len();
                tot_size += size;
                log::info!("Adding file: {}", text.display());
                urls.push(Task {
                    task_type: "text",
                    task_id: text_id,
                    task_size: size,
                    task_data: TaskData { text },
                });
            }
        }
        Ok((urls, tot_size))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to get paths")]
pub struct PrepareUrlsError;

fn get_paths_for_corpus(
    conf: &CorpusConfig,
    config: &StrixConfig,
) -> Result<Vec<PathBuf>, Exn<GetPathsError>> {
    let corpus_dir_name = if let Some(corpus_dir) = conf.corpus_dir() {
        corpus_dir
    } else {
        conf.corpus_id()
    };
    dbg!(&corpus_dir_name);
    let texts_dir = if config.texts_dir().starts_with("/") {
        config.texts_dir().join(corpus_dir_name)
    } else {
        config
            .base_dir()
            .join(config.texts_dir())
            .join(corpus_dir_name)
    };
    dbg!(&texts_dir);
    let glob_pattern = format!("{}/**/*.xml", texts_dir.display());
    let mut xml_paths = Vec::new();
    for entry in glob::glob(&glob_pattern).expect("Failed to read glob pattern") {
        let entry = entry.or_raise(|| GetPathsError)?;
        xml_paths.push(entry);
    }
    Ok(xml_paths)
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to get paths")]
pub struct GetPathsError;
