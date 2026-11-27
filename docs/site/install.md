# Install

Each release comes as five archives with one `SHA256SUMS` file, and as an image. Its tag is the package's version with
no `v` in front, such as `0.1.0`. The [changelog](changelog.md) says what each release holds.

## From a release archive

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

## From the image

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
[Beside Phoebis Router](beside-the-router.md) says which network the allow list names behind a published port.
`phoebis:8443` stands for the host and the port of the router's feature listener. On Docker's default network the name
`phoebis` does not resolve to the router's container, so the proxy starts and each `CONNECT` for a feature host gets the
502. A run therefore names the router's host and port as the container reaches them, or it starts the container on the
router's network with `--network`, where the service's name resolves.
