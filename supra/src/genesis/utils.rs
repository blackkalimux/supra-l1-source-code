use crate::common::error::CliError;
use serde::Deserialize;
use std::{fs::File, io::Read};

pub fn from_json_file_path<T>(path: &String) -> Result<T, CliError>
where
    T: for<'a> Deserialize<'a>,
{
    let mut f = open(path)?;
    let mut buff = String::new();
    f.read_to_string(&mut buff)?;
    let object: T = serde_json::from_str(&buff)?;
    Ok(object)
}

pub fn open(path: &String) -> Result<File, CliError> {
    File::open(path)
        .map_err(|_| CliError::GeneralError(format!("No such file or directory: {path}")))
}
