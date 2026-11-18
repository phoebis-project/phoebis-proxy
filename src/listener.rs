//! The accept loop, the allow list, the read of a `CONNECT` head and its answer.

use std::fmt;
use std::future::Future;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

use crate::config::{authority, LogLevel, Settings};
use crate::tunnel;

/// The most bytes a `CONNECT` head takes, its blank line included.
const HEAD_LIMIT: usize = 8 * 1024;
/// How long a client has to send its `CONNECT` head.
const HEAD_TIMEOUT: Duration = Duration::from_secs(30);

/// The wait after an accept that failed for want of a resource, such as a file descriptor, so the loop does not spin
/// while the shortage lasts.
const ACCEPT_PAUSE: Duration = Duration::from_secs(1);

const ESTABLISHED: &[u8] = b"HTTP/1.1 200 Connection established\r\n\r\n";

/// An answer that ends a connection without a tunnel.
struct Refusal {
    status: u16,
    answer: &'static [u8],
}

const BAD_REQUEST: Refusal = Refusal {
    status: 400,
    answer: b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
};
const METHOD_NOT_ALLOWED: Refusal = Refusal {
    status: 405,
    answer: b"HTTP/1.1 405 Method Not Allowed\r\nAllow: CONNECT\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
};
const BAD_GATEWAY: Refusal = Refusal {
    status: 502,
    answer: b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
};

/// Accepts connections until `stop` resolves, each served on a task of its own.
pub(crate) async fn accept(
    listener: TcpListener,
    settings: Settings,
    stop: impl Future<Output = ()>,
) {
    let settings = Arc::new(settings);
    tokio::pin!(stop);
    loop {
        let accepted = tokio::select! {
            () = &mut stop => return,
            accepted = listener.accept() => accepted,
        };
        match accepted {
            Ok((client, peer)) => {
                // The compare comes before the first read, so a source off the list gets no byte read and no answer.
                if admitted(&settings, peer) {
                    tokio::spawn(serve_connection(client, Arc::clone(&settings)));
                } else {
                    debug(
                        &settings,
                        format_args!("closed a connection from {peer}, outside the allow list"),
                    );
                }
            }
            // The client left between its connect and the accept, which ends that connection alone.
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset
                ) => {}
            // No accept error ends the loop: a shortage passes, and an end of `serve` would drop each open tunnel with
            // the process.
            Err(error) => {
                eprintln!(
                    "accepting a connection failed, the next attempt follows a pause: {error}"
                );
                tokio::select! {
                    () = &mut stop => return,
                    () = sleep(ACCEPT_PAUSE) => {}
                }
            }
        }
    }
}

fn admitted(settings: &Settings, peer: SocketAddr) -> bool {
    settings.allow.is_empty()
        || settings
            .allow
            .iter()
            .any(|network| network.contains(peer.ip()))
}

/// Reads one `CONNECT` head, answers it and carries the tunnel behind a 200.
async fn serve_connection(mut client: TcpStream, settings: Arc<Settings>) {
    let started = Instant::now();
    // A head past either bound gets no answer: the connection closes, and what a client sends costs no more than the
    // two bounds.
    let Ok(Some((head, early))) = timeout(HEAD_TIMEOUT, read_head(&mut client)).await else {
        return;
    };
    let (host, port) = match connect_target(&head) {
        Ok((host, port)) => (host.to_owned(), port),
        Err(refusal) => {
            // A request line holds a method and a path, which no line of the log carries.
            debug(
                &settings,
                format_args!("refused a request with {}", refusal.status),
            );
            let _ = client.write_all(refusal.answer).await;
            return;
        }
    };
    drop(head);
    let (to_host, to_port) = tunnel::destination(&host, port, &settings.upstream);
    let far = match tunnel::connect(to_host, to_port).await {
        Ok(far) => far,
        Err(error) => {
            // The default level shows an operator that the router cannot be reached, and the line names no target,
            // since the names a user's programs call stay at `debug`. A target that names the upstream's own address
            // writes it too, which shows no more than the start line. The line is written ahead of the 502, so it
            // stands in the log once the client has read the answer.
            if to_host == settings.upstream.host && to_port == settings.upstream.port {
                eprintln!("cannot reach the upstream {}: {error}", settings.upstream);
            }
            debug(
                &settings,
                format_args!(
                    "CONNECT {host}:{port} to {to_host}:{to_port}: {} after {:.1?} ({error})",
                    BAD_GATEWAY.status,
                    started.elapsed()
                ),
            );
            let _ = client.write_all(BAD_GATEWAY.answer).await;
            return;
        }
    };
    if client.write_all(ESTABLISHED).await.is_err() {
        return;
    }
    let opened = Instant::now();
    tunnel::carry(client, far, &early).await;
    debug(
        &settings,
        format_args!(
            "CONNECT {host}:{port} to {to_host}:{to_port}: 200, the tunnel closed after {:.1?}",
            opened.elapsed()
        ),
    );
}

/// Reads up to the blank line that ends a head. Answers the head and the bytes that arrived behind it in the same
/// read, or `None` where a read fails, the client ends first or it sends `HEAD_LIMIT` bytes without the blank line.
async fn read_head(client: &mut TcpStream) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut buffer = vec![0; HEAD_LIMIT];
    let mut filled = 0;
    while filled < buffer.len() {
        let read = client.read(&mut buffer[filled..]).await.ok()?;
        if read == 0 {
            return None;
        }
        // The blank line can straddle two reads, so the search starts three bytes ahead of this read's first byte.
        let from = filled.saturating_sub(3);
        filled += read;
        if let Some(at) = buffer[from..filled]
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
        {
            let end = from + at + 4;
            let early = buffer[end..filled].to_vec();
            buffer.truncate(end);
            return Some((buffer, early));
        }
    }
    None
}

/// The target of a request line `CONNECT <host>:<port> HTTP/1.1`, or the refusal of another request.
fn connect_target(head: &[u8]) -> Result<(&str, u16), Refusal> {
    let line = head.split(|byte| *byte == b'\r').next().unwrap_or_default();
    let line = std::str::from_utf8(line).map_err(|_| BAD_REQUEST)?;
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(BAD_REQUEST);
    };
    if method != "CONNECT" {
        return Err(METHOD_NOT_ALLOWED);
    }
    if !version.starts_with("HTTP/1.") {
        return Err(BAD_REQUEST);
    }
    authority(target).ok_or(BAD_REQUEST)
}

fn debug(settings: &Settings, line: fmt::Arguments<'_>) {
    if settings.log == LogLevel::Debug {
        eprintln!("{line}");
    }
}
