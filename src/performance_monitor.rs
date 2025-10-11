// Real-time Performance Monitoring System
// Collects VM metrics for display in GUI graphs and CLI output

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

const MAX_HISTORY_POINTS: usize = 300; // 5 minutes at 1-second intervals

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMetrics {
    pub vm_name: String,
    pub timestamp: DateTime<Utc>,
    pub cpu_percent: f64,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub memory_percent: f64,
    pub disk_read_mb_per_sec: f64,
    pub disk_write_mb_per_sec: f64,
    pub network_rx_mb_per_sec: f64,
    pub network_tx_mb_per_sec: f64,
    pub disk_iops_read: u64,
    pub disk_iops_write: u64,
}

#[derive(Debug, Clone)]
pub struct MetricsHistory {
    pub cpu_history: VecDeque<f64>,
    pub memory_history: VecDeque<f64>,
    pub disk_read_history: VecDeque<f64>,
    pub disk_write_history: VecDeque<f64>,
    pub network_rx_history: VecDeque<f64>,
    pub network_tx_history: VecDeque<f64>,
    pub timestamps: VecDeque<DateTime<Utc>>,
}

impl MetricsHistory {
    pub fn new() -> Self {
        Self {
            cpu_history: VecDeque::with_capacity(MAX_HISTORY_POINTS),
            memory_history: VecDeque::with_capacity(MAX_HISTORY_POINTS),
            disk_read_history: VecDeque::with_capacity(MAX_HISTORY_POINTS),
            disk_write_history: VecDeque::with_capacity(MAX_HISTORY_POINTS),
            network_rx_history: VecDeque::with_capacity(MAX_HISTORY_POINTS),
            network_tx_history: VecDeque::with_capacity(MAX_HISTORY_POINTS),
            timestamps: VecDeque::with_capacity(MAX_HISTORY_POINTS),
        }
    }

    pub fn add_metrics(&mut self, metrics: &VmMetrics) {
        // Add new data point
        self.cpu_history.push_back(metrics.cpu_percent);
        self.memory_history.push_back(metrics.memory_percent);
        self.disk_read_history.push_back(metrics.disk_read_mb_per_sec);
        self.disk_write_history.push_back(metrics.disk_write_mb_per_sec);
        self.network_rx_history.push_back(metrics.network_rx_mb_per_sec);
        self.network_tx_history.push_back(metrics.network_tx_mb_per_sec);
        self.timestamps.push_back(metrics.timestamp);

        // Remove old data if we exceed max history
        if self.cpu_history.len() > MAX_HISTORY_POINTS {
            self.cpu_history.pop_front();
            self.memory_history.pop_front();
            self.disk_read_history.pop_front();
            self.disk_write_history.pop_front();
            self.network_rx_history.pop_front();
            self.network_tx_history.pop_front();
            self.timestamps.pop_front();
        }
    }

    pub fn get_latest_cpu(&self) -> f64 {
        self.cpu_history.back().copied().unwrap_or(0.0)
    }

    pub fn get_latest_memory(&self) -> f64 {
        self.memory_history.back().copied().unwrap_or(0.0)
    }

    pub fn get_average_cpu(&self, duration_secs: usize) -> f64 {
        if self.cpu_history.is_empty() {
            return 0.0;
        }

        let count = duration_secs.min(self.cpu_history.len());
        let sum: f64 = self.cpu_history.iter().rev().take(count).sum();
        sum / count as f64
    }
}

pub struct PerformanceCollector {
    connection: Option<Arc<virt::Connect>>,
    metrics: Arc<RwLock<HashMap<String, VmMetrics>>>,
    history: Arc<RwLock<HashMap<String, MetricsHistory>>>,
    collection_interval: Duration,
    running: Arc<RwLock<bool>>,
    previous_stats: Arc<RwLock<HashMap<String, PreviousStats>>>,
}

#[derive(Debug, Clone)]
struct PreviousStats {
    cpu_time_ns: u64,
    timestamp: Instant,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
    network_rx_bytes: u64,
    network_tx_bytes: u64,
}

impl PerformanceCollector {
    pub fn new() -> Self {
        Self {
            connection: None,
            metrics: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(HashMap::new())),
            collection_interval: Duration::from_secs(1),
            running: Arc::new(RwLock::new(false)),
            previous_stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.collection_interval = interval;
        self
    }

    /// Connect to libvirt
    pub fn connect(&mut self) -> Result<(), String> {
        let conn = virt::Connect::open("qemu:///system")
            .map_err(|e| format!("Failed to connect to libvirt: {}", e))?;

        self.connection = Some(Arc::new(conn));
        Ok(())
    }

    /// Start collecting metrics in the background
    pub async fn start_collection(&mut self) -> Result<(), String> {
        if self.connection.is_none() {
            self.connect()?;
        }

        let mut running = self.running.write().await;
        if *running {
            return Err("Collection already running".to_string());
        }
        *running = true;
        drop(running);

        let connection = self.connection.clone().unwrap();
        let metrics = self.metrics.clone();
        let history = self.history.clone();
        let interval = self.collection_interval;
        let running = self.running.clone();
        let previous_stats = self.previous_stats.clone();

        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);

            loop {
                interval_timer.tick().await;

                let is_running = *running.read().await;
                if !is_running {
                    break;
                }

                if let Ok(domains) = connection.list_all_domains(0) {
                    for domain in domains {
                        if let Ok(name) = domain.get_name() {
                            if let Ok(vm_metrics) = Self::collect_domain_metrics(
                                &domain,
                                &name,
                                &previous_stats,
                            ).await {
                                // Store current metrics
                                metrics.write().await.insert(name.clone(), vm_metrics.clone());

                                // Add to history
                                let mut history_map = history.write().await;
                                history_map
                                    .entry(name.clone())
                                    .or_insert_with(MetricsHistory::new)
                                    .add_metrics(&vm_metrics);
                            }
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop collecting metrics
    pub async fn stop_collection(&self) {
        let mut running = self.running.write().await;
        *running = false;
    }

    /// Collect metrics for a single domain
    async fn collect_domain_metrics(
        domain: &virt::Domain,
        vm_name: &str,
        previous_stats: &Arc<RwLock<HashMap<String, PreviousStats>>>,
    ) -> Result<VmMetrics, String> {
        let info = domain.get_info()
            .map_err(|e| format!("Failed to get domain info: {}", e))?;

        let memory_total_mb = info.max_mem / 1024; // KiB to MiB
        let memory_used_mb = info.memory / 1024;
        let memory_percent = (memory_used_mb as f64 / memory_total_mb as f64) * 100.0;

        // Get CPU usage
        let cpu_stats = domain.get_cpu_stats(0, 1, 0)
            .ok()
            .and_then(|stats| stats.into_iter().next());

        let cpu_time_ns = cpu_stats
            .and_then(|s| s.cpu_time)
            .unwrap_or(0);

        // Calculate CPU percentage
        let now = Instant::now();
        let cpu_percent = {
            let mut prev_map = previous_stats.write().await;
            let prev = prev_map.entry(vm_name.to_string()).or_insert(PreviousStats {
                cpu_time_ns,
                timestamp: now,
                disk_read_bytes: 0,
                disk_write_bytes: 0,
                network_rx_bytes: 0,
                network_tx_bytes: 0,
            });

            let elapsed = now.duration_since(prev.timestamp).as_nanos() as u64;
            let cpu_diff = cpu_time_ns.saturating_sub(prev.cpu_time_ns);

            let cpu_pct = if elapsed > 0 {
                (cpu_diff as f64 / elapsed as f64) * 100.0 * info.nr_virt_cpu as f64
            } else {
                0.0
            };

            // Update previous stats
            prev.cpu_time_ns = cpu_time_ns;
            prev.timestamp = now;

            cpu_pct.min(100.0 * info.nr_virt_cpu as f64) // Cap at 100% per vCPU
        };

        // Get disk I/O stats
        let (disk_read_mb_per_sec, disk_write_mb_per_sec, disk_iops_read, disk_iops_write) =
            Self::get_disk_stats(domain, vm_name, previous_stats).await;

        // Get network stats
        let (network_rx_mb_per_sec, network_tx_mb_per_sec) =
            Self::get_network_stats(domain, vm_name, previous_stats).await;

        Ok(VmMetrics {
            vm_name: vm_name.to_string(),
            timestamp: Utc::now(),
            cpu_percent,
            memory_used_mb,
            memory_total_mb,
            memory_percent,
            disk_read_mb_per_sec,
            disk_write_mb_per_sec,
            network_rx_mb_per_sec,
            network_tx_mb_per_sec,
            disk_iops_read,
            disk_iops_write,
        })
    }

    async fn get_disk_stats(
        domain: &virt::Domain,
        vm_name: &str,
        previous_stats: &Arc<RwLock<HashMap<String, PreviousStats>>>,
    ) -> (f64, f64, u64, u64) {
        // Get block device stats
        let block_stats = domain.block_stats("vda")
            .ok();

        if let Some(stats) = block_stats {
            let read_bytes = stats.rd_bytes.unwrap_or(0) as u64;
            let write_bytes = stats.wr_bytes.unwrap_or(0) as u64;
            let read_ops = stats.rd_req.unwrap_or(0) as u64;
            let write_ops = stats.wr_req.unwrap_or(0) as u64;

            let mut prev_map = previous_stats.write().await;
            if let Some(prev) = prev_map.get_mut(vm_name) {
                let elapsed_secs = Instant::now()
                    .duration_since(prev.timestamp)
                    .as_secs_f64();

                if elapsed_secs > 0.0 {
                    let read_diff = read_bytes.saturating_sub(prev.disk_read_bytes);
                    let write_diff = write_bytes.saturating_sub(prev.disk_write_bytes);

                    let read_mb_per_sec = (read_diff as f64 / elapsed_secs) / (1024.0 * 1024.0);
                    let write_mb_per_sec = (write_diff as f64 / elapsed_secs) / (1024.0 * 1024.0);

                    prev.disk_read_bytes = read_bytes;
                    prev.disk_write_bytes = write_bytes;

                    return (read_mb_per_sec, write_mb_per_sec, read_ops, write_ops);
                }
            }
        }

        (0.0, 0.0, 0, 0)
    }

    async fn get_network_stats(
        domain: &virt::Domain,
        vm_name: &str,
        previous_stats: &Arc<RwLock<HashMap<String, PreviousStats>>>,
    ) -> (f64, f64) {
        // Get network interface stats
        let interface_stats = domain.interface_stats("vnet0")
            .ok();

        if let Some(stats) = interface_stats {
            let rx_bytes = stats.rx_bytes.unwrap_or(0) as u64;
            let tx_bytes = stats.tx_bytes.unwrap_or(0) as u64;

            let mut prev_map = previous_stats.write().await;
            if let Some(prev) = prev_map.get_mut(vm_name) {
                let elapsed_secs = Instant::now()
                    .duration_since(prev.timestamp)
                    .as_secs_f64();

                if elapsed_secs > 0.0 {
                    let rx_diff = rx_bytes.saturating_sub(prev.network_rx_bytes);
                    let tx_diff = tx_bytes.saturating_sub(prev.network_tx_bytes);

                    let rx_mb_per_sec = (rx_diff as f64 / elapsed_secs) / (1024.0 * 1024.0);
                    let tx_mb_per_sec = (tx_diff as f64 / elapsed_secs) / (1024.0 * 1024.0);

                    prev.network_rx_bytes = rx_bytes;
                    prev.network_tx_bytes = tx_bytes;

                    return (rx_mb_per_sec, tx_mb_per_sec);
                }
            }
        }

        (0.0, 0.0)
    }

    /// Get current metrics for a VM
    pub async fn get_metrics(&self, vm_name: &str) -> Option<VmMetrics> {
        self.metrics.read().await.get(vm_name).cloned()
    }

    /// Get metrics history for a VM
    pub async fn get_history(&self, vm_name: &str) -> Option<MetricsHistory> {
        self.history.read().await.get(vm_name).cloned()
    }

    /// Get all current metrics
    pub async fn get_all_metrics(&self) -> HashMap<String, VmMetrics> {
        self.metrics.read().await.clone()
    }

    /// Clear history for a VM
    pub async fn clear_history(&self, vm_name: &str) {
        self.history.write().await.remove(vm_name);
    }

    /// Export metrics to Prometheus format
    pub fn export_prometheus(&self, metrics: &HashMap<String, VmMetrics>) -> String {
        let mut output = String::new();

        output.push_str("# HELP nova_vm_cpu_percent CPU usage percentage\n");
        output.push_str("# TYPE nova_vm_cpu_percent gauge\n");
        for (vm_name, m) in metrics {
            output.push_str(&format!(
                "nova_vm_cpu_percent{{vm=\"{}\"}} {}\n",
                vm_name, m.cpu_percent
            ));
        }

        output.push_str("\n# HELP nova_vm_memory_percent Memory usage percentage\n");
        output.push_str("# TYPE nova_vm_memory_percent gauge\n");
        for (vm_name, m) in metrics {
            output.push_str(&format!(
                "nova_vm_memory_percent{{vm=\"{}\"}} {}\n",
                vm_name, m.memory_percent
            ));
        }

        output.push_str("\n# HELP nova_vm_memory_used_mb Memory used in MB\n");
        output.push_str("# TYPE nova_vm_memory_used_mb gauge\n");
        for (vm_name, m) in metrics {
            output.push_str(&format!(
                "nova_vm_memory_used_mb{{vm=\"{}\"}} {}\n",
                vm_name, m.memory_used_mb
            ));
        }

        output.push_str("\n# HELP nova_vm_disk_read_mb_per_sec Disk read MB/s\n");
        output.push_str("# TYPE nova_vm_disk_read_mb_per_sec gauge\n");
        for (vm_name, m) in metrics {
            output.push_str(&format!(
                "nova_vm_disk_read_mb_per_sec{{vm=\"{}\"}} {}\n",
                vm_name, m.disk_read_mb_per_sec
            ));
        }

        output.push_str("\n# HELP nova_vm_disk_write_mb_per_sec Disk write MB/s\n");
        output.push_str("# TYPE nova_vm_disk_write_mb_per_sec gauge\n");
        for (vm_name, m) in metrics {
            output.push_str(&format!(
                "nova_vm_disk_write_mb_per_sec{{vm=\"{}\"}} {}\n",
                vm_name, m.disk_write_mb_per_sec
            ));
        }

        output.push_str("\n# HELP nova_vm_network_rx_mb_per_sec Network RX MB/s\n");
        output.push_str("# TYPE nova_vm_network_rx_mb_per_sec gauge\n");
        for (vm_name, m) in metrics {
            output.push_str(&format!(
                "nova_vm_network_rx_mb_per_sec{{vm=\"{}\"}} {}\n",
                vm_name, m.network_rx_mb_per_sec
            ));
        }

        output.push_str("\n# HELP nova_vm_network_tx_mb_per_sec Network TX MB/s\n");
        output.push_str("# TYPE nova_vm_network_tx_mb_per_sec gauge\n");
        for (vm_name, m) in metrics {
            output.push_str(&format!(
                "nova_vm_network_tx_mb_per_sec{{vm=\"{}\"}} {}\n",
                vm_name, m.network_tx_mb_per_sec
            ));
        }

        output
    }
}

// Mock virt module for compilation (replace with actual virt crate)
mod virt {
    use std::fmt;

    pub struct Connect;
    pub struct Domain;
    pub struct DomainInfo {
        pub max_mem: u64,
        pub memory: u64,
        pub nr_virt_cpu: u8,
    }

    pub struct DomainCpuStats {
        pub cpu_time: Option<u64>,
    }

    pub struct DomainBlockStats {
        pub rd_bytes: Option<i64>,
        pub wr_bytes: Option<i64>,
        pub rd_req: Option<i64>,
        pub wr_req: Option<i64>,
    }

    pub struct DomainInterfaceStats {
        pub rx_bytes: Option<i64>,
        pub tx_bytes: Option<i64>,
    }

    impl Connect {
        pub fn open(_uri: &str) -> Result<Self, String> {
            Ok(Self)
        }

        pub fn list_all_domains(&self, _flags: u32) -> Result<Vec<Domain>, String> {
            Ok(vec![])
        }
    }

    impl Domain {
        pub fn get_name(&self) -> Result<String, String> {
            Ok("test-vm".to_string())
        }

        pub fn get_info(&self) -> Result<DomainInfo, String> {
            Ok(DomainInfo {
                max_mem: 16 * 1024 * 1024,
                memory: 8 * 1024 * 1024,
                nr_virt_cpu: 4,
            })
        }

        pub fn get_cpu_stats(&self, _start: u32, _count: u32, _flags: u32) -> Result<Vec<DomainCpuStats>, String> {
            Ok(vec![DomainCpuStats { cpu_time: Some(1000000000) }])
        }

        pub fn block_stats(&self, _dev: &str) -> Result<DomainBlockStats, String> {
            Ok(DomainBlockStats {
                rd_bytes: Some(0),
                wr_bytes: Some(0),
                rd_req: Some(0),
                wr_req: Some(0),
            })
        }

        pub fn interface_stats(&self, _dev: &str) -> Result<DomainInterfaceStats, String> {
            Ok(DomainInterfaceStats {
                rx_bytes: Some(0),
                tx_bytes: Some(0),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_history() {
        let mut history = MetricsHistory::new();

        let metrics = VmMetrics {
            vm_name: "test-vm".to_string(),
            timestamp: Utc::now(),
            cpu_percent: 50.0,
            memory_used_mb: 4096,
            memory_total_mb: 8192,
            memory_percent: 50.0,
            disk_read_mb_per_sec: 10.0,
            disk_write_mb_per_sec: 20.0,
            network_rx_mb_per_sec: 5.0,
            network_tx_mb_per_sec: 3.0,
            disk_iops_read: 100,
            disk_iops_write: 200,
        };

        history.add_metrics(&metrics);

        assert_eq!(history.get_latest_cpu(), 50.0);
        assert_eq!(history.get_latest_memory(), 50.0);
    }

    #[test]
    fn test_metrics_history_overflow() {
        let mut history = MetricsHistory::new();

        // Add more than MAX_HISTORY_POINTS
        for i in 0..MAX_HISTORY_POINTS + 100 {
            let metrics = VmMetrics {
                vm_name: "test-vm".to_string(),
                timestamp: Utc::now(),
                cpu_percent: i as f64,
                memory_used_mb: 4096,
                memory_total_mb: 8192,
                memory_percent: 50.0,
                disk_read_mb_per_sec: 0.0,
                disk_write_mb_per_sec: 0.0,
                network_rx_mb_per_sec: 0.0,
                network_tx_mb_per_sec: 0.0,
                disk_iops_read: 0,
                disk_iops_write: 0,
            };
            history.add_metrics(&metrics);
        }

        // Should never exceed MAX_HISTORY_POINTS
        assert_eq!(history.cpu_history.len(), MAX_HISTORY_POINTS);
    }

    #[test]
    fn test_average_cpu() {
        let mut history = MetricsHistory::new();

        for i in 0..10 {
            let metrics = VmMetrics {
                vm_name: "test-vm".to_string(),
                timestamp: Utc::now(),
                cpu_percent: i as f64 * 10.0,
                memory_used_mb: 4096,
                memory_total_mb: 8192,
                memory_percent: 50.0,
                disk_read_mb_per_sec: 0.0,
                disk_write_mb_per_sec: 0.0,
                network_rx_mb_per_sec: 0.0,
                network_tx_mb_per_sec: 0.0,
                disk_iops_read: 0,
                disk_iops_write: 0,
            };
            history.add_metrics(&metrics);
        }

        let avg = history.get_average_cpu(10);
        assert!((avg - 45.0).abs() < 0.1); // Average of 0,10,20,...,90
    }
}
