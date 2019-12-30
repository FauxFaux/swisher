use std::collections::HashMap;

use failure::Error;
use hyper::Body;
use hyper::Request;

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

pub fn handle(req: Request<Body>) -> Result<SimpleResponse, Error> {
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

    Ok(SimpleResponse {
        status: 404,
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
