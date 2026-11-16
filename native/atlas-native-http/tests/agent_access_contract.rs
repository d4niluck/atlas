// Behavioral contract for the forthcoming agent-access module.
// The production module is intentionally absent until the red phase is complete.
#[path = "../src/agent_access.rs"]
mod agent_access;
use agent_access::{AgentStore, Permission};

#[test]
fn agent_keys_are_scoped_to_explicit_workspace_grants() {
    let mut store = AgentStore::default();
    store.register("agent-a", "token-a").unwrap();
    store.register("agent-b", "token-b").unwrap();
    assert!(!store.authorize("token-a", "workspace-a", Permission::Read));
    store.grant("agent-a", "workspace-a", Permission::Read).unwrap();
    assert!(store.authorize("token-a", "workspace-a", Permission::Read));
    assert!(!store.authorize("token-b", "workspace-a", Permission::Read));
    assert!(!store.authorize("token-a", "workspace-b", Permission::Read));
    assert!(!store.authorize("token-a", "workspace-a", Permission::Write));
    assert!(!store.authorize("invalid", "workspace-a", Permission::Read));
}

#[test]
fn search_modes_and_revocation_are_independent() {
    let mut store = AgentStore::default();
    store.register("agent-a", "token-a").unwrap();
    store.grant("agent-a", "workspace-a", Permission::LexicalSearch).unwrap();
    assert!(store.authorize("token-a", "workspace-a", Permission::LexicalSearch));
    assert!(!store.authorize("token-a", "workspace-a", Permission::SemanticSearch));
    assert!(!store.authorize("token-a", "workspace-a", Permission::SmartSearch));
    store.revoke("agent-a").unwrap();
    assert!(!store.authorize("token-a", "workspace-a", Permission::LexicalSearch));
}
