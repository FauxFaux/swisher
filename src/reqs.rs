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

pub fn bucket_name(host_header: Option<&String>, path: &str) -> Option<(String, String)> {
    host_header
        .and_then(|host| first_dns_part(host))
        .map(|bucket| (bucket.to_string(), path.to_string()))
        .or_else(|| first_path_part(path))
}

fn first_dns_part(host: &str) -> Option<&str> {
    let mut parts = host.split('.');
    let bucket = parts.next();
    parts.next().and(bucket)
}

fn first_path_part(path: &str) -> Option<(String, String)> {
    path[1..]
        .find('/')
        .map(|end| (path[1..end + 1].to_string(), path[end + 1..].to_string()))
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

    let (bucket, path) = match bucket_name(headers.get("Host"), hyp::path(&req)) {
        Some(b_p) => b_p,
        None => return Ok(not_reasonable),
    };

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
    assert_eq!(None, bucket_name(None, "/"));
    assert_eq!(None, bucket_name(None, "/potato"));
    assert_eq!(
        Some(("potato".into(), "/".into())),
        bucket_name(None, "/potato/")
    );
    assert_eq!(
        Some(("potato".into(), "/an/d".into())),
        bucket_name(None, "/potato/an/d")
    );

    assert_eq!(None, bucket_name(Some(&"foo".into()), "/"));
    assert_eq!(
        Some(("plants".into(), "/greens".into())),
        bucket_name(Some(&"foo".into()), "/plants/greens")
    );
    assert_eq!(
        Some(("potato".into(), "/".into())),
        bucket_name(Some(&"potato.foo".into()), "/")
    );
    assert_eq!(
        Some(("potato".into(), "/cheese/and/beans".into())),
        bucket_name(Some(&"potato.foo".into()), "/cheese/and/beans")
    );
}
