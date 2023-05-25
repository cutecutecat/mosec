// Copyright 2022 MOSEC Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod args;
mod coordinator;
mod errors;
mod metrics;
mod protocol;
mod tasks;

use std::{collections::HashMap, net::SocketAddr, time::Duration};

use args::Endpoint;
use async_channel::{bounded, Receiver, Sender};
use axum::{
    extract::State,
    routing::{get, post},
    Router,
};
use bytes::Bytes;
use hyper::{
    body::to_bytes,
    header::{HeaderValue, CONTENT_TYPE},
    Body, Request, Response, StatusCode,
};
use metrics::{CodeLabel, DURATION_LABEL, REGISTRY};
use prometheus_client::encoding::text::encode;
use tokio::signal::unix::{signal, SignalKind};
use tracing::error;
use tracing::info;
use tracing_subscriber::fmt::time::OffsetTime;
use tracing_subscriber::{filter, prelude::*, Layer};

use crate::coordinator::Coordinator;
use crate::errors::ServiceError;
use crate::metrics::Metrics;
use crate::tasks::{TaskCode, TaskManager};
use crate::{args::Opts, metrics::METRICS, tasks::TASK_MANAGER};

const SERVER_INFO: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const RESPONSE_DEFAULT: &[u8] = b"MOSEC service";
const RESPONSE_EMPTY: &[u8] = b"no data provided";
const RESPONSE_SHUTDOWN: &[u8] = b"gracefully shutting down";

#[derive(Clone)]
struct AppState {
    mime: HashMap<Endpoint, String>,
}

async fn index(_: Request<Body>) -> Response<Body> {
    let task_manager = TaskManager::global();
    if task_manager.is_shutdown() {
        build_response(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(RESPONSE_SHUTDOWN),
        )
    } else {
        build_response(StatusCode::OK, Bytes::from_static(RESPONSE_DEFAULT))
    }
}

async fn metrics(_: Request<Body>) -> Response<Body> {
    let mut encoded = String::new();
    let registry = REGISTRY.get().unwrap();
    encode(&mut encoded, registry).unwrap();
    build_response(StatusCode::OK, Bytes::from(encoded))
}

async fn inference(State(state): State<AppState>, req: Request<Body>) -> Response<Body> {
    let endpoint = Endpoint::new(req.uri().path());
    let task_manager = TaskManager::global();
    let data = to_bytes(req.into_body()).await.unwrap();
    let metrics = Metrics::global();

    if task_manager.is_shutdown() {
        return build_response(
            StatusCode::SERVICE_UNAVAILABLE,
            Bytes::from_static(RESPONSE_SHUTDOWN),
        );
    }

    if data.is_empty() {
        return build_response(StatusCode::OK, Bytes::from_static(RESPONSE_EMPTY));
    }

    let (status, content);
    metrics.remaining_task.inc();
    match task_manager.submit_task(data, &endpoint).await {
        Ok(task) => {
            content = task.data;
            status = match task.code {
                TaskCode::Normal => {
                    // Record latency only for successful tasks
                    metrics
                        .duration
                        .get_or_create(
                            DURATION_LABEL
                                .get()
                                .expect("DURATION_LABEL is not initialized"),
                        )
                        .observe(task.create_at.elapsed().as_secs_f64());
                    StatusCode::OK
                }
                TaskCode::BadRequestError => StatusCode::BAD_REQUEST,
                TaskCode::ValidationError => StatusCode::UNPROCESSABLE_ENTITY,
                TaskCode::TimeoutError => StatusCode::REQUEST_TIMEOUT,
                TaskCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        Err(err) => {
            // Handle errors for which tasks cannot be retrieved
            content = Bytes::from(err.to_string());
            status = match err {
                ServiceError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
                ServiceError::Timeout => StatusCode::REQUEST_TIMEOUT,
                ServiceError::UnknownError => StatusCode::INTERNAL_SERVER_ERROR,
            };
        }
    }
    metrics.remaining_task.dec();
    metrics
        .throughput
        .get_or_create(&CodeLabel {
            code: status.as_u16(),
        })
        .inc();

    let mut resp = build_response(status, content);
    if status == StatusCode::OK {
        let mime = state.mime.get(&endpoint).unwrap();
        resp.headers_mut()
            .insert(CONTENT_TYPE, HeaderValue::from_str(mime).unwrap());
    }
    resp
}

fn build_response(status: StatusCode, content: Bytes) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("server", HeaderValue::from_static(SERVER_INFO))
        .body(Body::from(content))
        .unwrap()
}

async fn shutdown_signal() {
    let mut interrupt = signal(SignalKind::interrupt()).unwrap();
    let mut terminate = signal(SignalKind::terminate()).unwrap();
    loop {
        tokio::select! {
            _ = interrupt.recv() => {
                info!("service received interrupt signal, will ignore it here \
                    since it should be controlled by the main process (send SIGTERM \
                    to `mosec` if you really want to kill it manually)");
            },
            _ = terminate.recv() => {
                info!("service received terminate signal");
                let task_manager = TaskManager::global();
                task_manager.shutdown().await;
                info!("service shutdown complete");
                break;
            },
        };
    }
}

fn init_global(
    opts: &Opts,
) -> (
    HashMap<Endpoint, Sender<u32>>,
    HashMap<Endpoint, Receiver<u32>>,
) {
    // init the global task manager
    let (mut senders, mut receivers) = (HashMap::new(), HashMap::new());
    for e in opts.endpoints.iter() {
        let (sender, receiver) = bounded(opts.capacity);
        senders.insert(e.to_owned(), sender);
        receivers.insert(e.to_owned(), receiver);
    }
    let timeout = Duration::from_millis(opts.timeout);
    let task_manager = TaskManager::new(timeout, opts.endpoints.clone(), senders.clone());
    TASK_MANAGER.set(task_manager).unwrap();
    let metrics = Metrics::init_with_namespace(&opts.namespace, opts.timeout);
    METRICS.set(metrics).unwrap();

    (senders, receivers)
}

#[tokio::main]
async fn run(opts: &Opts) {
    let state = AppState {
        mime: opts
            .endpoints
            .clone()
            .into_iter()
            .zip(opts.mime.clone().into_iter())
            .collect::<HashMap<Endpoint, String>>(),
    };
    let (senders, receivers) = init_global(opts);
    let endpoint_opts = opts.devide_into_endpoints();

    let mut app = Router::new()
        .route("/", get(index))
        .route("/metrics", get(metrics));
    for endpoint in opts.endpoints.iter() {
        let sender = senders.get(endpoint).unwrap().clone();
        let receiver = receivers.get(endpoint).unwrap().clone();
        let (batches, waits) = endpoint_opts.get(endpoint).unwrap();

        let coordinator = Coordinator::init(
            batches.clone(),
            waits.clone(),
            opts.path.clone(),
            opts.capacity,
            sender,
            receiver,
        );
        let barrier = coordinator.run(endpoint.clone());
        barrier.wait().await;
        app = app.route(endpoint.path(), post(inference));
    }
    let app = app.with_state(state);

    let addr: SocketAddr = format!("{}:{}", opts.address, opts.port).parse().unwrap();
    info!(?addr, "http service is running");
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

fn main() {
    let opts: Opts = argh::from_env();
    if let Err(err) = opts.validate() {
        error!(%err, "mosec server start failed");
        return;
    }

    // this has to be defined before tokio multi-threads
    let timer = OffsetTime::local_rfc_3339().expect("local time offset");
    if opts.debug {
        // use colorful log for debug
        let output = tracing_subscriber::fmt::layer().compact().with_timer(timer);
        tracing_subscriber::registry()
            .with(
                output
                    .with_filter(filter::filter_fn(|metadata| {
                        !metadata.target().starts_with("hyper")
                    }))
                    .with_filter(filter::LevelFilter::DEBUG),
            )
            .init();
    } else {
        // use JSON format for production
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .json()
            .with_timer(timer)
            .init();
    }
    info!(?opts, "parse service arguments");
    run(&opts);
}
