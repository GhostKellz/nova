# Observability Stack · RC5

Nova RC5 introduces a first-class observability suite centered around a Prometheus-compatible exporter, curated Grafana dashboards, and alerting primers that ops teams can drop into existing monitoring stacks.

## Exporter Overview

- **Binary**: `nova-metrics` (integrated into the main Nova binary behind a config flag).
- **Endpoint**: `http://<host>:9640/metrics` by default.
- **Protocols**: Plain HTTP with optional TLS termination handled by a reverse proxy.
- **Availability**: Linux (Arch, Ubuntu) with integration smoke-tests gating every merge.

### Enabling the exporter

```toml
# NovaFile (excerpt)
[metrics]
# Enable the Prometheus scrape endpoint
exporter = true
# Optional bind address (defaults to 0.0.0.0)
listen = "0.0.0.0:9640"
# Toggle collection for experimental probes
collect_libvirt_stats = true
collect_container_stats = true
```

After updating the NovaFile, reload the daemon:

```bash
# Restart Nova services to pick up the metrics listener
systemctl restart nova-metrics.service
```

### Authentication & Hardening

1. Restrict access to the `/metrics` endpoint via network policy or a sidecar proxy (e.g., Caddy/NGINX with basic auth).
2. Regenerate the exporter token (`nova metrics rotate-token`) during RC5 rollout and store it in the secret manager of choice.
3. Use the sample namespace in `examples/prometheus-scrape.yml` to isolate Nova metrics from existing jobs.

## Metrics Catalog

| Metric | Type | Description |
| ------ | ---- | ----------- |
| `nova_vm_status` | gauge | `1` when a VM is running, `0` when stopped; labelled with `name`, `host`. |
| `nova_vm_cpu_seconds_total` | counter | CPU seconds consumed by a VM, reset on reboot. |
| `nova_container_restart_total` | counter | Restart count per container (labels: `name`, `runtime`). |
| `nova_host_memory_bytes` | gauge | Total host memory visible to Nova; complements system exporters. |
| `nova_network_rx_bytes_total` | counter | Cumulative RX bytes per virtual switch/interface (labels: `switch`, `interface`). |
| `nova_support_bundle_generated_total` | counter | Number of support bundles created (labels: `mode`, `redaction`). |

> **Tip**: Extend the catalog by tagging new metrics with the `RC5` label in code reviews; the lint pipeline blocks undocumented metrics.

## Grafana Dashboards

Clone the starter dashboards from `examples/grafana-nova.json` and import them into your Grafana instance:

1. Navigate to **Dashboards → Import**.
2. Upload the JSON file or paste its contents.
3. Set the Prometheus data source that scrapes the Nova exporter.
4. Adjust panel thresholds to match your environment (defaults mirror the RC5 advisory group).

### Dashboard Highlights

- **Overview**: VM/container counts, health spark-lines, platform alerts.
- **VM Deep Dive**: Top CPU/memory consumers, lifecycle state transitions, event heatmap.
- **Networking**: Bridge throughput, error/drops trend, interface availability heatmap.
- **Support Ops**: Support bundle generation rate, error glossary hits, docs search success.

## Alerting Primer

| Alert | Expression | Default Severity | Notes |
| ----- | ---------- | ---------------- | ----- |
| `NovaExporterDown` | `absent(nova_vm_status)` | Critical | Triggers when the exporter disappears from the scrape target. |
| `NovaVmUnresponsive` | `max_over_time(nova_vm_status{state="running"}[15m]) == 0` | High | Detects VMs that flapped or stalled for 15+ minutes. |
| `NovaNetworkDrops` | `increase(nova_network_rx_drops_total[10m]) > 0` | Medium | Surface packet drops for follow-up in the networking UI. |
| `NovaDocsSearchFailure` | `avg_over_time(nova_docs_search_failure_ratio[30m]) > 0.3` | Low | Signals docs discoverability issues after releases. |

Drop the canned alerts from `examples/alert-rules.yml` into your existing Alertmanager infrastructure and tune the annotations before go-live.

## Operational Playbook

1. **Provisioning**: Deploy the exporter alongside Nova services using the provided systemd unit (`examples/systemd/nova-metrics.service`).
2. **Scraping**: Import the scrape job stub (`examples/prometheus-scrape.yml`) and validate using `promtool check config`.
3. **Dashboards**: Import dashboards, wire data source, validate with sample environment, and snapshot into `docs/assets/` for release notes.
4. **Alerting**: Apply Alertmanager rules, configure notification routes (Slack/email), and dry-run with test alerts.
5. **Telemetry Feedback**: Monitor `nova_docs_search_failure_ratio` after docs pushes to keep the portal content aligned with user demand.

## Troubleshooting

| Symptom | Mitigation |
| ------- | ---------- |
| Exporter returns `404` | Verify the `metrics.exporter` flag, restart Nova services, confirm port binding with `ss -ltnp`. |
| Prometheus scrape timeout | Reduce `collect_libvirt_stats`, ensure host has libvirt sockets reachable, check host load. |
| Grafana panels empty | Confirm datasource selection, refresh dashboard variables, and inspect `nova_exporter_build_info`. |
| Alert noise during maintenance | Use the maintenance silence template in `examples/alertmanager-silence.json`. |

For additional scenarios, open a docs issue (`roadmap-input`) so the RC5 team can document the fix.
