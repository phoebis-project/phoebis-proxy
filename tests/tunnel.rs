mod common;

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Notify};
use tokio::time::timeout;

use common::{answer_head, connect, settings, start, Fake, BOUND};

/// A far side of the test's own on `127.0.0.1:0`, which runs `serve` on each connection it accepts. The listener and
/// its connections live until the test's runtime ends.
async fn far_side<F, S>(serve: F) -> SocketAddr
where
    F: Fn(TcpStream) -> S + Send + 'static,
    S: Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            tokio::spawn(serve(stream));
        }
    });
    addr
}

/// Reads the two bytes `go` the test sends through its tunnel. A connection that sends nothing, as a port scanner or a
/// monitoring daemon opens, never passes this read, so the far side acts on the tunnel's connection alone.
async fn the_tunnels_connection(stream: &mut TcpStream) -> bool {
    let mut go = [0; 2];
    stream.read_exact(&mut go).await.is_ok() && &go == b"go"
}

#[tokio::test]
async fn another_host_is_tunneled_to_itself() {
    let echo = Fake::start(b"", b"").await;
    let upstream = Fake::start(b"", b"").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    let (mut client, head) = connect(proxy.addr, &echo.addr.to_string()).await;
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );
    client.write_all(b"ping").await.unwrap();
    let mut echoed = [0; 4];
    timeout(BOUND, client.read_exact(&mut echoed))
        .await
        .expect("the echo comes back within five seconds")
        .unwrap();

    assert_eq!(&echoed, b"ping");
    assert_eq!(
        upstream.received(),
        b"",
        "the upstream received bytes of another host's tunnel"
    );
}

#[tokio::test]
async fn bytes_behind_the_head_reach_the_far_side() {
    let echo = Fake::start(b"", b"").await;
    let upstream = Fake::start(b"", b"").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    // The head and the first bytes of the tunnel in one write, as a client that does not wait for the 200 sends them.
    let target = echo.addr.to_string();
    let mut client = TcpStream::connect(proxy.addr).await.unwrap();
    client
        .write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\nping").as_bytes())
        .await
        .unwrap();
    let head = answer_head(&mut client).await;
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );
    let mut echoed = [0; 4];
    timeout(BOUND, client.read_exact(&mut echoed))
        .await
        .expect("the bytes behind the head come back within five seconds")
        .unwrap();

    assert_eq!(&echoed, b"ping");
}

#[tokio::test]
async fn bytes_pass_as_they_arrive() {
    let first_read = Arc::new(Notify::new());
    let held = Arc::clone(&first_read);
    let far = far_side(move |mut stream| {
        let held = Arc::clone(&held);
        async move {
            if !the_tunnels_connection(&mut stream).await {
                return;
            }
            stream.write_all(b"one").await.unwrap();
            held.notified().await;
            stream.write_all(b"two").await.unwrap();
        }
    })
    .await;
    let upstream = Fake::start(b"", b"").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    let (mut client, head) = connect(proxy.addr, &far.to_string()).await;
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );
    client.write_all(b"go").await.unwrap();
    let mut piece = [0; 3];
    timeout(BOUND, client.read_exact(&mut piece))
        .await
        .expect("the first piece arrives while the far side holds the second")
        .unwrap();
    assert_eq!(&piece, b"one");
    first_read.notify_one();
    timeout(BOUND, client.read_exact(&mut piece))
        .await
        .expect("the second piece arrives within five seconds")
        .unwrap();
    assert_eq!(&piece, b"two");
}

#[tokio::test]
async fn a_far_side_that_closes_ends_the_clients_side() {
    let (report, mut reports) = mpsc::unbounded_channel();
    let far = far_side(move |mut stream| {
        let report = report.clone();
        async move {
            if !the_tunnels_connection(&mut stream).await {
                return;
            }
            // The far side ends its sending and keeps reading, so what it reads next shows whether the tunnel still
            // carries the client's bytes.
            stream.shutdown().await.unwrap();
            let mut after = vec![0; 16];
            let read = stream
                .read(&mut after)
                .await
                .map(|read| after[..read].to_vec());
            let _ = report.send(read.map_err(|error| error.kind()));
        }
    })
    .await;
    let upstream = Fake::start(b"", b"").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    let (mut client, head) = connect(proxy.addr, &far.to_string()).await;
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );
    client.write_all(b"go").await.unwrap();
    let mut rest = [0; 16];
    let read = timeout(BOUND, client.read(&mut rest))
        .await
        .expect("the client's side ends within five seconds of the far side's end")
        .unwrap();
    assert_eq!(read, 0, "the client read bytes where its side should end");
    // The proxy closed this socket, so the write can land in the client's buffer or meet a reset. Either way nothing
    // of it may reach the far side.
    let _ = client.write_all(b"late").await;

    let after = timeout(BOUND, reports.recv())
        .await
        .expect("the far side's read ends within five seconds")
        .unwrap();
    assert_eq!(after, Ok(Vec::new()), "the far side read past its end");
}
