mod common;

use std::net::SocketAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

use common::{connect, settings, start, Fake, BOUND};

/// Opens a tunnel for `target` and writes `hello` into it. The upstream of these tests answers `world` once it read
/// `hello`, so the `world` read back shows that the tunnel ends at the upstream.
async fn reaches_the_upstream(proxy: SocketAddr, target: &str) {
    let (mut client, head) = connect(proxy, target).await;
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT {target} answered {head:?}"
    );
    client.write_all(b"hello").await.unwrap();
    let mut answer = [0; 5];
    timeout(BOUND, client.read_exact(&mut answer))
        .await
        .expect("the upstream's answer comes back within five seconds")
        .unwrap();
    assert_eq!(
        &answer, b"world",
        "the tunnel of {target} read another answer"
    );
}

#[tokio::test]
async fn a_feature_hosts_connect_reaches_the_upstream() {
    let upstream = Fake::start(b"hello", b"world").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    reaches_the_upstream(proxy.addr, "api.anthropic.com:443").await;
    reaches_the_upstream(proxy.addr, "mcp-proxy.anthropic.com:443").await;
}

#[tokio::test]
async fn a_feature_host_in_another_spelling_still_reaches_the_upstream() {
    let upstream = Fake::start(b"hello", b"world").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    reaches_the_upstream(proxy.addr, "API.Anthropic.com.:443").await;
}

#[tokio::test]
async fn an_upstream_that_refuses_the_connection_answers_the_connect() {
    let closed = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap();
    let proxy = start(settings(&[], closed)).await;

    let (mut client, head) = connect(proxy.addr, "api.anthropic.com:443").await;
    assert!(
        head.starts_with("HTTP/1.1 502 "),
        "the CONNECT answered {head:?}"
    );
    let mut rest = [0; 16];
    let read = timeout(BOUND, client.read(&mut rest))
        .await
        .expect("the connection ends within five seconds of the 502")
        .unwrap();
    assert_eq!(read, 0, "the proxy sent bytes behind its 502");
}

#[tokio::test]
async fn a_connect_for_a_host_with_an_empty_label_gets_the_400() {
    let upstream = Fake::start(b"hello", b"world").await;
    let proxy = start(settings(&[], upstream.addr)).await;

    let (_client, head) = connect(proxy.addr, "host.invalid..:443").await;
    assert!(
        head.starts_with("HTTP/1.1 400 "),
        "the CONNECT answered {head:?}"
    );
}
