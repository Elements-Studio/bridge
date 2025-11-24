// Wrapper for Starcoin config - provides Starcoin-compatible Config API
#![allow(dead_code, unused_variables)]

use anyhow::Result;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::Path;

// Config trait compatible with Starcoin's interface
// Wraps Starcoin's config functionality
pub trait Config: Serialize + DeserializeOwned {
    fn persisted(self, path: &Path) -> PersistedConfig<Self>
    where
        Self: Sized,
    {
        PersistedConfig {
            inner: self,
            path: path.to_path_buf(),
        }
    }

    fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;
        // Support both YAML and JSON formats
        let config: Self = if path.extension().and_then(|s| s.to_str()) == Some("yaml") || 
                              path.extension().and_then(|s| s.to_str()) == Some("yml") {
            serde_yaml::from_str(&content)?
        } else {
            serde_json::from_str(&content)?
        };
        Ok(config)
    }

    fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

pub struct PersistedConfig<C> {
    inner: C,
    path: std::path::PathBuf,
}

impl<C: Config> PersistedConfig<C> {
    pub fn read(&self) -> Result<C> {
        C::load(&self.path)
    }

    pub fn save(&self) -> Result<()> {
        self.inner.save(&self.path)
    }
}

// Re-export Starcoin's available_port utilities
pub mod local_ip_utils {
    use std::net::IpAddr;
    
    // Wrap Starcoin's get_random_available_port to match Starcoin's signature
    pub fn get_available_port(_host: &IpAddr) -> u16 {
        starcoin_config::get_random_available_port()
    }
    
    pub fn get_available_ports(_host: &IpAddr, count: usize) -> Vec<u16> {
        starcoin_config::get_random_available_ports(count)
    }
    
    // Testing helper
    pub fn localhost_for_testing() -> IpAddr {
        IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
    }
}

// Re-export commonly used config types if needed
pub use starcoin_config;
