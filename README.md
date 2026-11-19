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

A start needs the upstream alone: `phoebis-proxy --upstream router.example:8443` listens on `127.0.0.1:8318` and serves
the users of its own machine.

The documentation lives under [`docs/site/`](docs/site/index.md), starting with [Install](docs/site/install.md), and is
published at <https://phoebis-project.github.io/phoebis-docs/> once GitHub Pages is enabled for the documentation site.

## License

Released under the MIT license. See [`LICENSE`](LICENSE) for the full text.
