//! Where a target's bytes go, the connection there and the copy between the two sockets.

use std::io;
use std::time::Duration;

use socket2::SockRef;
use tokio::io::{copy, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::config::Upstream;

/// How long the connection to a destination may take, the lookup of its name included.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The two hosts whose port 443 goes to the router's feature listener. The router's certificate authority is
/// bound to the same two names, so a third name here would get no certificate there.
const FEATURE_HOSTS: [&str; 2] = ["api.anthropic.com", "mcp-proxy.anthropic.com"];

/// Where a target's bytes go: the upstream's host and port for a feature host on port 443, compared without case and
/// without a trailing dot, and the target itself for each other target. A tunnel of a feature host to itself would let
/// the client finish its TLS with the real host and send it the router key.
pub(crate) fn destination<'a>(host: &'a str, port: u16, upstream: &'a Upstream) -> (&'a str, u16) {
    let name = host.strip_suffix('.').unwrap_or(host);
    if port == 443
        && FEATURE_HOSTS
            .iter()
            .any(|feature| feature.eq_ignore_ascii_case(name))
    {
        (upstream.host.as_str(), upstream.port)
    } else {
        (host, port)
    }
}

/// Connects to a destination within `CONNECT_TIMEOUT`.
pub(crate) async fn connect(host: &str, port: u16) -> io::Result<TcpStream> {
    timeout(CONNECT_TIMEOUT, TcpStream::connect((host, port)))
        .await
        .unwrap_or_else(|_| {
            Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the connection attempt timed out",
            ))
        })
}

/// Sends `early`, the bytes the client wrote behind its head, to the far side, then copies bytes both ways until one
/// side ends.
pub(crate) async fn carry(mut client: TcpStream, mut far: TcpStream, early: &[u8]) {
    for stream in [&client, &far] {
        // A tunnel has no idle limit, so keepalive is what ends one whose peer died without a close. A socket that
        // refuses either option still carries its bytes, so a refusal is no reason to end the tunnel.
        let _ = SockRef::from(stream).set_keepalive(true);
        // Each piece goes on as it arrives, where Nagle's wait for an acknowledgment would hold a short event back.
        let _ = stream.set_nodelay(true);
    }
    if far.write_all(early).await.is_err() {
        return;
    }
    let (mut client_read, mut client_write) = client.split();
    let (mut far_read, mut far_write) = far.split();
    // The first copy to end, by a close or by an error, ends the tunnel: both sockets drop on the return, so the other
    // side reads its end, and nothing its peer sends after that is carried.
    tokio::select! {
        _ = copy(&mut client_read, &mut far_write) => {}
        _ = copy(&mut far_read, &mut client_write) => {}
    }
}
