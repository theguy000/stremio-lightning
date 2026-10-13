//! `stremio-core` host environment for native desktop shells.
//!
//! The interface is [`NativeEnv::init`]: call it once with a data directory and a
//! tokio runtime handle, then use `NativeEnv` as the `Env` type parameter of the core.

use std::path::PathBuf;
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use futures::{Future, FutureExt};
use http::Request;
use serde::{Deserialize, Serialize};
use stremio_core::models::ctx::Ctx;
use stremio_core::models::streaming_server::StreamingServer;
use stremio_core::runtime::{ConditionalSend, Env, EnvError, EnvFuture, TryEnvFuture};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

struct Host {
    data_dir: PathBuf,
    runtime: Handle,
    sequential: UnboundedSender<BoxFuture<'static, ()>>,
    http: reqwest::Client,
}

static HOST: OnceLock<Host> = OnceLock::new();

pub struct NativeEnv;

impl NativeEnv {
    /// Starts the sequential executor and records where storage lives.
    /// Returns an error if called twice.
    pub fn init(data_dir: PathBuf, runtime: Handle) -> Result<(), EnvError> {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| EnvError::StorageWriteError(e.to_string()))?;
        let (sequential, mut queue) = unbounded_channel::<BoxFuture<'static, ()>>();
        runtime.spawn(async move {
            while let Some(job) = queue.recv().await {
                job.await;
            }
        });
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| EnvError::Fetch(e.to_string()))?;
        HOST.set(Host {
            data_dir,
            runtime,
            sequential,
            http,
        })
        .map_err(|_| EnvError::Other("NativeEnv::init called twice".into()))
    }
}

fn host() -> Result<&'static Host, EnvError> {
    HOST.get().ok_or(EnvError::StorageUnavailable)
}

fn storage_path(host: &Host, key: &str) -> PathBuf {
    let safe: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    host.data_dir.join(format!("{safe}.json"))
}

impl Env for NativeEnv {
    fn fetch<
        IN: Serialize + ConditionalSend + 'static,
        OUT: for<'de> Deserialize<'de> + ConditionalSend + 'static,
    >(
        request: Request<IN>,
    ) -> TryEnvFuture<OUT> {
        async move {
            let host = host()?;
            let (parts, body) = request.into_parts();
            let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())
                .map_err(|e| EnvError::Fetch(e.to_string()))?;
            let mut builder = host.http.request(method, parts.uri.to_string());
            for (name, value) in &parts.headers {
                builder = builder.header(name.as_str(), value.as_bytes());
            }
            if parts.method != http::Method::GET && parts.method != http::Method::HEAD {
                builder = builder
                    .header("content-type", "application/json")
                    .body(serde_json::to_vec(&body)?);
            }
            let response = builder
                .send()
                .await
                .map_err(|e| EnvError::Fetch(e.to_string()))?;
            if !response.status().is_success() {
                return Err(EnvError::Fetch(format!("HTTP {}", response.status())));
            }
            let bytes = response
                .bytes()
                .await
                .map_err(|e| EnvError::Fetch(e.to_string()))?;
            Ok(serde_json::from_slice(&bytes)?)
        }
        .boxed()
    }

    fn get_storage<T: for<'de> Deserialize<'de> + ConditionalSend + 'static>(
        key: &str,
    ) -> TryEnvFuture<Option<T>> {
        let key = key.to_owned();
        async move {
            let path = storage_path(host()?, &key);
            match tokio::fs::read(&path).await {
                Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(EnvError::StorageReadError(e.to_string())),
            }
        }
        .boxed()
    }

    fn set_storage<T: Serialize>(key: &str, value: Option<&T>) -> TryEnvFuture<()> {
        let key = key.to_owned();
        let bytes = value.map(serde_json::to_vec).transpose();
        async move {
            let path = storage_path(host()?, &key);
            let write_err = |e: std::io::Error| EnvError::StorageWriteError(e.to_string());
            match bytes? {
                Some(bytes) => {
                    let tmp = path.with_extension("json.tmp");
                    tokio::fs::write(&tmp, bytes).await.map_err(write_err)?;
                    tokio::fs::rename(&tmp, &path).await.map_err(write_err)
                }
                None => match tokio::fs::remove_file(&path).await {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(write_err(e)),
                    _ => Ok(()),
                },
            }
        }
        .boxed()
    }

    fn exec_concurrent<F: Future<Output = ()> + ConditionalSend + 'static>(future: F) {
        if let Ok(host) = host() {
            host.runtime.spawn(future);
        }
    }

    fn exec_sequential<F: Future<Output = ()> + ConditionalSend + 'static>(future: F) {
        if let Ok(host) = host() {
            let _ = host.sequential.send(future.boxed());
        }
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn flush_analytics() -> EnvFuture<'static, ()> {
        async {}.boxed()
    }

    fn analytics_context(
        _ctx: &Ctx,
        _streaming_server: &StreamingServer,
        _path: &str,
    ) -> serde_json::Value {
        serde_json::Value::Null
    }

    #[cfg(debug_assertions)]
    fn log(message: String) {
        eprintln!("{message}");
    }
}
