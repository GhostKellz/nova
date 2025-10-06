use crate::firewall::FirewallManager;
use crate::port_monitor::PortMonitor;
use crate::{NovaError, Result, log_debug, log_error, log_info};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct PrometheusExporter {
    port: u16,
    metrics_registry: Arc<Mutex<MetricsRegistry>>,
    collection_interval_secs: u64,
    enabled: bool,
}

#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    counters: HashMap<String, Counter>,
    gauges: HashMap<String, Gauge>,
    histograms: HashMap<String, Histogram>,
    _summaries: HashMap<String, Summary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counter {
    pub name: String,
    pub help: String,
    pub labels: HashMap<String, String>,
    pub value: f64,
    pub created_at: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gauge {
    pub name: String,
    pub help: String,
    pub labels: HashMap<String, String>,
    pub value: f64,
    pub last_updated: SystemTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Histogram {
    pub name: String,
    pub help: String,
    pub labels: HashMap<String, String>,
    pub buckets: Vec<HistogramBucket>,
    pub sum: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBucket {
    pub le: f64, // Less than or equal to
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub name: String,
    pub help: String,
    pub labels: HashMap<String, String>,
    pub quantiles: Vec<Quantile>,
    pub sum: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quantile {
    pub quantile: f64,
    pub value: f64,
}

// Nova-specific metrics for HyperV Manager equivalent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NovaMetrics {
    // VM Management Metrics
    pub vms_total: u64,
    pub vms_running: u64,
    pub vms_stopped: u64,
    pub vms_paused: u64,
    pub vm_cpu_usage_percent: HashMap<String, f64>,
    pub vm_memory_usage_bytes: HashMap<String, u64>,
    pub vm_network_rx_bytes: HashMap<String, u64>,
    pub vm_network_tx_bytes: HashMap<String, u64>,
    pub vm_disk_read_bytes: HashMap<String, u64>,
    pub vm_disk_write_bytes: HashMap<String, u64>,

    // Container Metrics
    pub containers_total: u64,
    pub containers_running: u64,
    pub containers_stopped: u64,
    pub container_cpu_usage_percent: HashMap<String, f64>,
    pub container_memory_usage_bytes: HashMap<String, u64>,

    // Network Security Metrics
    pub firewall_rules_total: u64,
    pub firewall_rules_active: u64,
    pub port_scans_detected: u64,
    pub security_alerts_total: u64,
    pub network_connections_active: u64,
    pub exposed_ports_total: u64,
    pub high_risk_ports_total: u64,

    // Infrastructure Metrics
    pub host_cpu_usage_percent: f64,
    pub host_memory_usage_bytes: u64,
    pub host_memory_total_bytes: u64,
    pub host_disk_usage_bytes: u64,
    pub host_disk_total_bytes: u64,
    pub host_network_rx_bytes: u64,
    pub host_network_tx_bytes: u64,

    // Nova-specific Operational Metrics
    pub nova_uptime_seconds: u64,
    pub nova_version: String,
    pub libvirt_connection_status: f64, // 1.0 = connected, 0.0 = disconnected
    pub kvm_available: f64,
    pub docker_available: f64,
    pub migration_jobs_active: u64,
    pub migration_jobs_completed: u64,
    pub migration_jobs_failed: u64,

    // Performance Metrics
    pub api_requests_total: HashMap<String, u64>,
    pub api_request_duration_seconds: HashMap<String, f64>,
    pub console_sessions_active: u64,
    pub template_deployments_total: u64,
    pub template_deployments_failed: u64,
}

impl PrometheusExporter {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            metrics_registry: Arc::new(Mutex::new(MetricsRegistry::new())),
            collection_interval_secs: 15, // Collect metrics every 15 seconds
            enabled: true,
        }
    }

    pub async fn start(&self) -> Result<()> {
        if !self.enabled {
            log_info!("Prometheus exporter is disabled");
            return Ok(());
        }

        log_info!("Starting Prometheus metrics exporter on port {}", self.port);

        // Start metrics collection task
        let registry = self.metrics_registry.clone();
        let interval = self.collection_interval_secs;

        tokio::spawn(async move {
            let mut collection_interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval));

            loop {
                collection_interval.tick().await;

                if let Err(e) = Self::collect_system_metrics(registry.clone()).await {
                    log_error!("Failed to collect system metrics: {:?}", e);
                }
            }
        });

        // Start HTTP server for metrics endpoint
        self.start_metrics_server().await
    }

    async fn start_metrics_server(&self) -> Result<()> {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await
            .map_err(|_| NovaError::NetworkError("Failed to bind Prometheus server".to_string()))?;

        let registry = self.metrics_registry.clone();

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut stream, addr)) => {
                        log_debug!("Prometheus metrics request from {}", addr);

                        let registry = registry.clone();
                        tokio::spawn(async move {
                            if let Err(e) =
                                Self::handle_metrics_request(&mut stream, registry).await
                            {
                                log_error!("Failed to handle metrics request: {:?}", e);
                            }
                        });
                    }
                    Err(e) => {
                        log_error!("Failed to accept connection: {}", e);
                    }
                }
            }
        });

        log_info!(
            "Prometheus metrics server started on http://0.0.0.0:{}/metrics",
            self.port
        );
        Ok(())
    }

    async fn handle_metrics_request(
        stream: &mut tokio::net::TcpStream,
        registry: Arc<Mutex<MetricsRegistry>>,
    ) -> Result<()> {
        let mut reader = BufReader::new(&mut *stream);
        let mut request_line = String::new();
        reader
            .read_line(&mut request_line)
            .await
            .map_err(|_| NovaError::NetworkError("Failed to read request".to_string()))?;

        // Simple HTTP response with metrics
        let metrics_output = {
            let registry = registry.lock().unwrap();
            registry.export_prometheus_format()
        };

        let response = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: text/plain; version=0.0.4; charset=utf-8\r\n\
             Content-Length: {}\r\n\
             \r\n\
             {}",
            metrics_output.len(),
            metrics_output
        );

        stream
            .write_all(response.as_bytes())
            .await
            .map_err(|_| NovaError::NetworkError("Failed to write response".to_string()))?;

        Ok(())
    }

    async fn collect_system_metrics(registry: Arc<Mutex<MetricsRegistry>>) -> Result<()> {
        // Collect metrics separately to avoid holding lock across await
        let host_metrics = Self::collect_host_metrics_data().await?;
        let nova_metrics = Self::collect_nova_metrics_data().await?;

        // Update registry with collected data
        {
            let mut registry = registry.lock().unwrap();
            Self::update_host_metrics(&mut registry, host_metrics);
            Self::update_nova_metrics(&mut registry, nova_metrics);
        }

        Ok(())
    }

    async fn collect_host_metrics_data() -> Result<MetricsRegistry> {
        let mut metrics = MetricsRegistry::new();

        Self::collect_host_metrics(&mut metrics).await?;
        Ok(metrics)
    }

    async fn collect_nova_metrics_data() -> Result<MetricsRegistry> {
        let mut metrics = MetricsRegistry::new();

        Self::collect_nova_metrics(&mut metrics).await?;
        Ok(metrics)
    }

    fn update_host_metrics(registry: &mut MetricsRegistry, host_metrics: MetricsRegistry) {
        // Merge host metrics into main registry
        for (name, gauge) in host_metrics.gauges {
            registry.gauges.insert(name, gauge);
        }
        for (name, counter) in host_metrics.counters {
            registry.counters.insert(name, counter);
        }
        for (name, histogram) in host_metrics.histograms {
            registry.histograms.insert(name, histogram);
        }
    }

    fn update_nova_metrics(registry: &mut MetricsRegistry, nova_metrics: MetricsRegistry) {
        // Merge nova metrics into main registry
        for (name, gauge) in nova_metrics.gauges {
            registry.gauges.insert(name, gauge);
        }
        for (name, counter) in nova_metrics.counters {
            registry.counters.insert(name, counter);
        }
        for (name, histogram) in nova_metrics.histograms {
            registry.histograms.insert(name, histogram);
        }
    }

    async fn collect_host_metrics(registry: &mut MetricsRegistry) -> Result<()> {
        // CPU Usage
        if let Ok(cpu_usage) = Self::get_cpu_usage().await {
            registry.set_gauge(
                "nova_host_cpu_usage_percent",
                "Host CPU usage percentage",
                HashMap::new(),
                cpu_usage,
            );
        }

        // Memory Usage
        if let Ok((used, total)) = Self::get_memory_usage().await {
            registry.set_gauge(
                "nova_host_memory_usage_bytes",
                "Host memory usage in bytes",
                HashMap::new(),
                used as f64,
            );
            registry.set_gauge(
                "nova_host_memory_total_bytes",
                "Host total memory in bytes",
                HashMap::new(),
                total as f64,
            );
        }

        // Disk Usage
        if let Ok((used, total)) = Self::get_disk_usage().await {
            registry.set_gauge(
                "nova_host_disk_usage_bytes",
                "Host disk usage in bytes",
                HashMap::new(),
                used as f64,
            );
            registry.set_gauge(
                "nova_host_disk_total_bytes",
                "Host total disk space in bytes",
                HashMap::new(),
                total as f64,
            );
        }

        // Network I/O
        if let Ok((rx, tx)) = Self::get_network_io().await {
            registry.set_gauge(
                "nova_host_network_rx_bytes_total",
                "Host network bytes received",
                HashMap::new(),
                rx as f64,
            );
            registry.set_gauge(
                "nova_host_network_tx_bytes_total",
                "Host network bytes transmitted",
                HashMap::new(),
                tx as f64,
            );
        }

        Ok(())
    }

    async fn collect_nova_metrics(registry: &mut MetricsRegistry) -> Result<()> {
        // Nova uptime
        let uptime = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        registry.set_gauge(
            "nova_uptime_seconds",
            "Nova uptime in seconds",
            HashMap::new(),
            uptime as f64,
        );

        // System availability checks
        registry.set_gauge(
            "nova_kvm_available",
            "KVM availability (1=available, 0=unavailable)",
            HashMap::new(),
            if Self::check_kvm_available() {
                1.0
            } else {
                0.0
            },
        );

        registry.set_gauge(
            "nova_docker_available",
            "Docker availability (1=available, 0=unavailable)",
            HashMap::new(),
            if Self::check_docker_available() {
                1.0
            } else {
                0.0
            },
        );

        registry.set_gauge(
            "nova_libvirt_connection_status",
            "Libvirt connection status (1=connected, 0=disconnected)",
            HashMap::new(),
            if Self::check_libvirt_connection() {
                1.0
            } else {
                0.0
            },
        );

        Ok(())
    }

    pub async fn collect_vm_metrics(&self, vm_stats: &HashMap<String, VmStats>) -> Result<()> {
        let mut registry = self.metrics_registry.lock().unwrap();

        // VM counts by state
        let mut running = 0u64;
        let mut stopped = 0u64;
        let mut paused = 0u64;

        for (vm_name, stats) in vm_stats {
            match &stats.state {
                VmState::Running => running += 1,
                VmState::Stopped => stopped += 1,
                VmState::Paused => paused += 1,
            }

            // Individual VM metrics
            let mut labels = HashMap::new();
            labels.insert("vm_name".to_string(), vm_name.clone());

            registry.set_gauge(
                "nova_vm_cpu_usage_percent",
                "VM CPU usage percentage",
                labels.clone(),
                stats.cpu_usage_percent,
            );

            registry.set_gauge(
                "nova_vm_memory_usage_bytes",
                "VM memory usage in bytes",
                labels.clone(),
                stats.memory_usage_bytes as f64,
            );

            registry.set_gauge(
                "nova_vm_network_rx_bytes_total",
                "VM network bytes received",
                labels.clone(),
                stats.network_rx_bytes as f64,
            );

            registry.set_gauge(
                "nova_vm_network_tx_bytes_total",
                "VM network bytes transmitted",
                labels.clone(),
                stats.network_tx_bytes as f64,
            );

            registry.set_gauge(
                "nova_vm_disk_read_bytes_total",
                "VM disk bytes read",
                labels.clone(),
                stats.disk_read_bytes as f64,
            );

            registry.set_gauge(
                "nova_vm_disk_write_bytes_total",
                "VM disk bytes written",
                labels,
                stats.disk_write_bytes as f64,
            );
        }

        // Total VM counts
        registry.set_gauge(
            "nova_vms_total",
            "Total number of VMs",
            HashMap::new(),
            vm_stats.len() as f64,
        );

        registry.set_gauge(
            "nova_vms_running",
            "Number of running VMs",
            HashMap::new(),
            running as f64,
        );

        registry.set_gauge(
            "nova_vms_stopped",
            "Number of stopped VMs",
            HashMap::new(),
            stopped as f64,
        );

        registry.set_gauge(
            "nova_vms_paused",
            "Number of paused VMs",
            HashMap::new(),
            paused as f64,
        );

        Ok(())
    }

    pub async fn collect_network_security_metrics(
        &self,
        firewall_manager: &FirewallManager,
        port_monitor: &PortMonitor,
    ) -> Result<()> {
        let mut registry = self.metrics_registry.lock().unwrap();

        // Firewall metrics
        let total_rules = firewall_manager
            .get_tables()
            .values()
            .map(|table| {
                table
                    .chains
                    .values()
                    .map(|chain| chain.rules.len())
                    .sum::<usize>()
            })
            .sum::<usize>();

        registry.set_gauge(
            "nova_firewall_rules_total",
            "Total number of firewall rules",
            HashMap::new(),
            total_rules as f64,
        );

        registry.set_gauge(
            "nova_firewall_rules_conflicts",
            "Number of conflicting firewall rules",
            HashMap::new(),
            firewall_manager.get_rule_conflicts().len() as f64,
        );

        // Port monitoring metrics
        registry.set_gauge(
            "nova_network_connections_active",
            "Number of active network connections",
            HashMap::new(),
            port_monitor.get_active_connections().len() as f64,
        );

        registry.set_gauge(
            "nova_ports_listening_total",
            "Total number of listening ports",
            HashMap::new(),
            port_monitor.get_listening_ports().len() as f64,
        );

        registry.set_gauge(
            "nova_ports_exposed_external",
            "Number of externally exposed ports",
            HashMap::new(),
            port_monitor.get_exposed_services().len() as f64,
        );

        registry.set_gauge(
            "nova_ports_high_risk",
            "Number of high-risk ports",
            HashMap::new(),
            port_monitor.get_high_risk_ports().len() as f64,
        );

        registry.set_gauge(
            "nova_security_alerts_total",
            "Total number of security alerts",
            HashMap::new(),
            port_monitor.get_security_alerts().len() as f64,
        );

        // Alert counts by severity
        let mut critical_alerts = 0;
        let mut high_alerts = 0;
        let mut medium_alerts = 0;

        for alert in port_monitor.get_security_alerts() {
            match alert.severity {
                crate::port_monitor::AlertSeverity::Critical => critical_alerts += 1,
                crate::port_monitor::AlertSeverity::High => high_alerts += 1,
                crate::port_monitor::AlertSeverity::Medium => medium_alerts += 1,
                _ => {}
            }
        }

        registry.set_gauge(
            "nova_security_alerts_critical",
            "Number of critical security alerts",
            HashMap::new(),
            critical_alerts as f64,
        );

        registry.set_gauge(
            "nova_security_alerts_high",
            "Number of high severity security alerts",
            HashMap::new(),
            high_alerts as f64,
        );

        registry.set_gauge(
            "nova_security_alerts_medium",
            "Number of medium severity security alerts",
            HashMap::new(),
            medium_alerts as f64,
        );

        Ok(())
    }

    pub async fn record_api_request(&self, endpoint: &str, duration_seconds: f64) -> Result<()> {
        let mut registry = self.metrics_registry.lock().unwrap();

        let mut labels = HashMap::new();
        labels.insert("endpoint".to_string(), endpoint.to_string());

        // Increment request counter
        registry.increment_counter(
            "nova_api_requests_total",
            "Total number of API requests",
            labels.clone(),
            1.0,
        );

        // Record duration
        registry.observe_histogram(
            "nova_api_request_duration_seconds",
            "API request duration in seconds",
            labels,
            duration_seconds,
        );

        Ok(())
    }

    pub async fn record_migration_event(&self, event_type: &str, vm_name: &str) -> Result<()> {
        let mut registry = self.metrics_registry.lock().unwrap();

        let mut labels = HashMap::new();
        labels.insert("event_type".to_string(), event_type.to_string());
        labels.insert("vm_name".to_string(), vm_name.to_string());

        registry.increment_counter(
            "nova_migration_events_total",
            "Total number of migration events",
            labels,
            1.0,
        );

        // Update active migration count
        let active_migrations = if event_type == "started" { 1.0 } else { -1.0 };
        registry.add_to_gauge(
            "nova_migration_jobs_active",
            "Number of active migration jobs",
            HashMap::new(),
            active_migrations,
        );

        Ok(())
    }

    // System collection helpers
    async fn get_cpu_usage() -> Result<f64> {
        // Read from /proc/stat or use other system APIs
        // This is a simplified implementation
        Ok(25.5) // Placeholder
    }

    async fn get_memory_usage() -> Result<(u64, u64)> {
        // Read from /proc/meminfo
        // Returns (used, total) in bytes
        Ok((8_589_934_592, 17_179_869_184)) // Placeholder: 8GB used, 16GB total
    }

    async fn get_disk_usage() -> Result<(u64, u64)> {
        // Use statvfs or similar
        // Returns (used, total) in bytes
        Ok((107_374_182_400, 1_073_741_824_000)) // Placeholder: 100GB used, 1TB total
    }

    async fn get_network_io() -> Result<(u64, u64)> {
        // Read from /proc/net/dev
        // Returns (rx_bytes, tx_bytes)
        Ok((1_073_741_824, 536_870_912)) // Placeholder: 1GB rx, 512MB tx
    }

    fn check_kvm_available() -> bool {
        std::path::Path::new("/dev/kvm").exists()
    }

    fn check_docker_available() -> bool {
        std::process::Command::new("docker")
            .arg("version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn check_libvirt_connection() -> bool {
        std::process::Command::new("virsh")
            .args(&["version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self {
            counters: HashMap::new(),
            gauges: HashMap::new(),
            histograms: HashMap::new(),
            _summaries: HashMap::new(),
        }
    }

    pub fn increment_counter(
        &mut self,
        name: &str,
        help: &str,
        labels: HashMap<String, String>,
        value: f64,
    ) {
        let key = Self::make_key(name, &labels);

        if let Some(counter) = self.counters.get_mut(&key) {
            counter.value += value;
        } else {
            self.counters.insert(
                key,
                Counter {
                    name: name.to_string(),
                    help: help.to_string(),
                    labels,
                    value,
                    created_at: SystemTime::now(),
                },
            );
        }
    }

    pub fn set_gauge(
        &mut self,
        name: &str,
        help: &str,
        labels: HashMap<String, String>,
        value: f64,
    ) {
        let key = Self::make_key(name, &labels);

        self.gauges.insert(
            key,
            Gauge {
                name: name.to_string(),
                help: help.to_string(),
                labels,
                value,
                last_updated: SystemTime::now(),
            },
        );
    }

    pub fn add_to_gauge(
        &mut self,
        name: &str,
        help: &str,
        labels: HashMap<String, String>,
        value: f64,
    ) {
        let key = Self::make_key(name, &labels);

        if let Some(gauge) = self.gauges.get_mut(&key) {
            gauge.value += value;
            gauge.last_updated = SystemTime::now();
        } else {
            self.set_gauge(name, help, labels, value);
        }
    }

    pub fn observe_histogram(
        &mut self,
        name: &str,
        help: &str,
        labels: HashMap<String, String>,
        value: f64,
    ) {
        let key = Self::make_key(name, &labels);

        if let Some(histogram) = self.histograms.get_mut(&key) {
            histogram.sum += value;
            histogram.count += 1;

            // Update buckets
            for bucket in &mut histogram.buckets {
                if value <= bucket.le {
                    bucket.count += 1;
                }
            }
        } else {
            // Create new histogram with default buckets
            let buckets = vec![
                HistogramBucket {
                    le: 0.005,
                    count: if value <= 0.005 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 0.01,
                    count: if value <= 0.01 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 0.025,
                    count: if value <= 0.025 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 0.05,
                    count: if value <= 0.05 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 0.1,
                    count: if value <= 0.1 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 0.25,
                    count: if value <= 0.25 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 0.5,
                    count: if value <= 0.5 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 1.0,
                    count: if value <= 1.0 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 2.5,
                    count: if value <= 2.5 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 5.0,
                    count: if value <= 5.0 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: 10.0,
                    count: if value <= 10.0 { 1 } else { 0 },
                },
                HistogramBucket {
                    le: f64::INFINITY,
                    count: 1,
                },
            ];

            self.histograms.insert(
                key,
                Histogram {
                    name: name.to_string(),
                    help: help.to_string(),
                    labels,
                    buckets,
                    sum: value,
                    count: 1,
                },
            );
        }
    }

    fn make_key(name: &str, labels: &HashMap<String, String>) -> String {
        let mut label_str = labels
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", k, v))
            .collect::<Vec<_>>();
        label_str.sort();

        if label_str.is_empty() {
            name.to_string()
        } else {
            format!("{}|{}", name, label_str.join(","))
        }
    }

    pub fn export_prometheus_format(&self) -> String {
        let mut output = String::new();

        // Export counters
        for counter in self.counters.values() {
            output.push_str(&format!("# HELP {} {}\n", counter.name, counter.help));
            output.push_str(&format!("# TYPE {} counter\n", counter.name));

            let label_str = if counter.labels.is_empty() {
                String::new()
            } else {
                format!(
                    "{{{}}}",
                    counter
                        .labels
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, v))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };

            output.push_str(&format!(
                "{}{} {}\n",
                counter.name, label_str, counter.value
            ));
        }

        // Export gauges
        for gauge in self.gauges.values() {
            output.push_str(&format!("# HELP {} {}\n", gauge.name, gauge.help));
            output.push_str(&format!("# TYPE {} gauge\n", gauge.name));

            let label_str = if gauge.labels.is_empty() {
                String::new()
            } else {
                format!(
                    "{{{}}}",
                    gauge
                        .labels
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, v))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };

            output.push_str(&format!("{}{} {}\n", gauge.name, label_str, gauge.value));
        }

        // Export histograms
        for histogram in self.histograms.values() {
            output.push_str(&format!("# HELP {} {}\n", histogram.name, histogram.help));
            output.push_str(&format!("# TYPE {} histogram\n", histogram.name));

            let base_labels = if histogram.labels.is_empty() {
                String::new()
            } else {
                format!(
                    ",{}",
                    histogram
                        .labels
                        .iter()
                        .map(|(k, v)| format!("{}=\"{}\"", k, v))
                        .collect::<Vec<_>>()
                        .join(",")
                )
            };

            // Export buckets
            for bucket in &histogram.buckets {
                let le_label = if bucket.le == f64::INFINITY {
                    "+Inf".to_string()
                } else {
                    bucket.le.to_string()
                };

                output.push_str(&format!(
                    "{}_bucket{{le=\"{}\"{}}} {}\n",
                    histogram.name, le_label, base_labels, bucket.count
                ));
            }

            // Export sum and count
            output.push_str(&format!(
                "{}_sum{{{}}} {}\n",
                histogram.name,
                base_labels.trim_start_matches(','),
                histogram.sum
            ));
            output.push_str(&format!(
                "{}_count{{{}}} {}\n",
                histogram.name,
                base_labels.trim_start_matches(','),
                histogram.count
            ));
        }

        output
    }
}

// VM stats structure for metrics collection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStats {
    pub name: String,
    pub state: VmState,
    pub cpu_usage_percent: f64,
    pub memory_usage_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VmState {
    Running,
    Stopped,
    Paused,
}

impl Default for PrometheusExporter {
    fn default() -> Self {
        Self::new(9090)
    }
}
