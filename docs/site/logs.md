# Logs

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
