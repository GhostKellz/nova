# Observability

Nova exposes host, VM, container, network, and support workflow signals through a Prometheus-compatible metrics path. Use these docs to wire Nova into an existing monitoring stack.

## Exporter Overview

- **Binary/service**: `nova-metrics` or the packaged metrics service.
- **Endpoint**: `http://<host>:9640/metrics` by default.
- **Protocol**: Plain HTTP by default; terminate TLS at a reverse proxy when exposing beyond localhost.
- **Primary consumers**: Prometheus, Grafana, Alertmanager, and compatible OpenMetrics tooling.

## Enabling Metrics

```toml
# NovaFile excerpt
[metrics]
exporter = true
listen = "0.0.0.0:9640"
collect_libvirt_stats = true
collect_container_stats = true
```

Reload the service after changing metrics settings:

```bash
systemctl restart nova-metrics.service
```

## Hardening

- Bind to localhost or a management VLAN unless the endpoint is protected by a reverse proxy.
- Restrict scrape access with firewall rules, mTLS, basic auth, or a trusted Prometheus network.
- Rotate exporter credentials when changing operators or moving hosts between environments.
- Avoid placing support bundle or host-identifying labels on public dashboards.

## Metrics Catalog

| Metric | Type | Description |
| --- | --- | --- |
| `nova_vm_status` | gauge | VM state by name and host. |
| `nova_vm_cpu_seconds_total` | counter | CPU seconds consumed by a VM. |
| `nova_container_restart_total` | counter | Container restart count by name and runtime. |
| `nova_host_memory_bytes` | gauge | Total host memory visible to Nova. |
| `nova_network_rx_bytes_total` | counter | Cumulative receive bytes per virtual switch or interface. |
| `nova_support_bundle_generated_total` | counter | Support bundle generation count by mode and redaction state. |

When adding a metric, document its labels, units, reset behavior, and expected cardinality.

## Grafana

Starter dashboards live in `examples/grafana-nova.json`.

1. Import the dashboard JSON into Grafana.
2. Select the Prometheus data source scraping Nova.
3. Adjust thresholds for the host class and workload profile.
4. Save an environment-specific copy instead of editing the example directly.

Useful dashboard sections:

- VM and container inventory.
- CPU, memory, disk, and network pressure.
- Virtual switch throughput and error trends.
- Support bundle and diagnostic activity.

## Alerting

| Alert | Purpose |
| --- | --- |
| `NovaExporterDown` | Exporter target is missing or unreachable. |
| `NovaVmUnresponsive` | Running VM appears stalled or absent from expected metrics. |
| `NovaNetworkDrops` | Virtual network reports packet drops. |
| `NovaSupportBundleFailure` | Bundle generation fails repeatedly. |

Use `examples/alert-rules.yml` as a starting point and tune alert windows for the environment. Lab hosts and production hosts should not share identical thresholds.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Exporter returns `404` | Confirm metrics are enabled and the service was restarted. |
| Scrape timeout | Check host load, libvirt socket access, and scrape interval. |
| Grafana panels are empty | Confirm data source selection, metric names, and dashboard variables. |
| Alert noise during maintenance | Add a temporary Alertmanager silence before planned work. |

For support escalation, generate a redacted bundle with `nova support bundle --redact`.
