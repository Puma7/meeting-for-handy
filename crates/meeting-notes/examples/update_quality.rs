//! Run one synthetic three-update meeting through the production workspace and local model.
//! Usage: update_quality NEW_DIRECTORY MODEL de|en room|pilot
use anyhow::{ensure, Context, Result};
use meeting_notes::{
    live::{read_latest, LiveRunner, LiveText, Trigger},
    residency::{LocalModelPool, LocalProvider},
    AnalysisControl, Connection,
};
use serde_json::json;
use std::{fs, path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 4,
        "Expected NEW_DIRECTORY MODEL de|en room|pilot"
    );
    let folder = PathBuf::from(&args[0]);
    let model = &args[1];
    let language = &args[2];
    let scenario = &args[3];
    let (lines, selected, subject, owner, due) = match (language.as_str(), scenario.as_str()) {
        ("de", "room") => ([
            "Vielleicht könnten wir Raum A nehmen. Das ist nur ein Vorschlag. Robin sagt: Ich verschicke die Einladung bis Freitag.",
            "Wir entscheiden uns für Raum B. Raum A bleibt ein verworfener Vorschlag.",
            "Robin sagt: Ich habe die Einladung verschickt.",
        ], "raum b", "einladung", "Robin", "Freitag"),
        ("en", "room") => ([
            "Maybe we could use Room A. That is only a suggestion. Robin says: I will send the invitation by Friday.",
            "We decide to use Room B. Room A remains a rejected suggestion.",
            "Robin says: I have sent the invitation.",
        ], "room b", "invitation", "Robin", "Friday"),
        ("de", "pilot") => ([
            "Vielleicht verschieben wir den Test. Das ist noch offen. Mara sagt: Ich erstelle die Checkliste bis Dienstag.",
            "Wir beschließen den Start mit zwei Geräten.",
            "Mara sagt: Ich habe die Checkliste erstellt.",
        ], "zwei gerät", "checkliste", "Mara", "Dienstag"),
        ("en", "pilot") => ([
            "Perhaps we could postpone the trial. That is still open. Mara says: I will prepare the checklist by Tuesday.",
            "We decide to start with two devices.",
            "Mara says: I have prepared the checklist.",
        ], "two device", "checklist", "Mara", "Tuesday"),
        _ => anyhow::bail!("Unknown fixture"),
    };
    fs::create_dir(&folder).context("Use a new, private output directory")?;
    let connection = Connection {
        endpoint: "http://127.0.0.1:1235/v1/".into(),
        model: model.clone(),
        mode: "local".into(),
        cloud_consent: false,
        api_key: String::new(),
    };
    let pool = LocalModelPool::default();
    let init = Instant::now();
    let lease = pool.reserve(&connection, LocalProvider::LmStudio)?;
    let meeting = "synthetic-quality";
    let mut runner = LiveRunner::start_workspace(&folder, meeting, connection, language)?
        .with_workspace(Default::default())?
        .with_residency(lease)?;
    let setup_seconds = init.elapsed().as_secs_f64();
    let mut parts = Vec::new();
    let mut first_task = None;
    let mut first_decision = None;
    let mut updates = Vec::new();
    let mut passed = true;
    for (i, line) in lines.iter().enumerate() {
        parts.push(LiveText {
            key: format!("synthetic-{i}"),
            track: "system".into(),
            start_ms: i as u64 * 120_000,
            end_ms: (i as u64 + 1) * 120_000,
            text: (*line).into(),
        });
        let pending = runner.pending(&parts);
        let control = AnalysisControl::default();
        let start = Instant::now();
        runner.update(&pending, Trigger::Manual, &control)?;
        let seconds = start.elapsed().as_secs_f64();
        let view = runner.view();
        let state = view.workspace.as_ref().context("Missing workspace")?;
        let decisions: Vec<_> = state
            .entities
            .iter()
            .filter(|e| e.kind == "decision")
            .collect();
        let tasks: Vec<_> = state.entities.iter().filter(|e| e.kind == "task").collect();
        let task = tasks
            .iter()
            .copied()
            .find(|e| e.text.to_lowercase().contains(subject));
        if i == 0 {
            first_task = task.map(|e| e.id.clone());
        }
        if i == 1 && decisions.len() == 1 {
            first_decision = Some(decisions[0].id.clone());
        }
        let cites_window = |e: &meeting_notes::workspace::Entity, index: usize, phrase: &str| {
            e.evidence.iter().any(|ev| {
                ev.quote.to_lowercase().contains(phrase)
                    && state.originals.iter().any(|s| {
                        s.id == ev.source_id && s.text == lines[index] && s.text.contains(&ev.quote)
                    })
            })
        };
        let decision = if i == 0 {
            decisions.is_empty()
        } else {
            decisions.len() == 1
                && first_decision.as_ref() == Some(&decisions[0].id)
                && decisions[0].text.to_lowercase().contains(selected)
                && cites_window(decisions[0], 1, selected)
        };
        let deadline = task.is_some_and(|e| {
            e.due.as_ref().is_some_and(|d| {
                let d = d.trim().to_lowercase();
                d.strip_prefix("bis ")
                    .or_else(|| d.strip_prefix("by "))
                    .unwrap_or(&d)
                    == due.to_lowercase()
            })
        });
        let task_state = tasks.len() == 1
            && task.is_some_and(|e| {
                Some(&e.id) == first_task.as_ref()
                    && e.owner.as_deref() == Some(owner)
                    && e.status == if i == 2 { "completed" } else { "open" }
                    && (i != 2 || cites_window(e, 2, subject))
            });
        let grounded = state.entities.iter().all(|e| {
            !e.evidence.is_empty()
                && e.evidence.iter().all(|ev| {
                    !ev.quote.is_empty()
                        && state
                            .originals
                            .iter()
                            .any(|s| s.id == ev.source_id && s.text.contains(&ev.quote))
                })
        });
        passed &= decision && deadline && task_state && grounded;
        updates.push(json!({"update":i+1,"seconds":seconds,"retries":control.validation_retries(),"decision":decision,"deadline":deadline,"taskState":task_state,"grounded":grounded}));
        fs::write(
            folder.join(format!("window-{}.json", i + 1)),
            serde_json::to_vec_pretty(state)?,
        )?;
    }
    let expected = serde_json::to_vec(&runner.view().workspace)?;
    drop(runner);
    let restored = read_latest(&folder, meeting)?.context("Missing saved analysis")?;
    let reopened = serde_json::to_vec(&restored.workspace)? == expected;
    let result = json!({"model":model,"language":language,"scenario":scenario,"setupSeconds":setup_seconds,
        "updates":updates,"reopened":reopened,"passed":passed && reopened});
    fs::write(
        folder.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{result}");
    ensure!(passed && reopened, "Quality expectation failed");
    Ok(())
}
