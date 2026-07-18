# Releasing

This workspace publishes three crates in dependency order:

1. `vsomeip-proc-macro`
2. `vsomeip-sys`
3. `up-transport-vsomeip`

Publishing uses crates.io Trusted Publishing from
`.github/workflows/release.yaml`. No long-lived crates.io token is stored in
this repository.

## One-Time Setup

Configure a GitHub Trusted Publisher in the crates.io settings for each crate:

| Field             | Value                       |
| ----------------- | --------------------------- |
| Repository owner  | `eclipse-uprotocol`         |
| Repository name   | `up-transport-vsomeip-rust` |
| Workflow filename | `release.yaml`              |
| Environment       | Leave blank                 |

After configuring all three crates, manually dispatch the **Publish to
crates.io** workflow from `main`. The dispatch only exchanges and revokes a
short-lived token; it cannot run a publish command.

## Prepare a Release

1. Update the shared workspace version in `Cargo.toml`.
2. Update the version requirements of `vsomeip-proc-macro` and `vsomeip-sys` to
   the new release line.
3. Run the project validation commands:

   ```bash
   source build/envsetup.sh highest
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -W warnings -D warnings
   cargo test -- --test-threads 1
   cargo package --list -p vsomeip-proc-macro
   cargo package --list -p vsomeip-sys
   cargo package --list -p up-transport-vsomeip
   cargo package -p vsomeip-proc-macro
   cargo package -p vsomeip-sys
   ```

4. Merge the reviewed release PR and wait for `main` CI to pass.
5. Create an annotated `vMAJOR.MINOR.PATCH` tag at that exact commit and push
   only the tag.

The workflow validates the tag, confirms the commit is reachable from `main`,
runs the bundled and unbundled workflows, verifies each package, and publishes
the crates in dependency order. A manual workflow dispatch never publishes.

## Partial Releases

crates.io versions are immutable. A rerun from the same tag checks for each
crate version and skips stages already present on crates.io. Do not move a
release tag after any crate has been published.

If published contents are incorrect, yank the affected version where
appropriate and prepare a new patch release. Never attempt to overwrite a
published version or bypass failed package verification.

## Verify a Release

1. Confirm all three crate versions are present and not yanked on crates.io.
2. Confirm `up-transport-vsomeip` has the intended `up-rust` and internal crate
   requirements.
3. Check a clean downstream project using crates.io dependencies without path
   overrides.
4. Create the GitHub prerelease from the existing tag with compatibility and
   MSRV notes.
5. After the first successful OIDC release, enable trusted-publishing-only for
   all three crates.
