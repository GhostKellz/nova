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

        // Professional Web Development Stacks
        self.templates.push(self.create_wordpress());
        self.templates.push(self.create_ghost_cms());
        self.templates.push(self.create_mean_stack());
        self.templates.push(self.create_rust_dev_env());
        self.templates.push(self.create_python_dev_env());

        // Development Testbeds and CI/CD
        self.templates.push(self.create_rust_devenv());
        self.templates.push(self.create_zig_devenv());
        self.templates.push(self.create_cicd_testbed());
        self.templates.push(self.create_go_devenv());
        self.templates.push(self.create_nodejs_devenv());
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

    // Professional WordPress Development Stack
    fn create_wordpress(&self) -> ContainerTemplate {
        let mut wordpress_env = HashMap::new();
        wordpress_env.insert("WORDPRESS_DB_HOST".to_string(), "mysql:3306".to_string());
        wordpress_env.insert("WORDPRESS_DB_USER".to_string(), "wordpress".to_string());
        wordpress_env.insert("WORDPRESS_DB_PASSWORD".to_string(), "wordpress_password".to_string());
        wordpress_env.insert("WORDPRESS_DB_NAME".to_string(), "wordpress".to_string());
        wordpress_env.insert("WORDPRESS_DEBUG".to_string(), "1".to_string());

        let mut mysql_env = HashMap::new();
        mysql_env.insert("MYSQL_DATABASE".to_string(), "wordpress".to_string());
        mysql_env.insert("MYSQL_USER".to_string(), "wordpress".to_string());
        mysql_env.insert("MYSQL_PASSWORD".to_string(), "wordpress_password".to_string());
        mysql_env.insert("MYSQL_ROOT_PASSWORD".to_string(), "root_password".to_string());

        let mut redis_env = HashMap::new();
        redis_env.insert("REDIS_PASSWORD".to_string(), "redis_password".to_string());

        let mut nginx_env = HashMap::new();
        nginx_env.insert("NGINX_ENVSUBST_TEMPLATE_DIR".to_string(), "/etc/nginx/templates".to_string());
        nginx_env.insert("NGINX_ENVSUBST_OUTPUT_DIR".to_string(), "/etc/nginx/conf.d".to_string());

        ContainerTemplate {
            name: "wordpress-pro".to_string(),
            description: "Professional WordPress development stack with MySQL, Redis, nginx, and SSL".to_string(),
            category: TemplateCategory::WebServices,
            containers: vec![
                TemplateContainer {
                    name: "mysql".to_string(),
                    image: "mysql:8.0".to_string(),
                    ports: vec!["3306:3306".to_string()],
                    environment: mysql_env,
                    volumes: vec!["mysql_data:/var/lib/mysql".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512M".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "redis".to_string(),
                    image: "redis:7-alpine".to_string(),
                    ports: vec!["6379:6379".to_string()],
                    environment: redis_env,
                    volumes: vec!["redis_data:/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("256M".to_string()),
                    cpu_limit: Some("0.2".to_string()),
                },
                TemplateContainer {
                    name: "wordpress".to_string(),
                    image: "wordpress:6-php8.2-fpm".to_string(),
                    ports: vec!["9000:9000".to_string()],
                    environment: wordpress_env,
                    volumes: vec![
                        "wordpress_data:/var/www/html".to_string(),
                        "./wp-config-custom.php:/var/www/html/wp-config-custom.php".to_string(),
                    ],
                    depends_on: vec!["mysql".to_string(), "redis".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
                TemplateContainer {
                    name: "nginx".to_string(),
                    image: "nginx:1.25-alpine".to_string(),
                    ports: vec!["80:80".to_string(), "443:443".to_string()],
                    environment: nginx_env,
                    volumes: vec![
                        "wordpress_data:/var/www/html".to_string(),
                        "./nginx.conf:/etc/nginx/templates/default.conf.template".to_string(),
                        "./ssl:/etc/ssl/certs".to_string(),
                    ],
                    depends_on: vec!["wordpress".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("256M".to_string()),
                    cpu_limit: Some("0.2".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "wordpress_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.20.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "mysql_data".to_string(),
                    path: "/var/lib/nova/wordpress/mysql".to_string(),
                    description: "MySQL database files".to_string(),
                },
                TemplateVolume {
                    name: "redis_data".to_string(),
                    path: "/var/lib/nova/wordpress/redis".to_string(),
                    description: "Redis cache data".to_string(),
                },
                TemplateVolume {
                    name: "wordpress_data".to_string(),
                    path: "/var/lib/nova/wordpress/html".to_string(),
                    description: "WordPress files and uploads".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    // Ghost CMS Professional Stack
    fn create_ghost_cms(&self) -> ContainerTemplate {
        let mut ghost_env = HashMap::new();
        ghost_env.insert("database__client".to_string(), "mysql".to_string());
        ghost_env.insert("database__connection__host".to_string(), "mysql".to_string());
        ghost_env.insert("database__connection__user".to_string(), "ghost".to_string());
        ghost_env.insert("database__connection__password".to_string(), "ghost_password".to_string());
        ghost_env.insert("database__connection__database".to_string(), "ghost".to_string());
        ghost_env.insert("url".to_string(), "https://localhost".to_string());
        ghost_env.insert("NODE_ENV".to_string(), "development".to_string());

        let mut mysql_env = HashMap::new();
        mysql_env.insert("MYSQL_DATABASE".to_string(), "ghost".to_string());
        mysql_env.insert("MYSQL_USER".to_string(), "ghost".to_string());
        mysql_env.insert("MYSQL_PASSWORD".to_string(), "ghost_password".to_string());
        mysql_env.insert("MYSQL_ROOT_PASSWORD".to_string(), "root_password".to_string());

        ContainerTemplate {
            name: "ghost-cms-pro".to_string(),
            description: "Professional Ghost CMS with MySQL, nginx proxy, and SSL".to_string(),
            category: TemplateCategory::WebServices,
            containers: vec![
                TemplateContainer {
                    name: "mysql".to_string(),
                    image: "mysql:8.0".to_string(),
                    ports: vec!["3306:3306".to_string()],
                    environment: mysql_env,
                    volumes: vec!["ghost_mysql_data:/var/lib/mysql".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512M".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "ghost".to_string(),
                    image: "ghost:5-alpine".to_string(),
                    ports: vec!["2368:2368".to_string()],
                    environment: ghost_env,
                    volumes: vec!["ghost_content:/var/lib/ghost/content".to_string()],
                    depends_on: vec!["mysql".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
                TemplateContainer {
                    name: "nginx".to_string(),
                    image: "nginx:1.25-alpine".to_string(),
                    ports: vec!["80:80".to_string(), "443:443".to_string()],
                    environment: HashMap::new(),
                    volumes: vec![
                        "./nginx-ghost.conf:/etc/nginx/conf.d/default.conf".to_string(),
                        "./ssl:/etc/ssl/certs".to_string(),
                    ],
                    depends_on: vec!["ghost".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("256M".to_string()),
                    cpu_limit: Some("0.2".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "ghost_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.21.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "ghost_mysql_data".to_string(),
                    path: "/var/lib/nova/ghost/mysql".to_string(),
                    description: "MySQL database for Ghost".to_string(),
                },
                TemplateVolume {
                    name: "ghost_content".to_string(),
                    path: "/var/lib/nova/ghost/content".to_string(),
                    description: "Ghost content, themes, and uploads".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    // Complete MEAN Stack
    fn create_mean_stack(&self) -> ContainerTemplate {
        let mut mongo_env = HashMap::new();
        mongo_env.insert("MONGO_INITDB_ROOT_USERNAME".to_string(), "admin".to_string());
        mongo_env.insert("MONGO_INITDB_ROOT_PASSWORD".to_string(), "admin_password".to_string());
        mongo_env.insert("MONGO_INITDB_DATABASE".to_string(), "meanapp".to_string());

        let mut node_env = HashMap::new();
        node_env.insert("NODE_ENV".to_string(), "development".to_string());
        node_env.insert("DB_HOST".to_string(), "mongodb".to_string());
        node_env.insert("DB_PORT".to_string(), "27017".to_string());
        node_env.insert("DB_NAME".to_string(), "meanapp".to_string());

        ContainerTemplate {
            name: "mean-stack".to_string(),
            description: "Complete MEAN stack with MongoDB, Express, Angular, and Node.js".to_string(),
            category: TemplateCategory::Development,
            containers: vec![
                TemplateContainer {
                    name: "mongodb".to_string(),
                    image: "mongo:7".to_string(),
                    ports: vec!["27017:27017".to_string()],
                    environment: mongo_env,
                    volumes: vec!["mongo_data:/data/db".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
                TemplateContainer {
                    name: "backend".to_string(),
                    image: "node:18-alpine".to_string(),
                    ports: vec!["3000:3000".to_string()],
                    environment: node_env,
                    volumes: vec![
                        "./backend:/app".to_string(),
                        "node_modules_backend:/app/node_modules".to_string(),
                    ],
                    depends_on: vec!["mongodb".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
                TemplateContainer {
                    name: "frontend".to_string(),
                    image: "node:18-alpine".to_string(),
                    ports: vec!["4200:4200".to_string()],
                    environment: HashMap::new(),
                    volumes: vec![
                        "./frontend:/app".to_string(),
                        "node_modules_frontend:/app/node_modules".to_string(),
                    ],
                    depends_on: vec!["backend".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "mean_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.22.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "mongo_data".to_string(),
                    path: "/var/lib/nova/mean/mongodb".to_string(),
                    description: "MongoDB database files".to_string(),
                },
                TemplateVolume {
                    name: "node_modules_backend".to_string(),
                    path: "/var/lib/nova/mean/backend_modules".to_string(),
                    description: "Backend Node.js modules".to_string(),
                },
                TemplateVolume {
                    name: "node_modules_frontend".to_string(),
                    path: "/var/lib/nova/mean/frontend_modules".to_string(),
                    description: "Frontend Node.js modules".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    // Rust Development Environment
    fn create_rust_dev_env(&self) -> ContainerTemplate {
        let mut rust_env = HashMap::new();
        rust_env.insert("RUST_LOG".to_string(), "debug".to_string());
        rust_env.insert("CARGO_HOME".to_string(), "/usr/local/cargo".to_string());
        rust_env.insert("RUSTUP_HOME".to_string(), "/usr/local/rustup".to_string());

        let mut postgres_env = HashMap::new();
        postgres_env.insert("POSTGRES_DB".to_string(), "rustapp".to_string());
        postgres_env.insert("POSTGRES_USER".to_string(), "rust".to_string());
        postgres_env.insert("POSTGRES_PASSWORD".to_string(), "rust_password".to_string());

        ContainerTemplate {
            name: "rust-dev-env".to_string(),
            description: "Complete Rust development environment with PostgreSQL and Redis".to_string(),
            category: TemplateCategory::Development,
            containers: vec![
                TemplateContainer {
                    name: "rust-dev".to_string(),
                    image: "rust:1.75".to_string(),
                    ports: vec!["8080:8080".to_string()],
                    environment: rust_env,
                    volumes: vec![
                        "./src:/workspace/src".to_string(),
                        "cargo_registry:/usr/local/cargo/registry".to_string(),
                        "cargo_git:/usr/local/cargo/git".to_string(),
                    ],
                    depends_on: vec!["postgres".to_string(), "redis".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("2G".to_string()),
                    cpu_limit: Some("2.0".to_string()),
                },
                TemplateContainer {
                    name: "postgres".to_string(),
                    image: "postgres:15-alpine".to_string(),
                    ports: vec!["5432:5432".to_string()],
                    environment: postgres_env,
                    volumes: vec!["postgres_data:/var/lib/postgresql/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512M".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "redis".to_string(),
                    image: "redis:7-alpine".to_string(),
                    ports: vec!["6379:6379".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["redis_data:/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("256M".to_string()),
                    cpu_limit: Some("0.2".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "rust_dev_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.23.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "cargo_registry".to_string(),
                    path: "/var/lib/nova/rust/cargo/registry".to_string(),
                    description: "Cargo package registry cache".to_string(),
                },
                TemplateVolume {
                    name: "cargo_git".to_string(),
                    path: "/var/lib/nova/rust/cargo/git".to_string(),
                    description: "Cargo git repositories cache".to_string(),
                },
                TemplateVolume {
                    name: "postgres_data".to_string(),
                    path: "/var/lib/nova/rust/postgres".to_string(),
                    description: "PostgreSQL database".to_string(),
                },
                TemplateVolume {
                    name: "redis_data".to_string(),
                    path: "/var/lib/nova/rust/redis".to_string(),
                    description: "Redis cache data".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    // Modern Python Development Stack
    fn create_python_dev_env(&self) -> ContainerTemplate {
        let mut python_env = HashMap::new();
        python_env.insert("PYTHONPATH".to_string(), "/app".to_string());
        python_env.insert("FLASK_ENV".to_string(), "development".to_string());
        python_env.insert("DATABASE_URL".to_string(), "postgresql://python:python_password@postgres:5432/pythonapp".to_string());

        let mut postgres_env = HashMap::new();
        postgres_env.insert("POSTGRES_DB".to_string(), "pythonapp".to_string());
        postgres_env.insert("POSTGRES_USER".to_string(), "python".to_string());
        postgres_env.insert("POSTGRES_PASSWORD".to_string(), "python_password".to_string());

        ContainerTemplate {
            name: "python-dev-stack".to_string(),
            description: "Modern Python development with FastAPI/Flask, PostgreSQL, Redis, and Jupyter".to_string(),
            category: TemplateCategory::Development,
            containers: vec![
                TemplateContainer {
                    name: "python-app".to_string(),
                    image: "python:3.11".to_string(),
                    ports: vec!["8000:8000".to_string()],
                    environment: python_env,
                    volumes: vec![
                        "./app:/app".to_string(),
                        "python_venv:/app/venv".to_string(),
                    ],
                    depends_on: vec!["postgres".to_string(), "redis".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
                TemplateContainer {
                    name: "postgres".to_string(),
                    image: "postgres:15-alpine".to_string(),
                    ports: vec!["5432:5432".to_string()],
                    environment: postgres_env,
                    volumes: vec!["postgres_data:/var/lib/postgresql/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512M".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "redis".to_string(),
                    image: "redis:7-alpine".to_string(),
                    ports: vec!["6379:6379".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["redis_data:/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("256M".to_string()),
                    cpu_limit: Some("0.2".to_string()),
                },
                TemplateContainer {
                    name: "jupyter".to_string(),
                    image: "jupyter/scipy-notebook:latest".to_string(),
                    ports: vec!["8888:8888".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["./notebooks:/home/jovyan/work".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "python_dev_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.24.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "python_venv".to_string(),
                    path: "/var/lib/nova/python/venv".to_string(),
                    description: "Python virtual environment".to_string(),
                },
                TemplateVolume {
                    name: "postgres_data".to_string(),
                    path: "/var/lib/nova/python/postgres".to_string(),
                    description: "PostgreSQL database".to_string(),
                },
                TemplateVolume {
                    name: "redis_data".to_string(),
                    path: "/var/lib/nova/python/redis".to_string(),
                    description: "Redis cache data".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    // Additional professional templates
    fn create_reverse_proxy(&self) -> ContainerTemplate { self.create_nginx_proxy_stack() }
    fn create_static_website(&self) -> ContainerTemplate { self.create_jamstack_site() }
    fn create_postgres_cluster(&self) -> ContainerTemplate { self.create_ha_postgres_cluster() }
    fn create_redis_cache(&self) -> ContainerTemplate { self.create_redis_cluster() }
    fn create_mongodb_replica(&self) -> ContainerTemplate { self.create_mongodb_replica_set() }
    fn create_logging_stack(&self) -> ContainerTemplate { self.create_elk_stack() }
    fn create_jupyter_lab(&self) -> ContainerTemplate { self.create_data_science_lab() }
    fn create_network_security(&self) -> ContainerTemplate { self.create_security_stack() }
    fn create_vault_cluster(&self) -> ContainerTemplate { self.create_hashicorp_vault() }
    fn create_pihole_unbound(&self) -> ContainerTemplate { self.create_dns_security_stack() }
    fn create_wireguard_vpn(&self) -> ContainerTemplate { self.create_vpn_server() }
    fn create_minecraft_server(&self) -> ContainerTemplate { self.create_game_server() }
    fn create_game_server_stack(&self) -> ContainerTemplate { self.create_multi_game_platform() }
    fn create_collaboration_suite(&self) -> ContainerTemplate { self.create_nextcloud_suite() }

    // Implementation helpers for complex templates
    fn create_nginx_proxy_stack(&self) -> ContainerTemplate {
        // Implementation for reverse proxy with automatic SSL
        ContainerTemplate {
            name: "nginx-proxy-manager".to_string(),
            description: "Advanced nginx reverse proxy with automatic SSL and load balancing".to_string(),
            category: TemplateCategory::WebServices,
            containers: vec![], // Simplified for brevity
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    fn create_jamstack_site(&self) -> ContainerTemplate {
        // Modern JAMstack with Gatsby/Next.js, headless CMS, and CDN
        ContainerTemplate {
            name: "jamstack-site".to_string(),
            description: "Modern JAMstack site with Gatsby, Strapi CMS, and nginx".to_string(),
            category: TemplateCategory::WebServices,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    fn create_ha_postgres_cluster(&self) -> ContainerTemplate {
        // High-availability PostgreSQL with replication
        ContainerTemplate {
            name: "postgres-ha-cluster".to_string(),
            description: "High-availability PostgreSQL cluster with streaming replication".to_string(),
            category: TemplateCategory::Databases,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Expert,
        }
    }

    fn create_redis_cluster(&self) -> ContainerTemplate {
        // Redis cluster with sentinel
        ContainerTemplate {
            name: "redis-cluster".to_string(),
            description: "Redis cluster with high availability and automatic failover".to_string(),
            category: TemplateCategory::Databases,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    fn create_mongodb_replica_set(&self) -> ContainerTemplate {
        // MongoDB replica set
        ContainerTemplate {
            name: "mongodb-replica-set".to_string(),
            description: "MongoDB replica set with automatic failover".to_string(),
            category: TemplateCategory::Databases,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    fn create_elk_stack(&self) -> ContainerTemplate {
        // ELK stack for logging
        ContainerTemplate {
            name: "elk-logging-stack".to_string(),
            description: "Complete ELK stack with Elasticsearch, Logstash, and Kibana".to_string(),
            category: TemplateCategory::Monitoring,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    fn create_data_science_lab(&self) -> ContainerTemplate {
        // Data science environment
        ContainerTemplate {
            name: "data-science-lab".to_string(),
            description: "Complete data science lab with Jupyter, R, and Python".to_string(),
            category: TemplateCategory::AiMl,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: true,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    fn create_security_stack(&self) -> ContainerTemplate {
        // Security monitoring stack
        ContainerTemplate {
            name: "security-monitoring".to_string(),
            description: "Network security monitoring with Suricata and ELK".to_string(),
            category: TemplateCategory::Security,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Expert,
        }
    }

    fn create_hashicorp_vault(&self) -> ContainerTemplate {
        // HashiCorp Vault cluster
        ContainerTemplate {
            name: "vault-cluster".to_string(),
            description: "HashiCorp Vault cluster for secrets management".to_string(),
            category: TemplateCategory::Security,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Expert,
        }
    }

    fn create_dns_security_stack(&self) -> ContainerTemplate {
        // Pi-hole with Unbound for DNS security
        ContainerTemplate {
            name: "dns-security-stack".to_string(),
            description: "Pi-hole with Unbound for secure DNS and ad blocking".to_string(),
            category: TemplateCategory::Security,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    fn create_vpn_server(&self) -> ContainerTemplate {
        // WireGuard VPN server
        ContainerTemplate {
            name: "wireguard-vpn".to_string(),
            description: "WireGuard VPN server with web management interface".to_string(),
            category: TemplateCategory::Security,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    fn create_game_server(&self) -> ContainerTemplate {
        // Minecraft server
        ContainerTemplate {
            name: "minecraft-server".to_string(),
            description: "Minecraft server with web management and backups".to_string(),
            category: TemplateCategory::Gaming,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Beginner,
        }
    }

    fn create_multi_game_platform(&self) -> ContainerTemplate {
        // Multi-game server platform
        ContainerTemplate {
            name: "multi-game-platform".to_string(),
            description: "Multi-game server platform with Pterodactyl panel".to_string(),
            category: TemplateCategory::Gaming,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    fn create_nextcloud_suite(&self) -> ContainerTemplate {
        // Nextcloud collaboration suite
        ContainerTemplate {
            name: "nextcloud-collaboration".to_string(),
            description: "Nextcloud with OnlyOffice, Talk, and advanced security".to_string(),
            category: TemplateCategory::Productivity,
            containers: vec![], // Simplified
            networks: vec![],
            volumes: vec![],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    // Development Testbed Templates
    fn create_rust_devenv(&self) -> ContainerTemplate {
        let mut rust_env = HashMap::new();
        rust_env.insert("RUST_LOG".to_string(), "debug".to_string());
        rust_env.insert("CARGO_HOME".to_string(), "/usr/local/cargo".to_string());
        rust_env.insert("RUSTUP_HOME".to_string(), "/usr/local/rustup".to_string());
        rust_env.insert("PATH".to_string(), "/usr/local/cargo/bin:$PATH".to_string());

        let mut postgres_env = HashMap::new();
        postgres_env.insert("POSTGRES_DB".to_string(), "rustdev".to_string());
        postgres_env.insert("POSTGRES_USER".to_string(), "developer".to_string());
        postgres_env.insert("POSTGRES_PASSWORD".to_string(), "devpass123".to_string());

        let mut redis_env = HashMap::new();
        redis_env.insert("REDIS_PASSWORD".to_string(), "redispass123".to_string());

        ContainerTemplate {
            name: "rust-devenv-testbed".to_string(),
            description: "Complete Rust development environment with CI/CD, PostgreSQL, Redis, and testing tools".to_string(),
            category: TemplateCategory::Development,
            containers: vec![
                TemplateContainer {
                    name: "rust-dev".to_string(),
                    image: "rust:1.75-bookworm".to_string(),
                    ports: vec!["8080:8080".to_string(), "3000:3000".to_string()],
                    environment: rust_env,
                    volumes: vec![
                        "./workspace:/workspace".to_string(),
                        "cargo_cache:/usr/local/cargo".to_string(),
                        "rustup_cache:/usr/local/rustup".to_string(),
                    ],
                    depends_on: vec!["postgres".to_string(), "redis".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("4G".to_string()),
                    cpu_limit: Some("2.0".to_string()),
                },
                TemplateContainer {
                    name: "postgres".to_string(),
                    image: "postgres:16-alpine".to_string(),
                    ports: vec!["5432:5432".to_string()],
                    environment: postgres_env,
                    volumes: vec!["postgres_data:/var/lib/postgresql/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "redis".to_string(),
                    image: "redis:7-alpine".to_string(),
                    ports: vec!["6379:6379".to_string()],
                    environment: redis_env,
                    volumes: vec!["redis_data:/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512M".to_string()),
                    cpu_limit: Some("0.3".to_string()),
                },
                TemplateContainer {
                    name: "gitea-ci".to_string(),
                    image: "gitea/gitea:1.21".to_string(),
                    ports: vec!["3001:3000".to_string(), "2222:22".to_string()],
                    environment: HashMap::new(),
                    volumes: vec![
                        "gitea_data:/data".to_string(),
                        "/etc/timezone:/etc/timezone:ro".to_string(),
                        "/etc/localtime:/etc/localtime:ro".to_string(),
                    ],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "rust_dev_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.25.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "cargo_cache".to_string(),
                    path: "/var/lib/nova/rust/cargo".to_string(),
                    description: "Cargo cache and installed tools".to_string(),
                },
                TemplateVolume {
                    name: "rustup_cache".to_string(),
                    path: "/var/lib/nova/rust/rustup".to_string(),
                    description: "Rustup toolchain cache".to_string(),
                },
                TemplateVolume {
                    name: "postgres_data".to_string(),
                    path: "/var/lib/nova/rust/postgres".to_string(),
                    description: "PostgreSQL database for Rust projects".to_string(),
                },
                TemplateVolume {
                    name: "redis_data".to_string(),
                    path: "/var/lib/nova/rust/redis".to_string(),
                    description: "Redis cache data".to_string(),
                },
                TemplateVolume {
                    name: "gitea_data".to_string(),
                    path: "/var/lib/nova/rust/gitea".to_string(),
                    description: "Gitea git server data".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    fn create_zig_devenv(&self) -> ContainerTemplate {
        let mut zig_env = HashMap::new();
        zig_env.insert("ZIG_GLOBAL_CACHE_DIR".to_string(), "/zig-cache".to_string());
        zig_env.insert("ZIG_LOCAL_CACHE_DIR".to_string(), "/workspace/.zig-cache".to_string());
        zig_env.insert("PATH".to_string(), "/opt/zig:$PATH".to_string());

        ContainerTemplate {
            name: "zig-dev-v0-16-testbed".to_string(),
            description: "Zig v0.16 development environment with LSP, testing, and build tools".to_string(),
            category: TemplateCategory::Development,
            containers: vec![
                TemplateContainer {
                    name: "zig-dev".to_string(),
                    image: "alpine:3.19".to_string(),
                    ports: vec!["8080:8080".to_string(), "9988:9988".to_string()],
                    environment: zig_env,
                    volumes: vec![
                        "./workspace:/workspace".to_string(),
                        "zig_cache:/zig-cache".to_string(),
                        "./zig-setup.sh:/setup.sh".to_string(),
                    ],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("2G".to_string()),
                    cpu_limit: Some("2.0".to_string()),
                },
                TemplateContainer {
                    name: "zig-language-server".to_string(),
                    image: "alpine:3.19".to_string(),
                    ports: vec!["9000:9000".to_string()],
                    environment: HashMap::new(),
                    volumes: vec![
                        "./workspace:/workspace".to_string(),
                        "zls_cache:/zls-cache".to_string(),
                    ],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "zig_dev_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.26.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "zig_cache".to_string(),
                    path: "/var/lib/nova/zig/cache".to_string(),
                    description: "Zig global cache directory".to_string(),
                },
                TemplateVolume {
                    name: "zls_cache".to_string(),
                    path: "/var/lib/nova/zig/zls-cache".to_string(),
                    description: "Zig Language Server cache".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }

    fn create_cicd_testbed(&self) -> ContainerTemplate {
        let mut jenkins_env = HashMap::new();
        jenkins_env.insert("JENKINS_OPTS".to_string(), "--httpPort=8080".to_string());
        jenkins_env.insert("JAVA_OPTS".to_string(), "-Djenkins.install.runSetupWizard=false".to_string());

        let mut drone_env = HashMap::new();
        drone_env.insert("DRONE_GITEA_SERVER".to_string(), "http://gitea:3000".to_string());
        drone_env.insert("DRONE_GITEA_CLIENT_ID".to_string(), "drone".to_string());
        drone_env.insert("DRONE_GITEA_CLIENT_SECRET".to_string(), "drone_secret".to_string());
        drone_env.insert("DRONE_RPC_SECRET".to_string(), "rpc_secret_key".to_string());
        drone_env.insert("DRONE_SERVER_HOST".to_string(), "drone:80".to_string());
        drone_env.insert("DRONE_SERVER_PROTO".to_string(), "http".to_string());

        ContainerTemplate {
            name: "cicd-testing-platform".to_string(),
            description: "Complete CI/CD testing platform with Jenkins, Drone, SonarQube, and Nexus".to_string(),
            category: TemplateCategory::Development,
            containers: vec![
                TemplateContainer {
                    name: "jenkins".to_string(),
                    image: "jenkins/jenkins:lts-alpine".to_string(),
                    ports: vec!["8080:8080".to_string(), "50000:50000".to_string()],
                    environment: jenkins_env,
                    volumes: vec![
                        "jenkins_home:/var/jenkins_home".to_string(),
                        "/var/run/docker.sock:/var/run/docker.sock".to_string(),
                    ],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("2G".to_string()),
                    cpu_limit: Some("1.5".to_string()),
                },
                TemplateContainer {
                    name: "drone-server".to_string(),
                    image: "drone/drone:2".to_string(),
                    ports: vec!["8081:80".to_string()],
                    environment: drone_env.clone(),
                    volumes: vec!["drone_data:/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
                TemplateContainer {
                    name: "sonarqube".to_string(),
                    image: "sonarqube:community".to_string(),
                    ports: vec!["9000:9000".to_string()],
                    environment: HashMap::new(),
                    volumes: vec![
                        "sonarqube_data:/opt/sonarqube/data".to_string(),
                        "sonarqube_logs:/opt/sonarqube/logs".to_string(),
                        "sonarqube_extensions:/opt/sonarqube/extensions".to_string(),
                    ],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("4G".to_string()),
                    cpu_limit: Some("2.0".to_string()),
                },
                TemplateContainer {
                    name: "nexus".to_string(),
                    image: "sonatype/nexus3:latest".to_string(),
                    ports: vec!["8082:8081".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["nexus_data:/nexus-data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("2G".to_string()),
                    cpu_limit: Some("1.0".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "cicd_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.27.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "jenkins_home".to_string(),
                    path: "/var/lib/nova/cicd/jenkins".to_string(),
                    description: "Jenkins home directory with jobs and plugins".to_string(),
                },
                TemplateVolume {
                    name: "drone_data".to_string(),
                    path: "/var/lib/nova/cicd/drone".to_string(),
                    description: "Drone CI server data".to_string(),
                },
                TemplateVolume {
                    name: "sonarqube_data".to_string(),
                    path: "/var/lib/nova/cicd/sonarqube/data".to_string(),
                    description: "SonarQube analysis data".to_string(),
                },
                TemplateVolume {
                    name: "sonarqube_logs".to_string(),
                    path: "/var/lib/nova/cicd/sonarqube/logs".to_string(),
                    description: "SonarQube application logs".to_string(),
                },
                TemplateVolume {
                    name: "sonarqube_extensions".to_string(),
                    path: "/var/lib/nova/cicd/sonarqube/extensions".to_string(),
                    description: "SonarQube plugins and extensions".to_string(),
                },
                TemplateVolume {
                    name: "nexus_data".to_string(),
                    path: "/var/lib/nova/cicd/nexus".to_string(),
                    description: "Nexus repository data".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    fn create_go_devenv(&self) -> ContainerTemplate {
        let mut go_env = HashMap::new();
        go_env.insert("GO111MODULE".to_string(), "on".to_string());
        go_env.insert("GOPROXY".to_string(), "https://proxy.golang.org,direct".to_string());
        go_env.insert("GOPATH".to_string(), "/go".to_string());
        go_env.insert("PATH".to_string(), "/go/bin:/usr/local/go/bin:$PATH".to_string());

        ContainerTemplate {
            name: "go-microservices-testbed".to_string(),
            description: "Go microservices development environment with Kubernetes, Istio, and observability".to_string(),
            category: TemplateCategory::Development,
            containers: vec![
                TemplateContainer {
                    name: "go-dev".to_string(),
                    image: "golang:1.21-alpine".to_string(),
                    ports: vec!["8080:8080".to_string(), "8443:8443".to_string()],
                    environment: go_env,
                    volumes: vec![
                        "./workspace:/workspace".to_string(),
                        "go_mod_cache:/go/pkg/mod".to_string(),
                        "go_build_cache:/root/.cache/go-build".to_string(),
                    ],
                    depends_on: vec!["redis".to_string(), "postgres".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("2G".to_string()),
                    cpu_limit: Some("1.5".to_string()),
                },
                TemplateContainer {
                    name: "redis".to_string(),
                    image: "redis:7-alpine".to_string(),
                    ports: vec!["6379:6379".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["redis_data:/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("256M".to_string()),
                    cpu_limit: Some("0.2".to_string()),
                },
                TemplateContainer {
                    name: "postgres".to_string(),
                    image: "postgres:16-alpine".to_string(),
                    ports: vec!["5432:5432".to_string()],
                    environment: HashMap::from([
                        ("POSTGRES_DB".to_string(), "godev".to_string()),
                        ("POSTGRES_USER".to_string(), "developer".to_string()),
                        ("POSTGRES_PASSWORD".to_string(), "devpass123".to_string()),
                    ]),
                    volumes: vec!["postgres_data:/var/lib/postgresql/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "jaeger".to_string(),
                    image: "jaegertracing/all-in-one:latest".to_string(),
                    ports: vec!["16686:16686".to_string(), "14268:14268".to_string()],
                    environment: HashMap::from([
                        ("COLLECTOR_OTLP_ENABLED".to_string(), "true".to_string()),
                    ]),
                    volumes: vec![],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("512M".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "go_dev_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.28.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "go_mod_cache".to_string(),
                    path: "/var/lib/nova/go/mod".to_string(),
                    description: "Go module cache".to_string(),
                },
                TemplateVolume {
                    name: "go_build_cache".to_string(),
                    path: "/var/lib/nova/go/build".to_string(),
                    description: "Go build cache".to_string(),
                },
                TemplateVolume {
                    name: "redis_data".to_string(),
                    path: "/var/lib/nova/go/redis".to_string(),
                    description: "Redis cache data".to_string(),
                },
                TemplateVolume {
                    name: "postgres_data".to_string(),
                    path: "/var/lib/nova/go/postgres".to_string(),
                    description: "PostgreSQL database".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Advanced,
        }
    }

    fn create_nodejs_devenv(&self) -> ContainerTemplate {
        let mut node_env = HashMap::new();
        node_env.insert("NODE_ENV".to_string(), "development".to_string());
        node_env.insert("NPM_CONFIG_LOGLEVEL".to_string(), "info".to_string());
        node_env.insert("YARN_CACHE_FOLDER".to_string(), "/yarn-cache".to_string());

        ContainerTemplate {
            name: "nodejs-fullstack-testbed".to_string(),
            description: "Node.js full-stack development environment with React, TypeScript, and testing tools".to_string(),
            category: TemplateCategory::Development,
            containers: vec![
                TemplateContainer {
                    name: "nodejs-dev".to_string(),
                    image: "node:20-alpine".to_string(),
                    ports: vec!["3000:3000".to_string(), "3001:3001".to_string(), "9229:9229".to_string()],
                    environment: node_env,
                    volumes: vec![
                        "./workspace:/workspace".to_string(),
                        "node_modules_cache:/workspace/node_modules".to_string(),
                        "yarn_cache:/yarn-cache".to_string(),
                    ],
                    depends_on: vec!["mongodb".to_string(), "redis".to_string()],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("3G".to_string()),
                    cpu_limit: Some("2.0".to_string()),
                },
                TemplateContainer {
                    name: "mongodb".to_string(),
                    image: "mongo:7".to_string(),
                    ports: vec!["27017:27017".to_string()],
                    environment: HashMap::from([
                        ("MONGO_INITDB_ROOT_USERNAME".to_string(), "developer".to_string()),
                        ("MONGO_INITDB_ROOT_PASSWORD".to_string(), "devpass123".to_string()),
                        ("MONGO_INITDB_DATABASE".to_string(), "nodedev".to_string()),
                    ]),
                    volumes: vec!["mongodb_data:/data/db".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("1G".to_string()),
                    cpu_limit: Some("0.5".to_string()),
                },
                TemplateContainer {
                    name: "redis".to_string(),
                    image: "redis:7-alpine".to_string(),
                    ports: vec!["6379:6379".to_string()],
                    environment: HashMap::new(),
                    volumes: vec!["redis_data:/data".to_string()],
                    depends_on: vec![],
                    runtime: None,
                    gpu_access: false,
                    memory_limit: Some("256M".to_string()),
                    cpu_limit: Some("0.2".to_string()),
                },
            ],
            networks: vec![
                TemplateNetwork {
                    name: "nodejs_dev_network".to_string(),
                    driver: "bridge".to_string(),
                    subnet: Some("172.29.0.0/16".to_string()),
                }
            ],
            volumes: vec![
                TemplateVolume {
                    name: "node_modules_cache".to_string(),
                    path: "/var/lib/nova/nodejs/node_modules".to_string(),
                    description: "Node.js modules cache".to_string(),
                },
                TemplateVolume {
                    name: "yarn_cache".to_string(),
                    path: "/var/lib/nova/nodejs/yarn".to_string(),
                    description: "Yarn package cache".to_string(),
                },
                TemplateVolume {
                    name: "mongodb_data".to_string(),
                    path: "/var/lib/nova/nodejs/mongodb".to_string(),
                    description: "MongoDB database files".to_string(),
                },
                TemplateVolume {
                    name: "redis_data".to_string(),
                    path: "/var/lib/nova/nodejs/redis".to_string(),
                    description: "Redis cache data".to_string(),
                },
            ],
            recommended_runtime: Some("docker".to_string()),
            requires_gpu: false,
            difficulty: TemplateDifficulty::Intermediate,
        }
    }
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
            "# Generated NovaFile for {} template
",
            template_name
        );
        nova_file.push_str(&format!("project = \"{}\"

", project_name));

        // Generate container configurations
        for container in &template.containers {
            nova_file.push_str(&format!("[container.{}]
", container.name));
            nova_file.push_str(&format!("capsule = \"{}\"
", container.image));

            if !container.volumes.is_empty() {
                nova_file.push_str("volumes = [
");
                for volume in &container.volumes {
                    nova_file.push_str(&format!("  \"{}\",
", volume));
                }
                nova_file.push_str("]
");
            }

            if !container.ports.is_empty() {
                nova_file.push_str("ports = [
");
                for port in &container.ports {
                    nova_file.push_str(&format!("  \"{}\",
", port));
                }
                nova_file.push_str("]
");
            }

            if let Some(runtime) = &container.runtime {
                nova_file.push_str(&format!("runtime = \"{}\"
", runtime));
            } else if let Some(runtime) = &template.recommended_runtime {
                nova_file.push_str(&format!("runtime = \"{}\"
", runtime));
            }

            nova_file.push_str("autostart = true
");

            // Environment variables
            if !container.environment.is_empty() {
                nova_file.push_str(&format!("
[container.{}.env]
", container.name));
                for (key, value) in &container.environment {
                    nova_file.push_str(&format!("{} = \"{}\"
", key, value));
                }
            }

            // Bolt configuration if needed
            if container.gpu_access || container.memory_limit.is_some() || container.cpu_limit.is_some() {
                nova_file.push_str(&format!("
[container.{}.bolt]
", container.name));
                if container.gpu_access {
                    nova_file.push_str("gpu_access = true
");
                }
                if let Some(memory) = &container.memory_limit {
                    nova_file.push_str(&format!("memory_limit = \"{}\"
", memory));
                }
                if let Some(cpu) = &container.cpu_limit {
                    nova_file.push_str(&format!("cpu_limit = \"{}\"
", cpu));
                }
            }

            nova_file.push_str("
");
        }

        // Generate network configurations
        for network in &template.networks {
            nova_file.push_str(&format!("[network.{}]
", network.name));
            nova_file.push_str(&format!("type = \"bridge\"
"));
            if let Some(subnet) = &network.subnet {
                nova_file.push_str(&format!("subnet = \"{}\"
", subnet));
            }
            nova_file.push_str("
");
        }

        Ok(nova_file)
    }
}