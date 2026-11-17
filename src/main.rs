use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::process::ExitCode;

use clap::builder::{PossibleValuesParser, TypedValueParser};
use clap::Parser;
use phoebis_proxy::{LogLevel, Network, Proxy, Settings, Upstream};

/// A tunnel proxy for Claude Code sessions behind Phoebis Router. It answers each CONNECT and copies bytes to the
/// target.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// The source networks a client may connect from, `address/prefix` in a comma list. A listen address beyond
    /// loopback needs one.
    #[arg(
        long,
        env = "PHOEBIS_PROXY_ALLOW",
        value_name = "NETWORKS",
        value_delimiter = ','
    )]
    allow: Vec<Network>,

    /// The address and the port the proxy listens on.
    #[arg(
        long,
        env = "PHOEBIS_PROXY_LISTEN",
        value_name = "ADDRESS",
        default_value = "127.0.0.1:8318"
    )]
    listen: SocketAddr,

    /// `info` writes the start line, and `debug` adds one line per CONNECT, which names the target's host.
    #[arg(
        long,
        env = "PHOEBIS_PROXY_LOG",
        value_name = "LEVEL",
        default_value = "info",
        value_parser = PossibleValuesParser::new(["debug", "info"])
            .map(|level| if level == "debug" { LogLevel::Debug } else { LogLevel::Info }),
    )]
    log_level: LogLevel,

    /// The host and the port of the router's feature listener, such as router.example:8443.
    #[arg(long, env = "PHOEBIS_PROXY_UPSTREAM", value_name = "HOST:PORT")]
    upstream: Upstream,
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    let stop = match stop_signal() {
        Ok(stop) => stop,
        Err(error) => {
            eprintln!("cannot wait for the stop signals: {error}");
            return ExitCode::FAILURE;
        }
    };
    let allow = if args.allow.is_empty() {
        "none".to_owned()
    } else {
        args.allow
            .iter()
            .map(Network::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    let upstream = args.upstream.to_string();
    let settings = Settings {
        allow: args.allow,
        listen: args.listen,
        log: args.log_level,
        upstream: args.upstream,
    };
    let proxy = match Proxy::bind(settings).await {
        Ok(proxy) => proxy,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!(
        "phoebis-proxy {} listening on {}, upstream {upstream}, allow list {allow}",
        env!("CARGO_PKG_VERSION"),
        proxy.local_addr()
    );
    proxy.serve(stop).await;
    ExitCode::SUCCESS
}

/// Resolves on the first interrupt, or on the first termination signal.
#[cfg(unix)]
fn stop_signal() -> io::Result<impl Future<Output = ()>> {
    use tokio::signal::unix::{signal, SignalKind};
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    Ok(async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = terminate.recv() => {}
        }
    })
}

/// Resolves on the first interrupt.
#[cfg(not(unix))]
fn stop_signal() -> io::Result<impl Future<Output = ()>> {
    Ok(async {
        // A handler that failed to register leaves no interrupt to wait for, and a stop now would end the proxy at its
        // start, so it runs until its process is ended.
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    })
}
