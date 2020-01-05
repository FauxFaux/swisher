use std::cell::Cell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use failure::Error;
use hyper::service::make_service_fn;
use hyper::service::service_fn;
use hyper::Body;
use hyper::Request;
use hyper::Response;
use hyper::Server;
use log::debug;
use log::info;
use swisher::reqs::CopyState;
use swisher::reqs::SimpleMethod;
use swisher::MasterKey;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pretty_env_logger::init();
    match dotenv::dotenv() {
        Ok(path) => info!("loaded {:?}", path),
        Err(e) => debug!("no .env loaded: {:?}", e),
    }

    let state = CopyState {
        master: MasterKey::new(&env::var("SWISHER_MASTER_KEY")?),
    };

    let addr = SocketAddr::from(([0, 0, 0, 0], 8202));

    let (shutdown, mut is_shutdown) = mpsc::channel::<()>(1);

    let on_signal = Cell::new(Some(shutdown.clone()));
    ctrlc::set_handler(move || {
        let on_signal = on_signal.take();
        match on_signal {
            Some(mut on_signal) => {
                let success = attempt_shutdown(on_signal);
                log::warn!("signal, attempting shutdown, status: {:?}", success);
            }
            None => log::error!("ignoring termination signal"),
        }
    })?;

    let make_svc = make_service_fn(move |_conn| {
        let shutdown = shutdown.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                catch_handler(req, state, shutdown.clone())
            }))
        }
    });

    log::info!("server starting on http://localhost:8202/");

    Server::bind(&addr)
        .serve(make_svc)
        .with_graceful_shutdown(async {
            let _ = is_shutdown.recv().await;
        })
        .await?;

    Err("server exited".into())
}

async fn catch_handler(
    req: Request<Body>,
    state: CopyState,
    mut shutdown: mpsc::Sender<()>,
) -> Result<Response<Body>, Infallible> {
    // TODO: was expecting to catch_panic here but hyper doesn't want to play
    Ok(match handler(req, state).await {
        Ok(response) => response,
        Err(e) => {
            log::error!("internal error: {:?}", e);
            let success = attempt_shutdown(shutdown);
            log::warn!("error, attempting shutdown, status: {:?}", success);
            Response::builder()
                .status(500)
                .body(Body::empty())
                .expect("static builder")
        }
    })
}

fn attempt_shutdown(mut shutdown: mpsc::Sender<()>) -> bool {
    shutdown.try_send(()).is_ok()
}

async fn handler(req: Request<Body>, state: CopyState) -> Result<Response<Body>, Error> {
    let response = swisher::reqs::handle(req, state).await?;
    Ok(Response::builder()
        .status(response.status)
        .body(response.body)
        .expect("static builder"))
}
