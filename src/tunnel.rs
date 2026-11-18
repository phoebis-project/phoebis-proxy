//! Where a target's bytes go, the connection there and the copy between the two sockets.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::Duration;

use socket2::SockRef;
use tokio::io::{copy, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout, Sleep};

use crate::config::Upstream;

/// How long the connection to a destination may take, the lookup of its name included.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a write toward a socket of a tunnel may wait while the socket takes no byte, each byte it takes starting
/// the wait again. While a peer reads nothing, the copy toward it waits in its write and stops reading the other side,
/// so a close or a failed keepalive there goes unseen, and this limit is what ends the tunnel. A read waits for a peer
/// with nothing to send, such as an event stream that idles for hours, so a read has no limit.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

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
/// side ends or a write toward one side takes no byte for `WRITE_TIMEOUT`.
pub(crate) async fn carry(mut client: TcpStream, mut far: TcpStream, early: &[u8]) {
    for stream in [&client, &far] {
        // A tunnel has no idle limit, so keepalive is what ends one whose peer died without a close. A socket that
        // refuses either option still carries its bytes, so a refusal is no reason to end the tunnel.
        let _ = SockRef::from(stream).set_keepalive(true);
        // Each piece goes on as it arrives, where Nagle's wait for an acknowledgment would hold a short event back.
        let _ = stream.set_nodelay(true);
    }
    let (mut client_read, client_write) = client.split();
    let (mut far_read, far_write) = far.split();
    let mut client_write = Limited::new(client_write);
    let mut far_write = Limited::new(far_write);
    if far_write.write_all(early).await.is_err() {
        return;
    }
    // The first copy to end, by a close or by an error, ends the tunnel: both sockets drop on the return, so the other
    // side reads its end, and nothing its peer sends after that is carried.
    tokio::select! {
        _ = copy(&mut client_read, &mut far_write) => {}
        _ = copy(&mut far_read, &mut client_write) => {}
    }
}

/// A write half whose write fails once it has waited `WRITE_TIMEOUT` with no byte taken.
struct Limited<W> {
    inner: W,
    /// The end of the write's current wait, set where the write first waits and dropped once the socket takes a byte.
    stall: Option<Pin<Box<Sleep>>>,
}

impl<W> Limited<W> {
    fn new(inner: W) -> Limited<W> {
        Limited { inner, stall: None }
    }
}

impl<W: AsyncWrite + Unpin> AsyncWrite for Limited<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = &mut *self;
        if let Poll::Ready(written) = Pin::new(&mut this.inner).poll_write(cx, bytes) {
            this.stall = None;
            return Poll::Ready(written);
        }
        let stall = this
            .stall
            .get_or_insert_with(|| Box::pin(sleep(WRITE_TIMEOUT)));
        ready!(stall.as_mut().poll(cx));
        Poll::Ready(Err(io::ErrorKind::TimedOut.into()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
