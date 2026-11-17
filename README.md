<!-- markdownlint-disable-file MD033 MD041 -->
<div align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo-light.svg" alt="Phoebis Proxy" width="360">
  </picture>
</div>

Phoebis Proxy is a tunnel proxy for Claude Code sessions that reach Claude through
[Phoebis Router](https://github.com/phoebis-project/phoebis). Claude Code sends the calls of two claude.ai features,
Artifacts and the connectors, to two fixed hosts, `api.anthropic.com` and `mcp-proxy.anthropic.com`, whatever
`ANTHROPIC_BASE_URL` names. The proxy answers the client's `CONNECT` requests and ends a tunnel for one of the two hosts
at the router, which serves the feature calls under an account the operator selected. A tunnel for any other host ends
at that host, so traffic to other hosts does not pass the router.

The proxy holds no router key, no account and no certificate authority. It ends no TLS and reads no request, so the
client's TLS session ends at the router and the router key stays inside it.

## Install

Each release comes as five archives with one `SHA256SUMS` file, and as an image. Its tag is the package's version with
no `v` in front, such as `0.1.0`.

### From a release archive

The [releases page](https://github.com/phoebis-project/phoebis-proxy/releases) holds one archive per platform:

| Platform | Archive |
| --- | --- |
| Linux on aarch64 | `phoebis-proxy-<version>-aarch64-unknown-linux-musl.tar.gz` |
| Linux on x86_64 | `phoebis-proxy-<version>-x86_64-unknown-linux-musl.tar.gz` |
| macOS on aarch64 (Apple silicon) | `phoebis-proxy-<version>-aarch64-apple-darwin.tar.gz` |
| macOS on x86_64 (Intel) | `phoebis-proxy-<version>-x86_64-apple-darwin.tar.gz` |
| Windows on x86_64 | `phoebis-proxy-<version>-x86_64-pc-windows-msvc.zip` |

An archive holds one directory, named as the archive without its extension, with `LICENSE` and the binary in it:
`phoebis-proxy`, or `phoebis-proxy.exe` in the Windows archive. The two Linux binaries are statically linked.
`SHA256SUMS` holds the SHA-256 sum of each archive. On Linux on x86_64, with the archive and `SHA256SUMS` in the current
directory:

```sh
sha256sum --check --ignore-missing SHA256SUMS
tar -xzf phoebis-proxy-0.1.0-x86_64-unknown-linux-musl.tar.gz
./phoebis-proxy-0.1.0-x86_64-unknown-linux-musl/phoebis-proxy --version
```

The last line prints `phoebis-proxy 0.1.0`. A start needs the upstream alone:
`phoebis-proxy --upstream router.example:8443` listens on `127.0.0.1:8318` and serves the users of its own machine.

### From the image

The image is `ghcr.io/phoebis-project/phoebis-proxy`, built for `linux/amd64` and `linux/arm64` and tagged with the
release's version. It holds the static binary on a base without a shell, runs as uid 65532 and declares no volume. The
binary is the entry point, so each argument behind the image's name is an argument of the proxy.

The image sets `PHOEBIS_PROXY_LISTEN` to `0.0.0.0:8318`, since a container's loopback is its own. That address lies
beyond loopback, so a container starts only with an allow list. A run sets the upstream and the allow list and publishes
the port:

```sh
docker run \
  --env PHOEBIS_PROXY_ALLOW=192.0.2.0/24 \
  --env PHOEBIS_PROXY_UPSTREAM=phoebis:8443 \
  --publish 8318:8318 \
  ghcr.io/phoebis-project/phoebis-proxy:0.1.0
```

The two values are those of the stack example. `192.0.2.0/24` stands for the network the connections arrive from, and
[Beside Phoebis Router](#beside-phoebis-router) says which network the allow list names behind a published port.
`phoebis:8443` stands for the host and the port of the router's feature listener. On Docker's default network the name
`phoebis` does not resolve to the router's container, so the proxy starts and each `CONNECT` for a feature host gets the
502. A run therefore names the router's host and port as the container reaches them, or it starts the container on the
router's network with `--network`, where the service's name resolves.

## Settings

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

**The log level** is `debug` or `info`, and [Logs](#logs) names the lines of each.

**The upstream** is a host and a port with no scheme and no path, such as `router.example:8443`. The host is a name or
an IP address, and an IPv6 address stands in brackets. The proxy resolves a name at each `CONNECT` it sends there, so a
changed address needs no restart.

The proxy refuses to start, with the reason on standard error, on an upstream that is no host with a port and on a
listen address beyond loopback without an allow list. The proxy tunnels each host, so on an address that other machines
reach it would be an open forward proxy for whoever reaches its port.

## What a `CONNECT` gets

The proxy serves one kind of request, `CONNECT`, over plain HTTP. It reads the request line
`CONNECT <host>:<port> HTTP/1.1` and the head up to its blank line, and the target decides where the bytes go:

| Target | Destination |
| --- | --- |
| `api.anthropic.com` or `mcp-proxy.anthropic.com` on port 443 | the upstream, which is the router's feature listener |
| each other target, the two hosts on another port among them | the target itself |

The proxy compares the target's host with the two names without case and without a trailing dot. The two names are fixed
in the program and are no setting, since the router's certificate authority is bound to the same two names. A `CONNECT`
for one of the two on port 443 goes to the upstream and to no other destination: a tunnel to the real host would let the
client finish its TLS there and send it the router key. The proxy's own machine resolves the name of each other target.

A request gets one of four answers:

| Answer | When |
| --- | --- |
| `200 Connection established` | The TCP connection to the destination stands. The tunnel follows. |
| `400 Bad Request` | The proxy cannot read the request line, or a `CONNECT` names a target that is no host with a port or a version other than HTTP/1.x. |
| `405 Method Not Allowed` | The request has a method other than `CONNECT`, a plain HTTP request in absolute form among them. The answer carries `Allow: CONNECT`. |
| `502 Bad Gateway` | The connection to the destination fails or takes longer than ten seconds, the lookup of its name included. |

Each of 400, 405 and 502 has an empty body, and the proxy closes the connection behind it. A 502 for a feature host
means that the proxy cannot reach the router.

Behind a 200 the proxy copies bytes in both directions until one side closes, and then closes the other. It reads none
of them. A tunnel has no idle limit, and TCP keepalive is on for both of its sockets, so a dead peer ends it.

A client has 30 seconds to send its `CONNECT` head, and a head takes 8 KiB at most, its blank line included. The proxy
closes a connection that passes either bound and sends no answer.

## Beside Phoebis Router

The proxy runs on the client's machine, on another machine, or beside the router in one compose project.
[`deploy/compose.yaml`](deploy/compose.yaml) is an example of the last, with the router and the proxy as two services
under bridge networking. The router's service is the one the router's repository ships in its own `deploy/compose.yaml`,
with the same ports.

The router's mounted `config.yaml` holds the `features` block, which starts the router's feature listener on port 8443:

```yaml
features:
  listen: 0.0.0.0:8443
```

The example does not publish port 8443. The proxy reaches the router's feature listener inside the compose network by
the service name, so its upstream is `phoebis:8443`. It resolves that name at each `CONNECT`, so the two services need
no start order, and the proxy needs no restart when the router's container changes its address. The proxy's own port,
8318, is published to the network the clients come from.

The proxy tunnels each host for whoever reaches its port, so a published port takes two guards together.

**The allow list** names the network a connection arrives from as the proxy's listener sees it. A rootful Docker Engine
on Linux maps a published port by NAT rules, so a client on another machine arrives under its own address and the list
names the clients' network, as `192.0.2.0/24` does in the example. A connection that the engine's userland proxy carries
arrives from the compose network's gateway. So does a connection from the Docker host itself to the port on its
loopback, and each connection under a rootless engine whose port driver passes no source address on. For those the list
names the compose network's subnet. With `PHOEBIS_PROXY_LOG=debug` the proxy logs the source of each connection the
allow list closed, which is the address the list has to name.

**The host's side of the published port** stays on loopback for a proxy that serves its own machine alone:
`127.0.0.1:8318:8318` in place of `8318:8318`.

A client's user sets `HTTPS_PROXY` to the proxy's published address, and the
[claude.ai features](https://phoebis-project.github.io/phoebis/claude-ai-features/) page of the router's documentation
holds the client's whole setup, of which that variable is one line. The same page covers the router's certificate
authority, which the client has to trust.

Between a client and a proxy on another machine the `CONNECT` lines cross the network in the clear: the host name and
the port of each HTTPS call that the client sends through the proxy. The router key of a feature call crosses it inside
the tunnel, where the client opens TLS and the router's feature listener answers it. The proxy cannot read that stream,
so the key stays inside the client's TLS up to the router. In the client's setup a model request goes to the router's
base URL and does not pass the proxy, so the proxy adds nothing to the key's protection there.

## Logs

The proxy logs to standard error. At `info`, the default, it writes three lines:

| Line | When |
| --- | --- |
| `accepting a connection failed, the next attempt follows a pause: <error>` | The proxy's listener cannot accept a connection. The proxy waits a second and accepts again. |
| `cannot reach the upstream <host>:<port>: <error>` | A connect to the upstream fails. The line names no target. |
| `phoebis-proxy <version> listening on <address>:<port>, upstream <host>:<port>, allow list <networks or none>` | The proxy starts. |

At `debug` it adds the lines of a `CONNECT`, of a connection the allow list closed and of a refused request:

| Line | When |
| --- | --- |
| `closed a connection from <address>:<port>, outside the allow list` | The allow list closes a connection. The address is the source as the proxy's listener sees it. |
| `CONNECT <target> to <destination>: 200, the tunnel closed after <duration>` | A tunnel closes. |
| `CONNECT <target> to <destination>: 502 after <duration> (<error>)` | The proxy cannot reach the destination of a `CONNECT`. |
| `refused a request with <status>` | The proxy answers a request with 400 or 405. The line holds the status alone. |

A `CONNECT` line names the target and where its bytes went, the upstream or the target itself. It names a host that a
user's programs call, which is why it stays at `debug`. A `CONNECT` whose target the proxy cannot parse writes the line
of a refused request in its place, and a head that does not arrive whole within its bounds and a 200 that the proxy
cannot write leave no line.

No line holds a method, a path, a header or a body of a request. Inside a tunnel the proxy copies bytes and reads none
of them, and for a request it refuses it logs the status of its answer alone.

A start that fails writes its reason and no start line.

## Limits

- The proxy chains to no further proxy: it connects to each destination directly, whatever proxy variables its own
  environment holds.
- The proxy has no proxy authentication, so the allow list and the listen address alone decide who can use it.
- The proxy serves plain HTTP on its own address, so a client names it with an `http://` address and the `CONNECT` lines
  cross the network to it in the clear.

## License

Released under the MIT license. See [`LICENSE`](LICENSE) for the full text.
