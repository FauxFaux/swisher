use std::collections::HashMap;

use failure::Error;

pub trait SimpleRequest {
    fn method(&self) -> Option<SimpleMethod>;
    fn path(&self) -> &str;
    fn query(&self) -> &str;
    fn headers(&self) -> Result<HashMap<String, String>, Error>;
}

pub struct SimpleResponse {
    pub status: u16,
    pub body: SimpleBody,
}

pub enum SimpleBody {
    Empty,
}

pub enum SimpleMethod {
    Get,
    Put,
    Post,
    Delete,
}

pub fn handle<Q: SimpleRequest>(req: Q) -> Result<SimpleResponse, Error> {
    Ok(SimpleResponse {
        status: 404,
        body: SimpleBody::Empty,
    })
}
