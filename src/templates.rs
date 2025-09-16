use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerTemplate {
    pub name: String,
    pub description: String,
    pub category: TemplateCategory,
    pub containers: Vec<TemplateContainer>,
    pub networks: Vec<TemplateNetwork>,
    pub volumes: Vec<TemplateVolume>,
    pub recommended_runtime: Option<String>,
    pub requires_gpu: bool,
    pub difficulty: TemplateDifficulty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateContainer {
    pub name: String,
    pub image: String,
    pub ports: Vec<String>,
    pub environment: HashMap<String, String>,
    pub volumes: Vec<String>,
    pub depends_on: Vec<String>,
    pub runtime: Option<String>,
    pub gpu_access: bool,
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateNetwork {
    pub name: String,
    pub driver: String,
    pub subnet: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateVolume {
    pub name: String,
    pub path: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemplateCategory {
    Development,
    WebServices,
    Databases,
    Monitoring,
    AiMl,
    Security,
    Networking,
    Gaming,
    Productivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemplateDifficulty {
    Beginner,
    Intermediate,
    Advanced,
    Expert,
}

pub struct TemplateManager {
    templates: Vec<ContainerTemplate>,
}

impl TemplateManager {
    pub fn new() -> Self {
        let mut manager = Self {
            templates: Vec::new(),
        };
        manager.load_builtin_templates();
        manager
    }

    pub fn get_templates(&self) -> &Vec<ContainerTemplate> {
        &self.templates
    }

    pub fn get_template(&self, name: &str) -> Option<&ContainerTemplate> {
        self.templates.iter().find(|t| t.name == name)
    }

    pub fn get_templates_by_category(&self, category: &TemplateCategory) -> Vec<&ContainerTemplate> {
        self.templates.iter()
            .filter(|t| std::mem::discriminant(&t.category) == std::mem::discriminant(category))
            .collect()
    }

    fn load_builtin_templates(&mut self) {
        // Only load implemented templates for now

        // Development Templates
        self.templates.push(self.create_lamp_stack());

        // Monitoring
        self.templates.push(self.create_monitoring_stack());

        // AI/ML
        self.templates.push(self.create_ml_workspace());

        // Productivity
        self.templates.push(self.create_nextcloud());
    }

    fn create_lamp_stack(&self) -> ContainerTemplate {
        ContainerTemplate {
            name: "lamp-stack".to_string(),
            description: "Complete LAMP (Linux, Apache, MySQL, PHP) development environment".to_string(),
            category: TemplateCategory::Development,
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Beginner,
            networks: vec![
                TemplateNetwork {
                    name: "lamp-network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.20.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "lamp_web".to_string(),
                    path: "./web".to_string(),
                    description: "Web root directory for Apache".to_string(),
                },
                TemplateVolume {
                    name: "lamp_db".to_string(),
                    path: "./mysql-data".to_string(),
                    description: "MySQL database files".to_string(),
                }
            ],
            containers: vec![
                TemplateContainer {
                    name: "mysql".to_string(),
                    image: "mysql:8.0".to_string(),
                    ports: vec!["3306:3306".to_string()],
                    environment: [
                        ("MYSQL_ROOT_PASSWORD".to_string(), "rootpass123".to_string()),
                        ("MYSQL_DATABASE".to_string(), "webapp".to_string()),
                        ("MYSQL_USER".to_string(), "webuser".to_string()),
                        ("MYSQL_PASSWORD".to_string(), "webpass123".to_string()),
                    ].iter().cloned().collect(),
                    volumes: vec!["./mysql-data:/var/lib/mysql".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512Mi".to_string()),
                    cpu_limit: Some("1".to_string()),
                },
                TemplateContainer {
                    name: "apache-php".to_string(),
                    image: "php:8.2-apache".to_string(),
                    ports: vec!["80:80".to_string(), "443:443".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["./web:/var/www/html".to_string()],
                    depends_on: vec!["mysql".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("256Mi".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "phpmyadmin".to_string(),
                    image: "phpmyadmin/phpmyadmin:latest".to_string(),
                    ports: vec!["8080:80".to_string()],
                    environment: [
                        ("PMA_HOST".to_string(), "mysql".to_string()),
                        ("PMA_USER".to_string(), "root".to_string()),
                        ("PMA_PASSWORD".to_string(), "rootpass123".to_string()),
                    ].iter().cloned().collect(),
                    volumes: vec![],
                    depends_on: vec!["mysql".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("128Mi".to_string()),
                    cpu_limit: Some("0.25".to_string()),
                }
            ],
        }
    }

    fn create_ml_workspace(&self) -> ContainerTemplate {
        ContainerTemplate {
            name: "ml-workspace".to_string(),
            description: "GPU-accelerated ML development environment with Jupyter, PyTorch, and TensorFlow".to_string(),
            category: TemplateCategory::AiMl,
            recommended_runtime: Some("bolt".to_string()),
            requires_gpu: true,
            difficulty: TemplateDifficulty::Intermediate,
            networks: vec![
                TemplateNetwork {
                    name: "ml-network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.21.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "ml_notebooks".to_string(),
                    path: "./notebooks".to_string(),
                    description: "Jupyter notebook files".to_string(),
                },
                TemplateVolume {
                    name: "ml_datasets".to_string(),
                    path: "./datasets".to_string(),
                    description: "Training datasets".to_string(),
                },
                TemplateVolume {
                    name: "ml_models".to_string(),
                    path: "./models".to_string(),
                    description: "Trained model files".to_string(),
                }
            ],
            containers: vec![
                TemplateContainer {
                    name: "jupyter-pytorch".to_string(),
                    image: "pytorch/pytorch:latest".to_string(),
                    ports: vec!["8888:8888".to_string()],
                    environment: [
                        ("JUPYTER_ENABLE_LAB".to_string(), "yes".to_string()),
                        ("JUPYTER_TOKEN".to_string(), "nova-ml-workspace".to_string()),
                    ].iter().cloned().collect(),
                    volumes: vec![
                        "./notebooks:/workspace/notebooks".to_string(),
                        "./datasets:/workspace/datasets".to_string(),
                        "./models:/workspace/models".to_string(),
                    ],
                    depends_on: vec![],
                    runtime: Some("bolt".to_string()),
                    gpu_access: true,
                    memory_limit: Some("8Gi".to_string()),
                    cpu_limit: Some("4".to_string()),
                },
                TemplateContainer {
                    name: "tensorboard".to_string(),
                    image: "tensorflow/tensorflow:latest".to_string(),
                    ports: vec!["6006:6006".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["./models:/logs".to_string()],
                    depends_on: vec![],
                    runtime: Some("bolt".to_string()),
                    gpu_access: true,
                    memory_limit: Some("2Gi".to_string()),
                    cpu_limit: Some("1".to_string()),
                },
                TemplateContainer {
                    name: "mlflow".to_string(),
                    image: "python:3.9".to_string(),
                    ports: vec!["5000:5000".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["./models:/mlruns".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512Mi".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                }
            ],
        }
    }

    fn create_monitoring_stack(&self) -> ContainerTemplate {
        ContainerTemplate {
            name: "monitoring-stack".to_string(),
            description: "Complete monitoring solution with Prometheus, Grafana, and Alertmanager".to_string(),
            category: TemplateCategory::Monitoring,
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
            networks: vec![
                TemplateNetwork {
                    name: "monitoring".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.22.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "prometheus_data".to_string(),
                    path: "./prometheus".to_string(),
                    description: "Prometheus time-series data".to_string(),
                },
                TemplateVolume {
                    name: "grafana_data".to_string(),
                    path: "./grafana".to_string(),
                    description: "Grafana dashboards and configuration".to_string(),
                }
            ],
            containers: vec![
                TemplateContainer {
                    name: "prometheus".to_string(),
                    image: "prom/prometheus:latest".to_string(),
                    ports: vec!["9090:9090".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["./prometheus:/prometheus".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1Gi".to_string()),
                    cpu_limit: Some("1".to_string()),
                },
                TemplateContainer {
                    name: "grafana".to_string(),
                    image: "grafana/grafana:latest".to_string(),
                    ports: vec!["3000:3000".to_string()],
                    environment: [
                        ("GF_SECURITY_ADMIN_PASSWORD".to_string(), "admin".to_string()),
                    ].iter().cloned().collect(),
                    volumes: vec!["./grafana:/var/lib/grafana".to_string()],
                    depends_on: vec!["prometheus".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512Mi".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "node-exporter".to_string(),
                    image: "prom/node-exporter:latest".to_string(),
                    ports: vec!["9100:9100".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["/proc:/host/proc:ro".to_string(), "/sys:/host/sys:ro".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("128Mi".to_string()),
                    cpu_limit: Some("0.25".to_string()),
                }
            ],
        }
    }

    fn create_nextcloud(&self) -> ContainerTemplate {
        ContainerTemplate {
            name: "nextcloud".to_string(),
            description: "Self-hosted cloud storage and collaboration platform".to_string(),
            category: TemplateCategory::Productivity,
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Beginner,
            networks: vec![
                TemplateNetwork {
                    name: "nextcloud".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.23.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "nextcloud_data".to_string(),
                    path: "./nextcloud".to_string(),
                    description: "Nextcloud user data and files".to_string(),
                },
                TemplateVolume {
                    name: "postgres_data".to_string(),
                    path: "./postgres".to_string(),
                    description: "PostgreSQL database files".to_string(),
                }
            ],
            containers: vec![
                TemplateContainer {
                    name: "postgres".to_string(),
                    image: "postgres:15".to_string(),
                    ports: vec![],
                    environment: [
                        ("POSTGRES_DB".to_string(), "nextcloud".to_string()),
                        ("POSTGRES_USER".to_string(), "nextcloud".to_string()),
                        ("POSTGRES_PASSWORD".to_string(), "nextcloud123".to_string()),
                    ].iter().cloned().collect(),
                    volumes: vec!["./postgres:/var/lib/postgresql/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512Mi".to_string()),
                    cpu_limit: Some("1".to_string()),
                },
                TemplateContainer {
                    name: "nextcloud".to_string(),
                    image: "nextcloud:latest".to_string(),
                    ports: vec!["80:80".to_string()],
                    environment: [
                        ("POSTGRES_HOST".to_string(), "postgres".to_string()),
                        ("POSTGRES_DB".to_string(), "nextcloud".to_string()),
                        ("POSTGRES_USER".to_string(), "nextcloud".to_string()),
                        ("POSTGRES_PASSWORD".to_string(), "nextcloud123".to_string()),
                    ].iter().cloned().collect(),
                    volumes: vec!["./nextcloud:/var/www/html".to_string()],
                    depends_on: vec!["postgres".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1Gi".to_string()),
                    cpu_limit: Some("1".to_string()),
                }
            ],
        }
    }

    // Placeholder methods for other templates
    fn create_mean_stack(&self) -> ContainerTemplate { todo!() }
    fn create_rust_dev_env(&self) -> ContainerTemplate { todo!() }
    fn create_python_dev_env(&self) -> ContainerTemplate { todo!() }
    fn create_reverse_proxy(&self) -> ContainerTemplate { todo!() }
    fn create_static_website(&self) -> ContainerTemplate { todo!() }
    fn create_wordpress(&self) -> ContainerTemplate { todo!() }
    fn create_postgres_cluster(&self) -> ContainerTemplate { todo!() }
    fn create_redis_cache(&self) -> ContainerTemplate { todo!() }
    fn create_mongodb_replica(&self) -> ContainerTemplate { todo!() }
    fn create_logging_stack(&self) -> ContainerTemplate { todo!() }
    fn create_jupyter_lab(&self) -> ContainerTemplate { todo!() }
    fn create_network_security(&self) -> ContainerTemplate { todo!() }
    fn create_vault_cluster(&self) -> ContainerTemplate { todo!() }
    fn create_pihole_unbound(&self) -> ContainerTemplate { todo!() }
    fn create_wireguard_vpn(&self) -> ContainerTemplate { todo!() }
    fn create_minecraft_server(&self) -> ContainerTemplate { todo!() }
    fn create_game_server_stack(&self) -> ContainerTemplate { todo!() }
    fn create_collaboration_suite(&self) -> ContainerTemplate { todo!() }
}

impl Default for TemplateManager {
    fn default() -> Self {
        Self::new()
    }
}

// Template deployment functionality
impl TemplateManager {
    pub fn deploy_template(&self, template_name: &str, project_name: &str) -> Result<String> {
        let template = self.get_template(template_name)
            .ok_or_else(|| crate::NovaError::InvalidConfig)?;

        let mut nova_file = format!(
            "# Generated NovaFile for {} template\n",
            template_name
        );
        nova_file.push_str(&format!("project = \"{}\"\n\n", project_name));

        // Generate container configurations
        for container in &template.containers {
            nova_file.push_str(&format!("[container.{}]\n", container.name));
            nova_file.push_str(&format!("capsule = \"{}\"\n", container.image));

            if !container.volumes.is_empty() {
                nova_file.push_str("volumes = [\n");
                for volume in &container.volumes {
                    nova_file.push_str(&format!("  \"{}\",\n", volume));
                }
                nova_file.push_str("]\n");
            }

            if !container.ports.is_empty() {
                nova_file.push_str("ports = [\n");
                for port in &container.ports {
                    nova_file.push_str(&format!("  \"{}\",\n", port));
                }
                nova_file.push_str("]\n");
            }

            if let Some(runtime) = &container.runtime {
                nova_file.push_str(&format!("runtime = \"{}\"\n", runtime));
            } else if let Some(runtime) = &template.recommended_runtime {
                nova_file.push_str(&format!("runtime = \"{}\"\n", runtime));
            }

            nova_file.push_str("autostart = true\n");

            // Environment variables
            if !container.environment.is_empty() {
                nova_file.push_str(&format!("\n[container.{}.env]\n", container.name));
                for (key, value) in &container.environment {
                    nova_file.push_str(&format!("{} = \"{}\"\n", key, value));
                }
            }

            // Bolt configuration if needed
            if container.gpu_access || container.memory_limit.is_some() || container.cpu_limit.is_some() {
                nova_file.push_str(&format!("\n[container.{}.bolt]\n", container.name));
                if container.gpu_access {
                    nova_file.push_str("gpu_access = true\n");
                }
                if let Some(memory) = &container.memory_limit {
                    nova_file.push_str(&format!("memory_limit = \"{}\"\n", memory));
                }
                if let Some(cpu) = &container.cpu_limit {
                    nova_file.push_str(&format!("cpu_limit = \"{}\"\n", cpu));
                }
            }

            nova_file.push_str("\n");
        }

        // Generate network configurations
        for network in &template.networks {
            nova_file.push_str(&format!("[network.{}]\n", network.name));
            nova_file.push_str(&format!("type = \"bridge\"\n"));
            if let Some(subnet) = &network.subnet {
                nova_file.push_str(&format!("subnet = \"{}\"\n", subnet));
            }
            nova_file.push_str("\n");
        }

        Ok(nova_file)
    }
}