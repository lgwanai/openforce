use glob::Pattern;
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum AclError {
    #[error("path not allowed: {path}")]
    PathNotAllowed { path: String },
}

pub struct PathAcl {
    allowed_read: Vec<Pattern>,
    allowed_write: Vec<Pattern>,
    forbidden: Vec<Pattern>,
}

impl PathAcl {
    pub fn new(
        allowed_read_paths: &[String],
        allowed_write_paths: &[String],
        forbidden_paths: &[String],
    ) -> Self {
        Self {
            allowed_read: compile_patterns(allowed_read_paths),
            allowed_write: compile_patterns(allowed_write_paths),
            forbidden: compile_patterns(forbidden_paths),
        }
    }

    pub fn can_read(&self, path: &str) -> bool {
        if self.matches_any(&self.forbidden, path) { return false; }
        self.matches_any(&self.allowed_read, path)
    }

    pub fn can_write(&self, path: &str) -> bool {
        if self.matches_any(&self.forbidden, path) { return false; }
        self.matches_any(&self.allowed_write, path)
    }

    pub fn can_delete(&self, path: &str) -> bool {
        self.can_write(path)
    }

    fn matches_any(&self, patterns: &[Pattern], path: &str) -> bool {
        if patterns.is_empty() { return true; }
        patterns.iter().any(|p| p.matches(path))
    }
}

fn compile_patterns(paths: &[String]) -> Vec<Pattern> {
    paths.iter().filter_map(|p| Pattern::new(p).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_allowed() {
        let acl = PathAcl::new(
            &["frontend/src/**".into()],
            &[],
            &[],
        );
        assert!(acl.can_read("frontend/src/components/button.tsx"));
        assert!(!acl.can_read("backend/auth/service.go"));
    }

    #[test]
    fn test_write_forbidden() {
        let acl = PathAcl::new(
            &[],
            &["frontend/src/**".into()],
            &["infra/prod/**".into(), "migrations/**".into()],
        );
        assert!(acl.can_write("frontend/src/pages/login.tsx"));
        assert!(!acl.can_write("infra/prod/deploy.yaml"));
        assert!(!acl.can_write("migrations/001.sql"));
    }

    #[test]
    fn test_delete_blocked_by_forbidden() {
        let acl = PathAcl::new(
            &[],
            &["**".into()],
            &["backend/auth/**".into(), "migrations/**".into()],
        );
        assert!(!acl.can_delete("backend/auth/service.go"));
        assert!(!acl.can_delete("migrations/004.sql"));
        assert!(acl.can_delete("frontend/readme.md"));
    }

    #[test]
    fn test_empty_rules_allow_all() {
        let acl = PathAcl::new(&[], &[], &[]);
        assert!(acl.can_read("anything.txt"));
        assert!(acl.can_write("anything.txt"));
    }

    #[test]
    fn test_forbidden_takes_priority() {
        let acl = PathAcl::new(
            &["**".into()],
            &["**".into()],
            &["secrets/**".into()],
        );
        assert!(!acl.can_read("secrets/token.env"));
        assert!(!acl.can_write("secrets/token.env"));
    }
}
