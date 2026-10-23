use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use crate::coordinator::{DelegateAgentRuntime, DelegatePromptDelivery};
use crate::shell::ShellManager;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct NativeProfile {
    pub guid: String,
    pub name: String,
    pub agent_id: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    commandline: String,
    #[serde(default)]
    managed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Placement {
    #[default]
    Tab,
    Pane,
}

pub(crate) struct LaunchRequest<'a> {
    pub profile: Option<&'a str>,
    pub provider: Option<&'a str>,
    pub source_pane: Option<&'a str>,
    pub window_id: Option<u64>,
    pub cwd: Option<&'a str>,
    pub task: Option<&'a str>,
    pub include_context: bool,
    pub placement: Placement,
    pub direction: &'a str,
    pub size: f64,
    pub background: bool,
    pub title: Option<&'a str>,
}

pub(crate) async fn list(shell: &ShellManager, window: Option<u64>) -> Result<Vec<NativeProfile>> {
    let profiles: Vec<NativeProfile> =
        serde_json::from_value(shell.wt_agent_profiles(window, None).await?)
            .context("Terminal returned an invalid agent profile catalog")?;
    Ok(profiles
        .into_iter()
        .filter(|profile| {
            profile.agent_id.starts_with("custom:")
                || crate::agent_registry::lookup_profile_by_id(&profile.agent_id).delegate_supported
        })
        .collect())
}

fn select<'a>(
    profiles: &'a [NativeProfile],
    selector: Option<&str>,
    provider: Option<&str>,
) -> Result<&'a NativeProfile> {
    if let Some(selector) = selector {
        if let Some(profile) = profiles
            .iter()
            .find(|profile| profile.guid.eq_ignore_ascii_case(selector))
        {
            return Ok(profile);
        }
        let mut matches = profiles
            .iter()
            .filter(|profile| profile.name.eq_ignore_ascii_case(selector));
        let Some(profile) = matches.next() else {
            let mut matches = profiles
                .iter()
                .filter(|profile| profile.agent_id.eq_ignore_ascii_case(selector));
            let profile = matches
                .next()
                .ok_or_else(|| anyhow!("the selected agent profile is unavailable"))?;
            anyhow::ensure!(
                matches.next().is_none(),
                "agent has multiple profiles; select its exact profile entry"
            );
            return Ok(profile);
        };
        anyhow::ensure!(
            matches.next().is_none(),
            "profile name is ambiguous; select its exact profile entry"
        );
        return Ok(profile);
    }
    profiles
        .iter()
        .find(|profile| {
            provider.map_or(profile.default, |id| {
                profile.agent_id.eq_ignore_ascii_case(id)
            })
        })
        .ok_or_else(|| anyhow!("no profile is available for the configured delegate agent"))
}

fn runtime(profile: &NativeProfile) -> Result<DelegateAgentRuntime> {
    let commandline = if profile.managed {
        // Terminal wraps managed npm shims with the system cmd.exe. Deliver
        // the startup task to the inner CLI, not to the command interpreter.
        if let Some((_, inner)) = profile.commandline.split_once(" /d /s /v:off /c \"") {
            inner
                .strip_suffix('"')
                .ok_or_else(|| anyhow!("invalid managed profile command"))?
                .to_string()
        } else {
            profile.commandline.clone()
        }
    } else {
        profile.commandline.clone()
    };
    anyhow::ensure!(
        !commandline.trim().is_empty(),
        "agent profile has no launch command"
    );
    let executable = crate::coordinator::split_windows_commandline(&commandline)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("agent profile has no executable"))?;
    let name = executable
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(&executable)
        .to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "cmd"
            | "cmd.exe"
            | "powershell"
            | "powershell.exe"
            | "pwsh"
            | "pwsh.exe"
            | "wsl"
            | "wsl.exe"
    ) {
        bail!("this profile uses a shell wrapper; delegation requires a direct interactive agent command");
    }
    Ok(DelegateAgentRuntime {
        id: profile.agent_id.clone(),
        name: profile.name.clone(),
        description: String::new(),
        commandline,
        prompt_delivery: DelegatePromptDelivery::LaunchWithStartupPrompt,
        // The profile, not the old delegate model setting, owns its arguments.
        model: None,
    })
}

fn new_session_id(runtime: &DelegateAgentRuntime) -> Option<String> {
    let provider = crate::agent_registry::lookup_profile_by_id(&runtime.id);
    let flag = provider.new_session_id_flag?;
    let owns_session = crate::coordinator::split_windows_commandline(&runtime.commandline)
        .iter()
        .skip(1)
        .any(|argument| {
            let option = argument.split('=').next().unwrap_or(argument);
            option == flag
                || (!provider.resume_flag.is_empty() && option == provider.resume_flag)
                || matches!(option, "--continue" | "-c" | "-r" | "--")
        });
    (!owns_session).then(|| uuid::Uuid::new_v4().to_string())
}

pub(crate) async fn launch(
    shell: &ShellManager,
    request: LaunchRequest<'_>,
) -> Result<serde_json::Value> {
    let lines = if request.include_context { 30 } else { 0 };
    let chars = if request.include_context { 4000 } else { 0 };
    let snapshot = match request.source_pane {
        Some(owner) => {
            shell
                .wt()?
                .request(
                    "get_delegate_context",
                    serde_json::json!({
                        "session_id": owner, "max_lines": lines, "max_chars": chars,
                    }),
                )
                .await
        }
        None => shell.wt_get_pane_context(None, lines, chars).await,
    }
    .context("could not capture the source terminal")?;
    let pane = &snapshot["pane"];
    let source_id = pane["session_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow!("source terminal has no pane identity"))?;
    let window = pane["window_id"]
        .as_u64()
        .filter(|id| *id != 0)
        .or(request.window_id)
        .filter(|id| *id != 0)
        .ok_or_else(|| anyhow!("source terminal has no window identity"))?;
    let profiles = list(shell, Some(window)).await?;
    let selected = select(&profiles, request.profile, request.provider)?;
    anyhow::ensure!(
        request.provider.map_or(true, |provider| selected
            .agent_id
            .eq_ignore_ascii_case(provider)),
        "selected profile belongs to a different delegate agent"
    );
    let resolved: Vec<NativeProfile> = serde_json::from_value(
        shell
            .wt_agent_profiles(Some(window), Some(&selected.guid))
            .await?,
    )?;
    let profile = resolved
        .first()
        .filter(|profile| profile.guid == selected.guid && profile.agent_id == selected.agent_id)
        .ok_or_else(|| anyhow!("selected agent profile changed before launch"))?;
    let runtime = runtime(profile)?;
    let cwd = request
        .cwd
        .filter(|cwd| !cwd.trim().is_empty())
        .or_else(|| pane["cwd"].as_str().filter(|cwd| !cwd.is_empty()));
    if let Some(cwd) = cwd {
        anyhow::ensure!(
            std::path::Path::new(cwd).is_absolute() && !cwd.starts_with('/'),
            "the selected host profile cannot use the source terminal's working directory"
        );
    }
    let task = request.task.map(|task| {
        let output = snapshot["content"].as_str().unwrap_or_default();
        if !request.include_context || output.is_empty() {
            return task.to_string();
        }
        format!(
            "{task}\n\n{}{})\nWorking directory: {}\n```\n{output}\n```",
            crate::session_registry::TERMINAL_CONTEXT_TITLE_MARKER,
            source_id,
            cwd.unwrap_or(""),
        )
    });
    let session_id = new_session_id(&runtime);
    let commandline = crate::coordinator::build_profile_launch_commandline(
        &runtime,
        task.as_deref(),
        session_id.as_deref(),
    )?;
    let commandline = match (request.placement, cwd) {
        (Placement::Pane, Some(cwd)) => {
            crate::coordinator::with_windows_delegate_cwd(&commandline, cwd)?
        }
        _ => commandline,
    };
    let mut params = serde_json::json!({
        "commandline": commandline,
        "profile": profile.guid,
        "native_agent_provider_id": profile.agent_id,
        "window_id": window,
        "background": request.background,
    });
    if let Some(cwd) = cwd {
        params["cwd"] = cwd.into();
    }
    if let Some(title) = request.title {
        params["title"] = title.into();
    }
    let method = match request.placement {
        Placement::Tab => "create_tab",
        Placement::Pane => {
            params["session_id"] = source_id.into();
            params["direction"] = request.direction.into();
            params["size"] = request.size.into();
            "split_pane"
        }
    };
    let mut created = shell.wt()?.request(method, params).await?;
    let created_id = created["session_id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or_else(|| anyhow!("Terminal created no agent pane"))?;
    if request.background {
        shell
            .wt_focus_pane(created_id)
            .await
            .context("agent pane was created but could not be focused")?;
    }
    shell
        .wt_publish_event(serde_json::json!({
            "type": "event", "method": "agent_event",
            "params": {
                "event": "agent.pane.launch.source", "pane_id": created_id,
                "agent": profile.agent_id,
                "location": crate::agent_source::AgentSource::Host.session_location(),
            }
        }))
        .await
        .context("agent pane was created but source publication failed")?;
    created["agent_id"] = profile.agent_id.clone().into();
    if let Some(session_id) = session_id {
        created["agent_session_id"] = session_id.into();
    }
    if let Some(cwd) = cwd {
        created["cwd"] = cwd.into();
    }
    Ok(created)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DelegateInput {
    pub profile: Option<String>,
    pub placement: Placement,
    pub task: String,
}

fn token(input: &str) -> Result<(&str, &str)> {
    let input = input.trim_start();
    if let Some(quoted) = input.strip_prefix('"') {
        let end = quoted
            .find('"')
            .ok_or_else(|| anyhow!("unterminated profile name"))?;
        anyhow::ensure!(
            quoted[end + 1..].is_empty() || quoted[end + 1..].starts_with(char::is_whitespace),
            "expected a space after the profile name"
        );
        Ok((&quoted[..end], &quoted[end + 1..]))
    } else {
        let end = input.find(char::is_whitespace).unwrap_or(input.len());
        Ok((&input[..end], &input[end..]))
    }
}

pub(crate) fn parse(mut input: &str) -> Result<DelegateInput> {
    let mut profile = None;
    let mut placement = Placement::Tab;
    let mut target_set = false;
    loop {
        input = input.trim_start();
        if !input.starts_with("--") {
            break;
        }
        let (flag, rest) = token(input)?;
        match flag {
            "--agent" | "--profile" => {
                anyhow::ensure!(profile.is_none(), "profile was specified more than once");
                let (value, rest) = token(rest)?;
                anyhow::ensure!(
                    !value.is_empty() && !value.starts_with("--"),
                    "a profile is required"
                );
                profile = Some(value.to_string());
                input = rest;
            }
            "--target" => {
                anyhow::ensure!(!target_set, "target was specified more than once");
                let (value, rest) = token(rest)?;
                placement = match value {
                    "tab" => Placement::Tab,
                    "pane" => Placement::Pane,
                    _ => bail!("target must be tab or pane"),
                };
                target_set = true;
                input = rest;
            }
            "--" => {
                input = rest.trim_start();
                break;
            }
            flag if flag.starts_with("--") => bail!("unknown delegation option"),
            _ => break,
        }
    }
    anyhow::ensure!(!input.trim().is_empty(), "a delegation task is required");
    Ok(DelegateInput {
        profile,
        placement,
        task: input.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    fn profile(guid: &str, name: &str, default: bool) -> NativeProfile {
        NativeProfile {
            guid: guid.into(),
            name: name.into(),
            agent_id: "claude".into(),
            default,
            commandline: r#""C:\Agent Tools\custom-claude.exe" --model configured"#.into(),
            managed: false,
        }
    }

    #[test]
    fn delegate_parses_optional_profile_and_placement_without_touching_task() {
        let parsed = parse(
                            "--target pane --profile \"Claude Project\" fix this\nand preserve --target tab in the task",
                        )
                        .unwrap();
        assert_eq!(parsed.profile.as_deref(), Some("Claude Project"));
        assert_eq!(parsed.placement, Placement::Pane);
        assert_eq!(
            parsed.task,
            "fix this\nand preserve --target tab in the task"
        );
        assert_eq!(
            parse("\"unfinished task").unwrap().task,
            "\"unfinished task"
        );
        assert_eq!(parse("-- --literal task").unwrap().task, "--literal task");
        assert_eq!(parse("fix it").unwrap().placement, Placement::Tab);
    }

    #[test]
    fn delegate_rejects_missing_task_bad_target_and_duplicate_options() {
        for input in [
            "",
            "--profile",
            "--profile claude",
            "--target nowhere task",
            "--target pane",
            "--profile claude --profile other task",
            "--target tab --target pane task",
            "--unknown task",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn selection_uses_profile_identity_and_rejects_ambiguous_names() {
        let profiles = vec![
            profile("{first}", "Project", true),
            profile("{second}", "Project", false),
        ];
        assert_eq!(select(&profiles, None, None).unwrap().guid, "{first}");
        assert_eq!(
            select(&profiles, Some("{SECOND}"), None).unwrap().guid,
            "{second}"
        );
        assert!(select(&profiles, Some("Project"), None).is_err());
        assert!(select(&profiles, Some("missing"), None).is_err());
        assert!(select(&profiles, None, Some("codex")).is_err());
    }

    #[test]
    fn managed_shim_preserves_executable_and_profile_arguments() {
        let mut selected = profile("{id}", "Claude", true);
        selected.managed = true;
        selected.commandline = r#""C:\Windows\System32\cmd.exe" /d /s /v:off /c ""C:\Agent Tools\claude.cmd" "--model" "chosen" "--continue"""#.into();
        let runtime = runtime(&selected).unwrap();
        assert_eq!(
            runtime.commandline,
            r#""C:\Agent Tools\claude.cmd" "--model" "chosen" "--continue""#
        );
        assert!(runtime.model.is_none());
        assert!(
            crate::coordinator::build_profile_launch_commandline(&runtime, Some("task"), None)
                .is_err()
        );
    }

    #[test]
    fn custom_executable_uses_profile_provider_for_prompt_delivery() {
        let mut selected = profile("{id}", "Custom OpenCode", true);
        selected.agent_id = "opencode".into();
        let runtime = runtime(&selected).unwrap();
        let command =
            crate::coordinator::build_profile_launch_commandline(&runtime, Some("task"), None)
                .unwrap();
        assert_eq!(
            crate::coordinator::split_windows_commandline(&command),
            [
                r"C:\Agent Tools\custom-claude.exe",
                "--model",
                "configured",
                "--prompt",
                "task"
            ],
        );
    }

    #[test]
    fn profile_owned_session_options_are_not_overridden_by_a_new_binding() {
        let selected = profile("{id}", "Project", true);
        let mut runtime = runtime(&selected).unwrap();
        assert!(new_session_id(&runtime).is_some());
        for options in [
            "--session-id existing",
            "--session-id=existing",
            "--resume saved",
            "--continue",
            "-c",
            "-r saved",
            "--",
        ] {
            runtime.commandline = format!("{} {options}", selected.commandline);
            assert!(new_session_id(&runtime).is_none(), "{options}");
        }
    }

    struct RecordingChannel {
        requests: Mutex<Vec<(String, Value)>>,
        cwd: String,
        unavailable: bool,
    }

    #[async_trait::async_trait]
    impl crate::shell::wt_channel::WtChannel for RecordingChannel {
        async fn request(&self, method: &str, params: Value) -> Result<Value> {
            self.requests
                .lock()
                .unwrap()
                .push((method.into(), params.clone()));
            Ok(match method {
                "get_delegate_context" | "get_pane_context" => json!({
                    "pane": {"session_id": "source", "window_id": 42, "cwd": self.cwd},
                    "content": "source output",
                }),
                "agent_profiles" if self.unavailable => json!([]),
                "agent_profiles" => {
                    let mut entry = json!({
                        "guid": "{chosen}", "name": "Claude Project", "agent_id": "claude", "default": true,
                    });
                    if params["profile"].is_string() {
                        entry["commandline"] =
                            r#""C:\Agent Tools\custom-claude.exe" --model profile-model"#.into();
                    }
                    json!([entry])
                }
                "create_tab" | "split_pane" => json!({"session_id": "created"}),
                "publish" | "focus_pane" => json!({}),
                _ => bail!("unexpected method {method}"),
            })
        }

        fn is_available(&self) -> bool {
            true
        }
    }

    fn request(placement: Placement) -> LaunchRequest<'static> {
        LaunchRequest {
            profile: Some("{chosen}"),
            provider: None,
            source_pane: Some("owning-helper"),
            window_id: Some(7),
            cwd: None,
            task: Some("fix it"),
            include_context: true,
            placement,
            direction: "auto",
            size: 0.5,
            background: false,
            title: None,
        }
    }

    #[tokio::test]
    async fn tab_and_pane_launch_share_profile_cwd_context_and_window() {
        let mut tab_launch = None;
        for placement in [Placement::Tab, Placement::Pane] {
            let channel = Arc::new(RecordingChannel {
                requests: Mutex::new(Vec::new()),
                cwd: r"C:\Source Project".into(),
                unavailable: false,
            });
            let shell = ShellManager::new().with_wt_channel(channel.clone());
            let created = launch(&shell, request(placement)).await.unwrap();
            assert_eq!(created["session_id"], "created");
            let requests = channel.requests.lock().unwrap();
            assert_eq!(requests[0].0, "get_delegate_context");
            assert_eq!(requests[0].1["session_id"], "owning-helper");
            assert_eq!(requests[0].1["max_lines"], 30);
            assert_eq!(requests[0].1["max_chars"], 4000);
            assert_eq!(requests[1].1["window_id"], 42);
            let (method, args) = &requests[3];
            assert_eq!(
                method,
                if placement == Placement::Tab {
                    "create_tab"
                } else {
                    "split_pane"
                }
            );
            assert_eq!(args["profile"], "{chosen}");
            assert_eq!(args["cwd"], r"C:\Source Project");
            assert_eq!(args["window_id"], 42);
            if placement == Placement::Pane {
                let (tab_command, tab_session): (String, String) = tab_launch.take().unwrap();
                let expected = tab_command
                    .replace(&tab_session, created["agent_session_id"].as_str().unwrap());
                assert_eq!(args["session_id"], "source");
                assert_eq!(
                    args["commandline"],
                    crate::coordinator::with_windows_delegate_cwd(&expected, r"C:\Source Project")
                        .unwrap()
                );
                continue;
            }
            tab_launch = Some((
                args["commandline"].as_str().unwrap().to_string(),
                created["agent_session_id"].as_str().unwrap().to_string(),
            ));
            let command = crate::coordinator::split_windows_commandline(
                args["commandline"].as_str().unwrap(),
            );
            assert_eq!(command[0], r"C:\Agent Tools\custom-claude.exe");
            assert_eq!(&command[1..3], ["--model", "profile-model"]);
            assert!(command.last().unwrap().starts_with("fix it\n\n"));
            assert!(command.last().unwrap().contains("source output"));
            assert!(command.last().unwrap().contains(r"C:\Source Project"));
        }
    }

    #[tokio::test]
    async fn unavailable_profiles_and_cross_environment_cwd_do_not_create_a_target() {
        for (unavailable, cwd) in [(true, r"C:\repo"), (false, "/home/user/repo")] {
            let channel = Arc::new(RecordingChannel {
                requests: Mutex::new(Vec::new()),
                cwd: cwd.into(),
                unavailable,
            });
            let shell = ShellManager::new().with_wt_channel(channel.clone());
            assert!(launch(&shell, request(Placement::Tab)).await.is_err());
            assert!(!channel
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|(method, _)| method == "create_tab"));
        }
    }

    #[tokio::test]
    async fn explicit_profile_cannot_silently_override_the_requested_provider() {
        let channel = Arc::new(RecordingChannel {
            requests: Mutex::new(Vec::new()),
            cwd: r"C:\repo".into(),
            unavailable: false,
        });
        let shell = ShellManager::new().with_wt_channel(channel.clone());
        let mut launch_request = request(Placement::Tab);
        launch_request.provider = Some("copilot");
        assert!(launch(&shell, launch_request).await.is_err());
        assert!(!channel
            .requests
            .lock()
            .unwrap()
            .iter()
            .any(|(method, _)| method == "create_tab"));
    }
}
