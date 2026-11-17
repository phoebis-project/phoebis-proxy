//! The log, read from the standard error of the built binary, since the library has no other reader of it.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{self, Command, Stdio};
use std::sync::mpsc;
use std::thread;

use common::BOUND;

/// The built binary as a child process on `127.0.0.1:0`, with `router.example:8443` as its upstream, a name no test
/// connects to. The drop ends the process, so a failed assertion leaves none behind.
struct Child {
    process: process::Child,
    lines: mpsc::Receiver<String>,
}

impl Child {
    fn start(level: &str) -> Child {
        let mut process = Command::new(env!("CARGO_BIN_EXE_phoebis-proxy"))
            .args([
                "--listen",
                "127.0.0.1:0",
                "--upstream",
                "router.example:8443",
                "--log-level",
                level,
            ])
            // An allow list in the environment of the run would close the test's own connection.
            .env_remove("PHOEBIS_PROXY_ALLOW")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary starts");
        let stderr = process.stderr.take().unwrap();
        let (send, lines) = mpsc::channel();
        // The reads run on a thread of their own, so a child that writes no start line fails the test within the
        // bound instead of hanging it.
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if send.send(line).is_err() {
                    return;
                }
            }
        });
        Child { process, lines }
    }

    /// The listen address: the one token of the start line that reads as a socket address. The line puts a comma
    /// behind the address, so the split is at spaces and commas.
    fn listen_address(&self) -> SocketAddr {
        let line = self
            .lines
            .recv_timeout(BOUND)
            .expect("the binary writes its start line within five seconds");
        line.split([' ', ','])
            .find_map(|token| token.parse().ok())
            .unwrap_or_else(|| panic!("the start line {line:?} holds no socket address"))
    }

    /// Ends the process and answers the lines it wrote behind the start line.
    fn end(&mut self) -> String {
        self.process.kill().expect("the binary ends");
        self.process.wait().unwrap();
        self.lines.iter().collect::<Vec<_>>().join("\n")
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Writes `request` to the proxy and reads the answer up to the close behind it.
fn answer(proxy: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(proxy).unwrap();
    stream.set_read_timeout(Some(BOUND)).unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("the proxy answers and closes the connection within five seconds");
    String::from_utf8(answer).unwrap()
}

#[test]
fn the_debug_log_holds_no_part_of_a_refused_request() {
    let mut child = Child::start("debug");
    let proxy = child.listen_address();

    let answer = answer(
        proxy,
        "GET http://refused.example/albums?search=holiday-photos HTTP/1.1\r\nHost: refused.example\r\n\r\n",
    );
    assert!(
        answer.starts_with("HTTP/1.1 405 "),
        "the request answered {answer:?}"
    );
    let log = child.end();

    assert!(
        !log.contains("refused.example"),
        "the log holds the host: {log:?}"
    );
    assert!(!log.contains("/albums"), "the log holds the path: {log:?}");
    assert!(
        !log.contains("holiday-photos"),
        "the log holds the query: {log:?}"
    );
}

#[test]
fn the_info_log_holds_no_destination_of_a_failed_connect() {
    // A port the test held and closed. The IPv4 bind works on a machine without IPv6 too, where the connect to the
    // IPv6 loopback address fails as well.
    let closed = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let mut child = Child::start("info");
    let proxy = child.listen_address();

    let target = format!("[::1]:{closed}");
    let answer = answer(
        proxy,
        &format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n"),
    );
    assert!(
        answer.starts_with("HTTP/1.1 502 "),
        "the CONNECT answered {answer:?}"
    );
    let log = child.end();

    assert!(!log.contains("::1"), "the log holds the target: {log:?}");
}
