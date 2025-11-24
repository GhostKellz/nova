# Support Bundle Walkthrough

This example demonstrates how to generate, validate, and share a Nova RC5 support bundle using the new CLI enhancements.

## 1. Generate the bundle

```bash
nova support bundle \
  --output ./artifacts/nova-support-prod-$(date +%Y%m%d%H%M).tar.gz \
  --since 24h \
  --include network,metrics \
  --scrub
```

- `--since 24h` limits the log horizon.
- `--include network,metrics` adds the RC5 collectors for bridge health and exporter status.
- `--scrub` redacts secrets, tokens, MAC/IP addresses.

## 2. Verify the bundle contents

```bash
nova support bundle validate ./artifacts/nova-support-prod-20250925.tar.gz
```

Expected output:

```
✔ Archive digest: sha256:7f6c7b1c...
✔ Redaction profile: strict
✔ Collectors: core, network, metrics
✔ Size within limits (23 MB)
```

## 3. Extract for local analysis

```bash
mkdir -p /tmp/nova-bundle && \
  tar -xzf ./artifacts/nova-support-prod-20250925.tar.gz -C /tmp/nova-bundle
ls /tmp/nova-bundle
```

Key directories:

- `logs/` – Nova, exporter, libvirt, container runtimes.
- `nova/` – Nova configuration snapshot plus `nova/gpu-capabilities.json` with per-GPU generation, minimum driver, kernel, and TCC requirements.
- `nova/observability/` – Snapshot of high-value gauges and counters when `--include metrics` is used.
- `network/` – Output of `nova network status`, bridge memberships, interface stats.
- `metadata.json` – Manifest with Nova version, collectors, checksum, timestamp.

## 4. Share securely

Upload the bundle to the shared support bucket (example):

```bash
aws s3 cp ./artifacts/nova-support-prod-20250925.tar.gz \
  s3://nova-support-dropbox/prod/ --sse AES256
```

Add the checksum and S3 URL to the GitHub issue using the support template.

## 5. Clean up

```bash
rm -rf /tmp/nova-bundle
find ./artifacts -type f -mtime +14 -delete
```

The lifecycle policy removes bundles older than 14 days from the S3 bucket automatically.
