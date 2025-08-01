//! Mock server
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream},
    sync::{Arc, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::Duration,
};

use http_body::Body;
use hyper::{body::Incoming, service::Service};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn,
};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use tokio::net::TcpListener;
use tracing::{debug, error, info};
use url::Url;

use crate::{
    mock::Mock,
    mock_builder::{Then, When},
    mock_set::MockSet,
    service::{GrpcMockService, HttpMockService},
    Error,
};

/// A mock server.
pub struct MockServer {
    name: &'static str,
    kind: ServerKind,
    addr: OnceLock<SocketAddr>,
    base_url: OnceLock<Url>,
    state: Arc<MockServerState>,
    config: MockServerConfig,
}

impl MockServer {
    /// Creates a new HTTP [`MockServer`].
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            kind: ServerKind::Http,
            addr: OnceLock::new(),
            base_url: OnceLock::new(),
            state: Arc::new(MockServerState::default()),
            config: MockServerConfig::default(),
        }
    }

    /// Creates a new HTTP [`MockServer`].
    pub fn new_http(name: &'static str) -> Self {
        Self {
            name,
            kind: ServerKind::Http,
            addr: OnceLock::new(),
            base_url: OnceLock::new(),
            state: Arc::new(MockServerState::default()),
            config: MockServerConfig::default(),
        }
    }

    /// Creates a new gRPC [`MockServer`].
    pub fn new_grpc(name: &'static str) -> Self {
        Self {
            name,
            kind: ServerKind::Grpc,
            addr: OnceLock::new(),
            base_url: OnceLock::new(),
            state: Arc::new(MockServerState::default()),
            config: MockServerConfig::default(),
        }
    }

    /// Sets the server type to gRPC.
    #[deprecated(since = "0.3.0", note = "please use `new_grpc` instead")]
    pub fn grpc(mut self) -> Self {
        self.kind = ServerKind::Grpc;
        self
    }

    /// Sets the server mocks.
    pub fn with_mocks(self, mocks: MockSet) -> Self {
        *self.state.mocks.write().unwrap() = mocks;
        self
    }

    /// Sets the server configuration.
    pub fn with_config(mut self, config: MockServerConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn start(&self) -> Result<(), Error> {
        if self.addr().is_some() {
            return Err(Error::ServerError("already running".into()));
        }

        let mut counter = 0;
        let mut rng = SmallRng::from_os_rng();

        let listener = loop {
            let port: u16 =
                rng.random_range(self.config.port_range_start..self.config.port_range_end);
            let addr = SocketAddr::from((self.config.listen_addr, port));
            if let Ok(listener) = TcpListener::bind(&addr).await {
                break listener;
            }

            if counter == self.config.bind_max_retries {
                return Err(Error::ServerError("server failed to bind to port".into()));
            }
            counter += 1;
        };

        let addr = listener.local_addr()?;
        info!("started {} [{}] server on {addr}", self.name(), &self.kind);
        let base_url = Url::parse(&format!("http://{}", &addr)).unwrap();

        match self.kind {
            ServerKind::Http => {
                let service = HttpMockService::new(self.state.clone());
                tokio::spawn(run_server(listener, self.kind, service));
            }
            ServerKind::Grpc => {
                let service = GrpcMockService::new(self.state.clone());
                tokio::spawn(run_server(listener, self.kind, service));
            }
        };
        // Wait for server to become ready
        let mut counter = 0;
        loop {
            if TcpStream::connect_timeout(&addr, self.config.ready_connect_timeout).is_ok() {
                break;
            }
            if counter == self.config.ready_connect_max_retries {
                return Err(Error::ServerError("server failed to become ready".into()));
            }
            counter += 1;
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        info!("{} server ready", self.name());

        self.addr.set(addr).unwrap();
        self.base_url.set(base_url).unwrap();

        Ok(())
    }

    pub fn name(&self) -> &str {
        self.name
    }

    pub fn addr(&self) -> Option<&SocketAddr> {
        self.addr.get()
    }

    pub fn hostname(&self) -> Option<String> {
        self.addr().map(|addr| addr.ip().to_string())
    }

    pub fn port(&self) -> Option<u16> {
        self.addr.get().map(|v| v.port())
    }

    pub fn base_url(&self) -> Option<&Url> {
        self.base_url.get()
    }

    pub fn url(&self, path: &str) -> Url {
        if let Some(url) = self.base_url() {
            url.join(path).unwrap()
        } else {
            panic!("server not running");
        }
    }

    pub fn is_running(&self) -> bool {
        self.addr().is_some()
    }

    pub fn mocks(&self) -> RwLockWriteGuard<'_, MockSet> {
        self.state.mocks.write().unwrap()
    }

    /// Builds and inserts a mock with default options.
    pub fn mock<F>(&mut self, f: F)
    where
        F: FnOnce(When, Then),
    {
        let mock = Mock::new(f);
        self.state.mocks.write().unwrap().insert(mock);
    }

    /// Builds and inserts a mock with options.
    pub fn mock_with_options<F>(&mut self, priority: u8, limit: Option<usize>, f: F)
    where
        F: FnOnce(When, Then),
    {
        let mut mock = Mock::new(f).with_priority(priority);
        if let Some(limit) = limit {
            mock = mock.with_limit(limit);
        }
        self.state.mocks.write().unwrap().insert(mock);
    }
}

/// Mock server state.
#[derive(Debug, Default)]
pub struct MockServerState {
    pub mocks: RwLock<MockSet>,
}

impl MockServerState {
    pub fn new(mocks: MockSet) -> Self {
        Self {
            mocks: RwLock::new(mocks),
        }
    }

    pub fn mocks(&self) -> RwLockReadGuard<'_, MockSet> {
        self.mocks.read().unwrap()
    }
}

#[derive(Debug, Clone, Copy)]
enum ServerKind {
    Http,
    Grpc,
}

impl std::fmt::Display for ServerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerKind::Http => write!(f, "http"),
            ServerKind::Grpc => write!(f, "grpc"),
        }
    }
}

/// Runs the main server loop to accept and serve connections.
async fn run_server<S, B>(
    listener: TcpListener,
    server_kind: ServerKind,
    service: S,
) -> Result<(), Error>
where
    S: Service<http::Request<Incoming>, Response = http::Response<B>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    B: Body + Send + 'static,
    B::Data: Send + 'static,
    B::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    // Spawn task to accept new connections
    tokio::spawn(async move {
        loop {
            let (stream, addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(err) => {
                    error!("connection accept error: {err}");
                    continue;
                }
            };
            debug!("connection accepted: {addr}");
            let io = TokioIo::new(stream);
            let service = service.clone();
            // Spawn task to serve connection
            tokio::spawn(async move {
                let builder = match server_kind {
                    ServerKind::Http => conn::auto::Builder::new(TokioExecutor::new()),
                    ServerKind::Grpc => conn::auto::Builder::new(TokioExecutor::new()).http2_only(),
                };
                if let Err(err) = builder.serve_connection(io, service).await {
                    debug!("connection error: {err}");
                }
                debug!("connection dropped: {addr}");
            });
        }
    });

    Ok(())
}

#[derive(Debug)]
pub struct MockServerConfig {
    pub listen_addr: IpAddr,
    pub port_range_start: u16,
    pub port_range_end: u16,
    pub bind_max_retries: usize,
    pub ready_connect_max_retries: usize,
    pub ready_connect_timeout: Duration,
}

impl MockServerConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for MockServerConfig {
    fn default() -> Self {
        Self {
            listen_addr: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            port_range_start: 10000,
            port_range_end: 30000,
            bind_max_retries: 10,
            ready_connect_max_retries: 30,
            ready_connect_timeout: Duration::from_millis(10),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_server_send() {
        fn is_send<T: Send>() {}
        is_send::<MockServer>();
    }
}
