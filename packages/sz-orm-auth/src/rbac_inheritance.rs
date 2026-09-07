//! v6.7.0 RBAC 角色继承与循环检测。

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct RoleInheritance {
    pub child_role: String,
    pub parent_roles: Vec<String>,
}

pub struct InheritanceGraph {
    edges: HashMap<String, Vec<String>>,
}

impl InheritanceGraph {
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    pub fn add(&mut self, inheritance: RoleInheritance) -> Result<(), String> {
        for parent in &inheritance.parent_roles {
            self.edges
                .entry(inheritance.child_role.clone())
                .or_default()
                .push(parent.clone());
        }
        if let Some(cycle) = self.detect_cycle(&inheritance.child_role) {
            return Err(format!("RBAC_CYCLE_DETECTED: {:?}", cycle));
        }
        Ok(())
    }

    fn detect_cycle(&self, start: &str) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut path = Vec::new();
        self.dfs(start, &mut visited, &mut path)
    }

    fn dfs(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if path.contains(&node.to_string()) {
            path.push(node.to_string());
            return Some(path.clone());
        }
        if visited.contains(node) {
            return None;
        }
        visited.insert(node.to_string());
        path.push(node.to_string());

        if let Some(parents) = self.edges.get(node) {
            for parent in parents {
                if let Some(cycle) = self.dfs(parent, visited, path) {
                    return Some(cycle);
                }
            }
        }
        path.pop();
        None
    }

    pub fn effective_permissions(
        &self,
        role: &str,
        direct_permissions: &HashMap<String, HashSet<String>>,
    ) -> HashSet<String> {
        let mut result = HashSet::new();
        let mut visited = HashSet::new();
        self.collect_permissions(role, direct_permissions, &mut result, &mut visited);
        result
    }

    fn collect_permissions(
        &self,
        role: &str,
        direct_permissions: &HashMap<String, HashSet<String>>,
        result: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) {
        if !visited.insert(role.to_string()) {
            return;
        }
        if let Some(perms) = direct_permissions.get(role) {
            result.extend(perms.clone());
        }
        if let Some(parents) = self.edges.get(role) {
            for parent in parents {
                self.collect_permissions(parent, direct_permissions, result, visited);
            }
        }
    }
}

impl Default for InheritanceGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn child_inherits_parent_permissions() {
        let mut graph = InheritanceGraph::new();
        graph
            .add(RoleInheritance {
                child_role: "A".to_string(),
                parent_roles: vec!["B".to_string()],
            })
            .unwrap();
        let mut perms = HashMap::new();
        perms.insert("B".to_string(), HashSet::from(["read".to_string()]));
        let effective = graph.effective_permissions("A", &perms);
        assert!(effective.contains("read"));
    }

    #[test]
    fn cycle_detected() {
        let mut graph = InheritanceGraph::new();
        graph
            .add(RoleInheritance {
                child_role: "A".to_string(),
                parent_roles: vec!["B".to_string()],
            })
            .unwrap();
        let result = graph.add(RoleInheritance {
            child_role: "B".to_string(),
            parent_roles: vec!["A".to_string()],
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("RBAC_CYCLE_DETECTED"));
    }

    #[test]
    fn multi_level_inheritance() {
        let mut graph = InheritanceGraph::new();
        graph
            .add(RoleInheritance {
                child_role: "A".to_string(),
                parent_roles: vec!["B".to_string()],
            })
            .unwrap();
        graph
            .add(RoleInheritance {
                child_role: "B".to_string(),
                parent_roles: vec!["C".to_string()],
            })
            .unwrap();
        let mut perms = HashMap::new();
        perms.insert("C".to_string(), HashSet::from(["admin".to_string()]));
        let effective = graph.effective_permissions("A", &perms);
        assert!(effective.contains("admin"));
    }

    #[test]
    fn self_cycle_detected() {
        let mut graph = InheritanceGraph::new();
        let result = graph.add(RoleInheritance {
            child_role: "A".to_string(),
            parent_roles: vec!["A".to_string()],
        });
        assert!(result.is_err());
    }

    #[test]
    fn no_inheritance_own_permissions_only() {
        let graph = InheritanceGraph::new();
        let mut perms = HashMap::new();
        perms.insert("A".to_string(), HashSet::from(["write".to_string()]));
        let effective = graph.effective_permissions("A", &perms);
        assert_eq!(effective, HashSet::from(["write".to_string()]));
    }

    #[test]
    fn diamond_inheritance_no_duplicate() {
        let mut graph = InheritanceGraph::new();
        graph
            .add(RoleInheritance {
                child_role: "A".to_string(),
                parent_roles: vec!["B".to_string(), "C".to_string()],
            })
            .unwrap();
        graph
            .add(RoleInheritance {
                child_role: "B".to_string(),
                parent_roles: vec!["D".to_string()],
            })
            .unwrap();
        graph
            .add(RoleInheritance {
                child_role: "C".to_string(),
                parent_roles: vec!["D".to_string()],
            })
            .unwrap();
        let mut perms = HashMap::new();
        perms.insert("D".to_string(), HashSet::from(["read".to_string()]));
        let effective = graph.effective_permissions("A", &perms);
        assert_eq!(effective.len(), 1);
        assert!(effective.contains("read"));
    }
}
