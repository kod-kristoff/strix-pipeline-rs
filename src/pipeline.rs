use std::borrow::Cow;
use std::io::{BufWriter, Write};
use std::{io, path::Path};

use exn::{Exn, ResultExt};
use fs_err as fs;
use serde_json::json;

use crate::domain::strix::models::vector::VectorGenerationType;
use crate::insertdata::{self as insert_data_strix, Task};
use crate::sparv_decoder::CorpusConfig;
use crate::xmlparser::ParsePipelineXmlOptions;
use crate::{StrixConfig, xmlparser};

/// py: do_vector_generation
pub fn generate_vectors(
    corpus: &str,
    vector_generation_type: VectorGenerationType,
    config: &StrixConfig,
    corpus_conf: &CorpusConfig,
) -> Result<(), Exn<GenerateVectorError>> {
    let make_error = || GenerateVectorError::Failure;

    check_vector_settings(corpus, config).or_raise(make_error)?;

    let mut insert_data = insert_data_strix::InsertData::new(corpus, corpus_conf);
    let (task_data, tot_size) = insert_data.prepare_urls(config).or_raise(make_error)?;

    dbg!(&task_data);
    let split_document = corpus_conf.split();
    let text_tags = corpus_conf.text_tags();

    let text_attributes = json!({"_id": {}});

    for Task {
        task_type,
        task_id,
        task_size: size,
        task_data: task,
    } in task_data
    {
        let mut transformer_input = Vec::new();
        eprintln!("parsing xml");
        for text in xmlparser::parse_pipeline_xml(
            &task.text,
            split_document,
            &serde_json::Map::new(),
            ParsePipelineXmlOptions {
                text_attributes: Cow::Borrowed(text_attributes.as_object().unwrap()),
                text_tags: Cow::Borrowed(text_tags),
                ..Default::default()
            },
        )
        .or_raise(make_error)?
        {
            let doc_id = text.text_attributes["_id"].as_str().unwrap().to_string();
            transformer_input.push(vec![doc_id, text.dump.join(" ").replace('\n', " ")]);
        }
        let text_task_path = config
            .transformers_postprocess_dir()
            .unwrap()
            .join(format!("{corpus}/texts/{task_id}.jsonl"));
        eprintln!("writing to '{}'", text_task_path.display());
        let fp = fs::File::create(&text_task_path).or_raise(make_error)?;
        let mut fp = BufWriter::new(fp);
        for text in transformer_input {
            let json = serde_json::to_string(&text).or_raise(make_error)?;
            fp.write_all(json.as_bytes()).or_raise(make_error)?;
            fp.write(&[b'\n'][..]).or_raise(make_error)?;
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateVectorError {
    #[error("Failed to generate vectors")]
    Failure,
}

/// py: check_vectors_exist
pub fn check_vectors_exist(corpus: &str, config: &StrixConfig) -> Result<bool, Exn<VectorError>> {
    check_vector_settings(corpus, config)?;
    if let Some(transformers_postprocess_dir) = config.transformers_postprocess_dir() {
        let path = transformers_postprocess_dir.join(corpus).join("vectors");
        Ok(path.is_dir() && any_file_in_dir(&path).or_raise(|| VectorError::Failure)?)
    } else {
        exn::bail!(VectorError::WithMsg(
            "`transformers_postprocess_dir` is not set in config".to_string()
        ));
    }
}

fn any_file_in_dir(path: &Path) -> Result<bool, io::Error> {
    for f in fs::read_dir(path)? {
        let f = f?;
        let file_type = f.file_type()?;
        if file_type.is_file() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// py: check_vector_settings
fn check_vector_settings(corpus: &str, config: &StrixConfig) -> Result<(), Exn<VectorError>> {
    if let Some(transformers_postprocess_dir) = config.transformers_postprocess_dir() {
        let postprocess_texts_path = transformers_postprocess_dir.join(corpus).join("texts");
        fs::create_dir_all(&postprocess_texts_path).or_raise(|| VectorError::Failure)?;
        Ok(())
    } else {
        exn::bail!(VectorError::WithMsg(
            "`transformers_postprocess_dir` is not set in config".to_string()
        ));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VectorError {
    #[error("Vector handling failed")]
    Failure,
    #[error("Vector handling failed: {0}")]
    WithMsg(String),
}
