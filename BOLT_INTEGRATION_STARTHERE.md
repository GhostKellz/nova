# 🚀 Nova Integration Guide - START HERE

Welcome to the Bolt-Nova integration! This guide will get you up and running with Bolt containers in your Nova Velocity Manager.

## Quick Start

### 1. Add Bolt as a Dependency

Add this to your Nova project's `Cargo.toml`:

```toml
[dependencies]
bolt-runtime = { git = "https://github.com/CK-Technology/bolt", features = ["nvidia-support", "quic-networking"] }
tokio = { version = "1.0", features = ["full"] }
```

### 2. Basic Integration

```rust
use bolt::api::*;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize Bolt runtime
    let runtime = BoltNovaRuntime::new().await?;

    // Create container configuration from NovaFile
    let mut env = std::collections::HashMap::new();
    env.insert("API_KEY".to_string(), "secret".to_string());

    let config = NovaContainerConfig {
        capsule: "ubuntu:22.04".to_string(),
        volumes: vec!["./api:/srv/api".to_string()],
        network: "nova-net".to_string(),
        env,
        gpu_passthrough: true,
        memory_mb: Some(2048),
        cpus: Some(2),
    };

    // Start the container
    let handle = runtime.start_capsule("api", &config).await?;
    println!("Container started: {}", handle.name);

    // Get status and metrics
    let status = runtime.get_capsule_status("api").await?;
    let metrics = runtime.get_capsule_metrics("api").await?;

    Ok(())
}
```

### 3. NovaFile Configuration Mapping

Map your NovaFile containers to Bolt seamlessly:

```toml
# NovaFile
[container.api]
capsule = "ubuntu:22.04"
volumes = ["./api:/srv/api"]
network = "nova-net"
env.API_KEY = "secret"
gpu_passthrough = true
memory_mb = 2048
cpus = 2
```

Becomes:

```rust
let mut env = std::collections::HashMap::new();
env.insert("API_KEY".to_string(), "secret".to_string());

let config = NovaContainerConfig {
    capsule: "ubuntu:22.04".to_string(),
    volumes: vec!["./api:/srv/api".to_string()],
    network: "nova-net".to_string(),
    env,
    gpu_passthrough: true,
    memory_mb: Some(2048),
    cpus: Some(2),
};
```

## Core Features

### 🔌 Network Integration

Connect Bolt containers to Nova's bridge networks:

```rust
use bolt::api::*;

// Initialize bridge manager
let mut bridge_manager = NovaBridgeManager::new();

// Create Nova bridge
let bridge_config = NovaBridgeConfig {
    name: "nova-br0".to_string(),
    subnet: "172.20.0.0/16".to_string(),
    gateway: "172.20.0.1".to_string(),
    dns_servers: vec!["8.8.8.8".to_string()],
    mtu: 1500,
    enable_quic: true,
};

bridge_manager.create_bridge(bridge_config).await?;

// Connect container to bridge
let veth = bridge_manager.connect_container("nova-br0", &container_id).await?;
```

### 🎮 Gaming Container Support

Full GPU passthrough and gaming optimizations:

```rust
let gaming_config = NovaContainerConfig {
    capsule: "steam:latest".to_string(),
    volumes: vec![
        "/home/user/Games:/games".to_string(),
        "/tmp/.X11-unix:/tmp/.X11-unix".to_string(),
    ],
    network: "nova-br0".to_string(),
    env: {
        let mut env = std::collections::HashMap::new();
        env.insert("DISPLAY".to_string(), ":0".to_string());
        env.insert("PULSE_RUNTIME_PATH".to_string(), "/run/user/1000/pulse".to_string());
        env
    },
    gpu_passthrough: true,
    memory_mb: Some(8192),
    cpus: Some(4),
};

let handle = runtime.start_capsule("gaming", &gaming_config).await?;
runtime.configure_gpu_passthrough("gaming", "nvidia0").await?;
```

### 📊 Resource Monitoring

Real-time metrics for your Nova GUI:

```rust
// Get container metrics
let metrics = runtime.get_capsule_metrics("container-name").await?;

println!("CPU: {:.1}%", metrics.cpu_usage_percent);
println!("Memory: {} / {} MB", metrics.memory_usage_mb, metrics.memory_limit_mb);
println!("Network: RX: {} MB, TX: {} MB",
         metrics.network_rx_bytes / 1024 / 1024,
         metrics.network_tx_bytes / 1024 / 1024);
```

### 🔍 Service Discovery

Integrate with Nova's DNS and service discovery:

```rust
let mut service_discovery = NovaServiceDiscovery::new();

// Register a service
let service = ServiceEntry {
    name: "api-service".to_string(),
    container_id: handle.id,
    ip_address: "172.20.0.10".to_string(),
    ports: vec![80, 443],
    metadata: [("environment".to_string(), "production".to_string())].into(),
};

service_discovery.register_service(service)?;

// Lookup services
let service = service_discovery.lookup_service("api-service");
```

## Lifecycle Management

### Container Operations

```rust
// Start container
let handle = runtime.start_capsule("name", &config).await?;

// Stop container
runtime.stop_capsule("name").await?;

// Restart container
runtime.restart_capsule("name").await?;

// Remove container
runtime.remove_capsule("name", true).await?;

// List all containers
let containers = runtime.list_capsules().await?;
```

### Status Monitoring

```rust
use bolt::api::NovaStatus;

match runtime.get_capsule_status("name").await? {
    NovaStatus::Running => println!("✅ Container is running"),
    NovaStatus::Stopped => println!("⏹️ Container is stopped"),
    NovaStatus::Starting => println!("🔄 Container is starting"),
    NovaStatus::Error(msg) => println!("❌ Error: {}", msg),
}
```

## Integration with Nova GUI

### GUI Integration Pattern

```rust
pub struct NovaContainerManager {
    runtime: BoltNovaRuntime,
    bridge_manager: NovaBridgeManager,
    service_discovery: NovaServiceDiscovery,
}

impl NovaContainerManager {
    pub async fn new() -> anyhow::Result<Self> {
        Ok(Self {
            runtime: BoltNovaRuntime::new().await?,
            bridge_manager: NovaBridgeManager::new(),
            service_discovery: NovaServiceDiscovery::new(),
        })
    }

    // GUI calls this to create containers
    pub async fn create_from_nova_config(
        &self,
        name: String,
        toml_config: &str,
    ) -> anyhow::Result<CapsuleHandle> {
        // Parse TOML, convert to NovaContainerConfig
        let config = self.parse_nova_config(toml_config)?;
        self.runtime.start_capsule(&name, &config).await
    }

    // GUI calls this for status display
    pub async fn get_status_for_gui(&self, name: &str) -> GuiContainerStatus {
        let status = self.runtime.get_capsule_status(name).await.unwrap_or(NovaStatus::Error("Unknown".to_string()));
        let metrics = self.runtime.get_capsule_metrics(name).await.unwrap_or_default();

        GuiContainerStatus { status, metrics }
    }
}
```

### Error Handling

```rust
use bolt::api::NovaError;

match result {
    Err(NovaError::BoltError(e)) => {
        eprintln!("Bolt runtime error: {}", e);
    },
    Err(NovaError::CapsuleNotFound(name)) => {
        eprintln!("Container '{}' not found", name);
    },
    Err(NovaError::NetworkError(msg)) => {
        eprintln!("Network error: {}", msg);
    },
    Err(NovaError::GpuError(msg)) => {
        eprintln!("GPU configuration error: {}", msg);
    },
    Ok(result) => {
        // Success
    }
}
```

## Examples

Check out the comprehensive example at:
- `examples/nova_integration.rs` - Complete integration walkthrough

## Features Available

✅ **Completed Features:**
- Async/tokio compatibility with Nova's runtime
- Programmatic configuration API
- Resource monitoring and metrics
- Bridge network integration
- Service discovery integration
- GPU passthrough support
- Container lifecycle management
- Error handling integration

🔄 **Tokio Async Compatibility:**
All operations are fully async and compatible with Nova's tokio runtime.

🔗 **Network Integration:**
Seamless integration with Nova's software-defined networking including bridge networks and QUIC overlay support.

📊 **Monitoring:**
Real-time CPU, memory, network, and disk metrics for Nova's GUI.

## API Reference

### Core Types

- `BoltNovaRuntime` - Main runtime handle for Nova
- `NovaContainerConfig` - Configuration from NovaFiles
- `CapsuleHandle` - Handle to a running container
- `NovaStatus` - Unified status enum for GUI
- `CapsuleMetrics` - Resource usage metrics

### Network Types

- `NovaBridgeManager` - Manages Nova bridge networks
- `NovaBridgeConfig` - Bridge network configuration
- `NovaServiceDiscovery` - Service discovery integration

### Error Types

- `NovaError` - Unified error type for integration

## Next Steps

1. **Add to Nova's Cargo.toml**: Include Bolt as a git dependency
2. **Initialize Runtime**: Create `BoltNovaRuntime` in Nova's startup
3. **Parse NovaFiles**: Map container sections to `NovaContainerConfig`
4. **Integrate GUI**: Use the status and metrics APIs for display
5. **Network Setup**: Configure bridge networks for container connectivity
6. **Testing**: Run the integration example to verify everything works

## Support

For questions or issues:
- Check `NOVA_INTEGRATION.md` for detailed requirements
- Review `examples/nova_integration.rs` for usage patterns
- File issues at the Bolt repository

---

**Ready to integrate? Start with the example in `examples/nova_integration.rs` and adapt it to your Nova architecture!**