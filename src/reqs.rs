use std::collections::HashMap;
use std::io;
use std::io::Write;

use failure::Error;
use futures::io::AsyncWriteExt as _;
use http_body::Body as _;
use hyper::body::Buf;
use hyper::Body;
use hyper::Request;
use log::debug;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;

use super::hyp;

pub struct SimpleResponse {
    pub status: u16,
    pub body: Body,
}

pub enum SimpleMethod {
    Get,
    Put,
    Post,
    Delete,
}

pub fn bucket_name<'p>(host_header: Option<&String>, path: &'p str) -> Option<(String, &'p str)> {
    host_header
        .and_then(|host| first_dns_part(host))
        .map(|bucket| (bucket.to_string(), path))
        .or_else(|| first_path_part(path))
}

fn first_dns_part(host: &str) -> Option<&str> {
    let mut parts = host.split('.');
    let bucket = parts.next();
    parts.next().and(bucket)
}

fn first_path_part(path: &str) -> Option<(String, &str)> {
    path[1..]
        .find('/')
        .map(|end| (path[1..end + 1].to_string(), &path[end + 1..]))
}

pub async fn handle(req: Request<Body>) -> Result<SimpleResponse, Error> {
    match hyp::method(req.method()) {
        Some(SimpleMethod::Put) => (),
        other => {
            return Ok(SimpleResponse {
                status: 405,
                body: Body::empty(),
            })
        }
    };

    let headers = hyp::headers(&req)?;

    let (bucket, path) = match bucket_name(headers.get("Host"), hyp::path(&req)) {
        Some(b_p) => b_p,
        None => {
            return Ok(SimpleResponse {
                status: 400,
                body: Body::empty(),
            })
        }
    };

    let mut out: fs::File = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open("a.zst")
        .await?;

    let mut enc = zstd::stream::Encoder::new(io::Cursor::new(Vec::with_capacity(8 * 1024)), 3)?;

    let mut body = req.into_body();
    while let Some(data) = body.data().await {
        // typically 8 - 128kB chunks
        let mut data = data?;
        while !data.is_empty() {
            let written = enc.write(&data)?;
            data.advance(written);
            let cursor = enc.get_mut();
            let vec = cursor.get_mut();

            // frequently (for compressible data), the write has not caused any new frames
            if !vec.is_empty() {
                out.write_all(vec).await?;
                vec.clear();
                cursor.set_position(0);
            }
        }
    }

    out.write_all(enc.finish()?.get_ref()).await?;

    Ok(SimpleResponse {
        status: 202,
        body: Body::empty(),
    })
}

#[test]
fn name() {
    assert_eq!(None, bucket_name(None, "/"));
    assert_eq!(None, bucket_name(None, "/potato"));
    assert_eq!(Some(("potato".into(), "/")), bucket_name(None, "/potato/"));
    assert_eq!(
        Some(("potato".into(), "/an/d")),
        bucket_name(None, "/potato/an/d")
    );

    assert_eq!(None, bucket_name(Some(&"foo".into()), "/"));
    assert_eq!(
        Some(("plants".into(), "/greens")),
        bucket_name(Some(&"foo".into()), "/plants/greens")
    );
    assert_eq!(
        Some(("potato".into(), "/")),
        bucket_name(Some(&"potato.foo".into()), "/")
    );
    assert_eq!(
        Some(("potato".into(), "/cheese/and/beans")),
        bucket_name(Some(&"potato.foo".into()), "/cheese/and/beans")
    );
}
