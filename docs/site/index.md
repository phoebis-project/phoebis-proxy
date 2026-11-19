# Phoebis Proxy

Phoebis Proxy is a tunnel proxy for Claude Code sessions that reach Claude through [Phoebis Router](../router/index.md).
Claude Code sends the calls of two claude.ai features, Artifacts and the connectors, to two fixed hosts,
`api.anthropic.com` and `mcp-proxy.anthropic.com`, whatever `ANTHROPIC_BASE_URL` names. The proxy answers the client's
`CONNECT` requests and ends a tunnel for one of the two hosts at the router, which serves the feature calls under an
account the operator selected. A tunnel for any other host ends at that host, so traffic to other hosts does not pass
the router.

The proxy holds no router key, no account and no certificate authority. It ends no TLS and reads no request, so the
client's TLS session ends at the router and the router key stays inside it.

[Install](install.md) covers the release archives and the image, and [Settings](settings.md) the four settings. [What a
`CONNECT` gets](connect.md) says where each tunnel ends and what the proxy answers, [Beside Phoebis
Router](beside-the-router.md) how the proxy runs with the router in one compose project, and [Logs](logs.md) what the
proxy writes at each level.

## Limits

- The proxy chains to no further proxy: it connects to each destination directly, whatever proxy variables its own
  environment holds.
- The proxy has no proxy authentication, so the allow list and the listen address alone decide who can use it.
- The proxy serves plain HTTP on its own address, so a client names it with an `http://` address and the `CONNECT` lines
  cross the network to it in the clear.
