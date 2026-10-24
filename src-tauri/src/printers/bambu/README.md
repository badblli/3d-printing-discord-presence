# Bambu LAN adapter

`BambuSource` implements the shared `PrinterSource` contract. The caller supplies
the LAN host, serial, access code and optional display model. Credentials remain
in memory. There is no logger or raw-error forwarding in this adapter.

The MQTT session subscribes to the device report channel, waits for SUBACK, then
sends a status snapshot request immediately and every 60 seconds. All outbound
publishes pass through `request_snapshot`, whose topic suffix and JSON payload
are fixed. Each reconnect creates a fresh clean session; queued requests and old
telemetry are discarded. Retry delays are 2, 4, 8, 16 and at most 30 seconds.
After a session lasting at least 60 seconds, the retry delay resets to 2 seconds.
Missing telemetry clears the status and reports `Stale` after 90 seconds.

Telemetry uses monotonic time for freshness and the remaining-time countdown.
Job identity changes and terminal-to-active transitions clear the previous job's
fields. Invalid numeric values clear that field; absent fields survive deltas.
Progress is bounded to 100, remaining minutes to 100,000, layer counts to
1,000,000, nozzle temperature to 500 C and bed temperature to 200 C. Temperatures
retain fractions; progress, minutes and layers follow Python's integer truncation.
Paused statuses have no countdown; resume excludes the paused interval. Filenames
are reduced to a basename and limited to 100 characters. Display/privacy policy
belongs to the consumer of `PrintStatus`.

## TLS validation

Production trust anchors come exclusively from `src-tauri/certs/bambu.cert`.
TLS 1.2 is the only enabled protocol. A real A1 Combo has an X.509 v1 device
certificate, which webpki rejects even with hostname checks replaced. The
custom verifier parses v1 and v3 certificates with `x509-parser` and accepts a
direct issuer or at most one server-supplied intermediate. Issuer/subject names
must match byte-for-byte in DER. The rustls crypto provider checks every
certificate signature over the original TBS bytes, including the bundled CA's
own signature (or its signature by a bundled self-signed parent). It also checks
each certificate's validity interval and v3 CA basic constraints. Present CA
key usage must allow certificate signing. An unmatched issuer, unsupported
algorithm, bad signature, expired certificate or excess intermediate fails closed.

The TLS 1.2 handshake signature uses the same provider algorithms and the v1
leaf's public key; the webpki helper cannot parse that leaf. TLS 1.3 stays
disabled. Textual CN/SAN identities are compared to the configured serial.
Matching is exact, with no wildcard or substring acceptance. IP SAN entries
are transport locations, not device identities. Only when no textual device
identity exists is the name check omitted; chain and handshake signatures are
still verified. This specifically replaces Python's relaxed X.509 path while
retaining manufacturer CA trust and signature checks.

The application currently installs no logger. If application logging is added,
disable the `rumqttc` log target: that dependency's debug messages include MQTT
topics (and thus device serials). Do not enable protocol/debug tracing in probes.

## Offline fixtures and real-device probe

The JSON fixtures are synthetic representative A1 snapshot/delta packets using
the fake serial `00M00A000000000`. The public PEM files under `fixtures/tls`
and a signature over a fixed fake TLS 1.2 message were generated with OpenSSL
for these tests only. A minimal OpenSSL config plus `req -x509v1` was needed
to avoid automatic v3 SKI/AKI extensions on the v1 fixture. The temporary
private keys were deleted. These certificates are never production trust
anchors. Tests use a time inside fixture validity instead of the machine's
current date, and require no network or device.

Set `BAMBU_HOST`, `BAMBU_SERIAL` and `BAMBU_CODE` in the invoking shell without
writing them to a file, then run:

```sh
cargo run --offline --manifest-path src-tauri/Cargo.toml --example bambu_probe
```

The probe stops after two minutes, prints normalized statuses and connection
codes, and redacts filenames before output. It never prints credential values,
addresses, serials or raw protocol errors. Real A1/A1 mini/P1/X1/H2D hardware and
firmware compatibility remain the lead's manual verification step.
