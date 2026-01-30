//! Platform Configuration - Manages platform ID/name mappings
//!
//! Platform mappings are loaded from the database at startup and shared
//! across all components that need platform information.
//!
//! # Usage
//!
//! ```ignore
//! // At application startup, load from database
//! let registry = PlatformRegistry::from_database(&db_pool).await?;
//!
//! // Or create with default values for testing
//! let registry = PlatformRegistry::with_defaults();
//!
//! // Use the registry
//! let name = registry.get_name(2); // -> Some("tiktok")
//! let id = registry.get_id("tiktok"); // -> Some(2)
//! ```

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Global platform registry instance
static GLOBAL_REGISTRY: OnceLock<Arc<PlatformRegistry>> = OnceLock::new();

/// Platform information loaded from database
#[derive(Debug, Clone)]
pub struct PlatformInfo {
    /// Platform ID (primary key in database)
    pub id: i32,
    /// Platform name (e.g., "tiktok", "instagram")
    pub name: String,
    /// Display name (e.g., "TikTok", "Instagram")
    pub display_name: String,
    /// Whether the platform is active
    pub is_active: bool,
}

impl PlatformInfo {
    /// Create a new platform info
    pub fn new(id: i32, name: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            display_name: display_name.into(),
            is_active: true,
        }
    }

    /// Set active status
    pub fn with_active(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }
}

/// Trait for platform lookup operations
pub trait PlatformLookup: Send + Sync {
    /// Get platform name by ID
    fn get_name(&self, id: i32) -> Option<&str>;

    /// Get platform ID by name
    fn get_id(&self, name: &str) -> Option<i32>;

    /// Get full platform info by ID
    fn get_info(&self, id: i32) -> Option<&PlatformInfo>;

    /// Get full platform info by name
    fn get_info_by_name(&self, name: &str) -> Option<&PlatformInfo>;

    /// Get all platforms
    fn all_platforms(&self) -> Vec<&PlatformInfo>;

    /// Get all active platforms
    fn active_platforms(&self) -> Vec<&PlatformInfo>;

    /// Check if platform ID exists
    fn has_id(&self, id: i32) -> bool {
        self.get_name(id).is_some()
    }

    /// Check if platform name exists
    fn has_name(&self, name: &str) -> bool {
        self.get_id(name).is_some()
    }
}

/// Platform registry that maps between IDs and names
#[derive(Debug, Clone)]
pub struct PlatformRegistry {
    /// Map from ID to platform info
    by_id: HashMap<i32, PlatformInfo>,
    /// Map from name to ID (for reverse lookup)
    name_to_id: HashMap<String, i32>,
}

impl PlatformRegistry {
    /// Create an empty registry
    pub fn new() -> Self {
        Self {
            by_id: HashMap::new(),
            name_to_id: HashMap::new(),
        }
    }

    /// Create registry with default platform mappings
    ///
    /// This should only be used for testing or when database is unavailable.
    /// Production should use `from_database()`.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Default platforms (matching database schema)
        registry.register(PlatformInfo::new(1, "reddit", "Reddit"));
        registry.register(PlatformInfo::new(2, "tiktok", "TikTok"));
        registry.register(PlatformInfo::new(3, "facebook", "Facebook"));
        registry.register(PlatformInfo::new(4, "instagram", "Instagram"));
        registry.register(PlatformInfo::new(5, "twitter", "Twitter"));
        registry.register(PlatformInfo::new(6, "youtube", "YouTube"));

        registry
    }

    /// Register a platform
    pub fn register(&mut self, platform: PlatformInfo) {
        self.name_to_id.insert(platform.name.clone(), platform.id);
        self.by_id.insert(platform.id, platform);
    }

    /// Load platforms from database records
    ///
    /// # Arguments
    /// * `records` - Iterator of (id, name, display_name, is_active) tuples
    pub fn from_records<I>(records: I) -> Self
    where
        I: IntoIterator<Item = (i32, String, String, bool)>,
    {
        let mut registry = Self::new();

        for (id, name, display_name, is_active) in records {
            registry.register(PlatformInfo {
                id,
                name,
                display_name,
                is_active,
            });
        }

        registry
    }

    /// Get the number of registered platforms
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

impl Default for PlatformRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl PlatformLookup for PlatformRegistry {
    fn get_name(&self, id: i32) -> Option<&str> {
        self.by_id.get(&id).map(|p| p.name.as_str())
    }

    fn get_id(&self, name: &str) -> Option<i32> {
        self.name_to_id.get(name).copied()
    }

    fn get_info(&self, id: i32) -> Option<&PlatformInfo> {
        self.by_id.get(&id)
    }

    fn get_info_by_name(&self, name: &str) -> Option<&PlatformInfo> {
        self.name_to_id
            .get(name)
            .and_then(|id| self.by_id.get(id))
    }

    fn all_platforms(&self) -> Vec<&PlatformInfo> {
        self.by_id.values().collect()
    }

    fn active_platforms(&self) -> Vec<&PlatformInfo> {
        self.by_id.values().filter(|p| p.is_active).collect()
    }
}

// ============================================================
// Global Registry Access
// ============================================================

/// Initialize the global platform registry
///
/// This should be called once at application startup.
/// Subsequent calls will be ignored.
pub fn init_global_registry(registry: PlatformRegistry) {
    let _ = GLOBAL_REGISTRY.set(Arc::new(registry));
}

/// Get the global platform registry
///
/// Returns the default registry if not initialized.
pub fn global_registry() -> Arc<PlatformRegistry> {
    GLOBAL_REGISTRY
        .get()
        .cloned()
        .unwrap_or_else(|| Arc::new(PlatformRegistry::with_defaults()))
}

/// Get platform name by ID using global registry
pub fn platform_name(id: i32) -> Option<&'static str> {
    // Safety: We need to return a reference with 'static lifetime
    // This is safe because the global registry lives for the entire program
    GLOBAL_REGISTRY
        .get()
        .and_then(|r| r.get_name(id))
        .map(|s| {
            // Convert to 'static str by leaking the string
            // This is acceptable because platforms are loaded once at startup
            unsafe { &*(s as *const str) }
        })
}

/// Get platform ID by name using global registry
pub fn platform_id(name: &str) -> Option<i32> {
    GLOBAL_REGISTRY.get().and_then(|r| r.get_id(name))
}

// ============================================================
// Database Loading
// ============================================================

/// Load platform registry from database
///
/// This function queries the `gm_platforms` table and creates a registry.
///
/// # SQL Query
/// ```sql
/// SELECT id, name, display_name, is_active FROM gm_platforms WHERE is_active = true
/// ```
#[cfg(feature = "database")]
pub async fn load_from_database(
    pool: &diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<diesel::PgConnection>>,
) -> Result<PlatformRegistry, diesel::result::Error> {
    use diesel::prelude::*;

    // Define table structure
    diesel::table! {
        gm_platforms (id) {
            id -> Int4,
            name -> Varchar,
            display_name -> Varchar,
            is_active -> Bool,
        }
    }

    let mut conn = pool.get().map_err(|_| diesel::result::Error::NotFound)?;

    let records: Vec<(i32, String, String, bool)> = gm_platforms::table
        .select((
            gm_platforms::id,
            gm_platforms::name,
            gm_platforms::display_name,
            gm_platforms::is_active,
        ))
        .filter(gm_platforms::is_active.eq(true))
        .load(&mut conn)?;

    Ok(PlatformRegistry::from_records(records))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_registry_defaults() {
        let registry = PlatformRegistry::with_defaults();

        assert_eq!(registry.get_name(1), Some("reddit"));
        assert_eq!(registry.get_name(2), Some("tiktok"));
        assert_eq!(registry.get_name(3), Some("facebook"));
        assert_eq!(registry.get_name(4), Some("instagram"));
        assert_eq!(registry.get_name(5), Some("twitter"));
        assert_eq!(registry.get_name(6), Some("youtube"));
        assert_eq!(registry.get_name(999), None);
    }

    #[test]
    fn test_platform_registry_reverse_lookup() {
        let registry = PlatformRegistry::with_defaults();

        assert_eq!(registry.get_id("reddit"), Some(1));
        assert_eq!(registry.get_id("tiktok"), Some(2));
        assert_eq!(registry.get_id("unknown"), None);
    }

    #[test]
    fn test_platform_registry_info() {
        let registry = PlatformRegistry::with_defaults();

        let info = registry.get_info(2).unwrap();
        assert_eq!(info.id, 2);
        assert_eq!(info.name, "tiktok");
        assert_eq!(info.display_name, "TikTok");
        assert!(info.is_active);
    }

    #[test]
    fn test_platform_registry_from_records() {
        let records = vec![
            (10, "custom".to_string(), "Custom Platform".to_string(), true),
            (20, "another".to_string(), "Another Platform".to_string(), false),
        ];

        let registry = PlatformRegistry::from_records(records);

        assert_eq!(registry.get_name(10), Some("custom"));
        assert_eq!(registry.get_name(20), Some("another"));
        assert_eq!(registry.get_id("custom"), Some(10));

        let active = registry.active_platforms();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "custom");
    }

    #[test]
    fn test_platform_info_builder() {
        let info = PlatformInfo::new(100, "test", "Test Platform")
            .with_active(false);

        assert_eq!(info.id, 100);
        assert_eq!(info.name, "test");
        assert!(!info.is_active);
    }
}
