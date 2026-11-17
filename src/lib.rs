//! A tunnel proxy for Claude Code: it answers a `CONNECT` and copies bytes between the client and the target.
//!
//! The binary reads the settings, calls [`Proxy::bind`], writes its start line from [`Proxy::local_addr`] and calls
//! [`Proxy::serve`].

mod config;
mod listener;
mod tunnel;

use std::fmt;
use std::future::Future;
use std::net::SocketAddr;

use tokio::net::TcpListener;

pub use config::{LogLevel, Network, Settings, Upstream};

/// Why the proxy did not start.
#[derive(Debug)]
pub enum StartError {
    /// The listen address could not be bound.
    Bind(SocketAddr, std::io::Error),
    /// A listen address beyond loopback with no allow list.
    OpenBind(SocketAddr),
}

impl fmt::Display for StartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartError::Bind(listen, error) => write!(f, "cannot listen on {listen}: {error}"),
            StartError::OpenBind(listen) => write!(
                f,
                "{listen} is reachable beyond this machine, so the proxy listens there only with an allow list \
                 (--allow or PHOEBIS_PROXY_ALLOW)"
            ),
        }
    }
}

impl std::error::Error for StartError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StartError::Bind(_, error) => Some(error),
            StartError::OpenBind(_) => None,
        }
    }
}

/// A proxy whose listen address is bound.
pub struct Proxy {
    listener: TcpListener,
    local_addr: SocketAddr,
    settings: Settings,
}

impl Proxy {
    /// Checks the settings and binds the listen address.
    pub async fn bind(settings: Settings) -> Result<Proxy, StartError> {
        let listen = settings.listen;
        // The proxy tunnels each host, so on an address other machines reach it is an open forward proxy unless a
        // list names who may connect.
        if settings.allow.is_empty() && !listen.ip().to_canonical().is_loopback() {
            return Err(StartError::OpenBind(listen));
        }
        let listener = TcpListener::bind(listen)
            .await
            .map_err(|error| StartError::Bind(listen, error))?;
        let local_addr = listener
            .local_addr()
            .map_err(|error| StartError::Bind(listen, error))?;
        Ok(Proxy {
            listener,
            local_addr,
            settings,
        })
    }

    /// The bound address, which differs from the setting where it named port 0.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Accepts until `stop` resolves.
    pub async fn serve(self, stop: impl Future<Output = ()> + Send) {
        listener::accept(self.listener, self.settings, stop).await;
    }
}
