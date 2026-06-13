// Performance Optimization Module for Nova
// Provides one-click performance tuning for gaming/productivity VMs

use crate::{NovaError, Result, log_debug, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Performance profile for VM optimization
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PerformanceProfile {
    /// Optimized for gaming: low latency, CPU isolation, no power saving
    Gaming,
    /// Balanced settings for productivity workloads
    Productivity,
    /// Maximum performance for compute-heavy tasks
    Compute,
    /// Reset to system defaults
    Default,
}

impl std::fmt::Display for PerformanceProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PerformanceProfile::Gaming => write!(f, "gaming"),
            PerformanceProfile::Productivity => write!(f, "productivity"),
            PerformanceProfile::Compute => write!(f, "compute"),
            PerformanceProfile::Default => write!(f, "default"),
        }
    }
}

/// CPU topology information
#[derive(Debug, Clone, Serialize)]
pub struct CpuTopology {
    pub total_cpus: u32,
    pub physical_cores: u32,
    pub threads_per_core: u32,
    pub numa_nodes: u32,
    pub cpu_model: String,
    pub vendor: CpuVendor,
    /// Map of physical core ID to list of logical CPU IDs (for SMT siblings)
    pub core_map: HashMap<u32, Vec<u32>>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum CpuVendor {
    Intel,
    Amd,
    Unknown,
}

/// Result of applying performance optimizations
#[derive(Debug, Clone, Serialize)]
pub struct OptimizationResult {
    pub profile: PerformanceProfile,
    pub cpu_governor_set: bool,
    pub hugepages_configured: bool,
    pub hugepages_count: u64,
    pub cpu_isolation_recommended: Vec<u32>,
    pub pcie_power_disabled: bool,
    pub kernel_params_needed: Vec<String>,
    pub warnings: Vec<String>,
    pub applied_changes: Vec<String>,
}

impl std::fmt::Display for OptimizationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\n=== Nova Performance Optimization Results ===")?;
        writeln!(f, "Profile: {}", self.profile)?;
        writeln!(f)?;

        writeln!(f, "Applied Changes:")?;
        if self.applied_changes.is_empty() {
            writeln!(f, "  (none)")?;
        } else {
            for change in &self.applied_changes {
                writeln!(f, "  - {}", change)?;
            }
        }
        writeln!(f)?;

        if self.cpu_governor_set {
            writeln!(f, "CPU Governor: performance")?;
        }

        if self.hugepages_configured {
            writeln!(
                f,
                "Hugepages: {} pages ({}MB)",
                self.hugepages_count,
                self.hugepages_count * 2
            )?;
        }

        if !self.cpu_isolation_recommended.is_empty() {
            writeln!(f, "\nRecommended CPU Isolation (for VM pinning):")?;
            writeln!(f, "  CPUs: {:?}", self.cpu_isolation_recommended)?;
            writeln!(
                f,
                "  Add to kernel params: isolcpus={}",
                self.cpu_isolation_recommended
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )?;
        }

        if !self.kernel_params_needed.is_empty() {
            writeln!(f, "\nRecommended Kernel Parameters:")?;
            for param in &self.kernel_params_needed {
                writeln!(f, "  {}", param)?;
            }
            writeln!(f, "\n  Add to /etc/default/grub GRUB_CMDLINE_LINUX_DEFAULT")?;
            writeln!(f, "  Then run: sudo grub-mkconfig -o /boot/grub/grub.cfg")?;
        }

        if !self.warnings.is_empty() {
            writeln!(f, "\nWarnings:")?;
            for warning in &self.warnings {
                writeln!(f, "  - {}", warning)?;
            }
        }

        Ok(())
    }
}

pub struct PerformanceOptimizer {
    topology: Option<CpuTopology>,
}

impl PerformanceOptimizer {
    pub fn new() -> Self {
        Self { topology: None }
    }

    /// Detect CPU topology for intelligent core allocation
    pub fn detect_topology(&mut self) -> Result<&CpuTopology> {
        // `is_some` + `unwrap` rather than `if let` is required here: returning the
        // borrow from an `if let Some(t) = self.topology.as_ref()` arm extends the
        // borrow across the later `self.topology = ...` assignment (NLL limitation).
        #[allow(clippy::unnecessary_unwrap)]
        if self.topology.is_some() {
            return Ok(self.topology.as_ref().unwrap());
        }

        log_info!("Detecting CPU topology...");

        // Get total CPUs
        let total_cpus = self
            .read_sysfs_u32("/sys/devices/system/cpu/present")
            .unwrap_or(0)
            + 1;

        // Get CPU model
        let cpu_model = self.get_cpu_model();

        // Detect vendor
        let vendor = if cpu_model.to_lowercase().contains("intel") {
            CpuVendor::Intel
        } else if cpu_model.to_lowercase().contains("amd") {
            CpuVendor::Amd
        } else {
            CpuVendor::Unknown
        };

        // Build core map (physical core -> logical CPUs)
        let mut core_map: HashMap<u32, Vec<u32>> = HashMap::new();
        for cpu_id in 0..total_cpus {
            let core_id_path = format!("/sys/devices/system/cpu/cpu{}/topology/core_id", cpu_id);
            if let Ok(core_id) = self.read_sysfs_value::<u32>(&core_id_path) {
                core_map.entry(core_id).or_default().push(cpu_id);
            }
        }

        let physical_cores = core_map.len() as u32;
        let threads_per_core = total_cpus.checked_div(physical_cores).unwrap_or(1);

        // Detect NUMA nodes
        let numa_nodes = self.count_numa_nodes();

        let topology = CpuTopology {
            total_cpus,
            physical_cores,
            threads_per_core,
            numa_nodes,
            cpu_model,
            vendor,
            core_map,
        };

        log_info!(
            "CPU: {} ({} cores, {} threads, {} NUMA nodes)",
            topology.cpu_model,
            topology.physical_cores,
            topology.total_cpus,
            topology.numa_nodes
        );

        self.topology = Some(topology);
        Ok(self.topology.as_ref().unwrap())
    }

    /// Apply performance profile for a VM
    pub fn apply_profile(
        &mut self,
        profile: PerformanceProfile,
        vm_cores: u32,
        vm_memory_mb: u64,
    ) -> Result<OptimizationResult> {
        let topology = self.detect_topology()?.clone();

        let mut result = OptimizationResult {
            profile,
            cpu_governor_set: false,
            hugepages_configured: false,
            hugepages_count: 0,
            cpu_isolation_recommended: Vec::new(),
            pcie_power_disabled: false,
            kernel_params_needed: Vec::new(),
            warnings: Vec::new(),
            applied_changes: Vec::new(),
        };

        match profile {
            PerformanceProfile::Gaming => {
                self.apply_gaming_profile(&mut result, &topology, vm_cores, vm_memory_mb)?;
            }
            PerformanceProfile::Productivity => {
                self.apply_productivity_profile(&mut result, &topology, vm_cores, vm_memory_mb)?;
            }
            PerformanceProfile::Compute => {
                self.apply_compute_profile(&mut result, &topology, vm_cores, vm_memory_mb)?;
            }
            PerformanceProfile::Default => {
                self.reset_to_defaults(&mut result)?;
            }
        }

        Ok(result)
    }

    fn apply_gaming_profile(
        &self,
        result: &mut OptimizationResult,
        topology: &CpuTopology,
        vm_cores: u32,
        vm_memory_mb: u64,
    ) -> Result<()> {
        log_info!("Applying gaming performance profile...");

        // 1. Set CPU governor to performance
        if self.set_cpu_governor("performance").is_ok() {
            result.cpu_governor_set = true;
            result
                .applied_changes
                .push("CPU governor set to 'performance'".to_string());
        }

        // 2. Configure hugepages
        let hugepages_needed = self.calculate_hugepages(vm_memory_mb);
        if self.setup_hugepages(hugepages_needed).is_ok() {
            result.hugepages_configured = true;
            result.hugepages_count = hugepages_needed;
            result.applied_changes.push(format!(
                "Configured {} hugepages ({}MB)",
                hugepages_needed,
                hugepages_needed * 2
            ));
        }

        // 3. Disable PCIe power management
        if self.disable_pcie_power_management().is_ok() {
            result.pcie_power_disabled = true;
            result
                .applied_changes
                .push("PCIe power management disabled".to_string());
        }

        // 4. Recommend CPU isolation based on topology
        result.cpu_isolation_recommended = self.recommend_cpu_isolation(topology, vm_cores);

        // 5. Build kernel parameter recommendations
        self.build_gaming_kernel_params(result, topology);

        // 6. Disable USB autosuspend
        if self.disable_usb_autosuspend().is_ok() {
            result
                .applied_changes
                .push("USB autosuspend disabled".to_string());
        }

        Ok(())
    }

    fn apply_productivity_profile(
        &self,
        result: &mut OptimizationResult,
        topology: &CpuTopology,
        vm_cores: u32,
        vm_memory_mb: u64,
    ) -> Result<()> {
        log_info!("Applying productivity performance profile...");

        // 1. Set CPU governor to performance (still beneficial)
        if self.set_cpu_governor("performance").is_ok() {
            result.cpu_governor_set = true;
            result
                .applied_changes
                .push("CPU governor set to 'performance'".to_string());
        }

        // 2. Configure moderate hugepages
        let hugepages_needed = self.calculate_hugepages(vm_memory_mb);
        if self.setup_hugepages(hugepages_needed).is_ok() {
            result.hugepages_configured = true;
            result.hugepages_count = hugepages_needed;
            result.applied_changes.push(format!(
                "Configured {} hugepages ({}MB)",
                hugepages_needed,
                hugepages_needed * 2
            ));
        }

        // 3. Recommend CPU isolation
        result.cpu_isolation_recommended = self.recommend_cpu_isolation(topology, vm_cores);

        Ok(())
    }

    fn apply_compute_profile(
        &self,
        result: &mut OptimizationResult,
        topology: &CpuTopology,
        vm_cores: u32,
        vm_memory_mb: u64,
    ) -> Result<()> {
        log_info!("Applying compute performance profile...");

        // 1. Set CPU governor to performance
        if self.set_cpu_governor("performance").is_ok() {
            result.cpu_governor_set = true;
            result
                .applied_changes
                .push("CPU governor set to 'performance'".to_string());
        }

        // 2. Maximum hugepages for memory-intensive workloads
        let hugepages_needed = self.calculate_hugepages(vm_memory_mb);
        if self.setup_hugepages(hugepages_needed).is_ok() {
            result.hugepages_configured = true;
            result.hugepages_count = hugepages_needed;
            result.applied_changes.push(format!(
                "Configured {} hugepages ({}MB)",
                hugepages_needed,
                hugepages_needed * 2
            ));
        }

        // 3. Disable PCIe power management
        if self.disable_pcie_power_management().is_ok() {
            result.pcie_power_disabled = true;
            result
                .applied_changes
                .push("PCIe power management disabled".to_string());
        }

        // 4. Recommend NUMA-aware CPU isolation
        result.cpu_isolation_recommended = self.recommend_cpu_isolation(topology, vm_cores);

        // 5. Build compute-focused kernel params
        self.build_compute_kernel_params(result, topology);

        Ok(())
    }

    fn reset_to_defaults(&self, result: &mut OptimizationResult) -> Result<()> {
        log_info!("Resetting to default performance settings...");

        // Reset CPU governor to schedutil/ondemand
        if self.set_cpu_governor("schedutil").is_ok() || self.set_cpu_governor("ondemand").is_ok() {
            result
                .applied_changes
                .push("CPU governor reset to default".to_string());
        }

        // Note: hugepages and kernel params require manual reset
        result
            .warnings
            .push("Hugepages and kernel parameters require manual reset or reboot".to_string());

        Ok(())
    }

    /// Set CPU governor for all CPUs
    fn set_cpu_governor(&self, governor: &str) -> Result<()> {
        log_debug!("Setting CPU governor to '{}'", governor);

        // Check if cpupower is available
        let cpupower_result = Command::new("cpupower")
            .args(["frequency-set", "-g", governor])
            .output();

        if let Ok(output) = cpupower_result
            && output.status.success()
        {
            log_info!("CPU governor set to '{}' via cpupower", governor);
            return Ok(());
        }

        // Fallback: write directly to sysfs
        let cpu_count = fs::read_dir("/sys/devices/system/cpu")
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("cpu"))
                    .filter(|e| e.path().join("cpufreq").exists())
                    .count()
            })
            .unwrap_or(0);

        let mut success_count = 0;
        for i in 0..cpu_count {
            let governor_path =
                format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", i);
            if fs::write(&governor_path, governor).is_ok() {
                success_count += 1;
            }
        }

        if success_count > 0 {
            log_info!(
                "CPU governor set to '{}' for {}/{} CPUs",
                governor,
                success_count,
                cpu_count
            );
            Ok(())
        } else {
            log_warn!("Failed to set CPU governor - may require root privileges");
            Err(NovaError::SystemCommandFailed)
        }
    }

    /// Calculate required hugepages for VM memory
    fn calculate_hugepages(&self, vm_memory_mb: u64) -> u64 {
        // Each hugepage is 2MB
        // Add 10% overhead for QEMU/IVSHMEM
        let pages = (vm_memory_mb / 2) * 110 / 100;
        // Round up to next 64 for alignment
        pages.div_ceil(64) * 64
    }

    /// Setup hugepages
    fn setup_hugepages(&self, count: u64) -> Result<()> {
        log_debug!("Setting up {} hugepages", count);

        let hugepages_path = "/sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages";

        // Try direct write first
        if fs::write(hugepages_path, count.to_string()).is_ok() {
            log_info!("Configured {} hugepages", count);
            return Ok(());
        }

        // Fallback to sudo
        let result = Command::new("sh")
            .arg("-c")
            .arg(format!("echo {} | sudo tee {}", count, hugepages_path))
            .output();

        match result {
            Ok(output) if output.status.success() => {
                log_info!("Configured {} hugepages via sudo", count);
                Ok(())
            }
            _ => {
                log_warn!("Failed to configure hugepages - may require root privileges");
                Err(NovaError::SystemCommandFailed)
            }
        }
    }

    /// Disable PCIe power management for all devices
    fn disable_pcie_power_management(&self) -> Result<()> {
        log_debug!("Disabling PCIe power management");

        let pci_path = Path::new("/sys/bus/pci/devices");
        if !pci_path.exists() {
            return Err(NovaError::SystemCommandFailed);
        }

        let mut success_count = 0;
        if let Ok(entries) = fs::read_dir(pci_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let control_path = entry.path().join("power/control");
                if control_path.exists() && fs::write(&control_path, "on").is_ok() {
                    success_count += 1;
                }
            }
        }

        if success_count > 0 {
            log_info!(
                "Disabled PCIe power management for {} devices",
                success_count
            );
            Ok(())
        } else {
            Err(NovaError::SystemCommandFailed)
        }
    }

    /// Disable USB autosuspend
    fn disable_usb_autosuspend(&self) -> Result<()> {
        log_debug!("Disabling USB autosuspend");

        let usb_path = Path::new("/sys/bus/usb/devices");
        if !usb_path.exists() {
            return Err(NovaError::SystemCommandFailed);
        }

        let mut success_count = 0;
        if let Ok(entries) = fs::read_dir(usb_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let control_path = entry.path().join("power/control");
                if control_path.exists() && fs::write(&control_path, "on").is_ok() {
                    success_count += 1;
                }
            }
        }

        if success_count > 0 {
            log_info!("Disabled USB autosuspend for {} devices", success_count);
            Ok(())
        } else {
            Err(NovaError::SystemCommandFailed)
        }
    }

    /// Recommend CPUs to isolate for VM pinning
    fn recommend_cpu_isolation(&self, topology: &CpuTopology, vm_cores: u32) -> Vec<u32> {
        // Strategy: Reserve physical cores for VM, keep some for host
        // Prefer to keep CPU 0 and its SMT sibling for host

        let host_reserved = 2.min(topology.physical_cores);
        let available_physical = topology.physical_cores.saturating_sub(host_reserved);
        let cores_to_isolate = vm_cores.min(available_physical);

        let mut isolated_cpus = Vec::new();
        let mut allocated = 0;

        // Sort core IDs and skip the first `host_reserved` cores
        let mut core_ids: Vec<_> = topology.core_map.keys().copied().collect();
        core_ids.sort();

        for core_id in core_ids.into_iter().skip(host_reserved as usize) {
            if allocated >= cores_to_isolate {
                break;
            }
            if let Some(cpus) = topology.core_map.get(&core_id) {
                isolated_cpus.extend(cpus.iter().copied());
                allocated += 1;
            }
        }

        isolated_cpus.sort();
        isolated_cpus
    }

    /// Build gaming-focused kernel parameter recommendations
    fn build_gaming_kernel_params(&self, result: &mut OptimizationResult, topology: &CpuTopology) {
        let mut params = Vec::new();

        // IOMMU settings
        match topology.vendor {
            CpuVendor::Intel => {
                params.push("intel_iommu=on".to_string());
            }
            CpuVendor::Amd => {
                params.push("amd_iommu=on".to_string());
            }
            _ => {}
        }
        params.push("iommu=pt".to_string());

        // CPU isolation (if recommended)
        if !result.cpu_isolation_recommended.is_empty() {
            let cpus = result
                .cpu_isolation_recommended
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",");
            params.push(format!("isolcpus={}", cpus));
            params.push(format!("nohz_full={}", cpus));
            params.push(format!("rcu_nocbs={}", cpus));
        }

        // Disable CPU power saving for latency
        params.push("processor.max_cstate=1".to_string());
        if topology.vendor == CpuVendor::Intel {
            params.push("intel_idle.max_cstate=0".to_string());
            params.push("intel_pstate=disable".to_string());
        }

        // Hugepages
        params.push("default_hugepagesz=2M".to_string());
        params.push("hugepagesz=2M".to_string());
        params.push("transparent_hugepage=never".to_string());

        // Disable watchdog for less jitter
        params.push("nmi_watchdog=0".to_string());

        result.kernel_params_needed = params;
    }

    /// Build compute-focused kernel parameter recommendations
    fn build_compute_kernel_params(&self, result: &mut OptimizationResult, topology: &CpuTopology) {
        let mut params = Vec::new();

        // IOMMU settings
        match topology.vendor {
            CpuVendor::Intel => {
                params.push("intel_iommu=on".to_string());
            }
            CpuVendor::Amd => {
                params.push("amd_iommu=on".to_string());
            }
            _ => {}
        }
        params.push("iommu=pt".to_string());

        // CPU isolation
        if !result.cpu_isolation_recommended.is_empty() {
            let cpus = result
                .cpu_isolation_recommended
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(",");
            params.push(format!("isolcpus={}", cpus));
            params.push(format!("nohz_full={}", cpus));
        }

        // Hugepages
        params.push("default_hugepagesz=2M".to_string());
        params.push("hugepagesz=2M".to_string());

        result.kernel_params_needed = params;
    }

    // Helper functions

    fn get_cpu_model(&self) -> String {
        if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
            for line in cpuinfo.lines() {
                if line.starts_with("model name")
                    && let Some(model) = line.split(':').nth(1)
                {
                    return model.trim().to_string();
                }
            }
        }
        "Unknown CPU".to_string()
    }

    fn count_numa_nodes(&self) -> u32 {
        let numa_path = Path::new("/sys/devices/system/node");
        if !numa_path.exists() {
            return 1;
        }

        fs::read_dir(numa_path)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_name().to_string_lossy().starts_with("node"))
                    .count() as u32
            })
            .unwrap_or(1)
    }

    fn read_sysfs_u32(&self, path: &str) -> Option<u32> {
        fs::read_to_string(path).ok().and_then(|content| {
            // Handle ranges like "0-15" -> return max
            if content.contains('-') {
                content.trim().split('-').next_back()?.parse().ok()
            } else {
                content.trim().parse().ok()
            }
        })
    }

    fn read_sysfs_value<T: std::str::FromStr>(&self, path: &str) -> Result<T> {
        fs::read_to_string(path)
            .map_err(|_| NovaError::SystemCommandFailed)?
            .trim()
            .parse()
            .map_err(|_| NovaError::SystemCommandFailed)
    }

    /// Generate CPU pinning XML for libvirt based on topology
    pub fn generate_cpu_pinning_xml(&mut self, vm_cores: u32) -> Result<String> {
        let topology = self.detect_topology()?.clone();
        let isolated = self.recommend_cpu_isolation(&topology, vm_cores);

        if isolated.is_empty() {
            return Err(NovaError::ConfigError(
                "No CPUs available for isolation".to_string(),
            ));
        }

        let mut xml = String::new();
        xml.push_str(&format!("<vcpu placement='static'>{}</vcpu>\n", vm_cores));
        xml.push_str("<cputune>\n");

        for (vcpu_id, &host_cpu) in isolated.iter().take(vm_cores as usize).enumerate() {
            xml.push_str(&format!(
                "  <vcpupin vcpu='{}' cpuset='{}'/>\n",
                vcpu_id, host_cpu
            ));
        }

        // Pin emulator to first host CPUs (not isolated)
        xml.push_str("  <emulatorpin cpuset='0,1'/>\n");
        xml.push_str("</cputune>\n");

        Ok(xml)
    }

    /// Check current system performance state
    pub fn check_current_state(&self) -> PerformanceState {
        let governor = self.get_current_governor();
        let hugepages = self.get_current_hugepages();
        let iommu_enabled = Path::new("/sys/kernel/iommu_groups").exists();

        PerformanceState {
            cpu_governor: governor,
            hugepages_configured: hugepages,
            iommu_enabled,
        }
    }

    fn get_current_governor(&self) -> Option<String> {
        let path = "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor";
        fs::read_to_string(path).ok().map(|s| s.trim().to_string())
    }

    fn get_current_hugepages(&self) -> u64 {
        let path = "/sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages";
        fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0)
    }
}

impl Default for PerformanceOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Current performance state of the system
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceState {
    pub cpu_governor: Option<String>,
    pub hugepages_configured: u64,
    pub iommu_enabled: bool,
}

impl std::fmt::Display for PerformanceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Current Performance State ===")?;
        writeln!(
            f,
            "CPU Governor: {}",
            self.cpu_governor.as_deref().unwrap_or("unknown")
        )?;
        writeln!(
            f,
            "Hugepages: {} ({}MB)",
            self.hugepages_configured,
            self.hugepages_configured * 2
        )?;
        writeln!(
            f,
            "IOMMU: {}",
            if self.iommu_enabled {
                "enabled"
            } else {
                "disabled"
            }
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hugepage_calculation() {
        let optimizer = PerformanceOptimizer::new();

        // 16GB VM
        let pages = optimizer.calculate_hugepages(16 * 1024);
        assert!(pages >= 8192, "16GB should need at least 8192 pages");

        // 8GB VM
        let pages = optimizer.calculate_hugepages(8 * 1024);
        assert!(pages >= 4096, "8GB should need at least 4096 pages");
    }

    #[test]
    fn test_profile_display() {
        assert_eq!(PerformanceProfile::Gaming.to_string(), "gaming");
        assert_eq!(PerformanceProfile::Productivity.to_string(), "productivity");
    }
}
