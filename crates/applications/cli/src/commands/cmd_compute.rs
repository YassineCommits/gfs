use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use gfs_domain::model::config::GfsConfig;
use gfs_domain::ports::compute::{Compute, InstanceId, InstanceState, InstanceStatus, LogsOptions};
use serde_json::json;

use crate::ComputeAction;
use crate::cli_utils::{get_repo_dir, relativize_to_repo};
use crate::output::{
    bold, box_bottom, box_row, box_top, dimmed, fmt_box_row, fmt_box_row_colored, green, red,
    yellow,
};

use super::compute_support::compute_for_repo;

// ---------------------------------------------------------------------------
// Entry point called from main
// ---------------------------------------------------------------------------

/// Resolve the compute instance id: from action's --id if set, otherwise from
/// repo config at path (or cwd) as runtime.container_name.
fn resolve_id(path: Option<PathBuf>, action: &ComputeAction) -> Result<String> {
    let id_from_action = match action {
        ComputeAction::Status { id } => id.as_deref(),
        ComputeAction::Start { id } => id.as_deref(),
        ComputeAction::Stop { id } => id.as_deref(),
        ComputeAction::Restart { id } => id.as_deref(),
        ComputeAction::Pause { id } => id.as_deref(),
        ComputeAction::Unpause { id } => id.as_deref(),
        ComputeAction::Logs { id, .. } => id.as_deref(),
        ComputeAction::Config { .. } => return Ok(String::new()),
    };
    if let Some(id) = id_from_action {
        return Ok(id.to_string());
    }
    let repo_path = path.unwrap_or_else(get_repo_dir);
    let config = GfsConfig::load(&repo_path)
        .context("not a gfs repository (use --path <repo> or run from a repo)")?;
    let container_name = config
        .runtime
        .as_ref()
        .map(|r| r.container_name.as_str())
        .filter(|s| !s.is_empty())
        .context("no container_name in repo config (set runtime.container_name or pass --id)")?;
    Ok(container_name.to_string())
}

pub async fn run(path: Option<PathBuf>, action: ComputeAction, json_output: bool) -> Result<()> {
    if let ComputeAction::Config { ref key, ref value } = action {
        return handle_config(path, key, value, json_output);
    }
    let repo_path = path.clone().unwrap_or_else(get_repo_dir);
    let repository: Arc<dyn gfs_domain::ports::repository::Repository> =
        Arc::new(gfs_domain::adapters::gfs_repository::GfsRepository::new());
    let compute = compute_for_repo(&repository, &repo_path).await?;
    let id = resolve_id(Some(repo_path), &action)?;
    dispatch_dyn(compute.as_ref(), &id, action, path, json_output).await
}

fn handle_config(path: Option<PathBuf>, key: &str, value: &str, json_output: bool) -> Result<()> {
    match key {
        "db.port" => {
            let port: u16 = value
                .parse()
                .map_err(|_| anyhow::anyhow!("'{}' is not a valid port number (1-65535)", value))?;
            let repo_path = path.unwrap_or_else(get_repo_dir);
            let mut config = GfsConfig::load(&repo_path)
                .context("not a gfs repository (use --path <repo> or run from a repo)")?;
            if let Some(ref mut env) = config.environment {
                env.database_port = Some(port);
            } else {
                anyhow::bail!(
                    "no environment config found; run 'gfs init --database-provider ...' first"
                );
            }
            config.save(&repo_path).context("failed to save config")?;
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "action": "config_set",
                        "key": "db.port",
                        "value": port,
                    }))?
                );
                return Ok(());
            }
            println!(
                "{} database_port updated to {}. Run 'gfs compute restart' to apply.",
                green("✓"),
                port
            );
            Ok(())
        }
        _ => anyhow::bail!("unknown config key '{}'; supported keys: db.port", key),
    }
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

async fn dispatch_dyn(
    compute: &dyn Compute,
    id: &str,
    action: ComputeAction,
    path: Option<PathBuf>,
    json_output: bool,
) -> Result<()> {
    let instance_id = InstanceId(id.to_string());

    match action {
        ComputeAction::Status { .. } => {
            let status = compute
                .status(&instance_id)
                .await
                .map_err(anyhow::Error::from)?;
            // Only Docker compute can report bind-mount host paths; k8s returns None.
            let data_dir: Option<&str> = None;
            if json_output {
                print_status_json(&status, data_dir, path.as_ref(), None)?;
            } else {
                print_status(&status, data_dir, path.as_ref());
            }
        }

        ComputeAction::Start { .. } => {
            let _repo_path = path.clone().unwrap_or_else(get_repo_dir);
            let status = compute.start(&instance_id, Default::default()).await?;
            let data_dir: Option<&str> = None;
            if json_output {
                print_status_json(&status, data_dir, path.as_ref(), Some("start"))?;
            } else {
                println!("{} Compute started", green("✓"));
                print_status(&status, data_dir, path.as_ref());
            }
        }

        ComputeAction::Stop { .. } => {
            let status = compute
                .stop(&instance_id)
                .await
                .map_err(anyhow::Error::from)?;
            if json_output {
                print_status_json(&status, None, path.as_ref(), Some("stop"))?;
            } else {
                println!("{} Compute stopped", green("✓"));
                print_status(&status, None, path.as_ref());
            }
        }

        ComputeAction::Restart { .. } => {
            let status = compute.restart(&instance_id).await?;
            let data_dir: Option<&str> = None;
            if json_output {
                print_status_json(&status, data_dir, path.as_ref(), Some("restart"))?;
            } else {
                println!("{} Compute restarted", green("✓"));
                print_status(&status, data_dir, path.as_ref());
            }
        }

        ComputeAction::Pause { .. } => {
            let status = compute
                .pause(&instance_id)
                .await
                .map_err(anyhow::Error::from)?;
            if json_output {
                print_status_json(&status, None, path.as_ref(), Some("pause"))?;
            } else {
                println!("{} Compute paused", green("✓"));
                print_status(&status, None, path.as_ref());
            }
        }

        ComputeAction::Unpause { .. } => {
            let status = compute
                .unpause(&instance_id)
                .await
                .map_err(anyhow::Error::from)?;
            if json_output {
                print_status_json(&status, None, path.as_ref(), Some("unpause"))?;
            } else {
                println!("{} Compute unpaused", green("✓"));
                print_status(&status, None, path.as_ref());
            }
        }

        ComputeAction::Config { .. } => unreachable!("Config is handled before dispatch"),

        ComputeAction::Logs {
            tail,
            since,
            stdout,
            stderr,
            ..
        } => {
            let since_dt = since
                .as_deref()
                .map(|s| {
                    chrono::DateTime::parse_from_rfc3339(s)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|e| anyhow::anyhow!("invalid --since timestamp: {e}"))
                })
                .transpose()?;

            let options = LogsOptions {
                tail,
                since: since_dt,
                stdout: stdout.unwrap_or(true),
                stderr: stderr.unwrap_or(true),
            };

            let entries = compute
                .logs(&instance_id, options)
                .await
                .map_err(anyhow::Error::from)?;

            if json_output {
                let out: Vec<_> = entries
                    .iter()
                    .map(|e| {
                        json!({
                            "timestamp": e.timestamp.to_rfc3339(),
                            "stream": match e.stream {
                                gfs_domain::ports::compute::LogStream::Stdout => "stdout",
                                gfs_domain::ports::compute::LogStream::Stderr => "stderr",
                            },
                            "message": e.message,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "action": "logs",
                        "id": instance_id.0,
                        "entries": out,
                    }))?
                );
                return Ok(());
            }

            for entry in &entries {
                println!(
                    "[{}] [{}] {}",
                    entry.timestamp.format("%Y-%m-%dT%H:%M:%SZ"),
                    match entry.stream {
                        gfs_domain::ports::compute::LogStream::Stdout => "stdout",
                        gfs_domain::ports::compute::LogStream::Stderr => "stderr",
                    },
                    entry.message.trim_end()
                );
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Display helper — boxed status
// ---------------------------------------------------------------------------

const BOX_W: usize = 40;
const LABEL_W: usize = 20;

fn print_status_json(
    s: &InstanceStatus,
    data_dir: Option<&str>,
    path: Option<&PathBuf>,
    action: Option<&str>,
) -> Result<()> {
    let repo_path = path.cloned().unwrap_or_else(get_repo_dir);
    let rel_data_dir = data_dir.map(|d| relativize_to_repo(&repo_path, d));

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "action": action,
            "status": {
                "id": s.id.0,
                "state": format_state(&s.state),
                "pid": s.pid,
                "started_at": s.started_at.map(|t| t.to_rfc3339()),
                "exit_code": s.exit_code,
                "data_dir": rel_data_dir,
            }
        }))?
    );
    Ok(())
}

fn print_status(s: &InstanceStatus, data_dir: Option<&str>, path: Option<&PathBuf>) {
    println!("{}", box_top(&bold("Compute"), BOX_W));

    // ID
    let truncated_id = truncate_id(&s.id.0);
    let row = fmt_box_row_colored("id", &dimmed(&truncated_id), &truncated_id, LABEL_W, BOX_W);
    println!("{}", box_row(&row, BOX_W));

    // State with dot indicator
    let state_str = format_state(&s.state);
    let dot = status_indicator_colored(&s.state);
    let colored_state = format!("{} {}", dot, format_state_colored_text(&s.state));
    let raw_state = format!("{} {}", status_indicator_raw(&s.state), state_str);
    let row = fmt_box_row_colored("state", &colored_state, &raw_state, LABEL_W, BOX_W);
    println!("{}", box_row(&row, BOX_W));

    // PID
    if let Some(pid) = s.pid {
        let pid_str = pid.to_string();
        let row = fmt_box_row("pid", &pid_str, LABEL_W, BOX_W);
        println!("{}", box_row(&row, BOX_W));
    }

    // Started at
    if let Some(started_at) = s.started_at {
        let ts = started_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let row = fmt_box_row("started_at", &ts, LABEL_W, BOX_W);
        println!("{}", box_row(&row, BOX_W));
    }

    // Exit code
    if let Some(code) = s.exit_code {
        let code_str = code.to_string();
        let row = fmt_box_row("exit_code", &code_str, LABEL_W, BOX_W);
        println!("{}", box_row(&row, BOX_W));
    }

    // Container data dir
    if let Some(dir) = data_dir {
        let repo_path = path.cloned().unwrap_or_else(get_repo_dir);
        let rel = relativize_to_repo(&repo_path, dir);
        let row = fmt_box_row("data dir", &rel, LABEL_W, BOX_W);
        println!("{}", box_row(&row, BOX_W));
    }

    println!("{}", box_bottom(BOX_W));
}

fn format_state(state: &InstanceState) -> &'static str {
    match state {
        InstanceState::Starting => "starting",
        InstanceState::Running => "running",
        InstanceState::Paused => "paused",
        InstanceState::Stopping => "stopping",
        InstanceState::Stopped => "stopped",
        InstanceState::Restarting => "restarting",
        InstanceState::Failed => "failed",
        InstanceState::Unknown => "unknown",
    }
}

fn status_indicator_raw(state: &InstanceState) -> &'static str {
    match state {
        InstanceState::Running => "●",
        InstanceState::Starting | InstanceState::Restarting => "◐",
        InstanceState::Stopped | InstanceState::Stopping | InstanceState::Paused => "○",
        InstanceState::Failed | InstanceState::Unknown => "✕",
    }
}

fn status_indicator_colored(state: &InstanceState) -> String {
    let dot = status_indicator_raw(state);
    match state {
        InstanceState::Running => green(dot),
        InstanceState::Starting | InstanceState::Restarting => yellow(dot),
        InstanceState::Stopped | InstanceState::Stopping | InstanceState::Paused => dimmed(dot),
        InstanceState::Failed | InstanceState::Unknown => red(dot),
    }
}

fn format_state_colored_text(state: &InstanceState) -> String {
    let s = format_state(state);
    match state {
        InstanceState::Running => green(s),
        InstanceState::Starting | InstanceState::Restarting => yellow(s),
        InstanceState::Stopped | InstanceState::Stopping | InstanceState::Paused => dimmed(s),
        InstanceState::Failed | InstanceState::Unknown => red(s),
    }
}

fn truncate_id(id: &str) -> String {
    if id.len() <= 16 {
        id.to_string()
    } else {
        format!("{}…", &id[..12])
    }
}
