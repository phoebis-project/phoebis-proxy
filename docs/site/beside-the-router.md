# Beside Phoebis Router

The proxy runs on the client's machine, on another machine, or beside the router in one compose project.
[`deploy/compose.yaml`](https://github.com/phoebis-project/phoebis-proxy/blob/main/deploy/compose.yaml) is an example of
the last, with the router and the proxy as two services under bridge networking. The router's service is the one the
router's repository ships in its own `deploy/compose.yaml`, with the same ports.

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
[claude.ai features](../router/claude-ai-features.md) page of the router's documentation holds the client's whole setup,
of which that variable is one line. The same page covers the router's certificate authority, which the client has to
trust.

Between a client and a proxy on another machine the `CONNECT` lines cross the network in the clear: the host name and
the port of each HTTPS call that the client sends through the proxy. The router key of a feature call crosses it inside
the tunnel, where the client opens TLS and the router's feature listener answers it. The proxy cannot read that stream,
so the key stays inside the client's TLS up to the router. In the client's setup a model request goes to the router's
base URL and does not pass the proxy, so the proxy adds nothing to the key's protection there.
