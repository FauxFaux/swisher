use std::path::Path;

use failure::bail;
use failure::Error;
use hyper::Body;
use hyper::Request;

use super::bucket;
use super::dir;
use super::dir::Intermediate;
use super::hyp;
use super::sig;
use crate::sig::Validation;

pub struct SimpleResponse {
    pub status: u16,
    pub body: Body,
}

#[derive(Copy, Clone, Debug)]
pub enum SimpleMethod {
    Get,
    Put,
    Post,
    Delete,
}

fn bucket_name(path: &str) -> (&str, &str) {
    assert!(path.starts_with('/'));
    let path = &path[1..];
    match path.find('/') {
        Some(slash) => (&path[..slash], &path[slash..]),
        None => (path, ""),
    }
}

pub async fn handle(req: Request<Body>) -> Result<SimpleResponse, Error> {
    let not_found = SimpleResponse {
        status: 404,
        body: Body::empty(),
    };

    let not_reasonable = SimpleResponse {
        status: 400,
        body: Body::empty(),
    };

    let method = match hyp::method(req.method()) {
        Some(method) => method,
        _ => {
            return Ok(SimpleResponse {
                status: 405,
                body: Body::empty(),
            })
        }
    };

    let headers = hyp::headers(&req)?;
    let (user, headers) = match sig::validate(headers) {
        Validation::Invalid | Validation::Unsupported => {
            return Ok(SimpleResponse {
                status: 403,
                body: Body::empty(),
            })
        }
        Validation::Anonymous(headers) => (None, headers),
        Validation::Valid(user, headers) => (Some(user), headers),
    };

    log::info!("{:?}, {:?}, {:?}", method, hyp::path(&req), headers);

    let (bucket, path) = bucket_name(hyp::path(&req));

    let bucket = match bucket::Name::from(bucket) {
        Some(bucket) => bucket,
        None => return Ok(not_reasonable),
    };

    let config = bucket::get_config(Path::new("."), &bucket).await?;

    match method {
        SimpleMethod::Get => {
            let (_meta, file) = match dir::get(Path::new("."), &path).await? {
                Some(parts) => parts,
                None => return Ok(not_found),
            };
            let (sender, body) = Body::channel();
            tokio::spawn(super::hyper_files::stream_unpack(file, sender));
            Ok(SimpleResponse { status: 200, body })
        }
        SimpleMethod::Put => {
            let mut temp = super::temp::NamedTempFile::new_in(".").await?;
            // BORROW CHECKER
            let path = path.to_string();
            let content = super::hyper_files::stream_pack(req.into_body(), &mut temp).await?;
            let temp = temp.into_temp_path();

            dir::store(
                Path::new("."),
                &tokio::sync::Mutex::new(()),
                &path,
                headers,
                Intermediate { temp, content },
            )
            .await?;

            Ok(SimpleResponse {
                status: 202,
                body: Body::empty(),
            })
        }
        other => bail!("not implemented: {:?}", other),
    }
}

#[test]
fn name() {
    assert_eq!(("", ""), bucket_name("/"));
    assert_eq!(("potato", ""), bucket_name("/potato"));
    assert_eq!(("potato", "/"), bucket_name("/potato/"));
    assert_eq!(("potato", "/an/d"), bucket_name("/potato/an/d"));
}
