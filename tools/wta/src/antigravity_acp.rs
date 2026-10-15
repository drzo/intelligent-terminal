// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, ensure, Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

pub(crate) const VERSION: &str = "1.4.0";
const OWNER_FILE: &str = ".intelligent-terminal-acp.json";
const MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 1200 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const INSTALL_LOCK_TIMEOUT: Duration = Duration::from_secs(240);
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    WindowsX64,
    WindowsArm64,
    LinuxX64,
    LinuxArm64,
}

impl Platform {
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::WindowsX64 => "windows-x86_64",
            Self::WindowsArm64 => "windows-aarch64",
            Self::LinuxX64 => "linux-x86_64",
            Self::LinuxArm64 => "linux-aarch64",
        }
    }

    fn archive_url(self) -> &'static str {
        match self {
            Self::WindowsX64 => "https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-1.4.0-windows-x86_64.zip",
            Self::WindowsArm64 => "https://dl.google.com/agy-extensions/releases/windows/agy-acp-server-1.4.0-windows-arm64.zip",
            Self::LinuxX64 => "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-1.4.0-linux-x86_64.zip",
            Self::LinuxArm64 => "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-1.4.0-linux-arm64.zip",
        }
    }

    fn archive_sha256(self) -> &'static str {
        match self {
            Self::WindowsX64 => "d737aad2d6388e071bc907db415c129a256915d275f436e68f10dcdf6af896da",
            Self::WindowsArm64 => {
                "37fac14835f06caf05b4b93fed59a0e16e7c716e3ec95f03d4779a61abe3c2e6"
            }
            Self::LinuxX64 => "fbb2d54dd94fb897845308d3b6b46187c627f1c60078ded81adeeaf8b90c5d7a",
            Self::LinuxArm64 => "3dc43a21f5402b23a4dba7b320a6745e9dbf4b6ead912ce9fc76011ea3a08a7e",
        }
    }

    pub(crate) fn files(self) -> [&'static str; 2] {
        match self {
            Self::WindowsX64 | Self::WindowsArm64 => {
                ["agy_acp_server.exe", "localharness_external.exe"]
            }
            Self::LinuxX64 | Self::LinuxArm64 => ["agy_acp_server.par", "localharness_external"],
        }
    }

    fn is_windows(self) -> bool {
        matches!(self, Self::WindowsX64 | Self::WindowsArm64)
    }
}

pub(crate) fn host_platform() -> Result<Platform> {
    match std::env::consts::ARCH {
        "x86_64" => Ok(Platform::WindowsX64),
        "aarch64" => Ok(Platform::WindowsArm64),
        architecture => bail!("Antigravity ACP does not provide a Windows {architecture} archive"),
    }
}

fn installation_root() -> Result<PathBuf> {
    crate::runtime_paths::intelligent_terminal_local_root()
        .map(|root| root.join("agents").join("antigravity-acp"))
        .context("the Antigravity ACP installation cache is unavailable")
}

fn platform_directory(root: &Path, platform: Platform) -> PathBuf {
    root.join(VERSION).join(platform.id())
}

#[derive(Debug, Deserialize, Serialize)]
struct InstalledFile {
    name: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Ownership {
    agent: String,
    version: String,
    platform: String,
    archive_sha256: String,
    files: Vec<InstalledFile>,
}

fn read_ownership(directory: &Path, platform: Platform) -> Result<Ownership> {
    let metadata = fs::symlink_metadata(directory.join(OWNER_FILE))
        .context("inspect ACP installation ownership")?;
    ensure!(
        metadata.is_file()
            && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
            && metadata.len() <= 8192,
        "ACP installation ownership is not a regular bounded file"
    );
    let manifest: Ownership = serde_json::from_slice(
        &fs::read(directory.join(OWNER_FILE)).context("read ACP installation ownership")?,
    )
    .context("parse ACP installation ownership")?;
    ensure!(
        manifest.agent == crate::agent_registry::ANTIGRAVITY_AGENT_ID
            && manifest.version == VERSION
            && manifest.platform == platform.id()
            && manifest.archive_sha256 == platform.archive_sha256()
            && manifest.files.len() == 2,
        "the ACP installation directory is not owned by this installer"
    );
    for name in platform.files() {
        let entry = manifest
            .files
            .iter()
            .filter(|file| file.name == name)
            .collect::<Vec<_>>();
        ensure!(
            entry.len() == 1
                && entry[0].sha256.len() == 64
                && entry[0].sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                && entry[0].bytes > 0,
            "ACP installation ownership does not describe both expected files"
        );
    }
    Ok(manifest)
}

fn managed_pair_present(directory: &Path, platform: Platform) -> bool {
    if !fs::symlink_metadata(directory).is_ok_and(|metadata| {
        metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }) {
        return false;
    }
    let Ok(ownership) = read_ownership(directory, platform) else {
        return false;
    };
    ownership.files.iter().all(|entry| {
        fs::symlink_metadata(directory.join(&entry.name)).is_ok_and(|metadata| {
            metadata.is_file()
                && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
                && metadata.len() == entry.bytes
        })
    })
}

pub(crate) fn managed_host_executable() -> Option<PathBuf> {
    let platform = host_platform().ok()?;
    let directory = platform_directory(&installation_root().ok()?, platform);
    managed_pair_present(&directory, platform).then(|| directory.join(platform.files()[0]))
}

async fn acquire_lock(root: &Path) -> Result<File> {
    tokio::fs::create_dir_all(root)
        .await
        .context("create the ACP installation cache")?;
    let path = root.join("install.lock");
    let deadline = tokio::time::Instant::now() + INSTALL_LOCK_TIMEOUT;
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(&path)
        {
            Ok(file) => return Ok(file),
            Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => {
                ensure!(
                    tokio::time::Instant::now() < deadline,
                    "another Antigravity ACP installation is still running"
                );
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            Err(error) => return Err(error).context("open the ACP installation lock"),
        }
    }
}

async fn download_archive(platform: Platform, destination: &Path) -> Result<()> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(DOWNLOAD_TIMEOUT)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 3
                && attempt.url().scheme() == "https"
                && attempt.url().host_str() == Some("dl.google.com")
            {
                attempt.follow()
            } else {
                attempt.error("ACP download left the trusted vendor origin")
            }
        }))
        .build()
        .context("create the ACP download client")?;
    let response = client
        .get(platform.archive_url())
        .send()
        .await
        .context("download the official Antigravity ACP archive")?
        .error_for_status()
        .context("the ACP archive download was rejected")?;
    ensure!(
        response
            .content_length()
            .is_none_or(|size| size <= MAX_ARCHIVE_BYTES),
        "the ACP archive exceeds the download limit"
    );
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)
        .await
        .context("create the staged ACP archive")?;
    let mut stream = response.bytes_stream();
    let mut hash = Sha256::new();
    let mut received = 0u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("read the ACP archive download")?;
        received += chunk.len() as u64;
        ensure!(
            received <= MAX_ARCHIVE_BYTES,
            "the ACP archive exceeds the download limit"
        );
        hash.update(&chunk);
        output
            .write_all(&chunk)
            .await
            .context("write the ACP archive")?;
    }
    output.sync_all().await.context("flush the ACP archive")?;
    ensure!(
        format!("{:x}", hash.finalize()) == platform.archive_sha256(),
        "the ACP archive does not match its pinned SHA-256"
    );
    Ok(())
}

fn extract_archive(archive: &Path, directory: &Path, platform: Platform) -> Result<Ownership> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?).context("open the ACP archive")?;
    ensure!(
        zip.len() == 2,
        "the ACP archive does not contain exactly two expected files"
    );
    let mut total = 0u64;
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        ensure!(
            platform.files().contains(&entry.name()) && !entry.is_dir(),
            "the ACP archive contains an unexpected or unsafe path"
        );
        total = total
            .checked_add(entry.size())
            .context("ACP archive size overflow")?;
    }
    ensure!(
        total <= MAX_EXPANDED_BYTES,
        "the expanded ACP archive exceeds its size limit"
    );
    for name in platform.files() {
        zip.by_name(name)
            .context("the ACP archive is missing a companion")?;
    }
    let mut files = Vec::with_capacity(2);
    for name in platform.files() {
        let mut entry = zip
            .by_name(name)
            .context("the ACP archive is missing a companion")?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join(name))?;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = entry
                .read(&mut buffer)
                .context("read an ACP archive member")?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            ensure!(
                bytes <= entry.size(),
                "an ACP archive member exceeded its declared size"
            );
            output.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
        }
        ensure!(bytes == entry.size(), "an ACP archive member was truncated");
        output.sync_all()?;
        files.push(InstalledFile {
            name: name.to_string(),
            bytes,
            sha256: format!("{:x}", hash.finalize()),
        });
    }
    Ok(Ownership {
        agent: crate::agent_registry::ANTIGRAVITY_AGENT_ID.to_string(),
        version: VERSION.to_string(),
        platform: platform.id().to_string(),
        archive_sha256: platform.archive_sha256().to_string(),
        files,
    })
}

async fn verify_windows_signatures(directory: &Path, platform: Platform) -> Result<()> {
    if !platform.is_windows() {
        return Ok(());
    }
    let powershell = crate::agent_check::find_exe("powershell")
        .context("Windows PowerShell is required to verify the ACP publisher")?;
    let script = r#"
$ErrorActionPreference = 'Stop'
$paths = $env:WTA_ACP_SIGNATURE_PATHS | ConvertFrom-Json
foreach ($path in $paths) {
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.Status -ne 'Valid' -or
        $signature.SignerCertificate.Subject -notmatch '(?:^|,\s*)O=Google LLC(?:,|$)') {
        throw 'The Antigravity ACP executable does not have a valid Google LLC signature.'
    }
}
"#;
    let paths = platform.files().map(|name| directory.join(name));
    let mut command = tokio::process::Command::new(powershell);
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .env("WTA_ACP_SIGNATURE_PATHS", serde_json::to_string(&paths)?)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let result = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .context("ACP publisher verification timed out")?
        .context("start ACP publisher verification")?;
    ensure!(
        result.status.success(),
        "ACP publisher verification failed: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    Ok(())
}

fn check_owned_destination(directory: &Path, platform: Platform) -> Result<()> {
    let metadata = match fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("inspect the ACP installation destination"),
    };
    ensure!(
        metadata.is_dir() && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
        "the ACP installation destination is a link, not an owned directory"
    );
    read_ownership(directory, platform)?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        ensure!(
            name == OWNER_FILE || platform.files().iter().any(|expected| name == *expected),
            "the ACP installation directory contains user-owned files"
        );
        ensure!(
            entry.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
                && entry.file_type()?.is_file(),
            "the ACP installation directory contains an unexpected file type"
        );
    }
    Ok(())
}

fn activate_stage(stage: &Path, target: &Path, platform: Platform) -> Result<()> {
    let parent = target.parent().context("ACP installation has no parent")?;
    let backup = parent.join(format!(".previous-{}", uuid::Uuid::new_v4()));
    let replaced = target.exists();
    if replaced {
        check_owned_destination(target, platform)?;
        fs::rename(target, &backup).context("preserve the owned partial ACP install")?;
    }
    if let Err(error) = fs::rename(stage, target) {
        if replaced {
            fs::rename(&backup, target)
                .context("restore the previous ACP installation after activation failure")?;
        }
        return Err(error).context("activate the verified ACP installation");
    }
    if replaced {
        check_owned_destination(&backup, platform)?;
        fs::remove_dir_all(&backup).context("remove the replaced owned ACP installation")?;
    }
    Ok(())
}

async fn install_distribution(root: &Path, platform: Platform) -> Result<PathBuf> {
    let _lock = acquire_lock(root).await?;
    let target = platform_directory(root, platform);
    if managed_pair_present(&target, platform) {
        return Ok(target);
    }
    check_owned_destination(&target, platform)?;
    let parent = target.parent().context("ACP installation has no parent")?;
    tokio::fs::create_dir_all(parent).await?;
    let stage = parent.join(format!(".stage-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir(&stage).await?;
    let result: Result<PathBuf> = async {
        tracing::info!(target: "agent_install", agent = "antigravity", platform = platform.id(), version = VERSION, "downloading verified ACP dependency");
        let archive = stage.join("download.zip");
        download_archive(platform, &archive).await?;
        let extract_dir = stage.clone();
        let ownership = tokio::task::spawn_blocking(move || {
            extract_archive(&archive, &extract_dir, platform)
        })
        .await
        .context("ACP archive extraction task failed")??;
        verify_windows_signatures(&stage, platform).await?;
        tokio::fs::remove_file(stage.join("download.zip")).await?;
        tokio::fs::write(stage.join(OWNER_FILE), serde_json::to_vec(&ownership)?).await?;
        let activation_stage = stage.clone();
        let activation_target = target.clone();
        let activation_lock = _lock;
        // Keep the preserve/activate/restore transaction running if its async caller is cancelled.
        tokio::task::spawn_blocking(move || {
            let _lock = activation_lock;
            activate_stage(&activation_stage, &activation_target, platform)
        })
        .await
        .context("ACP installation activation task failed")??;
        tracing::info!(target: "agent_install", agent = "antigravity", platform = platform.id(), version = VERSION, "ACP dependency installation completed");
        Ok(target.clone())
    }
    .await;
    if stage.exists() {
        tokio::fs::remove_dir_all(&stage)
            .await
            .context("clean the owned ACP installation staging directory")?;
    }
    result
}

pub(crate) async fn ensure_host() -> Result<PathBuf> {
    if let Some(path) =
        crate::agent_check::find_acp_exe(crate::agent_registry::ANTIGRAVITY_AGENT_ID)
    {
        return Ok(PathBuf::from(path));
    }
    ensure!(
        crate::agent_check::find_exe(crate::agent_registry::ANTIGRAVITY_AGENT_ID).is_some(),
        "the ordinary Antigravity CLI is not installed"
    );
    let path = crate::agent_check::spawn_path().unwrap_or_default();
    ensure!(
        !std::env::split_paths(&path)
            .any(|directory| directory.join("agy_acp_server.exe").is_file()),
        "an earlier Antigravity ACP PATH entry is incomplete; repair it before automatic installation"
    );
    let platform = host_platform()?;
    let directory = install_distribution(&installation_root()?, platform).await?;
    Ok(directory.join(platform.files()[0]))
}

pub(crate) fn wsl_cache_setup() -> String {
    let Ok(root) = installation_root() else {
        return "printf '%s\\n' 'The Antigravity ACP cache is unavailable.' >&2; exit 1"
            .to_string();
    };
    let namespace = format!("{:x}", Sha256::digest(root.to_string_lossy().as_bytes()));
    format!(
        "case $(uname -m) in x86_64) wta_platform=linux-x86_64; \
         wta_digest={} ;; aarch64|arm64) wta_platform=linux-aarch64; \
         wta_digest={} ;; *) wta_platform=unsupported; wta_digest= ;; esac; \
         wta_acp_root=\"${{XDG_DATA_HOME:-$HOME/.local/share}}/intelligent-terminal/acp/{}/antigravity-acp/{VERSION}/$wta_platform\"; \
         wta_acp_owner=\"IntelligentTerminal Antigravity ACP {VERSION} $wta_platform $wta_digest\"",
        Platform::LinuxX64.archive_sha256(),
        Platform::LinuxArm64.archive_sha256(),
        &namespace[..namespace.len().min(16)]
    )
}

pub(crate) fn wsl_cached_server_fallback() -> String {
    format!(
        "{}; if [ -z \"$resolved\" ] && [ \"$wta_platform\" != unsupported ] && \
         [ -f \"$wta_acp_root/{OWNER_FILE}\" ] && \
         [ \"$(cat \"$wta_acp_root/{OWNER_FILE}\")\" = \"$wta_acp_owner\" ]; then \
         resolved=\"$wta_acp_root/agy_acp_server.par\"; fi",
        wsl_cache_setup()
    )
}

pub(crate) fn wsl_server_resolution_script() -> String {
    format!(
        "resolved=$(command -v agy_acp_server.par 2>/dev/null || true); {}; \
         native=$(readlink -f -- \"$resolved\" 2>/dev/null || true); \
         companion=$(readlink -f -- \"${{native%/*}}/localharness_external\" 2>/dev/null || true); \
         case \"$native\" in /mnt/*|'') resolved= ;; *) \
         case \"$companion\" in /mnt/*|'') resolved= ;; *) \
         if [ ! -f \"$native\" ] || [ ! -x \"$native\" ] || \
         [ ! -f \"$companion\" ] || [ ! -x \"$companion\" ]; then resolved=; fi ;; esac ;; esac",
        wsl_cached_server_fallback()
    )
}

async fn wsl_command(distro: &str, script: &str) -> Result<tokio::process::Command> {
    ensure!(
        crate::agent_source::is_safe_wsl_distro_name(distro),
        "the Antigravity ACP WSL source is invalid"
    );
    let mut command = tokio::process::Command::new("wsl.exe");
    command
        .args([
            "-d",
            distro,
            "--exec",
            "bash",
            "--noprofile",
            "--norc",
            "-c",
        ])
        .arg(format!(
            "bash -lc {} 3>&1 >/dev/null",
            crate::coordinator::sh_quote(&format!("exec 1>&3 3>&-; {script}"))
        ))
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    Ok(command)
}

async fn run_wsl_script(distro: &str, script: &str) -> Result<String> {
    let mut command = wsl_command(distro, script).await?;
    command.stdin(std::process::Stdio::null());
    let result = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .context("Antigravity ACP WSL setup timed out")?
        .context("start Antigravity ACP WSL setup")?;
    ensure!(
        result.status.success(),
        "Antigravity ACP WSL setup failed: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    String::from_utf8(result.stdout).context("WSL setup returned invalid UTF-8")
}

fn parse_wsl_install_target(text: &str) -> Result<(Platform, String, String)> {
    let mut lines = text
        .lines()
        .skip_while(|line| *line != "__WTA_ACP_TARGET__");
    ensure!(
        lines.next().is_some(),
        "WSL setup did not report its installation target"
    );
    let platform = match lines.next() {
        Some("linux-x86_64") => Platform::LinuxX64,
        Some("linux-aarch64") => Platform::LinuxArm64,
        _ => bail!("Antigravity ACP does not support this WSL architecture"),
    };
    let directory = lines
        .next()
        .context("WSL setup omitted its target directory")?;
    let owner = lines
        .next()
        .context("WSL setup omitted its ownership identity")?;
    ensure!(
        directory.starts_with('/')
            && !directory.starts_with("/mnt/")
            && !directory.contains('\0')
            && lines.next() == Some("__WTA_ACP_TARGET_END__"),
        "Antigravity ACP must be installed in the selected user's native Linux filesystem"
    );
    Ok((platform, directory.to_string(), owner.to_string()))
}

async fn transfer_wsl_file(distro: &str, local: &Path, destination: &str) -> Result<()> {
    let script = format!(
        "set -eu; umask 077; cat > {}; chmod 700 {}",
        crate::coordinator::sh_quote(destination),
        crate::coordinator::sh_quote(destination)
    );
    let mut command = wsl_command(distro, &script).await?;
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .context("start the selected WSL ACP file transfer")?;
    let transfer = async {
        let mut input = tokio::fs::File::open(local).await?;
        let mut output = child
            .stdin
            .take()
            .context("WSL ACP transfer has no input pipe")?;
        tokio::io::copy(&mut input, &mut output).await?;
        output.shutdown().await?;
        drop(output);
        child
            .wait_with_output()
            .await
            .context("wait for the WSL ACP transfer")
    };
    let result = tokio::time::timeout(DOWNLOAD_TIMEOUT, transfer)
        .await
        .context("WSL ACP file transfer timed out")?
        .context("transfer the ACP file to the selected WSL user")?;
    ensure!(
        result.status.success(),
        "the selected WSL ACP transfer failed: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    Ok(())
}

pub(crate) async fn ensure_wsl(distro: &str) -> Result<String> {
    ensure!(
        crate::agent_source::is_safe_wsl_distro_name(distro),
        "the Antigravity ACP WSL source is invalid"
    );
    if let Some(path) = crate::agent_check::find_wsl_exe(distro, "agy_acp_server.par").await {
        return Ok(path);
    }
    ensure!(
        crate::agent_check::find_wsl_exe(distro, "agy")
            .await
            .is_some(),
        "the ordinary Antigravity CLI is not installed in the selected WSL source"
    );
    let script = format!(
        "set -eu; {}; if command -v agy_acp_server.par >/dev/null 2>&1; then \
         printf '%s\\n' 'An earlier Antigravity ACP PATH entry is incomplete.' >&2; exit 1; fi; \
         printf '__WTA_ACP_TARGET__\\n%s\\n%s\\n%s\\n__WTA_ACP_TARGET_END__\\n' \
         \"$wta_platform\" \"$wta_acp_root\" \"$wta_acp_owner\"",
        wsl_cache_setup()
    );
    let target = run_wsl_script(distro, &script).await?;
    let (platform, directory, owner) = parse_wsl_install_target(&target)?;
    let root = installation_root()?;
    let source_key = format!(
        "{:x}",
        Sha256::digest(format!("{}\0{directory}", distro.to_ascii_lowercase()).as_bytes())
    );
    let _source_lock = acquire_lock(&root.join(format!("wsl-{}", &source_key[..16]))).await?;
    if let Some(path) = crate::agent_check::find_wsl_exe(distro, "agy_acp_server.par").await {
        return Ok(path);
    }
    let extracted = install_distribution(&root, platform).await?;
    let ownership = read_ownership(&extracted, platform)?;
    let parent = directory
        .rsplit_once('/')
        .context("WSL ACP target has no parent")?
        .0;
    let stage_name = format!("{parent}/.stage-{}", uuid::Uuid::new_v4());
    let q = crate::coordinator::sh_quote;
    run_wsl_script(
        distro,
        &format!(
            "set -eu; umask 077; mkdir -p {}; \
             if [ -L {} ] || {{ [ -e {} ] && {{ [ ! -f {}/{} ] || [ \"$(cat {}/{})\" != {} ]; }}; }}; then \
             printf '%s\\n' 'Refusing to replace an unowned ACP directory.' >&2; exit 1; fi; \
             mkdir {}",
            q(parent), q(&directory), q(&directory), q(&directory), OWNER_FILE,
            q(&directory), OWNER_FILE, q(&owner), q(&stage_name)
        ),
    )
    .await?;
    let result: Result<String> = async {
        for name in platform.files() {
            transfer_wsl_file(distro, &extracted.join(name), &format!("{stage_name}/{name}")).await?;
        }
        let verify = ownership.files.iter().map(|file| {
            format!(
                "[ \"$(sha256sum -- {} | cut -d ' ' -f 1)\" = {} ]",
                q(&format!("{stage_name}/{}", file.name)),
                q(&file.sha256)
            )
        }).collect::<Vec<_>>().join(" && ");
        let backup = format!("{parent}/.previous-{}", uuid::Uuid::new_v4());
        run_wsl_script(
            distro,
            &format!(
                "set -eu; {verify}; printf '%s\\n' {} > {}/{}; \
                 if [ -e {} ]; then \
                 for f in {}/* {}/.[!.]* {}/..?*; do [ -e \"$f\" ] || [ -L \"$f\" ] || continue; \
                 case \"${{f##*/}}\" in agy_acp_server.par|localharness_external|{OWNER_FILE}) ;; \
                 *) printf '%s\\n' 'The ACP directory contains user-owned files.' >&2; exit 1 ;; esac; \
                 [ -f \"$f\" ] && [ ! -L \"$f\" ]; done; mv {} {}; fi; \
                 if ! mv {} {}; then if [ -d {} ]; then mv {} {}; fi; exit 1; fi; \
                 if [ -d {} ]; then rm -f -- {}/agy_acp_server.par {}/localharness_external {}/{}; rmdir {}; fi",
                q(&owner), q(&stage_name), OWNER_FILE, q(&directory),
                q(&directory), q(&directory), q(&directory), q(&directory), q(&backup),
                q(&stage_name), q(&directory), q(&backup), q(&backup), q(&directory),
                q(&backup), q(&backup), q(&backup), q(&backup), OWNER_FILE, q(&backup)
            ),
        )
        .await?;
        Ok(format!("{directory}/agy_acp_server.par"))
    }
    .await;
    let cleanup = run_wsl_script(
        distro,
        &format!(
            "if [ -d {} ]; then rm -f -- {}/agy_acp_server.par {}/localharness_external {}/{}; rmdir {}; fi",
            q(&stage_name), q(&stage_name), q(&stage_name), q(&stage_name), OWNER_FILE, q(&stage_name)
        ),
    )
    .await;
    match (result, cleanup) {
        (Ok(path), Ok(_)) => Ok(path),
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(error).context("clean the owned WSL ACP stage"),
        (Err(error), Err(cleanup)) => {
            Err(error).context(format!("WSL ACP stage cleanup also failed: {cleanup:#}"))
        }
    }
}

pub(crate) async fn prepare(
    agent_command: &str,
    agent_id: Option<&str>,
    source: &crate::agent_source::AgentSource,
) -> Result<()> {
    let id =
        agent_id.unwrap_or_else(|| crate::agent_registry::resolve_agent_id_from_cmd(agent_command));
    if crate::agent_registry::lookup_profile_by_id(id).id
        != crate::agent_registry::ANTIGRAVITY_AGENT_ID
    {
        return Ok(());
    }
    if matches!(source, crate::agent_source::AgentSource::Host) {
        ensure_host().await?;
    } else if let crate::agent_source::AgentSource::Wsl { distro } = source {
        ensure_wsl(distro).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("wta-acp-archive-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        root
    }

    fn archive(root: &Path, names: &[&str]) -> PathBuf {
        let path = root.join("fixture.zip");
        let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
        for name in names {
            writer
                .start_file(
                    *name,
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer
                .write_all(b"synthetic ACP file, never executed")
                .unwrap();
        }
        writer.finish().unwrap();
        path
    }

    #[test]
    fn archive_extracts_only_complete_expected_pair_and_commits_ownership_last() {
        let root = fixture();
        let zip = archive(&root, &Platform::WindowsX64.files());
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        let ownership = extract_archive(&zip, &stage, Platform::WindowsX64).unwrap();
        assert!(!stage.join(OWNER_FILE).exists());
        assert!(!managed_pair_present(&stage, Platform::WindowsX64));
        fs::write(
            stage.join(OWNER_FILE),
            serde_json::to_vec(&ownership).unwrap(),
        )
        .unwrap();
        assert!(managed_pair_present(&stage, Platform::WindowsX64));
        fs::remove_file(stage.join("localharness_external.exe")).unwrap();
        assert!(!managed_pair_present(&stage, Platform::WindowsX64));
        assert!(check_owned_destination(&stage, Platform::WindowsX64).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn archive_missing_companion_is_rejected_before_writing_files() {
        let root = fixture();
        let zip = archive(&root, &["agy_acp_server.exe"]);
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        assert!(extract_archive(&zip, &stage, Platform::WindowsX64).is_err());
        assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn case_colliding_archive_members_are_rejected_before_writing_files() {
        let root = fixture();
        let zip = archive(&root, &["agy_acp_server.exe", "AGY_ACP_SERVER.EXE"]);
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        assert!(extract_archive(&zip, &stage, Platform::WindowsX64).is_err());
        assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    fn owned_pair(root: &Path) -> PathBuf {
        let zip = archive(root, &Platform::WindowsX64.files());
        let directory = platform_directory(root, Platform::WindowsX64);
        fs::create_dir_all(&directory).unwrap();
        let ownership = extract_archive(&zip, &directory, Platform::WindowsX64).unwrap();
        fs::write(
            directory.join(OWNER_FILE),
            serde_json::to_vec(&ownership).unwrap(),
        )
        .unwrap();
        directory
    }

    #[tokio::test]
    async fn concurrent_repeated_install_preserves_complete_owned_pair_without_download() {
        let root = fixture();
        let directory = owned_pair(&root);
        let original = fs::read(directory.join(OWNER_FILE)).unwrap();
        let (first, second) = tokio::join!(
            install_distribution(&root, Platform::WindowsX64),
            install_distribution(&root, Platform::WindowsX64),
        );
        assert_eq!(first.unwrap(), directory);
        assert_eq!(second.unwrap(), directory);
        assert_eq!(fs::read(directory.join(OWNER_FILE)).unwrap(), original);
        assert_eq!(
            fs::read_dir(directory.parent().unwrap()).unwrap().count(),
            1
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn activation_failure_restores_previous_owned_installation() {
        let root = fixture();
        let target = owned_pair(&root);
        let original = fs::read(target.join("agy_acp_server.exe")).unwrap();
        assert!(
            activate_stage(&root.join("missing-stage"), &target, Platform::WindowsX64).is_err()
        );
        assert!(managed_pair_present(&target, Platform::WindowsX64));
        assert_eq!(
            fs::read(target.join("agy_acp_server.exe")).unwrap(),
            original
        );
        assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn authoritative_custom_and_other_provider_ids_do_not_prepare_antigravity() {
        let unselected = crate::agent_source::AgentSource::Wsl {
            distro: "not a valid selected distro!".into(),
        };
        for id in [
            "custom:antigravity",
            "custom",
            "copilot",
            "unknown-provider",
        ] {
            prepare("agy_acp_server.exe", Some(id), &unselected)
                .await
                .unwrap();
        }
    }

    #[test]
    fn archive_traversal_is_rejected_before_writing_files() {
        let root = fixture();
        let zip = archive(
            &root,
            &["../agy_acp_server.exe", "localharness_external.exe"],
        );
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        assert!(extract_archive(&zip, &stage, Platform::WindowsX64).is_err());
        assert!(!root.join("agy_acp_server.exe").exists());
        assert_eq!(fs::read_dir(&stage).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn owned_partial_install_does_not_adopt_additional_user_files() {
        let root = fixture();
        let zip = archive(&root, &Platform::WindowsX64.files());
        let stage = root.join("stage");
        fs::create_dir(&stage).unwrap();
        let ownership = extract_archive(&zip, &stage, Platform::WindowsX64).unwrap();
        fs::write(
            stage.join(OWNER_FILE),
            serde_json::to_vec(&ownership).unwrap(),
        )
        .unwrap();
        fs::write(stage.join("user-settings.txt"), b"preserve").unwrap();
        assert!(check_owned_destination(&stage, Platform::WindowsX64).is_err());
        assert_eq!(
            fs::read(stage.join("user-settings.txt")).unwrap(),
            b"preserve"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wsl_target_rejects_windows_mounts_and_unsupported_architectures() {
        assert!(parse_wsl_install_target(
            "__WTA_ACP_TARGET__\nlinux-x86_64\n/mnt/c/agent\nowner\n__WTA_ACP_TARGET_END__"
        )
        .is_err());
        assert!(parse_wsl_install_target(
            "__WTA_ACP_TARGET__\nunsupported\n/home/user/agent\nowner\n__WTA_ACP_TARGET_END__"
        )
        .is_err());
        let (_, path, _) = parse_wsl_install_target("__WTA_ACP_TARGET__\nlinux-x86_64\n/home/user/agent path\nowner\n__WTA_ACP_TARGET_END__").unwrap();
        assert_eq!(path, "/home/user/agent path");
    }

    #[test]
    fn distributions_pin_both_source_and_architecture() {
        for platform in [
            Platform::WindowsX64,
            Platform::WindowsArm64,
            Platform::LinuxX64,
            Platform::LinuxArm64,
        ] {
            assert_eq!(platform.archive_sha256().len(), 64);
            assert!(platform
                .archive_url()
                .starts_with("https://dl.google.com/agy-extensions/"));
            assert!(platform.archive_url().contains(VERSION));
            assert_eq!(platform.files().len(), 2);
        }
        assert_ne!(
            Platform::WindowsX64.archive_sha256(),
            Platform::WindowsArm64.archive_sha256()
        );
        assert_ne!(Platform::WindowsX64.files(), Platform::LinuxX64.files());
    }

    #[test]
    fn unowned_destination_is_not_replaced() {
        let root = std::env::temp_dir().join(format!("wta-acp-owner-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let file = root.join("agy_acp_server.exe");
        fs::write(&file, b"user-owned").unwrap();
        assert!(check_owned_destination(&root, Platform::WindowsX64).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"user-owned");
        fs::remove_file(file).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
