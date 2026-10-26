# Tailscale certificate failure capture (2165)

Captured on 2026-10-23 from this Mac, without changing tailnet settings.
`status.json` retains only BackendState, Self.DNSName, and CertDomains from
`TAILSCALE_BE_CLI=1 /Applications/Tailscale.app/Contents/MacOS/Tailscale status --json`.
`bundle-error.txt` records exit status and output from `cert --cert-file=<Gits temp path>
--key-file=<Gits temp path> <FQDN>` on that binary (version 1.104.1).

The app's signed entitlements include `com.apple.security.app-sandbox=true`,
Downloads read/write, user-selected read/write, and a home-relative `.kube`
exception, but no access to the caller's Gits temp directory. The same request
with `/opt/homebrew/bin/tailscale` (1.102.4) wrote both files successfully.
Both files were deleted after the probe. No private key is retained here.

The app CLI successfully returned certificate-only and key-only PEM using
`cert --cert-file=- <FQDN>` and `cert --key-file=- <FQDN>`. No PEM payload is
retained. This checks the actual installed CLI's stdout behavior, not a guessed
protocol. Separate requests suppress the other output file.

References:
- https://tailscale.com/docs/reference/tailscale-cli#cert
- https://raw.githubusercontent.com/tailscale/tailscale/main/cmd/tailscale/cli/cert.go

The upstream CLI source documents the macOS sandbox warning and implements
`-` as stdout. Certificate availability in `CertDomains` is not proof that a
consumer has loaded a certificate or bound a TLS listener.
