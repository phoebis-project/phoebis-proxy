# Development

## Releasing

A release publishes five archives with one `SHA256SUMS` file, and an image for two platforms. The `Release` workflow
runs on a published release, on a manual dispatch, and on a pull request that touches one of its six inputs:
`.dockerignore`, `.github/workflows/release.yml`, `Cargo.lock`, `Cargo.toml`, `Dockerfile` and `rust-toolchain.toml`. It
runs on no push, so a change to `src/` gets its release build with the next dry run or release, and the `Tests` workflow
covers it until then. To release:

1. On the day of the release, resolve the Dockerfile's one pinned digest again, the runtime base's, under the tag that
   the comment above its `FROM` line names. `docker buildx imagetools inspect <tag>` prints the tag's current digest.
   Where the tag moved, write the new digest into the `FROM` line and the day into the comment above it, and commit the
   change to `main`. A release on a later day than its dry run resolves the digest again, and a tag that moved by then
   takes a new commit and a new dry run. A base update reaches a published image with the next release and no earlier,
   and a version tag stays on the one build that its release made.
2. Set `version` under `[package]` in `Cargo.toml` to the version (`X.Y.Z`, or `X.Y.Z-rc.1` for a prerelease), rename
   the changelog's `## Unreleased` heading to `## X.Y.Z`, or write that section where no change opened one, and merge to
   `main`. The changelog's heading and the release's tag are the version exactly as this step sets it, a suffix
   included: `## X.Y.Z-rc.1` and the tag `X.Y.Z-rc.1` for a prerelease. A version with a suffix is a prerelease: its
   image takes none of the tags `X.Y`, `X` and `latest`, and step 3 creates its release as a prerelease. The version
   change moves `Cargo.lock` too: run `cargo update --workspace` to write it and commit both files, since each build
   runs with `--locked`.
3. Run the manual dispatch as the dry run on the head of `main` once the changes of steps 1 and 2 are on it, also for a
   release whose step 1 moved no pin. Then create the release on GitHub with the tag `X.Y.Z`, no `v` prefix, at the
   commit that the dry run proved, with the version as its title and a link to the [changelog](changelog.md) page in its
   notes. Create the release of a version with a suffix as a prerelease. Publishing it starts the workflow.
4. On each event the guard refuses a package version that is not `X.Y.Z` or `X.Y.Z-suffix`
   (`Cargo.toml states a package version that is not X.Y.Z or X.Y.Z-suffix`). On a pull request and on a dry run it
   refuses a changelog with no section for that version
   (`docs/site/changelog.md has no '## {version}' section for the package version`), so the dry run of step 3 shows a
   missed heading before the release exists. On a release it refuses the release before anything builds when the tag has
   another form (`release tag '{TAG}' is not X.Y.Z or X.Y.Z-suffix`), differs from the package version
   (`release tag '{TAG}' differs from the package version '{version}' in Cargo.toml`), or has no section in the
   changelog (`docs/site/changelog.md has no '## {TAG}' section for this release`). Otherwise the run builds two images,
   `linux/amd64` on `ubuntu-24.04` and `linux/arm64` on `ubuntu-24.04-arm`, and pushes each by digest. Each image runs
   through `--version`, which must print `phoebis-proxy {VERSION}`. The run builds five archives, each on a runner of
   its own platform, for `aarch64-apple-darwin`, `aarch64-unknown-linux-musl`, `x86_64-apple-darwin`,
   `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-musl`. Each binary runs through `--version` as the images do, and
   the two Linux binaries pass a static link check. An archive holds the binary and the license, as
   `phoebis-proxy-{VERSION}-{TARGET}.tar.gz`, or `.zip` for Windows. Once each image and each archive has passed, the
   run publishes one manifest list at `ghcr.io/phoebis-project/phoebis-proxy` under `X.Y.Z`, `X.Y`, `X` (from major 1),
   `latest` and `sha-<short sha>`, with none of `X.Y`, `X` and `latest` for a prerelease. It checks that the list holds
   amd64 and arm64, and uploads the five archives with `SHA256SUMS` to the release.

A manual dispatch (Actions, Release, Run workflow) is the dry run: it builds and smokes each target and both platforms
and pushes nothing. Run it on `main` after a change to a packaging input and ahead of each release. A release whose run
fails for a reason in its commit is deleted with its tag and created again at the corrected commit behind a green dry
run, as long as the run published no image tag and no archive under the version. Once an image tag or an archive is
public, the correction ships as the next patch version. A change that an operator sees writes its entry under
`## Unreleased`, the first section of the [changelog](changelog.md), and step 2 renames that heading to the version.
