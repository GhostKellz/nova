# Operations Overview

Operational docs cover support bundles, diagnostics, observability, metrics, packaging, and release support workflows.

## Pages

- [observability.md](observability.md) - Prometheus, Grafana, alerting, and metrics guidance.
- [support-tooling.md](support-tooling.md) - support bundle and escalation workflow.
- [packaging.md](packaging.md) - packaging layout, supported targets, and release artifact expectations.

## Common Commands

```bash
nova support preflight
nova support bundle --redact
nova gpu doctor
cargo audit
make -C packaging smoke-test
```
