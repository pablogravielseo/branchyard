use std::collections::HashMap;

use crate::config::{RepoConfig, ServiceConfig};

/// Port assignment for one worktree slot.
///
/// Layout per slot (base=3000, slot=1, 2 repos, 2 services):
///   slot offset = 1 * 10 = 10
///   repo[0]  → 3010
///   repo[1]  → 3011
///   service[0] (db)    → 3012
///   service[1] (redis) → 3013
#[derive(Debug, Clone)]
pub struct PortAssignment {
    pub slot: u16,
    /// Host port per repo, indexed same as config.repos.
    pub repo_ports: Vec<u16>,
    /// Host port per service, indexed same as config.services.
    pub service_ports: Vec<u16>,
}

impl PortAssignment {
    pub fn new(base: u16, slot: u16, repo_count: usize, service_count: usize) -> Self {
        let offset = slot * 10;
        let repo_ports = (0..repo_count as u16)
            .map(|i| base + offset + i)
            .collect::<Vec<_>>();
        let service_ports = (0..service_count as u16)
            .map(|i| base + offset + repo_count as u16 + i)
            .collect::<Vec<_>>();

        Self {
            slot,
            repo_ports,
            service_ports,
        }
    }

    /// Build a variable map for command interpolation.
    ///
    /// Keys available in commands:
    ///   {port}          → this repo's host port (set per-repo when interpolating)
    ///   {slug}          → worktree slug
    ///   {<name>_port}   → host port of repo named <name> (e.g. {backend_port}, {api_port})
    ///   {<name>_port}   → host port of service named <name> (e.g. {db_port}, {redis_port})
    pub fn variable_map(
        &self,
        slug: &str,
        repos: &[RepoConfig],
        services: &[ServiceConfig],
    ) -> HashMap<String, String> {
        let mut vars = HashMap::new();

        vars.insert("slug".to_string(), slug.to_string());
        // {workspace} is populated by callers that have the workspace root available

        for (i, repo) in repos.iter().enumerate() {
            if let Some(&port) = self.repo_ports.get(i) {
                vars.insert(format!("{}_port", repo.name), port.to_string());
            }
        }

        for (i, svc) in services.iter().enumerate() {
            if let Some(&port) = self.service_ports.get(i) {
                vars.insert(format!("{}_port", svc.name), port.to_string());
            }
        }

        vars
    }
}

/// Interpolate `{key}` placeholders in a command string.
pub fn interpolate(template: &str, vars: &HashMap<String, String>) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{key}}}"), value);
    }
    result
}

/// Find the next available slot not already in use.
pub fn next_available_slot(used_slots: &[u16]) -> u16 {
    (0u16..)
        .find(|slot| !used_slots.contains(slot))
        .unwrap_or(0)
}

/// Read used slots by scanning the worktrees directory for slot files.
pub fn read_used_slots(worktrees_dir: &std::path::Path) -> Vec<u16> {
    let Ok(entries) = std::fs::read_dir(worktrees_dir) else {
        return vec![];
    };

    entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let slot_file = e.path().join(".slot");
            std::fs::read_to_string(slot_file)
                .ok()
                .and_then(|s| s.trim().parse::<u16>().ok())
        })
        .collect()
}
