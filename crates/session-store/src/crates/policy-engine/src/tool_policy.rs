pub struct ToolPolicyEngine;
#[derive(Debug, Clone, PartialEq, Eq)] pub enum ToolRisk { Safe, Moderate, Sensitive }
impl ToolPolicyEngine {
    pub fn classify(name: &str) -> ToolRisk {
        match name {
            "read_local_file" | "read_project_file" | "read_project_tree" | "run_tests" => ToolRisk::Safe,
            "write_local_file" | "write_project_patch" => ToolRisk::Moderate,
            "delete_project_file" | "direct_prod_deploy" | "direct_db_write" => ToolRisk::Sensitive,
            _ => ToolRisk::Moderate,
        }
    }
    pub fn requires_approval(name: &str) -> bool { Self::classify(name) == ToolRisk::Sensitive }
    pub fn is_blocked(blocked: &[String], name: &str) -> bool { blocked.iter().any(|b| b == name) }
}
