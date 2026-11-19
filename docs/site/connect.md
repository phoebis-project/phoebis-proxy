# What a `CONNECT` gets

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
of them. A tunnel has no idle limit, and TCP keepalive is on for both of its sockets, so a dead peer ends it. Where a
write toward one side waits five minutes without that side taking a byte, the proxy closes both sides. Each byte the
side takes starts the five minutes again, so a slow reader keeps its tunnel however long the transfer runs.

A client has 30 seconds to send its `CONNECT` head, and a head takes 8 KiB at most, its blank line included. The proxy
closes a connection that passes either bound and sends no answer.
