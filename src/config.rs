//! The proxy's settings, and the parse of the two values the package reads itself: a network of the allow list and
//! the upstream.

use std::fmt;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::str::FromStr;

/// A source network of the allow list, `address/prefix`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Network {
    address: IpAddr,
    prefix: u8,
}

impl FromStr for Network {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (address, prefix) = text.trim().split_once('/').ok_or_else(|| {
            "expected an address and a prefix length, such as 10.0.0.0/8".to_owned()
        })?;
        let address: IpAddr = address
            .parse()
            .map_err(|_| format!("{address} is not an IP address"))?;
        let width = if address.is_ipv4() { 32 } else { 128 };
        let prefix = prefix
            .parse::<u8>()
            .ok()
            .filter(|prefix| *prefix <= width)
            .ok_or_else(|| format!("the prefix length is a number from 0 to {width}"))?;
        Ok(Network { address, prefix })
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.address, self.prefix)
    }
}

impl Network {
    /// Whether `peer` lies in the network, an IPv4-mapped IPv6 peer read as its IPv4 address.
    pub(crate) fn contains(&self, peer: IpAddr) -> bool {
        // A prefix of 0 shifts by the whole width, which `checked_shl` answers with `None`: a mask with no bits, under
        // which the network holds each address of its family.
        match (self.address, peer.to_canonical()) {
            (IpAddr::V4(network), IpAddr::V4(peer)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                (u32::from(network) & mask) == (u32::from(peer) & mask)
            }
            (IpAddr::V6(network), IpAddr::V6(peer)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                (u128::from(network) & mask) == (u128::from(peer) & mask)
            }
            _ => false,
        }
    }
}

/// The router's feature listener, a host and a port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upstream {
    pub host: String,
    pub port: u16,
}

impl FromStr for Upstream {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (host, port) = authority(text).ok_or_else(|| {
            "expected a host and a port with no scheme and no path, such as router.example:8443"
                .to_owned()
        })?;
        Ok(Upstream {
            host: host.to_owned(),
            port,
        })
    }
}

impl fmt::Display for Upstream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

/// How much the proxy writes to standard error. `Debug` adds a line per `CONNECT`, which names the target's host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
}

/// The four settings of a start.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The source networks a client connects from. An empty list admits each source, which `Proxy::bind` takes on a
    /// loopback address alone.
    pub allow: Vec<Network>,
    pub listen: SocketAddr,
    pub log: LogLevel,
    pub upstream: Upstream,
}

/// Splits `host:port`, the form of a `CONNECT` target and of the upstream. An IPv6 address stands in brackets, and its
/// host comes back without them.
pub(crate) fn authority(text: &str) -> Option<(&str, u16)> {
    let (host, port) = text.rsplit_once(':')?;
    let port = port.parse().ok()?;
    if let Some(address) = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return address
            .parse::<Ipv6Addr>()
            .is_ok()
            .then_some((address, port));
    }
    let name = !host.is_empty()
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_'));
    name.then_some((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ipv4_network_admits_its_peer_in_the_mapped_form() {
        let network: Network = "10.0.0.0/8".parse().unwrap();
        assert!(network.contains("::ffff:10.1.2.3".parse().unwrap()));
        assert!(!network.contains("::ffff:11.0.0.1".parse().unwrap()));
    }

    #[test]
    fn a_prefix_past_the_address_width_is_refused() {
        assert!("10.0.0.0/33".parse::<Network>().is_err());
        assert!("10.0.0.0/32".parse::<Network>().is_ok());
        assert!("fd00::/129".parse::<Network>().is_err());
        assert!("fd00::/128".parse::<Network>().is_ok());
    }

    #[test]
    fn an_upstream_without_a_port_is_refused() {
        assert!("router.example".parse::<Upstream>().is_err());
        assert_eq!(
            "router.example:8443".parse(),
            Ok(Upstream {
                host: "router.example".to_owned(),
                port: 8443,
            })
        );
    }
}
