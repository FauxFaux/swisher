use std::collections::HashMap;

use failure::Error;
use hyper::Body;

use super::reqs::SimpleMethod;

pub fn method(method: &hyper::Method) -> Option<SimpleMethod> {
    Some(match *method {
        hyper::Method::GET => SimpleMethod::Get,
        hyper::Method::PUT => SimpleMethod::Put,
        hyper::Method::POST => SimpleMethod::Post,
        hyper::Method::DELETE => SimpleMethod::Delete,
        _ => return None,
    })
}

pub fn path(req: &hyper::Request<Body>) -> &str {
    req.uri().path()
}

pub fn query(req: &hyper::Request<Body>) -> &str {
    req.uri().query().unwrap_or("")
}

pub fn headers(req: &hyper::Request<Body>) -> Result<HashMap<String, String>, Error> {
    let orig = req.headers();
    let mut ret = HashMap::with_capacity(orig.keys_len());
    for (k, v) in orig {
        // TODO: reject duplicate keys, or handle them somehow
        ret.insert(k.as_str().to_string(), v.to_str()?.to_string());
    }
    Ok(ret)
}
