use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;

use failure::Error;
use hyper::service::make_service_fn;
use hyper::service::service_fn;
use hyper::Body;
use hyper::Request;
use hyper::Response;
use hyper::Server;
use swisher::reqs::SimpleBody;
use swisher::reqs::SimpleMethod;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pretty_env_logger::init();

    let addr = SocketAddr::from(([0, 0, 0, 0], 8202));

    let make_svc =
        make_service_fn(|_conn| async { Ok::<_, Infallible>(service_fn(catch_handler)) });

    log::info!("server starting on http://localhost:8202/");

    Server::bind(&addr).serve(make_svc).await?;
    Ok(())
}

async fn catch_handler(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    // TODO: was expecting to catch_panic here but hyper doesn't want to play
    Ok(match handler(req).await {
        Ok(response) => response,
        Err(e) => {
            log::error!("internal error: {:?}", e);
            Response::builder()
                .status(500)
                .body(Body::empty())
                .expect("static builder")
        },
    })
}

async fn handler(req: Request<Body>) -> Result<Response<Body>, Error> {
    let response = swisher::reqs::handle(HyperRequest(req))?;
    Ok(Response::builder()
        .status(response.status)
        .body(match response.body {
            SimpleBody::Empty => Body::empty(),
        })
        .expect("static builder"))
}

struct HyperRequest(Request<Body>);

impl swisher::reqs::SimpleRequest for HyperRequest {
    fn method(&self) -> Option<SimpleMethod> {
        Some(match *self.0.method() {
            hyper::Method::GET => SimpleMethod::Get,
            hyper::Method::PUT => SimpleMethod::Put,
            hyper::Method::POST => SimpleMethod::Post,
            hyper::Method::DELETE => SimpleMethod::Delete,
            _ => return None,
        })
    }

    fn path(&self) -> &str {
        self.0.uri().path()
    }

    fn query(&self) -> &str {
        self.0.uri().query().unwrap_or("")
    }

    fn headers(&self) -> Result<HashMap<String, String>, Error> {
        let orig = self.0.headers();
        let mut ret = HashMap::with_capacity(orig.keys_len());
        for (k, v) in orig {
            // TODO: reject duplicate keys, or handle them somehow
            ret.insert(k.as_str().to_string(), v.to_str()?.to_string());
        }
        Ok(ret)
    }
}
