use std::io;
use std::path::Path;

use failure::Error;
use serde_derive::Deserialize;
use serde_derive::Serialize;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;

#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum VersioningPolicy {
    Off,
    On,
    FileNotFound,
}

#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum LifecyclePolicy {
    Keep,
    CollectOlder,
}

#[derive(Serialize, Deserialize)]
pub struct BucketConfig {
    versioning: VersioningPolicy,
    lifecycle: LifecyclePolicy,
}

pub struct Name(String);

impl Name {
    pub fn from(val: impl ToString) -> Option<Name> {
        let name = val.to_string();
        if valid_bucket_name(&name) {
            Some(Name(name))
        } else {
            None
        }
    }
}

pub async fn get_config(storage: &Path, bucket: &Name) -> Result<Option<BucketConfig>, Error> {
    let mut dir = storage.to_path_buf();
    dir.push(&bucket.0);
    dir.push("config.json");
    match fs::read(&dir).await {
        Ok(c) => Ok(Some(serde_json::from_slice(&c)?)),
        Err(ref e) if io::ErrorKind::NotFound == e.kind() => Ok(None),
        Err(e) => return Err(e.into()),
    }
}

pub async fn put_config(storage: &Path, bucket: &Name, config: &BucketConfig) -> Result<(), Error> {
    let mut dir = storage.to_path_buf();
    dir.push(&bucket.0);
    fs::create_dir_all(&dir).await?;
    dir.push("config.json");
    let mut temp = super::temp::NamedTempFile::new_in(dir.parent().expect("just pushed")).await?;
    let content = serde_json::to_vec(config)?;
    temp.write_all(&content).await?;
    temp.into_temp_path()
        .persist(dir)
        .await
        .map_err(|e| e.error)?;
    Ok(())
}

// 3 to 63 lower case ascii letters or digits, dots and hyphens, with no double dots
fn valid_bucket_name(name: &str) -> bool {
    if name.len() < 3 || name.len() > 63 {
        return false;
    }

    if !name.starts_with(|c| alnum(c)) || !name.ends_with(|c| alnum(c)) {
        return false;
    }

    if !name.chars().all(|c| alnum(c) || ['.', '-'].contains(&c)) {
        return false;
    }

    if name.contains("..") {
        return false;
    }

    true
}

fn alnum(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit()
}

#[test]
fn naming() {
    assert!(valid_bucket_name("hello"));
    assert!(!valid_bucket_name("he"));
    assert!(valid_bucket_name("789"));
    assert!(!valid_bucket_name(".lol"));
    assert!(!valid_bucket_name("lol."));
    assert!(!valid_bucket_name("lol..ponies"));
    assert!(valid_bucket_name("lol.ponies"));
    assert!(valid_bucket_name("xn--wow-ee"));
    assert!(valid_bucket_name("xm--wow-ee"));
}
