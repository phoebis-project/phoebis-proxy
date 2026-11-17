mod common;

use std::io::ErrorKind;

use phoebis_proxy::{Proxy, Settings, StartError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use common::{connect, settings, start, Fake, BOUND};

/// Reads once under the bound and says whether the connection ended: a read of 0 bytes or a reset, since Linux
/// answers a close with unread bytes by a reset. A byte read means the proxy answered.
async fn ended(client: &mut TcpStream) -> Result<(), String> {
    let mut answer = [0; 64];
    match timeout(BOUND, client.read(&mut answer)).await {
        Err(_) => Err("the connection stayed open for five seconds".to_owned()),
        Ok(Ok(0)) => Ok(()),
        Ok(Err(error)) if error.kind() == ErrorKind::ConnectionReset => Ok(()),
        Ok(Ok(read)) => Err(format!(
            "the proxy answered {:?}",
            String::from_utf8_lossy(&answer[..read])
        )),
        Ok(Err(error)) => Err(format!("the read failed with {error}")),
    }
}

#[tokio::test]
async fn a_head_past_the_bound_ends_the_connection() {
    let far = Fake::start(b"", b"").await;
    let proxy = start(settings(&[], far.addr)).await;

    let mut endless = TcpStream::connect(proxy.addr).await.unwrap();
    endless.write_all(&[b'a'; 9216]).await.unwrap();
    assert_eq!(ended(&mut endless).await, Ok(()));

    let (_client, head) = connect(proxy.addr, &far.addr.to_string()).await;
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );
}

#[tokio::test]
async fn the_allow_list_holds() {
    let far = Fake::start(b"", b"").await;

    let elsewhere = start(settings(&["10.0.0.0/8"], far.addr)).await;
    let mut outside = TcpStream::connect(elsewhere.addr).await.unwrap();
    assert_eq!(
        ended(&mut outside).await,
        Ok(()),
        "a source off the list was served"
    );

    let loopback = start(settings(&["127.0.0.0/8"], far.addr)).await;
    let (_client, head) = connect(loopback.addr, &far.addr.to_string()).await;
    assert!(
        head.starts_with("HTTP/1.1 200 "),
        "the CONNECT answered {head:?}"
    );

    let open = Settings {
        listen: "0.0.0.0:0".parse().unwrap(),
        ..settings(&[], far.addr)
    };
    match Proxy::bind(open).await {
        Err(StartError::OpenBind(listen)) => assert_eq!(listen.to_string(), "0.0.0.0:0"),
        Err(error) => panic!("the open bind failed for another reason: {error}"),
        Ok(proxy) => panic!("the proxy bound {} with no allow list", proxy.local_addr()),
    }
}
