use clap::Parser;
use exn::{Exn, ResultExt};
use strix_pipeline::{StrixConfig, pipeline, sparv_decoder};

use crate::options::Args;

#[path = "strix_pipeline/options.rs"]
mod options;

fn main() -> Result<(), Exn<AppError>> {
    let make_error = || AppError::Failure;
    println!("Hello, world!");
    let args = Args::parse();
    dbg!(&args);
    let config = StrixConfig::from_path(&args.config).or_raise(make_error)?;
    dbg!(&config);
    match args.cmd {
        options::Command::Add(cmd) => {
            let vector_generation_type = cmd.vector_generation_type;

            let corpus_config = sparv_decoder::convert_to_strix_config(&cmd.corpus, &config)
                .or_raise(make_error)?;
            if let Some(vector_generation_type) = vector_generation_type {
                eprintln!("generate vectors");
                pipeline::generate_vectors(
                    &cmd.corpus,
                    vector_generation_type.as_domain(),
                    &config,
                    &corpus_config,
                )
                .or_raise(make_error)?;
            } else if !pipeline::check_vectors_exist(&cmd.corpus, &config).or_raise(make_error)? {
                exn::bail!(AppError::WithMsg(
                    "You must generate vectors first or use --vector-generation-type local/remote"
                        .to_string()
                ))
            }
        }
        options::Command::Delete(cmd) => {}
        options::Command::GenerateVectorData(cmd) => {}
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("App failed to run")]
    Failure,
    #[error("App failed to run: {0}")]
    WithMsg(String),
}
