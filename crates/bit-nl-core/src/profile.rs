#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct UserProfile {
    pub expertise_tags: Vec<String>,
    pub domain: Option<String>,
    pub known_entities: Vec<String>,
}

impl UserProfile {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_expertise(tags: Vec<String>) -> Self {
        Self {
            expertise_tags: tags,
            ..Default::default()
        }
    }

    /// Load from .nlprofile file
    pub fn load(path: &std::path::Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Save to .nlprofile file
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }

    /// Add an entity discovered during compilation
    pub fn register_entity(&mut self, name: &str) {
        if !self.known_entities.iter().any(|e| e == name) {
            self.known_entities.push(name.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn default_profile() {
        let p = UserProfile::new();
        assert!(p.expertise_tags.is_empty());
        assert!(p.domain.is_none());
        assert!(p.known_entities.is_empty());
    }

    #[test]
    fn with_expertise_constructor() {
        let p = UserProfile::with_expertise(vec!["rust".to_string(), "ml".to_string()]);
        assert_eq!(p.expertise_tags.len(), 2);
        assert!(p.domain.is_none());
    }

    #[test]
    fn register_entity_deduplicates() {
        let mut p = UserProfile::new();
        p.register_entity("User");
        p.register_entity("Order");
        p.register_entity("User"); // duplicate
        assert_eq!(p.known_entities.len(), 2);
    }

    #[test]
    fn serialization_round_trip() {
        let mut p = UserProfile::with_expertise(vec!["auth".to_string()]);
        p.domain = Some("fintech".to_string());
        p.register_entity("Account");

        let json = serde_json::to_string(&p).unwrap();
        let p2: UserProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(p2.expertise_tags, vec!["auth"]);
        assert_eq!(p2.domain, Some("fintech".to_string()));
        assert_eq!(p2.known_entities, vec!["Account"]);
    }

    #[test]
    fn save_and_load_round_trip() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_profile.nlprofile");

        let mut p = UserProfile::with_expertise(vec!["api".to_string()]);
        p.register_entity("Widget");
        p.save(&path).unwrap();

        let loaded = UserProfile::load(&path).unwrap();
        assert_eq!(loaded.expertise_tags, vec!["api"]);
        assert_eq!(loaded.known_entities, vec!["Widget"]);

        // Cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_returns_none() {
        let path = PathBuf::from("/tmp/nonexistent_profile_12345.nlprofile");
        assert!(UserProfile::load(&path).is_none());
    }
}
