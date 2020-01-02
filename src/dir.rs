use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::RwLock;

use chrono::DateTime;
use chrono::Utc;
use failure::err_msg;
use failure::Error;
use md5::digest::FixedOutput;
use md5::digest::Input;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;

use crate::temp::TempPath;

pub async fn load_meta(root: &Path, key: &PackedKey) -> Result<Option<FileMeta>, Error> {
    let mut root = key.as_path(root);
    assert!(root.set_extension("meta"));
    match fs::read(&root).await {
        Ok(data) => Ok(Some(serde_json::from_slice(&data)?)),
        Err(ref e) if io::ErrorKind::NotFound == e.kind() => Ok(None),
        Err(e) => Err(e)?,
    }
}

pub async fn open_version(root: &Path, key: &PackedKey, version: u64) -> Result<fs::File, Error> {
    let mut root = key.as_path(root);
    assert!(root.set_extension(format!("{}", version)));
    Ok(fs::File::open(root).await?)
}

pub async fn write_new_version(
    key: impl ToString,
    mut root: PathBuf,
    meta: HashMap<String, String>,
    intermediate: Intermediate,
    temp: TempPath,
) -> Result<(), Error> {
    let mut data = match fs::read(&root).await {
        Ok(data) => serde_json::from_slice(&data)?,
        Err(ref e) if io::ErrorKind::NotFound == e.kind() => FileMeta {
            key: key.to_string(),
            versions: Vec::with_capacity(1),
        },
        Err(e) => Err(e)?,
    };

    let new_version = data.versions.len();

    data.versions.push(FileVersion {
        modified: Utc::now(),
        content_length: intermediate.content_length,
        content_md5_base64: intermediate.content_md5_base64,
        meta,
        tombstone: false,
    });

    let data = serde_json::to_vec(&data)?;

    let mut meta_temp =
        super::temp::NamedTempFile::new_in(root.parent().expect("structured dir")).await?;
    meta_temp.write_all(&data).await?;
    let meta_temp = meta_temp.into_temp_path();

    // ensure the data exists before we write the metadata
    // this will clobber existing versions if they wrote before a crash before?
    assert!(root.set_extension(format!("{}", new_version)));
    temp.persist(&root).await.map_err(|e| e.error)?;

    assert!(root.set_extension("meta"));
    meta_temp.persist(root).await.map_err(|e| e.error)?;

    Ok(())
}

pub async fn put(
    root: &Path,
    meta_lock: &RwLock<()>,
    key: &str,
    meta: HashMap<String, String>,
    temp: TempPath,
    intermediate: Intermediate,
) -> Result<(), Error> {
    let root = PackedKey::from(key).as_path(root);

    fs::create_dir_all(root.parent().expect("structured path")).await?;

    {
        let _writing = meta_lock.write().expect("poisoned!");
        write_new_version(key, root, meta, intermediate, temp).await?;
    }

    Ok(())
}

pub struct Intermediate {
    pub content_length: u64,
    pub content_md5_base64: String,
}

#[derive(Clone)]
pub struct PackedKey(String);

impl From<&str> for PackedKey {
    fn from(name: &str) -> Self {
        let mut digest = sha2::Sha512::default();
        digest.input(name);

        // "dnssec" here just happens to be lower case, case insensitive,
        // extended hex, which seems ideal
        let data = data_encoding::BASE32_DNSSEC.encode(&digest.fixed_result());
        assert_eq!(103, data.len());
        Self(data)
    }
}

impl PackedKey {
    fn as_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        let mut buf = root.as_ref().to_path_buf();
        buf.push(&self.0[..4]);
        buf.push(&self.0[4..8]);
        buf.push(&self.0[8..]);
        buf
    }
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
pub struct FileMeta {
    key: String,
    versions: Vec<FileVersion>,
}

impl FileMeta {
    pub fn deleted(&self) -> Result<bool, Error> {
        Ok(self.latest_version()?.tombstone)
    }

    pub fn latest_version_id(&self) -> Result<usize, Error> {
        Ok(self
            .versions
            .len()
            .checked_sub(1)
            .ok_or_else(|| err_msg("versions array cannot be empty"))?)
    }

    pub fn latest_version(&self) -> Result<&FileVersion, Error> {
        Ok(&self.versions[self.latest_version_id()?])
    }
}

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
pub struct FileVersion {
    modified: DateTime<Utc>,
    content_length: u64,
    content_md5_base64: String,
    meta: HashMap<String, String>,
    tombstone: bool,
}
