use crate::{
    NovaError, Result,
    config::{NovaConfig, StoragePoolConfig},
};
use std::fs;
use std::path::{Path, PathBuf};

/// Manages storage pool definitions within the Nova configuration file.
pub struct StorageManager {
    config: NovaConfig,
    config_path: PathBuf,
}

impl StorageManager {
    /// Load storage configuration from a NovaFile (or create defaults if missing).
    pub fn load<P: AsRef<Path>>(config_path: P) -> Result<Self> {
        let path = config_path.as_ref().to_path_buf();
        let config = if path.exists() {
            NovaConfig::from_file(&path)?
        } else {
            NovaConfig::default()
        };

        Ok(Self {
            config,
            config_path: path,
        })
    }

    /// Reload the configuration from disk, discarding in-memory changes.
    pub fn reload(&mut self) -> Result<()> {
        if self.config_path.exists() {
            self.config = NovaConfig::from_file(&self.config_path)?;
        } else {
            self.config = NovaConfig::default();
        }
        Ok(())
    }

    /// Return the underlying configuration reference.
    pub fn config(&self) -> &NovaConfig {
        &self.config
    }

    /// Return a mutable reference to the underlying configuration (advanced usage).
    pub fn config_mut(&mut self) -> &mut NovaConfig {
        &mut self.config
    }

    /// Persist configuration changes to disk.
    pub fn save(&self) -> Result<()> {
        self.config.save_to_file(&self.config_path)
    }

    /// List storage pools as `(name, config)` pairs.
    pub fn list_pools(&self) -> Vec<(String, StoragePoolConfig)> {
        let mut pools: Vec<(String, StoragePoolConfig)> = self
            .config
            .storage
            .iter()
            .map(|(name, cfg)| (name.clone(), cfg.clone()))
            .collect();
        pools.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        pools
    }

    /// Retrieve a storage pool definition.
    pub fn get_pool(&self, name: &str) -> Option<&StoragePoolConfig> {
        self.config.storage.get(name)
    }

    /// Create a new storage pool definition.
    pub fn create_pool(&mut self, name: &str, pool: StoragePoolConfig) -> Result<()> {
        Self::validate_pool_name(name)?;
        if self.config.storage.contains_key(name) {
            return Err(NovaError::ConfigError(format!(
                "Storage pool '{}' already exists",
                name
            )));
        }

        let normalized = Self::normalize_pool_config(&pool)?;

        self.config.storage.insert(name.to_string(), normalized);
        self.save()
    }

    /// Update an existing storage pool definition.
    pub fn update_pool(&mut self, name: &str, pool: StoragePoolConfig) -> Result<()> {
        if !self.config.storage.contains_key(name) {
            return Err(NovaError::ConfigError(format!(
                "Storage pool '{}' does not exist",
                name
            )));
        }

        let normalized = Self::normalize_pool_config(&pool)?;
        self.config.storage.insert(name.to_string(), normalized);
        self.save()
    }

    /// Delete a storage pool definition. Optionally remove the directory from disk.
    pub fn delete_pool(&mut self, name: &str, remove_directory: bool) -> Result<StoragePoolConfig> {
        let removed =
            self.config.storage.remove(name).ok_or_else(|| {
                NovaError::ConfigError(format!("Storage pool '{}' not found", name))
            })?;

        if remove_directory {
            let path = Path::new(&removed.directory);
            if path.exists() {
                fs::remove_dir_all(path)?;
            }
        }

        self.save()?;
        Ok(removed)
    }

    fn validate_pool_name(name: &str) -> Result<()> {
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valid {
            return Err(NovaError::ConfigError(format!(
                "Storage pool name '{}' contains unsupported characters",
                name
            )));
        }
        Ok(())
    }

    fn normalize_pool_config(pool: &StoragePoolConfig) -> Result<StoragePoolConfig> {
        if pool.directory.trim().is_empty() {
            return Err(NovaError::ConfigError(
                "Storage pool directory cannot be empty".to_string(),
            ));
        }

        let mut normalized = pool.clone();
        let path = Path::new(&normalized.directory);

        if path.exists() {
            if let Ok(canon) = fs::canonicalize(path) {
                normalized.directory = canon.to_string_lossy().into_owned();
            }
        } else if normalized.auto_create {
            fs::create_dir_all(path)?;
            if let Ok(canon) = fs::canonicalize(path) {
                normalized.directory = canon.to_string_lossy().into_owned();
            }
        } else {
            return Err(NovaError::ConfigError(format!(
                "Directory '{}' does not exist",
                normalized.directory
            )));
        }

        Ok(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn create_list_delete_pool() {
        let file = NamedTempFile::new().unwrap();
        let path = file.path().to_path_buf();
        {
            // Ensure the file exists with default config
            let manager = StorageManager::load(&path).unwrap();
            manager.save().unwrap();
        }

        let mut manager = StorageManager::load(&path).unwrap();
        let mut pool_cfg = StoragePoolConfig::default();
        pool_cfg.directory = path
            .parent()
            .unwrap()
            .join("nova-storage-test")
            .to_string_lossy()
            .into_owned();

        manager.create_pool("images", pool_cfg.clone()).unwrap();
        let pools = manager.list_pools();
        assert_eq!(pools.len(), 1);
        assert_eq!(pools[0].0, "images");

        let fetched = manager.get_pool("images").unwrap();
        assert_eq!(fetched.directory, pool_cfg.directory);

        let removed = manager.delete_pool("images", true).unwrap();
        assert_eq!(removed.directory, pool_cfg.directory);
        assert!(manager.list_pools().is_empty());
    }
}
