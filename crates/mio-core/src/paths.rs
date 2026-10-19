//! Host-owned application locations. Mobile never derives these from HOME.
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    pub data: PathBuf,
    pub cache: PathBuf,
    pub downloads: PathBuf,
}

impl Paths {
    pub fn validate(&self) -> Result<(), String> {
        if [&self.data, &self.cache, &self.downloads]
            .into_iter()
            .any(|path| !path.is_absolute())
        {
            return Err("Application paths must be absolute".into());
        }
        Ok(())
    }
}
