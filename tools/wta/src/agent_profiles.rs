//! Effective profile capabilities supplied by Terminal after profile inheritance.

use anyhow::{ensure, Context, Result};
use serde::Deserialize;

use crate::{agent_source::AgentSource, custom_agents::Invocation};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentProfile {
    pub(crate) profile_id: String,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) source: String,
    #[serde(default)]
    pub(crate) wsl_distro: String,
    pub(crate) acp_enabled: bool,
    pub(crate) acp: Option<Invocation>,
    pub(crate) cli: Option<Invocation>,
    pub(crate) resume: Option<Invocation>,
}

impl AgentProfile {
    pub(crate) fn execution_source(&self) -> Result<AgentSource> {
        ensure!(
            matches!(self.source.as_str(), "host" | "wsl"),
            "unknown agent profile execution source"
        );
        ensure!(
            self.source != "host" || self.wsl_distro.is_empty(),
            "a host profile cannot name a WSL distribution"
        );
        AgentSource::from_wire(Some(&self.source), Some(&self.wsl_distro))
    }
}

pub(crate) fn catalog(settings: &serde_json::Value) -> Result<Option<Vec<AgentProfile>>> {
    let Some(value) = settings.get("resolvedAgentProfiles") else {
        return Ok(None);
    };
    let profiles: Vec<AgentProfile> =
        serde_json::from_value(value.clone()).context("invalid resolved agent profile catalog")?;
    let mut identities = std::collections::HashSet::new();
    for profile in &profiles {
        uuid::Uuid::parse_str(&profile.profile_id).context("invalid agent profile GUID")?;
        ensure!(
            identities.insert(profile.profile_id.to_ascii_lowercase()),
            "duplicate agent profile GUID"
        );
        ensure!(
            crate::agent_registry::is_known_id(&profile.id)
                || crate::custom_agents::is_custom_id(&profile.id),
            "unknown agent profile provider"
        );
        profile.execution_source()?;
        if let Some(invocation) = &profile.acp {
            invocation.validate(false)?;
        }
        if let Some(invocation) = &profile.cli {
            invocation.validate(false)?;
        }
        if let Some(invocation) = &profile.resume {
            invocation.validate(true)?;
        }
    }
    Ok(Some(profiles))
}

pub(crate) fn resolve(settings: &serde_json::Value, id: &str) -> Result<AgentProfile> {
    catalog(settings)?
        .context("the Terminal host did not provide resolved agent profiles")?
        .into_iter()
        .find(|profile| profile.profile_id.eq_ignore_ascii_case(id))
        .context("the selected agent profile no longer exists")
}

pub(crate) fn for_provider(
    settings: &serde_json::Value,
    provider: &str,
    source: &AgentSource,
) -> Result<Option<AgentProfile>> {
    let Some(profiles) = catalog(settings)? else {
        return Ok(None);
    };
    let mut matches = Vec::new();
    for profile in profiles {
        if profile.id.eq_ignore_ascii_case(provider) && profile.execution_source()? == *source {
            matches.push(profile);
        }
    }
    ensure!(matches.len() <= 1, "session provider has multiple profiles in this execution source; an exact profile is required");
    Ok(matches.pop())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn entry(id: &str, distro: &str) -> serde_json::Value {
        json!({
            "profileId": id,
            "id": "custom:work",
            "name": "Work",
            "source": if distro.is_empty() { "host" } else { "wsl" },
            "wslDistro": distro,
            "acpEnabled": true,
            "acp": {"command": "work", "args": ["--stdio"]}
        })
    }

    #[test]
    fn profiles_preserve_provider_and_source_identity() {
        let host_id = "{11111111-1111-1111-1111-111111111111}";
        let wsl_id = "{22222222-2222-2222-2222-222222222222}";
        let settings =
            json!({"resolvedAgentProfiles": [entry(host_id, ""), entry(wsl_id, "Ubuntu")]});
        let host = resolve(&settings, host_id).unwrap();
        let wsl = resolve(&settings, wsl_id).unwrap();
        assert_eq!(host.id, wsl.id);
        assert_eq!(host.execution_source().unwrap(), AgentSource::Host);
        assert_eq!(
            wsl.execution_source().unwrap(),
            AgentSource::Wsl {
                distro: "Ubuntu".into()
            }
        );
        assert!(resolve(&settings, "{33333333-3333-3333-3333-333333333333}").is_err());
        assert_eq!(
            for_provider(&settings, "custom:work", &AgentSource::Host)
                .unwrap()
                .unwrap()
                .profile_id,
            host_id
        );
        let ambiguous = json!({"resolvedAgentProfiles": [
            entry(host_id, ""), entry(wsl_id, "")
        ]});
        assert!(for_provider(&ambiguous, "custom:work", &AgentSource::Host).is_err());
    }

    #[test]
    fn invalid_source_and_duplicate_identity_fail_closed() {
        let id = "{11111111-1111-1111-1111-111111111111}";
        let mut profile = entry(id, "");
        profile["source"] = json!("remote");
        assert!(catalog(&json!({"resolvedAgentProfiles": [profile]})).is_err());
        assert!(
            catalog(&json!({"resolvedAgentProfiles": [entry(id, ""), entry(id, "Ubuntu")]}))
                .is_err()
        );
        assert!(catalog(&json!({"resolvedAgentProfiles": [entry(id, "Ubuntu;exec")]})).is_err());
    }

    #[test]
    fn disabled_profile_retains_history_capabilities() {
        let id = "{11111111-1111-1111-1111-111111111111}";
        let mut profile = entry(id, "");
        profile["acpEnabled"] = json!(false);
        let resolved = resolve(&json!({"resolvedAgentProfiles": [profile]}), id).unwrap();
        assert!(!resolved.acp_enabled);
        assert!(resolved.acp.is_some());
        assert!(catalog(&json!({})).unwrap().is_none());
        assert!(catalog(&json!({"resolvedAgentProfiles": []}))
            .unwrap()
            .unwrap()
            .is_empty());
    }
}
