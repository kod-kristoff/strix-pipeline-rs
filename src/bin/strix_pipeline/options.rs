use std::path::PathBuf;

#[derive(Debug, clap::Parser)]
pub struct Args {
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Add a corpus to strix
    Add(Add),
    /// Delete corpus from instance. This will remove both Elasticsearch indices and the configuration files from <settings_dir>/corpora/
    Delete(Delete),
    /// Either runs vector data generation locally or offloads vector creation to config.transformers_postprocess_server"
    GenerateVectorData(GenerateVectorData),
}

#[derive(Debug, clap::Parser)]
pub struct Add {
    /// Corpus to add
    pub corpus: String,
    #[clap(flatten)]
    pub common: Common,
    #[arg(long)]
    /// Set if you want a previous version of corpus to be deleted (if it exists).
    /// Alias for corpus is always deleted.
    pub delete_previous_versions: bool,
    #[arg(long, default_value = "none")]
    /// Document vectors can be generated on config.vector_server, locally or not at all.
    pub vector_generation_type: Option<VectorGeneration>,
}

#[derive(Debug, clap::Parser)]
pub struct Delete {
    /// Corpus to delete
    pub corpus: String,
    #[clap(flatten)]
    pub common: Common,
}

#[derive(Debug, clap::Parser)]
pub struct GenerateVectorData {
    /// Corpus to update
    pub corpus: String,
    #[clap(flatten)]
    pub common: Common,
    #[arg(long)]
    #[arg(long, default_value = "local")]
    /// Document vectors can be generated on config.vector_server or locally.
    pub vector_generation_type: VectorGeneration,
}

#[derive(Debug, clap::Parser)]
pub struct Common {
    #[arg(short, long, default_value = "./config.yaml")]
    /// Path to config file
    pub config: PathBuf,
}
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum VectorGeneration {
    Remote,
    Local,
}
