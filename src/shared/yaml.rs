use fs_err as fs;
use std::{
    io,
    path::{Path, PathBuf},
};

use exn::{Exn, ResultExt};

#[derive(Debug, thiserror::Error)]
#[error("Failed to load yaml from '{path}'")]
pub struct LoadYamlError {
    path: PathBuf,
}

pub fn load_yaml_from_path<T>(path: &Path) -> Result<T, Exn<LoadYamlError>>
where
    T: serde::de::DeserializeOwned,
{
    let make_error = || LoadYamlError {
        path: path.to_path_buf(),
    };
    let file = fs::File::open(path).or_raise(make_error)?;
    let reader = io::BufReader::new(file);
    let t = serde_saphyr::from_reader(reader).or_raise(make_error)?;
    Ok(t)
}

#[derive(Debug, thiserror::Error)]
#[error("Failed to dump yaml to '{path}'")]
pub struct DumpYamlError {
    path: PathBuf,
}

pub fn dump_yaml_from_path<T>(t: &T, path: &Path) -> Result<(), Exn<DumpYamlError>>
where
    T: serde::Serialize,
{
    let make_error = || DumpYamlError {
        path: path.to_path_buf(),
    };
    let file = fs::File::create(path).or_raise(make_error)?;
    let mut writer = io::BufWriter::new(file);
    serde_saphyr::to_io_writer(&mut writer, t).or_raise(make_error)?;
    Ok(())
}
