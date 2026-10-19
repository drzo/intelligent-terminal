//! Shared startup admission, independent of provider-specific launch details.

pub(crate) mod providers;
pub(crate) mod verification;

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

pub(crate) const MAX_STARTUP_ATTEMPTS: usize = 3;
pub(crate) const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StartupPhase {
    Preparation,
    Spawn,
    Initialize,
    Session,
    Cleanup,
}

impl std::fmt::Display for StartupPhase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for StartupPhase {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StartupReason {
    Failed,
    TimedOut,
    AuthenticationRequired,
    OperatingSystemPolicy,
    RetryLimitReached,
}

impl std::fmt::Display for StartupReason {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for StartupReason {}

#[derive(Debug)]
pub(crate) struct DependencyPrepared;

impl std::fmt::Display for DependencyPrepared {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ACP dependency is prepared")
    }
}

impl std::error::Error for DependencyPrepared {}

pub(crate) fn diagnose_stderr(lines: &[String]) -> Option<StartupReason> {
    lines
        .iter()
        .any(|line| line.to_ascii_lowercase().contains("application control policy has blocked"))
        .then_some(StartupReason::OperatingSystemPolicy)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StartupFailure {
    pub phase: StartupPhase,
    pub reason: StartupReason,
    pub attempts: usize,
    pub max_attempts: usize,
    pub dependency_present: Option<bool>,
    pub startup_verified: bool,
    pub os_error: Option<i32>,
    pub rpc_code: Option<i32>,
    pub diagnostic_id: String,
}

impl StartupFailure {
    pub(crate) fn from_error(
        error: &anyhow::Error,
        attempts: usize,
        dependency_present: Option<bool>,
        startup_verified: bool,
    ) -> Self {
        if let Some(failure) = error.downcast_ref::<Self>() {
            return failure.clone();
        }
        let rpc_code = error
            .downcast_ref::<agent_client_protocol::Error>()
            .map(|error| error.code.into());
        let reason = error
            .downcast_ref::<StartupReason>()
            .copied()
            .unwrap_or_else(|| {
                if error.is::<StartupLimitReached>() {
                    StartupReason::RetryLimitReached
                } else if error.is::<tokio::time::error::Elapsed>() {
                    StartupReason::TimedOut
                } else if rpc_code == Some(-32000) {
                    StartupReason::AuthenticationRequired
                } else {
                    StartupReason::Failed
                }
            });
        Self {
            phase: error
                .downcast_ref::<StartupPhase>()
                .copied()
                .unwrap_or(StartupPhase::Initialize),
            reason,
            attempts: error
                .downcast_ref::<StartupLimitReached>()
                .map_or(attempts, |limit| limit.attempts),
            max_attempts: MAX_STARTUP_ATTEMPTS,
            dependency_present: dependency_present
                .or_else(|| error.is::<DependencyPrepared>().then_some(true)),
            startup_verified,
            os_error: error
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::raw_os_error),
            rpc_code,
            diagnostic_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

impl std::fmt::Display for StartupFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ACP {:?} {:?} [{}/{}; diagnostic {}]",
            self.phase, self.reason, self.attempts, self.max_attempts, self.diagnostic_id
        )
    }
}

impl std::error::Error for StartupFailure {}

#[derive(Debug)]
pub(crate) struct InstallationStartupCheckFailed;

impl std::fmt::Display for InstallationStartupCheckFailed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ACP preparation succeeded but startup verification failed")
    }
}

impl std::error::Error for InstallationStartupCheckFailed {}

pub(crate) fn agent_key(
    command: &str,
    agent_id: Option<&str>,
    source: &crate::agent_source::AgentSource,
    provider_binding: &str,
    model_scoped: bool,
) -> String {
    let lifecycle = if model_scoped || command_is_model_scoped(command, agent_id) {
        "model:"
    } else {
        "warm:"
    };
    format!(
        "{lifecycle}{:?}",
        (source, agent_id, command, provider_binding)
    )
}

pub(crate) fn command_is_model_scoped(command: &str, agent_id: Option<&str>) -> bool {
    let id = agent_id.unwrap_or_else(|| crate::agent_registry::resolve_agent_id_from_cmd(command));
    if !crate::agent_registry::is_known_id(id)
        || crate::agent_registry::supports_live_model_switch(id)
    {
        return false;
    }
    let profile = crate::agent_registry::lookup_profile_by_id(id);
    let tokens = crate::coordinator::split_windows_commandline(command);
    let args: Vec<&str> = tokens.iter().skip(1).map(String::as_str).collect();
    crate::agent_registry::extract_model_from_args(&args, profile).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StartupLimitReached {
    pub attempts: usize,
}

impl std::fmt::Display for StartupLimitReached {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ACP startup attempt limit reached ({}/{MAX_STARTUP_ATTEMPTS})",
            self.attempts
        )
    }
}

impl std::error::Error for StartupLimitReached {}

#[derive(Debug)]
pub(crate) struct StartupCleanupUnconfirmed;

impl std::fmt::Display for StartupCleanupUnconfirmed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str("ACP startup is blocked because previous process cleanup was not confirmed")
    }
}

impl std::error::Error for StartupCleanupUnconfirmed {}

#[derive(Default, Serialize, Deserialize)]
struct StartupState {
    attempts: usize,
    #[serde(default)]
    last_failure: Option<StartupFailure>,
    #[serde(default)]
    cleanup_failed: bool,
}

#[derive(Default)]
pub(crate) struct StartupRegistry {
    states: Mutex<HashMap<String, Arc<AsyncMutex<StartupState>>>>,
    directory: Option<PathBuf>,
}

impl StartupRegistry {
    pub(crate) fn runtime() -> Result<Self> {
        let root = crate::runtime_paths::intelligent_terminal_local_root()
            .context("ACP startup state directory is unavailable")?;
        let executable = std::env::current_exe().context("resolve WTA startup identity")?;
        let metadata = executable
            .metadata()
            .context("inspect WTA startup identity")?;
        let identity = format!(
            "{}:{}:{}",
            executable.display(),
            metadata.file_size(),
            metadata.last_write_time()
        );
        let build = format!("{:x}", Sha256::digest(identity.as_bytes()));
        Ok(Self::persistent(root.join("acp-startup").join(build)))
    }

    fn persistent(directory: PathBuf) -> Self {
        Self {
            states: Mutex::new(HashMap::new()),
            directory: Some(directory),
        }
    }

    pub(crate) async fn lock(&self, key: &str) -> Result<StartupLease> {
        let state = {
            let mut states = self
                .states
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Arc::clone(states.entry(key.to_string()).or_default())
        };
        let mut state = state.lock_owned().await;
        let file = if let Some(directory) = &self.directory {
            let path = directory.join(format!("{:x}.json", Sha256::digest(key.as_bytes())));
            let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
            loop {
                let path = path.clone();
                let opened = tokio::task::spawn_blocking(move || open_startup_state(path))
                    .await
                    .context("ACP startup state task failed")??;
                if let Some((file, saved)) = opened {
                    *state = saved;
                    break Some(file);
                }
                ensure!(
                    tokio::time::Instant::now() < deadline,
                    "another ACP startup operation is still in progress"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        } else {
            None
        };
        Ok(StartupLease {
            state,
            file,
            attempt_number: 0,
            dependency_present: false,
            startup_verified: false,
            executable: None,
        })
    }

    pub(crate) async fn retry(&self, key: &str) -> Result<()> {
        let mut lease = self.lock(key).await?;
        lease.ensure_cleanup()?;
        lease.state.attempts = 0;
        lease.state.last_failure = None;
        lease.save()
    }
}

fn open_startup_state(path: PathBuf) -> Result<Option<(File, StartupState)>> {
    let parent = path.parent().context("ACP startup state has no parent")?;
    std::fs::create_dir_all(parent).context("create ACP startup state directory")?;
    let options = |create_new| {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .share_mode(0)
            .create_new(create_new)
            .custom_flags(0x0020_0000);
        options
    };
    let (mut file, fresh) = match options(true).open(&path) {
        Ok(file) => (file, true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            match options(false).open(&path) {
                Ok(file) => (file, false),
                Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => return Ok(None),
                Err(error) => return Err(error).context("open ACP startup state"),
            }
        }
        Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => return Ok(None),
        Err(error) => return Err(error).context("create ACP startup state"),
    };
    let metadata = file.metadata().context("inspect ACP startup state")?;
    ensure!(
        metadata.is_file() && metadata.file_attributes() & 0x400 == 0 && metadata.len() <= 4096,
        "ACP startup state is not a bounded regular file"
    );
    let state = if fresh {
        let state = StartupState::default();
        save_startup_state(&mut file, &state)?;
        state
    } else {
        let mut text = String::new();
        file.read_to_string(&mut text)
            .context("read ACP startup state")?;
        serde_json::from_str(&text).context("parse ACP startup state")?
    };
    Ok(Some((file, state)))
}

fn save_startup_state(file: &mut File, state: &StartupState) -> Result<()> {
    let data = serde_json::to_vec(state).context("serialize ACP startup state")?;
    file.seek(SeekFrom::Start(0))
        .context("seek ACP startup state")?;
    file.write_all(&data).context("write ACP startup state")?;
    file.set_len(data.len() as u64)
        .context("truncate ACP startup state")?;
    file.sync_data().context("persist ACP startup state")
}

pub(crate) struct StartupLease {
    state: OwnedMutexGuard<StartupState>,
    file: Option<File>,
    attempt_number: usize,
    dependency_present: bool,
    startup_verified: bool,
    executable: Option<String>,
}

impl StartupLease {
    fn ensure_cleanup(&self) -> Result<()> {
        if self.state.cleanup_failed {
            return Err(match &self.state.last_failure {
                Some(failure) => {
                    anyhow::Error::new(failure.clone()).context(StartupCleanupUnconfirmed)
                }
                None => {
                    anyhow::Error::new(StartupPhase::Cleanup).context(StartupCleanupUnconfirmed)
                }
            });
        }
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        match &mut self.file {
            Some(file) => save_startup_state(file, &self.state),
            None => Ok(()),
        }
    }

    pub(crate) fn begin(&mut self) -> Result<()> {
        self.ensure_cleanup()?;
        if self.state.attempts >= MAX_STARTUP_ATTEMPTS {
            let limit = StartupLimitReached {
                attempts: self.state.attempts,
            };
            return Err(match &self.state.last_failure {
                Some(failure) => anyhow::Error::new(failure.clone()).context(limit),
                None => limit.into(),
            });
        }
        self.state.attempts += 1;
        self.attempt_number = self.state.attempts;
        self.save()?;
        Ok(())
    }

    pub(crate) fn succeeded(&mut self) -> Result<()> {
        self.startup_verified = true;
        self.state.attempts = 0;
        self.state.last_failure = None;
        self.save()
    }

    pub(crate) fn prepared(&mut self, artifact: providers::PreparedArtifact) {
        self.dependency_present = artifact.outcome == providers::PreparationOutcome::Ready;
        self.executable = artifact.executable;
    }

    pub(crate) fn dependency_present(&self) -> bool {
        self.dependency_present
    }

    pub(crate) fn executable(&self) -> Option<&str> {
        self.executable.as_deref()
    }

    pub(crate) fn verified(&self) -> bool {
        self.startup_verified
    }

    pub(crate) fn failed(mut self, error: anyhow::Error) -> anyhow::Error {
        let failure = StartupFailure::from_error(
            &error,
            self.attempt_number,
            self.dependency_present.then_some(true),
            self.startup_verified,
        );
        let mut error = error.context(failure.clone());
        if failure.phase == StartupPhase::Cleanup {
            self.state.cleanup_failed = true;
        }
        if !self.startup_verified || self.state.cleanup_failed {
            self.state.last_failure = Some(failure);
            if let Err(persist_error) = self.save() {
                error = error.context(persist_error);
            }
        }
        if !self.startup_verified && self.state.attempts >= MAX_STARTUP_ATTEMPTS {
            error.context(StartupLimitReached {
                attempts: self.state.attempts,
            })
        } else {
            error
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn retry_opens_one_new_bounded_cycle_without_changing_another_target() {
        let registry = StartupRegistry::default();
        for _ in 0..MAX_STARTUP_ATTEMPTS {
            registry.lock("host:one").await.unwrap().begin().unwrap();
        }
        assert!(registry.lock("host:one").await.unwrap().begin().is_err());
        registry.lock("wsl:one").await.unwrap().begin().unwrap();
        registry.retry("host:one").await.unwrap();
        for _ in 0..MAX_STARTUP_ATTEMPTS {
            registry.lock("host:one").await.unwrap().begin().unwrap();
        }
        assert!(registry.lock("host:one").await.unwrap().begin().is_err());
        assert_eq!(registry.lock("wsl:one").await.unwrap().state.attempts, 1);
    }

    #[tokio::test]
    async fn completed_startup_resets_only_its_own_failure_budget() {
        let registry = StartupRegistry::default();
        registry.lock("failed").await.unwrap().begin().unwrap();
        let mut ready = registry.lock("ready").await.unwrap();
        ready.begin().unwrap();
        ready.succeeded().unwrap();
        drop(ready);
        assert_eq!(registry.lock("ready").await.unwrap().state.attempts, 0);
        assert_eq!(registry.lock("failed").await.unwrap().state.attempts, 1);
    }

    #[tokio::test]
    async fn replacement_waits_for_the_previous_attempt_lease() {
        use futures::FutureExt;
        let registry = StartupRegistry::default();
        let first = registry.lock("same").await.unwrap();
        assert!(registry.lock("same").now_or_never().is_none());
        assert!(registry.retry("same").now_or_never().is_none());
        drop(first);
        registry.lock("same").await.unwrap().begin().unwrap();
    }

    #[tokio::test]
    async fn independent_callers_share_persisted_failure_limits() {
        let directory = std::env::temp_dir().join(format!("wta-startup-{}", uuid::Uuid::new_v4()));
        let first = StartupRegistry::persistent(directory.clone());
        let second = StartupRegistry::persistent(directory.clone());
        for registry in [&first, &second, &first] {
            registry.lock("same-target").await.unwrap().begin().unwrap();
        }
        assert!(second.lock("same-target").await.unwrap().begin().is_err());
        second.retry("same-target").await.unwrap();
        first.lock("same-target").await.unwrap().begin().unwrap();
        drop((first, second));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn unconfirmed_cleanup_blocks_automatic_and_explicit_replacement_only_for_its_target() {
        let registry = StartupRegistry::default();
        let mut startup = registry.lock("blocked").await.unwrap();
        startup.begin().unwrap();
        let error = startup
            .failed(anyhow::anyhow!("controlled cleanup failure").context(StartupPhase::Cleanup));
        assert_eq!(
            error.downcast_ref::<StartupFailure>().unwrap().phase,
            StartupPhase::Cleanup
        );
        assert!(registry.lock("blocked").await.unwrap().begin().is_err());
        assert!(registry.retry("blocked").await.is_err());
        registry.lock("healthy").await.unwrap().begin().unwrap();
    }
}
