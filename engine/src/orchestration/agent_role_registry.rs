use std::collections::HashMap;

use super::schemas::AgentRole;

#[derive(Debug, Default)]
pub struct AgentRoleRegistry {
    roles: HashMap<String, AgentRole>,
    active_count: HashMap<String, usize>,
    assignments: HashMap<(String, String), String>,
}

impl AgentRoleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_role(&mut self, role: AgentRole) {
        self.active_count.entry(role.role_id.clone()).or_insert(0);
        self.roles.insert(role.role_id.clone(), role);
    }

    pub fn get_role(&self, role_id: &str) -> Option<&AgentRole> {
        self.roles.get(role_id)
    }

    pub fn roles_for_task_type(&self, task_type: &str) -> Vec<&AgentRole> {
        self.roles
            .values()
            .filter(|r| r.capabilities.contains(&task_type.to_string()))
            .collect()
    }

    pub fn assign_agent(
        &mut self,
        workflow_id: &str,
        node_id: &str,
        task_type: &str,
    ) -> Option<String> {
        let candidates: Vec<String> = self
            .roles_for_task_type(task_type)
            .into_iter()
            .map(|r| r.role_id.clone())
            .collect();
        for role_id in candidates {
            let active = self.active_count.get(&role_id).copied().unwrap_or(0);
            if let Some(role) = self.roles.get(&role_id) {
                if active < role.max_concurrent_nodes {
                    self.active_count.insert(role_id.clone(), active + 1);
                    self.assignments.insert(
                        (workflow_id.to_string(), node_id.to_string()),
                        role_id.clone(),
                    );
                    return Some(role_id);
                }
            }
        }
        None
    }

    pub fn release_agent(&mut self, role_id: &str) {
        let current = self.active_count.get(role_id).copied().unwrap_or(0);
        self.active_count
            .insert(role_id.to_string(), current.saturating_sub(1));
    }

    pub fn release_node(&mut self, workflow_id: &str, node_id: &str) -> Option<String> {
        let key = (workflow_id.to_string(), node_id.to_string());
        if let Some(role_id) = self.assignments.remove(&key) {
            self.release_agent(&role_id);
            Some(role_id)
        } else {
            None
        }
    }

    pub fn get_assignment(&self, workflow_id: &str, node_id: &str) -> Option<&str> {
        self.assignments
            .get(&(workflow_id.to_string(), node_id.to_string()))
            .map(|s| s.as_str())
    }

    pub fn all_roles(&self) -> Vec<&AgentRole> {
        self.roles.values().collect()
    }
}
