use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, instrument, warn};

use super::error::{AnalysisError, Result};

/// Run a Python script as a subprocess, sending JSON on stdin and reading JSON from stdout.
///
/// Returns the parsed JSON output on success.
#[instrument(name = "analysis.subprocess", skip(project_root, venv_path, input, timeout_seconds), fields(script = %script))]
pub async fn run_python_script(
    project_root: &Path,
    venv_path: &str,
    script: &str,
    input: &serde_json::Value,
    timeout_seconds: u64,
) -> Result<serde_json::Value> {
    let python = python_binary(project_root, venv_path);
    let script_path = project_root.join("scripts").join(script);

    debug!(script = %script, "running Python subprocess");

    let input_json = serde_json::to_vec(input)?;

    let mut child = Command::new(&python)
        .arg(&script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LD_LIBRARY_PATH", "/usr/lib")
        .current_dir(project_root)
        .spawn()
        .map_err(|e| {
            AnalysisError::Subprocess(format!("failed to spawn {}: {e}", python.display()))
        })?;

    // Write input to stdin
    let mut stdin = child.stdin.take().expect("stdin was piped");
    tokio::spawn(async move {
        if let Err(e) = stdin.write_all(&input_json).await {
            warn!("failed to write to subprocess stdin: {e}");
        }
        drop(stdin);
    });

    // Wait for completion with timeout
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_seconds),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| AnalysisError::Timeout(timeout_seconds))?
    .map_err(|e| AnalysisError::Subprocess(format!("subprocess IO error: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AnalysisError::Subprocess(format!(
            "subprocess exited with {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let result: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    Ok(result)
}

fn python_binary(project_root: &Path, venv_path: &str) -> PathBuf {
    project_root.join(venv_path).join("bin").join("python3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_binary_path() {
        let root = Path::new("/opt/kairex");
        let path = python_binary(root, ".venv");
        assert_eq!(path, PathBuf::from("/opt/kairex/.venv/bin/python3"));
    }
}
