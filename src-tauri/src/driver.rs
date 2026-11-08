//! Installing / updating the NPU driver from inside Hermio.
//!
//! * Intel: the latest "Intel NPU Driver – Windows" installer is downloaded from Intel's download
//!   centre (public, no login), its SHA-256 is checked against the page when published there, and
//!   the installer is started with administrator rights.
//! * AMD: AMD only offers its NPU driver behind an account login, so Hermio asks Windows Update for a
//!   newer NPU driver (elevated) and installs it. When Windows Update has none, the vendor page is
//!   opened so the user can download it.

use crate::npu;
use crate::state::AppState;
use anyhow::{anyhow, bail, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;

pub const INTEL_PAGE: &str = "https://www.intel.com/content/www/us/en/download/794734/intel-npu-driver-windows.html";
pub const AMD_PAGE: &str = "https://www.amd.com/en/support/download/drivers.html";

#[derive(Debug, Clone, Serialize, Default)]
pub struct Outcome {
    /// "intel-installer", "windows-update" or "manual".
    pub method: String,
    pub ok: bool,
    pub message: String,
    pub reboot: bool,
    /// Page to open when the driver must be downloaded by hand.
    pub url: String,
}

fn phase(state: &AppState, phase: &str, percent: Option<f64>, done: u64, total: u64) {
    state.emit("npu-progress", json!({ "kind": "driver", "tag": phase, "percent": percent, "done": done, "total": total, "line": "", "finished": false, "error": "" }));
}

fn client(secs: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(secs))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
        .build()
        .unwrap_or_default()
}

/// Find the installer link (and SHA-256 when shown) on Intel's driver page.
pub fn parse_intel_page(html: &str) -> Option<(String, String, Option<String>)> {
    let direct = regex_lite::Regex::new(r#"https://downloadmirror\.intel\.com/(\d+)/(npu_win_([\d.]+)\.exe)"#).unwrap();
    let (url, version) = if let Some(c) = direct.captures(html) {
        (c[0].to_string(), c[3].to_string())
    } else {
        // The page always links the release notes from the same folder: build the .exe link from it.
        let notes = regex_lite::Regex::new(r#"downloadmirror\.intel\.com/(\d+)/NPU_Win_Release_Notes_v([\d.]+)\.pdf"#).unwrap();
        let c = notes.captures(html)?;
        (format!("https://downloadmirror.intel.com/{}/npu_win_{}.exe", &c[1], &c[2]), c[2].to_string())
    };
    let sha = regex_lite::Regex::new(r"(?i)\b([A-F0-9]{64})\b").unwrap().captures(html).map(|c| c[1].to_uppercase());
    Some((url, version, sha))
}

/// Run a PowerShell script elevated (UAC prompt) and wait for it; returns its JSON output.
#[cfg(windows)]
async fn run_elevated(state: &AppState, script: &str) -> Result<Value> {
    let dir = state.db.data_dir.join("tmp");
    std::fs::create_dir_all(&dir)?;
    let ps1 = dir.join("hermio-npu-driver.ps1");
    let out = dir.join("hermio-npu-driver.json");
    let _ = std::fs::remove_file(&out);
    std::fs::write(&ps1, format!("param([string]$Out)\n{script}"))?;
    let launcher = format!(
        "try {{ $p = Start-Process -FilePath powershell -Verb RunAs -Wait -PassThru -WindowStyle Hidden -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File','{}','-Out','{}'); exit $p.ExitCode }} catch {{ exit 1223 }}",
        ps1.display().to_string().replace('\'', "''"),
        out.display().to_string().replace('\'', "''")
    );
    let mut c = tokio::process::Command::new("powershell");
    c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &launcher]);
    c.creation_flags(0x0800_0000);
    let st = tokio::time::timeout(Duration::from_secs(60 * 30), c.status()).await.map_err(|_| anyhow!("The driver installation took too long"))??;
    if st.code() == Some(1223) {
        bail!("Administrator permission was declined, so the driver wasn't installed.");
    }
    let txt = std::fs::read_to_string(&out).unwrap_or_default();
    serde_json::from_str(txt.trim_start_matches('\u{feff}').trim()).map_err(|_| anyhow!("The driver installer finished without a result (exit code {:?})", st.code()))
}

#[cfg(not(windows))]
async fn run_elevated(_state: &AppState, _script: &str) -> Result<Value> {
    bail!("Driver installation is only available on Windows")
}

const WU_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
try {
  $s = New-Object -ComObject Microsoft.Update.Session
  $r = $s.CreateUpdateSearcher().Search("IsInstalled=0 and Type='Driver'")
  $m = @($r.Updates | Where-Object { $_.Title -match 'NPU|IPU|AI Boost|Neural|Compute Accelerator' -or $_.DriverClass -match 'ComputeAccelerator' })
  if ($m.Count -eq 0) { @{ found = 0 } | ConvertTo-Json | Set-Content -Path $Out -Encoding UTF8; exit 0 }
  $coll = New-Object -ComObject Microsoft.Update.UpdateColl
  foreach ($u in $m) { [void]$coll.Add($u) }
  $d = $s.CreateUpdateDownloader(); $d.Updates = $coll; [void]$d.Download()
  $i = $s.CreateUpdateInstaller(); $i.Updates = $coll; $res = $i.Install()
  @{ found = $m.Count; titles = @($m | ForEach-Object { $_.Title }); result = [int]$res.ResultCode; reboot = [bool]$res.RebootRequired } | ConvertTo-Json | Set-Content -Path $Out -Encoding UTF8
} catch {
  @{ error = $_.Exception.Message } | ConvertTo-Json | Set-Content -Path $Out -Encoding UTF8
}
"#;

async fn windows_update(state: &AppState) -> Result<Outcome> {
    phase(state, "search", None, 0, 0);
    let v = run_elevated(state, WU_SCRIPT).await?;
    if let Some(e) = v["error"].as_str() {
        bail!("Windows Update: {e}");
    }
    if v["found"].as_i64().unwrap_or(0) == 0 {
        return Ok(Outcome { method: "windows-update".into(), ok: false, message: "Windows Update has no newer NPU driver for this PC.".into(), ..Default::default() });
    }
    let code = v["result"].as_i64().unwrap_or(0);
    let titles: Vec<String> = v["titles"].as_array().cloned().unwrap_or_default().iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect();
    Ok(Outcome {
        method: "windows-update".into(),
        ok: code == 2 || code == 3,
        message: titles.join(", "),
        reboot: v["reboot"].as_bool().unwrap_or(false),
        url: String::new(),
    })
}

async fn intel_installer(state: &AppState) -> Result<Outcome> {
    phase(state, "search", None, 0, 0);
    let html = client(30).get(INTEL_PAGE).send().await?.text().await?;
    let (url, version, sha) = parse_intel_page(&html).ok_or_else(|| anyhow!("Couldn't find the driver download on Intel's page"))?;
    let dir = state.db.data_dir.join("tmp");
    std::fs::create_dir_all(&dir)?;
    let file = dir.join(format!("npu_win_{version}.exe"));
    let mut resp = client(60 * 30).get(&url).header("Referer", INTEL_PAGE).send().await?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    let mut f = std::fs::File::create(&file)?;
    let mut done = 0u64;
    let mut hasher = <sha2::Sha256 as sha2::Digest>::new();
    let mut last = std::time::Instant::now();
    while let Some(chunk) = resp.chunk().await? {
        use std::io::Write;
        f.write_all(&chunk)?;
        sha2::Digest::update(&mut hasher, &chunk);
        done += chunk.len() as u64;
        if last.elapsed() > Duration::from_millis(150) {
            last = std::time::Instant::now();
            phase(state, "download", if total > 0 { Some(done as f64 * 100.0 / total as f64) } else { None }, done, total);
        }
    }
    drop(f);
    let got: String = sha2::Digest::finalize(hasher).iter().map(|b| format!("{b:02X}")).collect();
    if let Some(want) = sha {
        if want != got {
            let _ = std::fs::remove_file(&file);
            bail!("The downloaded driver failed its checksum check, so it wasn't installed.");
        }
    }
    phase(state, "install", None, 0, 0);
    let script = format!(
        "$p = Start-Process -FilePath '{}' -Wait -PassThru; @{{ code = $p.ExitCode }} | ConvertTo-Json | Set-Content -Path $Out -Encoding UTF8",
        file.display().to_string().replace('\'', "''")
    );
    let v = run_elevated(state, &script).await?;
    let code = v["code"].as_i64().unwrap_or(-1);
    let _ = std::fs::remove_file(&file);
    Ok(Outcome {
        method: "intel-installer".into(),
        ok: code == 0 || code == 3010,
        message: format!("Intel NPU driver {version}"),
        reboot: code == 3010,
        url: String::new(),
    })
}

/// Install or update the NPU driver for this PC.
pub async fn install(state: &AppState) -> Result<Outcome> {
    let info = npu::info(true).await;
    if !info.present {
        bail!("No NPU was found on this PC.");
    }
    let result = if info.vendor == "intel" {
        match intel_installer(state).await {
            Ok(o) if o.ok => Ok(o),
            // Fall back to Windows Update when Intel's page changed or the installer failed.
            Ok(_) | Err(_) => windows_update(state).await,
        }
    } else {
        windows_update(state).await
    };
    let mut out = match result {
        Ok(o) => o,
        Err(e) => {
            state.emit("npu-progress", json!({ "kind": "driver", "tag": "done", "finished": true, "error": format!("{e:#}") }));
            return Err(e);
        }
    };
    if !out.ok {
        out.method = "manual".into();
        out.url = if info.vendor == "intel" { INTEL_PAGE.into() } else { AMD_PAGE.into() };
    }
    state.emit("npu-progress", json!({ "kind": "driver", "tag": "done", "finished": true, "error": "" }));
    // Re-read the driver version.
    npu::detect().await;
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn intel_link() {
        let html = r#"<a href="https://downloadmirror.intel.com/927127/NPU_Win_Release_Notes_v32.0.100.5540.pdf">notes</a>
            SHA256: FBA32699B699918A793FCA05CFC6C607C8745772057347476ECD8C4CF7746048"#;
        let (u, v, s) = super::parse_intel_page(html).unwrap();
        assert_eq!(u, "https://downloadmirror.intel.com/927127/npu_win_32.0.100.5540.exe");
        assert_eq!(v, "32.0.100.5540");
        assert_eq!(s.unwrap(), "FBA32699B699918A793FCA05CFC6C607C8745772057347476ECD8C4CF7746048");
    }
}
