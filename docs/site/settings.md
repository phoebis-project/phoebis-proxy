# Settings

The proxy has four settings and no configuration file or data directory. Each setting is a flag or a variable of the
environment, and a flag wins over its variable.

| Setting | Flag | Variable | Default |
| --- | --- | --- | --- |
| The allow list of source networks | `--allow` | `PHOEBIS_PROXY_ALLOW` | none, loopback binds only |
| The listen address | `--listen` | `PHOEBIS_PROXY_LISTEN` | `127.0.0.1:8318` |
| The log level | `--log-level` | `PHOEBIS_PROXY_LOG` | `info` |
| The upstream, the host and the port of the router's feature listener | `--upstream` | `PHOEBIS_PROXY_UPSTREAM` | none, required |

**The allow list** is a comma list of networks, each as `address/prefix` in IPv4 or IPv6, such as
`192.0.2.0/24,2001:db8::/32`. Where a list is set, on a loopback address too, the proxy serves a connection from a
listed network alone. It closes each other connection before it reads a byte and sends no answer.

**The listen address** is an IP address with a port.

**The log level** is `debug` or `info`, and [Logs](logs.md) names the lines of each.

**The upstream** is a host and a port with no scheme and no path, such as `router.example:8443`. The host is a name or
an IP address, and an IPv6 address stands in brackets. The proxy resolves a name at each `CONNECT` it sends there, so a
changed address needs no restart.

The proxy refuses to start, with the reason on standard error, on an upstream that is no host with a port and on a
listen address beyond loopback without an allow list. The proxy tunnels each host, so on an address that other machines
reach it would be an open forward proxy for whoever reaches its port.
