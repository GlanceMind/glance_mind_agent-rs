//! Platform Registry - Manages available platforms

use std::collections::HashMap;
use std::sync::Arc;

use crate::platform::Platform;

/// Registry of available platforms
pub struct PlatformRegistry {
    platforms: HashMap<String, Arc<dyn Platform>>,
}

impl PlatformRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            platforms: HashMap::new(),
        }
    }

    /// Register a platform
    pub fn register(&mut self, platform: Arc<dyn Platform>) {
        self.platforms.insert(platform.name().to_string(), platform);
    }

    /// Get a platform by name
    pub fn get(&self, name: &str) -> Option<Arc<dyn Platform>> {
        self.platforms.get(name).cloned()
    }

    /// Get a platform by ID
    pub fn get_by_id(&self, id: i32) -> Option<Arc<dyn Platform>> {
        self.platforms
            .values()
            .find(|p| p.platform_id() == id)
            .cloned()
    }

    /// List all platform names
    pub fn names(&self) -> Vec<&str> {
        self.platforms.keys().map(|s| s.as_str()).collect()
    }

    /// Check if a platform is registered
    pub fn has(&self, name: &str) -> bool {
        self.platforms.contains_key(name)
    }

    /// Get the number of registered platforms
    pub fn len(&self) -> usize {
        self.platforms.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.platforms.is_empty()
    }
}

impl Default for PlatformRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_operations() {
        let registry = PlatformRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }
}
