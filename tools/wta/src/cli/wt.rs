use anyhow::{bail, Context, Result};
use serde_json::json;

use super::args::Command;
use crate::shell::wt_channel::{CliChannel, WtChannel};

pub(crate) async fn run(command: Command, json_mode: bool) -> Result<()> {
    match command {
        Command::Info => run_info_mode().await,
        Command::TestPipe => run_test_pipe().await,
        Command::ListWindows => {
            let result = wt_call("list_windows", json!({})).await?;
            print_output(&result, json_mode, format_windows_human);
            Ok(())
        }
        Command::ListTabs { window_id } => {
            let channel = connect_channel().await?;
            let wid = match window_id {
                Some(id) => id,
                None => get_first_window_id(&channel).await?,
            };
            let result = channel
                .request("list_tabs", json!({ "window_id": wid }))
                .await?;
            print_output(&result, json_mode, format_tabs_human);
            Ok(())
        }
        Command::ListPanes { tab_id, window_id } => {
            let channel = connect_channel().await?;
            let result = list_panes(&channel, tab_id, window_id).await?;
            print_output(&result, json_mode, format_panes_human);
            Ok(())
        }
        Command::NewTab {
            command,
            cwd,
            title,
        } => {
            let mut params = json!({});
            if let Some(c) = command {
                params["command"] = json!(c);
            }
            if let Some(d) = cwd {
                params["cwd"] = json!(d);
            }
            if let Some(t) = title {
                params["title"] = json!(t);
            }
            let result = wt_call("create_tab", params).await?;
            print_output(&result, json_mode, format_created_tab);
            Ok(())
        }
        Command::SplitPane {
            target,
            horizontal,
            vertical,
            size,
            command,
        } => {
            let channel = connect_channel().await?;
            let pane_id = resolve_pane_id(&channel, &target).await?;
            let split_dir = if horizontal {
                "horizontal"
            } else if vertical {
                "vertical"
            } else {
                "automatic"
            };
            let mut params = json!({
                "session_id": pane_id,
                "direction": split_dir,
            });
            if let Some(s) = size {
                params["size"] = json!(s);
            }
            if let Some(c) = command {
                params["command"] = json!(c);
            }
            let result = channel.request("split_pane", params).await?;
            print_output(&result, json_mode, format_created_pane);
            Ok(())
        }
        Command::CapturePane {
            target,
            max_lines,
            last_prompt,
        } => {
            let channel = connect_channel().await?;
            let pane_id = resolve_pane_id(&channel, &target).await?;
            let mut params = json!({ "session_id": pane_id });
            if let Some(n) = max_lines {
                params["max_lines"] = json!(n);
            }
            if last_prompt {
                params["source"] = json!("last_prompt");
            }
            let result = channel.request("read_pane_output", params).await?;
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else if let Some(output) = result.get("content").and_then(|v| v.as_str()) {
                print!("{}", output);
            }
            Ok(())
        }
        Command::KillPane { target } => {
            let channel = connect_channel().await?;
            let pane_id = resolve_pane_id(&channel, &target).await?;
            channel
                .request("close_pane", json!({ "session_id": pane_id }))
                .await?;
            if !json_mode {
                println!("{}", t!("output.pane_closed", pane_id = pane_id));
            }
            Ok(())
        }
        Command::ActivePane => {
            let result = wt_call("get_active_pane", json!({})).await?;
            print_output(&result, json_mode, format_active_pane);
            Ok(())
        }
        Command::PaneStatus { target } => {
            let channel = connect_channel().await?;
            let pane_id = resolve_pane_id(&channel, &target).await?;
            let result = channel
                .request("get_process_status", json!({ "session_id": pane_id }))
                .await?;
            print_output(&result, json_mode, format_pane_status);
            Ok(())
        }
        Command::WaitFor {
            target,
            interval,
            timeout,
        } => run_wait_for(&target, interval, timeout, json_mode).await,
        Command::PipeId => run_pipe_id(json_mode),
        Command::SetEnv { shell } => run_set_env(&shell),
        Command::Listen { target } => run_listen(target.as_deref()).await,
        _ => bail!("unsupported command for WT CLI handler"),
    }
}

pub(crate) async fn run_test_pipe() -> Result<()> {
    println!("Connecting to Windows Terminal protocol...");
    let channel = connect_channel().await?;
    println!("Connected and authenticated!\n");

    let result: serde_json::Value = channel.request("list_windows", json!({})).await?;
    println!("list_windows:");
    println!("{}\n", serde_json::to_string_pretty(&result)?);

    let result: serde_json::Value = channel.request("get_capabilities", json!({})).await?;
    println!("get_capabilities:");
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}

/// Show Windows Terminal protocol connection info and pane identity.
pub(crate) async fn run_info_mode() -> Result<()> {
    println!("Windows Terminal Protocol Info");
    println!("========================================");

    let clsid = match std::env::var("WT_COM_CLSID") {
        Ok(v) => v,
        Err(_) => {
            println!("  Status: Not running inside Windows Terminal");
            println!("  (WT_COM_CLSID not set)");
            return Ok(());
        }
    };

    println!("  COM CLSID: {}", clsid);
    println!("  Source: WT_COM_CLSID env var");
    println!();

    let channel = match CliChannel::connect().await {
        Ok(ch) => ch,
        Err(e) => {
            println!("  Connection failed: {}", e);
            return Ok(());
        }
    };

    let our_pid = std::process::id();
    let mut pane_info: Option<(String, String, String)> = None;
    let mut total_windows = 0u32;
    let mut total_tabs = 0u32;
    let mut total_panes = 0u32;

    if let Ok(windows) = channel.request("list_windows", json!({})).await {
        if let Some(windows_arr) = windows.get("windows").and_then(|v| v.as_array()) {
            total_windows = windows_arr.len() as u32;

            for win in windows_arr {
                let window_id = match win.get("window_id").and_then(|v| v.as_str()) {
                    Some(id) => id,
                    None => continue,
                };

                if let Ok(tabs) = channel
                    .request("list_tabs", json!({ "window_id": window_id }))
                    .await
                {
                    if let Some(tabs_arr) = tabs.get("tabs").and_then(|v| v.as_array()) {
                        total_tabs += tabs_arr.len() as u32;

                        for tab in tabs_arr {
                            let tab_id_str = match tab.get("tab_id") {
                                Some(serde_json::Value::String(s)) => s.clone(),
                                Some(serde_json::Value::Number(n)) => n.to_string(),
                                _ => continue,
                            };

                            if let Ok(panes) = channel
                                .request("list_panes", json!({ "tab_id": tab_id_str }))
                                .await
                            {
                                if let Some(panes_arr) =
                                    panes.get("panes").and_then(|v| v.as_array())
                                {
                                    total_panes += panes_arr.len() as u32;

                                    for pane in panes_arr {
                                        if let Some(pid) = pane.get("pid").and_then(|v| v.as_u64())
                                        {
                                            if pid == our_pid as u64 {
                                                let pane_id = match pane.get("session_id") {
                                                    Some(serde_json::Value::String(s)) => s.clone(),
                                                    Some(serde_json::Value::Number(n)) => {
                                                        n.to_string()
                                                    }
                                                    _ => "?".to_string(),
                                                };
                                                pane_info = Some((
                                                    pane_id,
                                                    tab_id_str.clone(),
                                                    window_id.to_string(),
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some((pane_id, tab_id, window_id)) = pane_info {
        println!("Current Pane (PID {}):", our_pid);
        println!("  Window ID: {}", window_id);
        println!("  Tab ID:    {}", tab_id);
        println!("  Pane ID:   {}", pane_id);
    } else {
        println!("Current Pane (PID {}): not found in WT pane list", our_pid);
    }

    println!();
    println!("Summary:");
    println!(
        "  Windows: {}, Tabs: {}, Panes: {}",
        total_windows, total_tabs, total_panes
    );

    Ok(())
}

async fn connect_channel() -> Result<CliChannel> {
    CliChannel::connect().await
}

async fn wt_call(method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let channel = connect_channel().await?;
    channel.request(method, params).await
}

async fn resolve_pane_id(channel: &CliChannel, target: &Option<String>) -> Result<String> {
    match target {
        Some(id) => Ok(id.clone()),
        None => {
            let result = channel.request("get_active_pane", json!({})).await?;
            result
                .get("session_id")
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .ok_or_else(|| anyhow::anyhow!("{}", t!("error.no_active_pane")))
        }
    }
}

async fn list_panes(
    channel: &dyn WtChannel,
    tab_id: Option<String>,
    window_id: Option<String>,
) -> Result<serde_json::Value> {
    let (tid, wid) = match tab_id {
        Some(id) => (id, window_id),
        None => {
            let wid = match window_id {
                Some(id) => id,
                None => get_first_window_id(channel).await?,
            };
            let tid = get_first_tab_id(channel, &wid).await?;
            (tid, Some(wid))
        }
    };
    let mut params = json!({ "tab_id": tid });
    // An explicit tab without a window retains the protocol's cross-window query.
    if let Some(wid) = wid {
        params["window_id"] = json!(wid);
    }
    channel.request("list_panes", params).await
}

async fn get_first_window_id(channel: &dyn WtChannel) -> Result<String> {
    let result = channel.request("list_windows", json!({})).await?;
    first_window_id(&result).ok_or_else(|| anyhow::anyhow!("{}", t!("output.no_windows_in_list")))
}

fn first_window_id(result: &serde_json::Value) -> Option<String> {
    result
        .get("windows")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|w| w.get("window_id"))
        .and_then(|id| {
            id.as_str()
                .map(str::to_string)
                .or_else(|| id.as_u64().map(|value| value.to_string()))
        })
}

#[cfg(test)]
mod window_id_tests {
    use super::*;

    #[test]
    fn accepts_numeric_window_ids_from_wtcli() {
        assert_eq!(
            first_window_id(&json!({"windows": [{"window_id": 1}]})),
            Some("1".to_owned())
        );
        assert_eq!(
            first_window_id(&json!({"windows": [{"window_id": u64::MAX}]})),
            Some(u64::MAX.to_string())
        );
    }

    #[test]
    fn preserves_string_window_ids_and_first_window_selection() {
        assert_eq!(
            first_window_id(&json!({"windows": [{"window_id": "7"}, {"window_id": "9"}]})),
            Some("7".to_owned())
        );
    }

    #[test]
    fn rejects_missing_or_invalid_window_ids() {
        for result in [
            json!({}),
            json!({"windows": []}),
            json!({"windows": [{}]}),
            json!({"windows": [{"window_id": null}, {"window_id": "2"}]}),
            json!({"windows": [{"window_id": true}]}),
            json!({"windows": [{"window_id": -1}]}),
            json!({"windows": [{"window_id": 1.5}]}),
        ] {
            assert_eq!(first_window_id(&result), None, "{result}");
        }
    }
}

async fn get_first_tab_id(channel: &dyn WtChannel, window_id: &str) -> Result<String> {
    let result = channel
        .request("list_tabs", json!({ "window_id": window_id }))
        .await?;
    result
        .get("tabs")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|tab| match tab.get("tab_id") {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            Some(serde_json::Value::Number(n)) => Some(n.to_string()),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("{}", t!("output.no_tabs_in_window", window_id = window_id)))
}

#[cfg(test)]
mod list_panes_tests {
    use super::*;
    use std::sync::Mutex;

    struct MultiWindowChannel {
        requests: Mutex<Vec<(String, serde_json::Value)>>,
        windows: serde_json::Value,
        tabs: serde_json::Value,
        fail_method: Option<&'static str>,
    }

    impl MultiWindowChannel {
        fn new() -> Self {
            Self {
                requests: Mutex::new(Vec::new()),
                windows: json!({"windows": [
                    {"window_id": 2, "is_focused": false},
                    {"window_id": 1, "is_focused": true}
                ]}),
                tabs: json!({"tabs": [
                    {"tab_id": 0, "is_active": false},
                    {"tab_id": 1, "is_active": true}
                ]}),
                fail_method: None,
            }
        }
    }

    #[async_trait::async_trait]
    impl WtChannel for MultiWindowChannel {
        async fn request(
            &self,
            method: &str,
            params: serde_json::Value,
        ) -> Result<serde_json::Value> {
            self.requests
                .lock()
                .unwrap()
                .push((method.to_owned(), params.clone()));
            if self.fail_method == Some(method) {
                bail!("failed {method}");
            }
            match method {
                "list_windows" => Ok(self.windows.clone()),
                "list_tabs" => Ok(self.tabs.clone()),
                "list_panes" => {
                    let panes = [
                        json!({"window_id": 2, "tab_id": 0, "session_id": "second-first"}),
                        json!({"window_id": 1, "tab_id": 0, "session_id": "first-first"}),
                        json!({"window_id": 2, "tab_id": 1, "session_id": "second-active"}),
                        json!({"window_id": 1, "tab_id": 1, "session_id": "first-active"}),
                    ];
                    let panes: Vec<_> = panes
                        .into_iter()
                        .filter(|pane| {
                            params.get("window_id").is_none_or(|window_id| {
                                window_id.as_str() == Some(pane["window_id"].to_string().as_str())
                            }) && params["tab_id"].as_str()
                                == Some(pane["tab_id"].to_string().as_str())
                        })
                        .collect();
                    Ok(json!({"panes": panes}))
                }
                _ => bail!("unexpected method {method}"),
            }
        }

        fn is_available(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn default_window_keeps_first_listed_window_and_tab_scope() {
        let channel = MultiWindowChannel::new();
        let result = list_panes(&channel, None, None).await.unwrap();
        assert_eq!(
            result["panes"],
            json!([
                {"window_id": 2, "tab_id": 0, "session_id": "second-first"}
            ])
        );
        assert_eq!(
            *channel.requests.lock().unwrap(),
            vec![
                ("list_windows".into(), json!({})),
                ("list_tabs".into(), json!({"window_id": "2"})),
                (
                    "list_panes".into(),
                    json!({"window_id": "2", "tab_id": "0"})
                ),
            ]
        );
    }

    #[tokio::test]
    async fn explicit_window_scopes_first_tab_without_window_discovery() {
        let channel = MultiWindowChannel::new();
        let result = list_panes(&channel, None, Some("1".into())).await.unwrap();
        assert_eq!(
            result["panes"],
            json!([
                {"window_id": 1, "tab_id": 0, "session_id": "first-first"}
            ])
        );
        assert_eq!(
            *channel.requests.lock().unwrap(),
            vec![
                ("list_tabs".into(), json!({"window_id": "1"})),
                (
                    "list_panes".into(),
                    json!({"window_id": "1", "tab_id": "0"})
                ),
            ]
        );
    }

    #[tokio::test]
    async fn explicit_window_and_tab_bypass_discovery() {
        let channel = MultiWindowChannel::new();
        let result = list_panes(&channel, Some("1".into()), Some("1".into()))
            .await
            .unwrap();
        assert_eq!(
            result["panes"],
            json!([
                {"window_id": 1, "tab_id": 1, "session_id": "first-active"}
            ])
        );
        assert_eq!(
            *channel.requests.lock().unwrap(),
            vec![(
                "list_panes".into(),
                json!({"window_id": "1", "tab_id": "1"})
            )]
        );
    }

    #[tokio::test]
    async fn explicit_tab_without_window_preserves_cross_window_query() {
        let channel = MultiWindowChannel::new();
        let result = list_panes(&channel, Some("0".into()), None).await.unwrap();
        assert_eq!(
            result["panes"],
            json!([
                {"window_id": 2, "tab_id": 0, "session_id": "second-first"},
                {"window_id": 1, "tab_id": 0, "session_id": "first-first"}
            ])
        );
        assert_eq!(
            *channel.requests.lock().unwrap(),
            vec![("list_panes".into(), json!({"tab_id": "0"}))]
        );
    }

    #[tokio::test]
    async fn empty_discovery_stops_before_listing_panes() {
        for no_windows in [true, false] {
            let mut channel = MultiWindowChannel::new();
            if no_windows {
                channel.windows = json!({"windows": []});
            } else {
                channel.tabs = json!({"tabs": []});
            }
            assert!(list_panes(&channel, None, None).await.is_err());
            let requests = channel.requests.lock().unwrap();
            assert_eq!(requests.len(), if no_windows { 1 } else { 2 });
            assert!(requests.iter().all(|(method, _)| method != "list_panes"));
        }
    }

    #[tokio::test]
    async fn request_errors_propagate_without_fallback() {
        for (index, method) in ["list_windows", "list_tabs", "list_panes"]
            .iter()
            .enumerate()
        {
            let mut channel = MultiWindowChannel::new();
            channel.fail_method = Some(method);
            let error = list_panes(&channel, None, None).await.unwrap_err();
            assert_eq!(error.to_string(), format!("failed {method}"));
            assert_eq!(channel.requests.lock().unwrap().len(), index + 1);
        }
    }
}

fn print_output(val: &serde_json::Value, json_mode: bool, formatter: fn(&serde_json::Value)) {
    if json_mode {
        println!(
            "{}",
            serde_json::to_string_pretty(val).unwrap_or_else(|_| val.to_string())
        );
    } else {
        formatter(val);
    }
}

fn format_windows_human(val: &serde_json::Value) {
    if let Some(windows) = val.get("windows").and_then(|v| v.as_array()) {
        if windows.is_empty() {
            println!("{}", t!("output.no_windows"));
            return;
        }
        println!("{}", t!("output.header.windows"));
        for w in windows {
            let id = json_str_or_num(w, "window_id");
            let title = w.get("title").and_then(|v| v.as_str()).unwrap_or("-");
            let focused = w
                .get("is_focused")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            println!(
                "{:<12} {:<30} {}",
                id,
                title,
                if focused { "*" } else { "" }
            );
        }
    } else {
        println!("{}", serde_json::to_string_pretty(val).unwrap_or_default());
    }
}

fn format_tabs_human(val: &serde_json::Value) {
    if let Some(tabs) = val.get("tabs").and_then(|v| v.as_array()) {
        if tabs.is_empty() {
            println!("{}", t!("output.no_tabs"));
            return;
        }
        println!("{}", t!("output.header.tabs"));
        for tab in tabs {
            let id = json_str_or_num(tab, "tab_id");
            let title = tab.get("title").and_then(|v| v.as_str()).unwrap_or("-");
            let focused = tab
                .get("is_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            println!(
                "{:<10} {:<30} {}",
                id,
                title,
                if focused { "*" } else { "" }
            );
        }
    } else {
        println!("{}", serde_json::to_string_pretty(val).unwrap_or_default());
    }
}

fn format_panes_human(val: &serde_json::Value) {
    if let Some(panes) = val.get("panes").and_then(|v| v.as_array()) {
        if panes.is_empty() {
            println!("{}", t!("output.no_panes"));
            return;
        }
        println!("{}", t!("output.header.panes"));
        for pane in panes {
            let id = json_str_or_num(pane, "session_id");
            let pid = pane
                .get("pid")
                .and_then(|v| v.as_u64())
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string());
            let active = pane
                .get("is_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let size = pane.get("size");
            let rows = size
                .and_then(|s| s.get("rows"))
                .and_then(|v| v.as_u64())
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string());
            let cols = size
                .and_then(|s| s.get("columns"))
                .and_then(|v| v.as_u64())
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".to_string());
            println!(
                "{:<10} {:<8} {:<8} {:<10} {}",
                id,
                pid,
                if active { "*" } else { "" },
                rows,
                cols
            );
        }
    } else {
        println!("{}", serde_json::to_string_pretty(val).unwrap_or_default());
    }
}

fn format_active_pane(val: &serde_json::Value) {
    let id = json_str_or_num(val, "session_id");
    let tab = json_str_or_num(val, "tab_id");
    let win = json_str_or_num(val, "window_id");
    println!(
        "{}",
        t!("output.active_pane", pane = id, tab = tab, window = win)
    );
}

fn format_pane_status(val: &serde_json::Value) {
    let state = val
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let running = state == "running";
    let exit_code = val
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "-".to_string());
    let pid = val
        .get("pid")
        .and_then(|v| v.as_u64())
        .map(|n| n.to_string())
        .unwrap_or_else(|| "-".to_string());
    if running {
        println!("{}", t!("output.pane_running", pid = pid));
    } else {
        println!("{}", t!("output.pane_exited", code = exit_code, pid = pid));
    }
}

fn format_created_tab(val: &serde_json::Value) {
    let tab_id = json_str_or_num(val, "tab_id");
    let pane_id = json_str_or_num(val, "session_id");
    println!(
        "{}",
        t!("output.created_tab", tab_id = tab_id, pane_id = pane_id)
    );
}

fn format_created_pane(val: &serde_json::Value) {
    let pane_id = json_str_or_num(val, "session_id");
    println!("{}", t!("output.created_pane", pane_id = pane_id));
}

pub(crate) fn json_str_or_num(val: &serde_json::Value, key: &str) -> String {
    match val.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => "-".to_string(),
    }
}

async fn run_wait_for(target: &str, interval: u64, timeout: u64, json_mode: bool) -> Result<()> {
    let wtcli = crate::shell::wt_channel::resolve_wtcli_path();
    let interval_str = interval.to_string();
    let timeout_str = timeout.to_string();
    let output = tokio::process::Command::new(&wtcli)
        .args([
            "--json",
            "wait-for",
            "-t",
            target,
            "--interval",
            &interval_str,
            "--timeout",
            &timeout_str,
        ])
        .output()
        .await
        .with_context(|| t!("error.wtcli_wait_for_spawn").into_owned())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "{}",
            t!("error.wtcli_wait_for_failed", stderr = stderr.trim())
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        let val: serde_json::Value = serde_json::from_str(trimmed)
            .with_context(|| t!("error.wtcli_wait_for_parse").into_owned())?;
        print_output(&val, json_mode, format_pane_status);
    }
    Ok(())
}

fn run_pipe_id(json_mode: bool) -> Result<()> {
    let clsid = std::env::var("WT_COM_CLSID")
        .map_err(|_| anyhow::anyhow!("{}", t!("error.wt_com_clsid_not_set")))?;
    if json_mode {
        let val = json!({ "connection_id": clsid, "env": "WT_COM_CLSID" });
        println!("{}", serde_json::to_string_pretty(&val)?);
    } else {
        println!("{}", clsid);
    }
    Ok(())
}

fn run_set_env(shell_type: &str) -> Result<()> {
    let clsid = std::env::var("WT_COM_CLSID")
        .map_err(|_| anyhow::anyhow!("{}", t!("error.wt_com_clsid_not_set")))?;

    match shell_type {
        "bash" | "sh" | "zsh" => {
            println!("export WT_COM_CLSID='{}'", clsid);
            eprintln!("# Run: eval \"$(wta set-env)\"");
        }
        "powershell" | "pwsh" | "ps" => {
            println!("$env:WT_COM_CLSID = '{}'", clsid);
            eprintln!("# Run: wta set-env -s powershell | Invoke-Expression");
        }
        "cmd" => {
            println!("set WT_COM_CLSID={}", clsid);
            eprintln!("REM Run in a for /f loop or copy-paste");
        }
        "fish" => {
            println!("set -gx WT_COM_CLSID '{}'", clsid);
            eprintln!("# Run: wta set-env -s fish | source");
        }
        other => bail!("{}", t!("error.unknown_shell_type", shell = other)),
    }

    Ok(())
}

async fn run_listen(pane_filter: Option<&str>) -> Result<()> {
    let channel = connect_channel().await?;
    let arc_channel = std::sync::Arc::new(channel);

    let mut event_rx = arc_channel.subscribe_events();
    arc_channel.start_reader().await;

    let _ = arc_channel.request("get_capabilities", json!({})).await;

    eprintln!("Connected. Listening for events... (Ctrl+C to stop)");
    if let Some(pane) = pane_filter {
        eprintln!("Filtering: pane_id={}", pane);
    }

    while let Some(msg) = event_rx.recv().await {
        if msg.get("type").and_then(|v| v.as_str()) != Some("event") {
            continue;
        }

        if let Some(filter) = pane_filter {
            let pane_id = msg
                .get("params")
                .and_then(|p| p.get("session_id"))
                .and_then(|v| v.as_str());
            if pane_id != Some(filter) {
                continue;
            }
        }

        println!("{}", serde_json::to_string(&msg).unwrap_or_default());
    }

    eprintln!("Event stream closed.");
    Ok(())
}
