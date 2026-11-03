//! Actual local-model reference with synthetic text; never reads installed meetings.
use anyhow::{ensure, Context, Result};
use meeting_notes::live::{LiveRunner, LiveText, Trigger};
use meeting_notes::residency::{LocalModelPool, LocalProvider};
use meeting_notes::workspace::{Agenda, WorkspaceContext};
use meeting_notes::{AnalysisControl, Connection};
use std::{path::PathBuf, time::Instant};
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let folder = PathBuf::from(
        args.next()
            .context("New private output directory required")?,
    );
    ensure!(!folder.exists(), "Preserve previous evidence.");
    std::fs::create_dir_all(&folder)?;
    let endpoint = args
        .next()
        .unwrap_or_else(|| "http://127.0.0.1:1235/v1/".into());
    let model = args.next().unwrap_or_else(|| "google/gemma-4-e4b".into());
    let connection = Connection {
        endpoint,
        model,
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    let provider = if connection.endpoint.contains(":11434/") {
        LocalProvider::Ollama
    } else {
        LocalProvider::LmStudio
    };
    let pool = LocalModelPool::default();
    let lease = pool.reserve(&connection, provider)?;
    let context = WorkspaceContext {
        agenda: vec![Agenda {
            id: "e_budget_agenda".into(),
            revision: 1,
            text: "Budgetrahmen klären".into(),
        }],
        ..Default::default()
    };
    let mut runner = LiveRunner::start(
        &folder.join("live-notes"),
        "synthetic-workspace-reference",
        connection,
        "de",
    )?
    .with_workspace(context)?
    .with_residency(lease.clone())?;
    let start = Instant::now();
    let lines=[
        "Robin sagt: Ich schreibe den Bericht bis Freitag. Zum Budget habe ich noch keine Zusage. Vielleicht könnten wir die Webseite im Dezember erneuern. Das ist keine Zusage.",
        "Wir entscheiden uns für den Pilotstart am 12. November. Die Freigabe für den Entwurf ist erteilt. Zum Budget wurde noch keine neue Entscheidung getroffen.",
    ];
    let mut total_retries = 0;
    let mut parts = vec![];
    let mut first_ids = vec![];
    for (i, text) in lines.iter().enumerate() {
        parts.push(LiveText {
            key: format!("synthetic-{i}"),
            track: "system".into(),
            start_ms: i as u64 * 180_000,
            end_ms: (i as u64 + 1) * 180_000,
            text: (*text).into(),
        });
        println!(
            "workspace reference window {} started at {:.1}s",
            i + 1,
            start.elapsed().as_secs_f64()
        );
        let pending = runner.pending(&parts);
        let control = AnalysisControl::default();
        runner.update(&pending, Trigger::Manual, &control)?;
        total_retries += control.validation_retries();
        let view = runner.view();
        let state = view.workspace.as_ref().context("Workspace view missing")?;
        ensure!(
            !state
                .entities
                .iter()
                .any(|e| e.kind == "task" && e.text.to_lowercase().contains("webseite")),
            "Nonbinding suggestion became a task"
        );
        ensure!(
            state
                .entities
                .iter()
                .any(|e| e.kind == "task" && e.status == "open"),
            "Positive binding task was not retained"
        );
        ensure!(
            state.entities.iter().filter(|e| e.kind == "task").count() == 1,
            "Synthetic missing pledge became an extra task"
        );
        let ids = state
            .entities
            .iter()
            .filter(|e| e.kind == "impulse")
            .map(|e| e.id.clone())
            .collect::<Vec<_>>();
        if i == 0 {
            ensure!(!ids.is_empty(), "No synthetic open-topic impulse generated");
            first_ids = ids;
        } else {
            ensure!(
                first_ids.iter().all(|id| ids.contains(id)),
                "Existing impulses were lost"
            );
        }
        std::fs::write(
            folder.join(format!("view-{}.json", i + 1)),
            serde_json::to_vec_pretty(&view)?,
        )?;
        println!(
            "workspace reference window {} saved at {:.1}s",
            i + 1,
            start.elapsed().as_secs_f64()
        );
    }
    let before = serde_json::to_vec(&runner.view().workspace)?;
    drop(runner);
    drop(lease);
    let reopened = meeting_notes::live::read_latest(
        &folder.join("live-notes"),
        "synthetic-workspace-reference",
    )?
    .context("Reopened workspace is missing")?;
    ensure!(
        serde_json::to_vec(&reopened.workspace)? == before,
        "Reopened state differs"
    );
    std::fs::write(
        folder.join("reference-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"synthetic":true,"validationRetries":total_retries,"promptVersion":meeting_notes::workspace::PROMPT_VERSION,"provider":"actual local","elapsedSeconds":start.elapsed().as_secs_f64(),"impulsesRetained":true,"positiveTask":true,"nonbindingNotTask":true,"stateBytes":before.len(),"reopenByteEqual":true,"hardwareCaptureTest":false}),
        )?,
    )?;
    Ok(())
}
