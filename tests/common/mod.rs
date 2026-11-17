//! Helpers shared by the proxy's test files.
//!
//! Each integration test binary compiles this module separately, so a helper only one binary uses looks dead to
//! the other.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use phoebis_proxy::{LogLevel, Proxy, Settings, Upstream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::timeout;

/// The bound on a read that would wait forever under a test's break, so the break fails the test instead of hanging
/// it.
pub const BOUND: Duration = Duration::from_secs(5);

/// The proxy as the binary starts it: `Proxy::bind`, the address from `local_addr`, and `serve` on a task of its
/// own, which a drop of the value stops.
pub struct Running {
    pub addr: SocketAddr,
    _stop: oneshot::Sender<()>,
    _task: JoinHandle<std::io::Result<()>>,
}

pub async fn start(settings: Settings) -> Running {
    let proxy = Proxy::bind(settings).await.expect("the proxy binds");
    let addr = proxy.local_addr();
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(proxy.serve(async move {
        // The sender's drop resolves the receiver with an error, which is the stop.
        let _ = stopped.await;
    }));
    Running {
        addr,
        _stop: stop,
        _task: task,
    }
}

/// Settings for a test: `127.0.0.1:0`, the log at `info`, the given allow list and upstream.
pub fn settings(allow: &[&str], upstream: SocketAddr) -> Settings {
    Settings {
        allow: allow
            .iter()
            .map(|network| network.parse().unwrap())
            .collect(),
        listen: "127.0.0.1:0".parse().unwrap(),
        log: LogLevel::Info,
        upstream: Upstream {
            host: upstream.ip().to_string(),
            port: upstream.port(),
        },
    }
}

/// A plain TCP listener on `127.0.0.1:0` that accepts in a loop and keeps each byte it reads. A connection that
/// sends nothing, as the development host's probe opens, adds nothing to `received`.
pub struct Fake {
    pub addr: SocketAddr,
    received: Arc<Mutex<Vec<u8>>>,
    task: JoinHandle<()>,
}

impl Fake {
    /// Writes `answer` on a connection once it read `expect` there, and writes back what it read where both are
    /// empty.
    pub async fn start(expect: &'static [u8], answer: &'static [u8]) -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let received = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&received);
        let task = tokio::spawn(async move {
            // The connections' tasks live in the set, so the abort of this task on the fake's drop ends them too.
            let mut connections = JoinSet::new();
            while let Ok((stream, _)) = listener.accept().await {
                connections.spawn(fake_connection(stream, expect, answer, Arc::clone(&kept)));
            }
        });
        Fake {
            addr,
            received,
            task,
        }
    }

    pub fn received(&self) -> Vec<u8> {
        self.received.lock().unwrap().clone()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn fake_connection(
    mut stream: TcpStream,
    expect: &'static [u8],
    answer: &'static [u8],
    received: Arc<Mutex<Vec<u8>>>,
) {
    let echo = expect.is_empty() && answer.is_empty();
    let mut seen = Vec::new();
    let mut answered = echo;
    let mut chunk = [0; 4096];
    loop {
        if !answered
            && (expect.is_empty() || seen.windows(expect.len()).any(|window| window == expect))
        {
            answered = true;
            if stream.write_all(answer).await.is_err() {
                return;
            }
        }
        let read = match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(read) => read,
        };
        received.lock().unwrap().extend_from_slice(&chunk[..read]);
        seen.extend_from_slice(&chunk[..read]);
        if echo && stream.write_all(&chunk[..read]).await.is_err() {
            return;
        }
    }
}

/// One client connection to the proxy with `CONNECT <target> HTTP/1.1` and its `Host` line written by hand.
/// Answers the stream and the answer's head up to its blank line.
pub async fn connect(proxy: SocketAddr, target: &str) -> (TcpStream, String) {
    let mut stream = TcpStream::connect(proxy).await.unwrap();
    stream
        .write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut head = Vec::new();
    // One byte per read, so no byte of the tunnel behind the blank line is taken from the stream.
    let read = async {
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            if stream.read(&mut byte).await.unwrap() == 0 {
                break;
            }
            head.push(byte[0]);
        }
    };
    timeout(BOUND, read)
        .await
        .expect("the proxy answers the CONNECT within five seconds");
    (stream, String::from_utf8(head).unwrap())
}
