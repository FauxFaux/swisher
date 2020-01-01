#![allow(unused)]

use std::io;
use std::mem;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use failure::err_msg;
use failure::format_err;
use failure::Error;
use failure::ResultExt;
use log::warn;
use pin_project::pin_project;
use tokio::fs;
use tokio::prelude::AsyncWrite;

#[derive(Debug)]
pub struct TempPath {
    path: PathBuf,
}

#[pin_project]
pub struct NamedTempFile {
    path: TempPath,
    #[pin]
    file: fs::File,
}

#[derive(Debug)]
pub struct PathPersistError {
    pub error: io::Error,
    pub path: TempPath,
}

impl NamedTempFile {
    pub async fn new_in<P: AsRef<Path>>(dir: P) -> Result<NamedTempFile, Error> {
        let mut path = dir.as_ref().to_path_buf();
        let cand: u64 = rand::random();
        for _ in 0..256 {
            //            let cand: String = std::iter::repeat_with(|| char::from(rng.gen_range(b'a', b'z')))
            //                .take(10)
            //                .collect();
            let cand = format!(".{:x}.tmp", cand);
            path.push(cand);
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await
            {
                Ok(file) => {
                    return Ok(NamedTempFile {
                        file,
                        path: TempPath::new(path),
                    })
                }
                Err(ref e) if io::ErrorKind::AlreadyExists == e.kind() => (),
                Err(e) => return Err(e.into()),
            }
        }
        assert!(path.pop(), "popping the temporary name off the dir");
        Err(err_msg("gave up trying to create a temporary file"))
    }

    pub fn into_temp_path(self) -> TempPath {
        self.path
    }
}

impl TempPath {
    fn new(path: PathBuf) -> Self {
        TempPath { path }
    }

    pub async fn close(mut self) -> Result<(), Error> {
        let result = fs::remove_file(&self.path)
            .await
            .with_context(|_| format_err!("removing {:?}", self.path));
        mem::replace(&mut self.path, PathBuf::new());
        mem::forget(self);
        Ok(result?)
    }

    pub async fn persist<P: AsRef<Path>>(mut self, new_path: P) -> Result<(), PathPersistError> {
        match fs::rename(&self.path, new_path.as_ref()).await {
            Ok(()) => {
                mem::replace(&mut self.path, PathBuf::new());
                mem::forget(self);
                Ok(())
            }
            Err(error) => Err(PathPersistError { error, path: self }),
        }
    }
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_file(&self.path) {
            warn!("unable to remove temporary file {:?}: {:?}", self.path, e);
        }
    }
}

impl AsyncWrite for NamedTempFile {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().file.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().file.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().file.poll_shutdown(cx)
    }
}
