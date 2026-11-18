mod common;

use std::future::{pending, Future};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tokio::io::{copy, repeat, AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpSocket, TcpStream};
use tokio::sync::{mpsc, oneshot, Notify};
use tokio::task;
use tokio::time::{sleep, timeout};

use common::{answer_head, connect, settings, start, Fake, BOUND};

/// A far side of the test's own on `127.0.0.1:0`, which runs `serve` on each connection it accepts. The listener and
/// its connections live until the test's runtime ends. Each connection keeps the small receive buffer the listener
/// asks for, so while a test reads nothing there, the bytes a tunnel has on their way to it stay well below what the
/// test sends.
async fn far_side<F, S>(serve: F) -> SocketAddr
where
    F: Fn(TcpStream) -> S + Send + 'static,
    S: Future<Output = ()> + Send + 'static,
{
    let socket = TcpSocket::new_v4().unwrap();
    // An accepted connection takes its buffer size from the listener, and a size the program set keeps the system from
    // growing it.
    socket.set_recv_buffer_size(64 * 1024).unwrap();
    socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let listener = socket.listen(1024).unwrap();
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

/// `work`'s output, or `None` where `work` takes longer than `BOUND` of real time. Meanwhile the paused clock moves on
/// to the next timer each time the runtime has nothing else to do. The bound is a thread's, since a timer of the test
/// would be one more point for the clock to move on to.
async fn within<T>(work: impl Future<Output = T>) -> Option<T> {
    let (expired, expiry) = oneshot::channel::<()>();
    thread::spawn(move || {
        thread::sleep(BOUND);
        let _ = expired.send(());
    });
    tokio::select! {
        output = work => Some(output),
        _ = expiry => None,
    }
}

/// `work`'s output, or `None` where `work` takes longer than `BOUND` of real time, with the paused clock held still
/// meanwhile. The runtime moves the clock on each time it has nothing else to do, which it can find while bytes are
/// still on their way through the system's sockets, and a blocking task that runs keeps it from doing so.
async fn held<T>(work: impl Future<Output = T>) -> Option<T> {
    // The sender lives until the return, so the blocking task ends at the bound or at the return, whichever is first.
    let (_running, returned) = std::sync::mpsc::channel::<()>();
    let hold = task::spawn_blocking(move || returned.recv_timeout(BOUND));
    tokio::select! {
        output = work => Some(output),
        _ = hold => None,
    }
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

#[tokio::test(start_paused = true)]
async fn a_write_that_takes_no_byte_past_the_limit_ends_the_tunnel() {
    // Each connection stays open and unread, a stray one included, so the tunnel's write toward it stands once the
    // buffers on the way are full.
    let far = far_side(|stream| async move {
        let _kept = stream;
        pending::<()>().await;
    })
    .await;
    let upstream = Fake::start(b"", b"").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    let (client, head) = held(connect(proxy.addr, &far.to_string()))
        .await
        .expect("the proxy answers the CONNECT within five seconds");
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );
    let (mut client_read, mut client_write) = client.into_split();
    // The client writes until a write fails. The test holds no timer, so once every write stands the clock moves on to
    // the tunnel's own limit.
    tokio::spawn(async move { copy(&mut repeat(0), &mut client_write).await });

    let mut rest = [0; 1];
    let read = within(client_read.read(&mut rest))
        .await
        .expect("the client's connection ends once the tunnel's write has stood for five minutes");
    assert!(
        matches!(read, Ok(0) | Err(_)),
        "the client read {read:?} from a far side that sends nothing"
    );
}

#[tokio::test(start_paused = true)]
async fn a_tunnel_that_idles_past_the_limit_still_carries_bytes() {
    let echo = Fake::start(b"", b"").await;
    let upstream = Fake::start(b"", b"").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    let (mut client, head) = held(connect(proxy.addr, &echo.addr.to_string()))
        .await
        .expect("the proxy answers the CONNECT within five seconds");
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );
    // Ten minutes, twice the limit, with no byte in either direction.
    sleep(Duration::from_secs(10 * 60)).await;

    client.write_all(b"ping").await.unwrap();
    let mut echoed = [0; 4];
    held(client.read_exact(&mut echoed))
        .await
        .expect("the echo comes back within five seconds")
        .expect("the tunnel carries the echo back after its idle time");
    assert_eq!(&echoed, b"ping");
}

#[tokio::test(start_paused = true)]
async fn a_far_side_that_takes_its_bytes_in_steps_keeps_the_tunnel() {
    let (tunnels, mut tunnel) = mpsc::unbounded_channel();
    let far = far_side(move |mut stream| {
        let tunnels = tunnels.clone();
        async move {
            if the_tunnels_connection(&mut stream).await {
                let _ = tunnels.send(stream);
            }
        }
    })
    .await;
    let upstream = Fake::start(b"", b"").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    let (client, head) = held(connect(proxy.addr, &far.to_string()))
        .await
        .expect("the proxy answers the CONNECT within five seconds");
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );
    let (mut client_read, mut client_write) = client.into_split();
    // 32 MiB, three steps of 8 MiB and the rest. The far side's buffer and the proxy's hold less than one step, so each
    // step takes bytes the tunnel wrote after the step began. The task's output keeps the write half, since its drop
    // would close the client's sending and so end the tunnel.
    let _sending = tokio::spawn(async move {
        client_write.write_all(b"go").await?;
        copy(&mut repeat(0).take(32 * 1024 * 1024), &mut client_write).await?;
        Ok::<_, io::Error>(client_write)
    });
    let mut far = held(tunnel.recv())
        .await
        .expect("the tunnel reaches the far side within five seconds")
        .unwrap();

    // Each wait is inside the limit of five minutes, and the three pass it twice over. The proxy writes faster than the
    // far side's small buffer drains, so the tunnel's write waits by the first step at the latest, and a limit that the
    // steps' bytes do not start again passes during the third.
    let mut step = vec![0; 8 * 1024 * 1024];
    sleep(Duration::from_secs(4 * 60)).await;
    held(far.read_exact(&mut step))
        .await
        .expect("the first step arrives within five seconds")
        .expect("the tunnel carries the first step");
    sleep(Duration::from_secs(4 * 60)).await;
    held(far.read_exact(&mut step))
        .await
        .expect("the second step arrives within five seconds")
        .expect("the tunnel carries the second step");
    sleep(Duration::from_secs(4 * 60)).await;
    held(far.read_exact(&mut step))
        .await
        .expect("the third step arrives within five seconds")
        .expect("the tunnel carries the third step");
    held(far.read_exact(&mut step))
        .await
        .expect("the rest arrives within five seconds")
        .expect("the tunnel carries the rest");

    far.write_all(b"!").await.unwrap();
    let mut back = [0; 1];
    held(client_read.read_exact(&mut back))
        .await
        .expect("the far side's byte arrives within five seconds")
        .expect("the tunnel still carries a byte back");
    assert_eq!(&back, b"!");
}
