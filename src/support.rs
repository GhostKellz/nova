use crate::{
    NovaError, Result,
    gpu_passthrough::{GpuCapabilities, GpuGeneration, GpuManager},
    log_info, log_warn,
    prometheus::PrometheusExporter,
};
use chrono::Utc;
use flate2::{Compression, write::GzEncoder};
use nix::unistd;
use regex::Regex;
use serde::Serialize;
use std::fmt;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tar::Builder;
use tempfile::tempdir;
use tokio::task;

#[derive(Debug, Clone)]
pub struct SupportBundleOptions {
    pub output_dir: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub include_logs: bool,
    pub include_system: bool,
    pub include_metrics: bool,
    pub include_diagnostics: bool,
    pub redact: bool,
}

impl Default for SupportBundleOptions {
    fn default() -> Self {
        Self {
            output_dir: None,
            config_path: None,
            include_logs: true,
            include_system: true,
            include_metrics: true,
            include_diagnostics: true,
            redact: false,
        }
    }
}

pub async fn generate_support_bundle(mut opts: SupportBundleOptions) -> Result<PathBuf> {
    let metrics_snapshot = if opts.include_metrics {
        let exporter = PrometheusExporter::new(0);
        match exporter.collect_once().await {
            Ok(snapshot) => Some(snapshot),
            Err(err) => {
                log_warn!("Failed to collect Prometheus metrics: {}", err);
                opts.include_metrics = false;
                None
            }
        }
    } else {
        None
    };

    let bundle_path = task::spawn_blocking(move || create_support_bundle(opts, metrics_snapshot))
        .await
        .map_err(|err| NovaError::IoError(io::Error::new(io::ErrorKind::Other, err.to_string())))?;

    bundle_path
}

fn create_support_bundle(
    opts: SupportBundleOptions,
    metrics_snapshot: Option<String>,
) -> Result<PathBuf> {
    let temp_dir = tempdir()?;
    let bundle_root = temp_dir.path();

    if opts.include_system {
        collect_system_info(bundle_root, opts.redact)?;
    }

    let metrics_snapshot_bytes =
        collect_nova_state(bundle_root, &opts, metrics_snapshot.as_deref())?;

    if opts.include_logs {
        collect_logs(bundle_root, opts.redact)?;
    }

    write_manifest(bundle_root, &opts, metrics_snapshot_bytes)?;

    let output_dir = opts.output_dir.unwrap_or_else(|| std::env::temp_dir());
    fs::create_dir_all(&output_dir)?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let bundle_path = output_dir.join(format!("nova-support-{}.tar.gz", timestamp));

    let tar_file = File::create(&bundle_path)?;
    let encoder = GzEncoder::new(tar_file, Compression::default());
    let mut builder = Builder::new(encoder);
    builder.append_dir_all("nova-support", bundle_root)?;
    let encoder = builder.into_inner()?;
    encoder.finish()?;

    temp_dir.close().ok();

    log_info!("Support bundle generated at {}", bundle_path.display());
    Ok(bundle_path)
}

fn collect_system_info(root: &Path, redact: bool) -> Result<()> {
    let system_dir = root.join("system");
    fs::create_dir_all(&system_dir)?;

    write_command_output(system_dir.join("uname.txt"), "uname", &["-a"], redact)?;
    write_command_output(
        system_dir.join("lsb_release.txt"),
        "lsb_release",
        &["-a"],
        redact,
    )?;
    write_command_output(system_dir.join("lsmod.txt"), "lsmod", &[], redact)?;
    write_command_output(system_dir.join("lsblk.txt"), "lsblk", &["-f"], redact)?;
    write_command_output(system_dir.join("df.txt"), "df", &["-h"], redact)?;

    if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo") {
        fs::write(
            system_dir.join("cpuinfo.txt"),
            maybe_redact(&cpuinfo, redact),
        )?;
    }

    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        fs::write(
            system_dir.join("meminfo.txt"),
            maybe_redact(&meminfo, redact),
        )?;
    }

    Ok(())
}

fn collect_nova_state(
    root: &Path,
    opts: &SupportBundleOptions,
    metrics_snapshot: Option<&str>,
) -> Result<Option<usize>> {
    let nova_dir = root.join("nova");
    fs::create_dir_all(&nova_dir)?;
    let mut metrics_snapshot_bytes = None;

    if let Some(config_path) = opts.config_path.as_ref() {
        if config_path.exists() {
            let dest = nova_dir.join(
                config_path
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("NovaFile")),
            );
            fs::copy(config_path, &dest)?;
        }
    }

    write_command_output(
        nova_dir.join("virsh_list.txt"),
        "virsh",
        &["list", "--all"],
        opts.redact,
    )?;
    write_command_output(
        nova_dir.join("docker_ps.txt"),
        "docker",
        &["ps", "-a"],
        opts.redact,
    )?;

    collect_gpu_snapshot(&nova_dir, opts.redact)?;

    if let Some(metrics) = metrics_snapshot {
        let observability_dir = nova_dir.join("observability");
        fs::create_dir_all(&observability_dir)?;
        fs::write(
            observability_dir.join("prometheus-metrics.txt"),
            maybe_redact(metrics, opts.redact),
        )?;
        metrics_snapshot_bytes = Some(metrics.as_bytes().len());
    }

    if opts.include_diagnostics {
        match diagnostics_inner() {
            Ok(report) => {
                let diag_dir = nova_dir.join("diagnostics");
                fs::create_dir_all(&diag_dir)?;
                let text_report = report.to_string();
                fs::write(
                    diag_dir.join("report.txt"),
                    maybe_redact(&text_report, opts.redact),
                )?;
                let json_report = serde_json::to_string_pretty(&report)?;
                fs::write(diag_dir.join("report.json"), json_report)?;
            }
            Err(err) => {
                log_warn!("Diagnostics capture failed: {}", err);
            }
        }
    }

    Ok(metrics_snapshot_bytes)
}

fn collect_gpu_snapshot(nova_dir: &Path, redact: bool) -> Result<()> {
    let payload = if let Ok(fake) = std::env::var("NOVA_FAKE_GPU_CAPS") {
        if fake.trim().is_empty() {
            None
        } else {
            Some(fake)
        }
    } else {
        let mut manager = GpuManager::new();
        match manager.discover() {
            Ok(_) => {
                let mut snapshot = Vec::new();
                for gpu in manager.list_gpus() {
                    let caps = manager.capabilities_for(&gpu.address);
                    snapshot.push(SupportGpuSnapshot::from(gpu, caps));
                }

                if snapshot.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string_pretty(&snapshot)?)
                }
            }
            Err(err) => {
                log_warn!("GPU discovery failed while building bundle: {}", err);
                None
            }
        }
    };

    if let Some(json) = payload {
        fs::write(
            nova_dir.join("gpu-capabilities.json"),
            maybe_redact(&json, redact),
        )?;
    }

    Ok(())
}

fn collect_logs(root: &Path, redact: bool) -> Result<()> {
    let logs_dir = root.join("logs");
    fs::create_dir_all(&logs_dir)?;

    write_command_output(
        logs_dir.join("journalctl-nova.txt"),
        "journalctl",
        &["-u", "nova", "--since", "-24h"],
        redact,
    )?;
    write_command_output(
        logs_dir.join("journalctl-nova-metrics.txt"),
        "journalctl",
        &["-u", "nova-metrics", "--since", "-24h"],
        redact,
    )?;
    write_command_output(logs_dir.join("dmesg.txt"), "dmesg", &["-T"], redact)?;

    Ok(())
}

fn write_manifest(
    root: &Path,
    opts: &SupportBundleOptions,
    metrics_snapshot_bytes: Option<usize>,
) -> Result<()> {
    let manifest = BundleManifest {
        generated_at: Utc::now().to_rfc3339(),
        nova_version: env!("CARGO_PKG_VERSION").to_string(),
        include_logs: opts.include_logs,
        include_system: opts.include_system,
        include_metrics: opts.include_metrics,
        redact_applied: opts.redact,
        host: get_hostname(),
        metrics_snapshot_bytes,
    };

    let manifest_path = root.join("manifest.json");
    let payload = serde_json::to_string_pretty(&manifest)?;
    fs::write(manifest_path, payload)?;
    Ok(())
}

fn write_command_output(path: PathBuf, command: &str, args: &[&str], redact: bool) -> Result<()> {
    if let Some(output) = run_command(command, args) {
        fs::write(path, maybe_redact(&output, redact))?;
    }
    Ok(())
}

fn run_command(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn maybe_redact(content: &str, redact: bool) -> String {
    if !redact {
        return content.to_string();
    }

    static IP_REGEX: OnceLock<Regex> = OnceLock::new();
    static MAC_REGEX: OnceLock<Regex> = OnceLock::new();

    let ip_regex =
        IP_REGEX.get_or_init(|| Regex::new(r"(?:\d{1,3}\.){3}\d{1,3}").expect("valid IP regex"));
    let mac_regex = MAC_REGEX.get_or_init(|| {
        Regex::new(r"(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}").expect("valid MAC regex")
    });

    let sanitized = ip_regex.replace_all(content, "REDACTED.IP");
    mac_regex
        .replace_all(&sanitized, "REDACTED.MAC")
        .to_string()
}

fn collect_command_bool(command: &str, args: &[&str]) -> bool {
    Command::new(command)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn get_hostname() -> Option<String> {
    unistd::gethostname()
        .ok()
        .and_then(|name| name.to_str().map(|s| s.to_string()))
}

#[derive(Debug, Serialize)]
struct BundleManifest {
    generated_at: String,
    nova_version: String,
    include_logs: bool,
    include_system: bool,
    include_metrics: bool,
    redact_applied: bool,
    host: Option<String>,
    metrics_snapshot_bytes: Option<usize>,
}

#[derive(Debug, Serialize)]
struct SupportGpuSnapshot {
    address: String,
    vendor: String,
    name: String,
    generation: Option<GpuGeneration>,
    vram_mb: Option<u64>,
    minimum_driver: Option<String>,
    recommended_kernel: Option<String>,
    tcc_supported: bool,
}

impl SupportGpuSnapshot {
    fn from(gpu: &crate::gpu_passthrough::PciDevice, caps: Option<&GpuCapabilities>) -> Self {
        SupportGpuSnapshot {
            address: gpu.address.clone(),
            vendor: gpu.vendor_name.clone(),
            name: gpu.device_name.clone(),
            generation: caps.and_then(|c| c.generation.clone()),
            vram_mb: caps.and_then(|c| c.vram_mb),
            minimum_driver: caps.and_then(|c| c.minimum_driver.clone()),
            recommended_kernel: caps.and_then(|c| c.recommended_kernel.clone()),
            tcc_supported: caps.map(|c| c.tcc_supported).unwrap_or(false),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DiagnosticReport {
    pub kvm_available: bool,
    pub iommu_enabled: bool,
    pub vfio_loaded: bool,
    pub libvirt_available: bool,
    pub docker_available: bool,
    pub gpu_count: usize,
    pub issues: Vec<String>,
}

impl DiagnosticReport {
    pub fn is_healthy(&self) -> bool {
        self.issues.is_empty()
    }
}

impl fmt::Display for DiagnosticReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Nova System Diagnostic Report")?;
        writeln!(f, "==============================")?;
        writeln!(f, "KVM available: {}", status_str(self.kvm_available))?;
        writeln!(f, "IOMMU enabled: {}", status_str(self.iommu_enabled))?;
        writeln!(f, "VFIO loaded: {}", status_str(self.vfio_loaded))?;
        writeln!(
            f,
            "libvirt reachable: {}",
            status_str(self.libvirt_available)
        )?;
        writeln!(f, "Docker reachable: {}", status_str(self.docker_available))?;
        writeln!(f, "GPUs detected: {}", self.gpu_count)?;

        if self.issues.is_empty() {
            writeln!(
                f,
                "\nNo issues detected. System is ready for Nova workflows."
            )
        } else {
            writeln!(f, "\nIssues:")?;
            for issue in &self.issues {
                writeln!(f, "  - {}", issue)?;
            }
            Ok(())
        }
    }
}

fn status_str(value: bool) -> &'static str {
    if value { "OK" } else { "Missing" }
}

pub async fn run_diagnostics() -> Result<DiagnosticReport> {
    task::spawn_blocking(|| diagnostics_inner())
        .await
        .map_err(|err| NovaError::IoError(io::Error::new(io::ErrorKind::Other, err.to_string())))?
}

fn diagnostics_inner() -> Result<DiagnosticReport> {
    let kvm_available = Path::new("/dev/kvm").exists();
    let iommu_enabled = Path::new("/sys/kernel/iommu_groups").exists();
    let vfio_loaded = Path::new("/sys/module/vfio_pci").exists();
    let libvirt_available = collect_command_bool("virsh", &["version"]);
    let docker_available = collect_command_bool("docker", &["info"]);

    let mut issues = Vec::new();

    if !kvm_available {
        issues.push("/dev/kvm not present".to_string());
    }
    if !iommu_enabled {
        issues.push("IOMMU not enabled or iommu_groups missing".to_string());
    }
    if !vfio_loaded {
        issues.push("vfio_pci kernel module not loaded".to_string());
    }
    if !libvirt_available {
        issues.push("Unable to contact libvirt via virsh".to_string());
    }
    if !docker_available {
        issues.push("Docker CLI unavailable or not responding".to_string());
    }

    let mut gpu_manager = GpuManager::new();
    let gpu_count = match gpu_manager.discover() {
        Ok(_) => gpu_manager.list_gpus().len(),
        Err(err) => {
            issues.push(format!("GPU discovery failed: {}", err));
            0
        }
    };

    if gpu_count == 0 {
        issues.push("No GPUs detected for passthrough".to_string());
    }

    Ok(DiagnosticReport {
        kvm_available,
        iommu_enabled,
        vfio_loaded,
        libvirt_available,
        docker_available,
        gpu_count,
        issues,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use serde_json::Value;
    use std::fs::File;
    use std::io::Read;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn redaction_masks_ip_and_mac_addresses() {
        let sample = "Device at 192.168.1.100 with MAC aa:bb:cc:dd:ee:ff";
        let redacted = maybe_redact(sample, true);

        assert!(!redacted.contains("192.168.1.100"));
        assert!(!redacted.contains("aa:bb:cc:dd:ee:ff"));
        assert!(redacted.contains("REDACTED.IP"));
        assert!(redacted.contains("REDACTED.MAC"));
    }

    #[tokio::test]
    async fn support_bundle_uses_fake_gpu_snapshot_and_manifest() {
        let output_dir = tempdir().expect("tempdir for bundle output");
        let config_dir = tempdir().expect("tempdir for config");
        let config_path = config_dir.path().join("NovaFile.toml");
        std::fs::write(&config_path, "gpu = \"test\"\n").expect("config write");

        let fake_caps = r#"[
            {"address": "0000:65:00.0", "vendor": "Test Vendor", "name": "Test GPU"}
        ]"#;
        unsafe {
            std::env::set_var("NOVA_FAKE_GPU_CAPS", fake_caps);
        }

        let mut opts = SupportBundleOptions::default();
        opts.output_dir = Some(output_dir.path().to_path_buf());
        opts.config_path = Some(config_path.clone());
        opts.include_logs = false;
        opts.include_system = false;
        opts.include_metrics = false;
        opts.include_diagnostics = false;

        let bundle_path = generate_support_bundle(opts).await.expect("bundle path");
        assert!(bundle_path.exists());

        let file = File::open(&bundle_path).expect("open bundle");
        let decoder = GzDecoder::new(file);
        let mut archive = tar::Archive::new(decoder);

        let mut found_gpu_caps = false;
        let mut found_manifest = false;
        let mut found_config_copy = false;

        for entry in archive.entries().expect("entries") {
            let mut entry = entry.expect("entry");
            let path = entry.path().expect("entry path").into_owned();
            let path_str = path.to_string_lossy();

            if path_str.ends_with("gpu-capabilities.json") {
                found_gpu_caps = true;
                let mut contents = String::new();
                entry.read_to_string(&mut contents).expect("read gpu json");
                assert!(contents.contains("Test Vendor"));
                assert!(contents.contains("Test GPU"));
            } else if path_str.ends_with("manifest.json") {
                found_manifest = true;
                let mut contents = String::new();
                entry.read_to_string(&mut contents).expect("read manifest");
                let manifest: Value = serde_json::from_str(&contents).expect("manifest json");
                assert_eq!(manifest["include_logs"], Value::Bool(false));
                assert_eq!(manifest["include_system"], Value::Bool(false));
                assert_eq!(manifest["include_metrics"], Value::Bool(false));
                assert_eq!(manifest["metrics_snapshot_bytes"], Value::Null);
            } else if path_str.ends_with(
                Path::new("nova")
                    .join(config_path.file_name().unwrap_or_default())
                    .to_string_lossy()
                    .as_ref(),
            ) {
                found_config_copy = true;
            }
        }

        unsafe {
            std::env::remove_var("NOVA_FAKE_GPU_CAPS");
        }

        assert!(
            found_gpu_caps,
            "expected GPU capabilities snapshot in bundle"
        );
        assert!(found_manifest, "expected manifest.json in bundle");
        assert!(found_config_copy, "expected NovaFile copy inside bundle");
    }
}
