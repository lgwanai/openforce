use glob::Pattern;
use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum AclError { #[error("path not allowed: {path}")] PathNotAllowed { path: String } }

pub struct PathAcl {
    allowed_read: Vec<Pattern>,
    allowed_write: Vec<Pattern>,
    forbidden: Vec<Pattern>,
}

impl PathAcl {
    pub fn new(allowed_read_paths: &[String], allowed_write_paths: &[String], forbidden_paths: &[String]) -> Self {
        Self {
            allowed_read: compile(allowed_read_paths),
            allowed_write: compile(allowed_write_paths),
            forbidden: compile(forbidden_paths),
        }
    }

    pub fn can_read(&self, path: &str) -> bool {
        let n = match normalize_path(path) { Some(p) => p, None => return false };
        if !self.forbidden.is_empty() && self.matches(&self.forbidden, &n) { return false; }
        self.allowed_read.is_empty() || self.matches(&self.allowed_read, &n)
    }

    pub fn can_write(&self, path: &str) -> bool {
        let n = match normalize_path(path) { Some(p) => p, None => return false };
        if !self.forbidden.is_empty() && self.matches(&self.forbidden, &n) { return false; }
        self.allowed_write.is_empty() || self.matches(&self.allowed_write, &n)
    }

    pub fn can_delete(&self, path: &str) -> bool { self.can_write(path) }

    fn matches(&self, patterns: &[Pattern], path: &str) -> bool {
        patterns.iter().any(|p| p.matches(path))
    }
}

fn compile(paths: &[String]) -> Vec<Pattern> {
    paths.iter().filter_map(|p| {
        Pattern::new(p).map_err(|e| { tracing::warn!("Invalid ACL pattern '{}': {}", p, e); }).ok()
    }).collect()
}

/// Normalize path by resolving `.` and `..` segments, collapsing duplicate slashes.
/// Returns None for paths that escape the root via `..` traversal.
pub fn normalize_path(path: &str) -> Option<String> {
    let mut out = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => continue,
            ".." => { if out.is_empty() { return None; } out.pop(); }
            s => out.push(s),
        }
    }
    Some(if out.is_empty() { ".".into() } else { out.join("/") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn test_normalize_dot() { assert_eq!(normalize_path("a/./b"), Some("a/b".into())); }
    #[test] fn test_normalize_dotdot() { assert_eq!(normalize_path("a/b/../c"), Some("a/c".into())); }
    #[test] fn test_normalize_doubleslash() { assert_eq!(normalize_path("a//b"), Some("a/b".into())); }
    #[test] fn test_traversal_blocked() { assert_eq!(normalize_path("../etc/passwd"), None); }
    #[test] fn test_deep_traversal_blocked() { assert_eq!(normalize_path("a/../../etc"), None); }

    #[test] fn test_traversal_normalized_into_forbidden() {
        let acl = PathAcl::new(&["**".into()], &["**".into()], &["secrets/*".into()]);
        assert!(!acl.can_read("secrets/token.env"));
        // Traversal normalizes to secrets/token.env → blocked by forbidden rule
        assert!(!acl.can_read("frontend/../../secrets/token.env"));
    }

    #[test] fn test_traversal_cannot_escape_allowed_dir() {
        let acl = PathAcl::new(&["frontend/src/**".into()], &[], &[]);
        assert!(acl.can_read("frontend/src/components/button.tsx"));
        assert!(!acl.can_read("frontend/src/../../../backend/secrets.env"));
    }

    #[test] fn test_read_allowed() {
        let acl = PathAcl::new(&["frontend/src/**".into()], &[], &[]);
        assert!(acl.can_read("frontend/src/components/button.tsx"));
        assert!(!acl.can_read("backend/auth/service.go"));
    }

    #[test] fn test_write_forbidden() {
        let acl = PathAcl::new(&[], &["frontend/src/**".into()], &["infra/prod/**".into()]);
        assert!(acl.can_write("frontend/src/pages/login.tsx"));
        assert!(!acl.can_write("infra/prod/deploy.yaml"));
    }

    #[test] fn test_forbidden_priority() {
        let acl = PathAcl::new(&["**".into()], &["**".into()], &["secrets/**".into()]);
        assert!(!acl.can_read("secrets/token.env"));
    }

    #[test] fn test_empty_rules_allow_all() {
        let acl = PathAcl::new(&[], &[], &[]);
        assert!(acl.can_read("anything.txt"));
    }
}
