use super::Error;
use crate::docgen::crate_syn_browser::Target;
use std::{fmt::Debug, fs::File as FsFile, io::Read, path::PathBuf};
use syn::File as SynFile;
use Error::*;

#[derive(Debug)]
pub struct File<'b> {
    pub target: &'b Target<'b>,
    pub file: SynFile,
    pub path: String,
    pub(crate) dir: PathBuf,
}

impl<'b> File<'b> {
    pub fn new(target: &'b Target<'b>, dir: &PathBuf, path: String) -> Result<File<'b>, Error> {
        let mut f = FsFile::open(dir).map_err(|e| FileIoError(Box::new(dir.clone()), Box::new(e)))?;
        let mut buffer = String::new();
        f.read_to_string(&mut buffer).map_err(|e| FileIoError(Box::new(dir.clone()), Box::new(e)))?;

        let file = syn::parse_file(buffer.as_str()).map_err(|e| FileParseError(Box::new(dir.clone()), Box::new(e)))?;

        let file = Self {
            target,
            file,
            path,
            dir: dir.parent().expect("Valid file path should have valid parent folder").to_path_buf(),
        };

        Ok(file)
    }
}
