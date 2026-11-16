//! Agent credentials and workspace grants. Not wired into request handling yet.
use std::collections::{HashMap, HashSet};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Permission { Read, Write, LexicalSearch, SemanticSearch, SmartSearch }

#[derive(Default)]
pub struct AgentStore { agents: HashMap<String, Agent> }
struct Agent { digest: [u8; 32], grants: HashMap<String, HashSet<Permission>> }

impl AgentStore {
    pub fn register(&mut self, id: &str, token: &str) -> Result<(), &'static str> {
        if id.is_empty() || token.is_empty() || self.agents.contains_key(id) { return Err("invalid agent"); }
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        if self.agents.values().any(|a| a.digest == digest) { return Err("duplicate credential"); }
        self.agents.insert(id.to_owned(), Agent { digest, grants: HashMap::new() });
        Ok(())
    }
    pub fn grant(&mut self, id: &str, workspace: &str, permission: Permission) -> Result<(), &'static str> {
        if workspace.is_empty() { return Err("invalid workspace"); }
        self.agents.get_mut(id).ok_or("unknown agent")?.grants.entry(workspace.to_owned()).or_default().insert(permission);
        Ok(())
    }
    pub fn authorize(&self, token: &str, workspace: &str, permission: Permission) -> bool {
        let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        self.agents.values().any(|a| {
            let difference = a.digest.iter().zip(digest).fold(0u8, |v, (x, y)| v | (x ^ y));
            difference == 0 && a.grants.get(workspace).is_some_and(|p| p.contains(&permission))
        })
    }
    pub fn revoke(&mut self, id: &str) -> Result<(), &'static str> {
        self.agents.remove(id).map(|_| ()).ok_or("unknown agent")
    }
}
