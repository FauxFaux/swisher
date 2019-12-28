use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::RwLock;

use chrono::DateTime;
use chrono::Utc;
use failure::Error;
use md5::digest::FixedOutput;
use md5::digest::Input;
use tempfile_fast::PersistableTempFile;
use tempfile_fast::Sponge;

pub struct DirStore<'p, 'l> {
    root: &'p Path,
    meta_lock: &'l RwLock<()>,
}

impl<'p, 'l> DirStore<'p, 'l> {
    pub fn new(root: &'p Path, meta_lock: &'l RwLock<()>) -> Self {
        DirStore { root, meta_lock }
    }

    pub fn head(&self, key: &str) -> Result<Option<FileMeta>, Error> {
        let mut root = PackedKey::from(key).as_path(&self.root);
        assert!(root.set_extension("meta"));
        match fs::read(&root) {
            Ok(data) => Ok(Some(serde_json::from_slice(&data)?)),
            Err(ref e) if io::ErrorKind::NotFound == e.kind() => Ok(None),
            Err(e) => Err(e)?,
        }
    }

    pub fn get(&self, key: &str) -> Result<Option<DirReader>, Error> {
        let data = match self.head(key)? {
            Some(data) => data,
            None => return Ok(None),
        };

        let latest = data.versions.len() - 1;
        let latest_version = data
            .versions
            .into_iter()
            .last()
            .expect("non-empty versions");

        if latest_version.tombstone {
            return Ok(None);
        }

        let mut root = PackedKey::from(key).as_path(&self.root);
        assert!(root.set_extension(format!("{}", latest)));

        let inner = zstd::stream::Decoder::new(fs::File::open(&root)?)?;

        Ok(Some(DirReader {
            inner,
            meta: latest_version,
        }))
    }

    pub fn put<R: Read>(
        &self,
        key: &str,
        meta: HashMap<String, String>,
        content: R,
    ) -> Result<(), Error> {
        let temp = to_temp_file(content, &self.root)?;
        let root = PackedKey::from(key);
        let mut root = root.as_path(&self.root);

        fs::create_dir_all(root.parent().expect("structured path"))?;

        assert!(root.set_extension("meta"));
        let mut sponge = Sponge::new_for(&root)?;

        {
            let _writing = self.meta_lock.write().expect("poisoned!");
            let mut data = match fs::read(&root) {
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
                content_length: temp.content_length,
                content_md5_base64: temp.content_md5_base64,
                meta,
                tombstone: false,
            });
            serde_json::to_writer(&mut sponge, &data)?;
            assert!(root.set_extension(format!("{}", new_version)));

            // ensure the data exists before we write the metadata
            // an inconvenient crash could make this file non-writable,
            // as we may try and write to a storage location that is already in use
            temp.temp.persist_noclobber(root).map_err(|e| e.error)?;
            sponge.commit()?;
        }

        Ok(())
    }
}

struct Intermediate {
    content_length: u64,
    content_md5_base64: String,
    temp: PersistableTempFile,
}

fn to_temp_file<R: Read, P: AsRef<Path>>(mut from: R, near: P) -> Result<Intermediate, Error> {
    let mut content_length = 0u64;
    let mut md5 = md5::Md5::default();
    let temp = PersistableTempFile::new_in(near)?;
    let mut temp = zstd::stream::Encoder::new(temp, 5)?;
    temp.include_checksum(true)?;

    loop {
        let mut buf = [0u8; 8 * 1024];
        let found = from.read(&mut buf)?;
        let buf = &buf[..found];
        if buf.is_empty() {
            break;
        }
        md5.input(buf);
        temp.write_all(buf)?;
        content_length += u64::try_from(buf.len()).expect("read result fits in u64");
    }
    let temp = temp.finish()?;
    let content_md5_base64 = base64::encode(&md5.fixed_result());

    Ok(Intermediate {
        content_md5_base64,
        content_length,
        temp,
    })
}

#[derive(Clone)]
struct PackedKey(String);

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

#[derive(serde_derive::Serialize, serde_derive::Deserialize)]
pub struct FileVersion {
    modified: DateTime<Utc>,
    content_length: u64,
    content_md5_base64: String,
    meta: HashMap<String, String>,
    tombstone: bool,
}

pub struct DirReader {
    inner: zstd::stream::Decoder<io::BufReader<fs::File>>,
    pub meta: FileVersion,
}

impl Read for DirReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}
