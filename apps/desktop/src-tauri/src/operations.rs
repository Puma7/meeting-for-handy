//! Long-running work shares one admission policy. A lease is held until its work
//! really finishes, including cancellation and unwinding, rather than until spawn.
use meeting_notes::AnalysisControl;
use std::sync::{Arc, Mutex, TryLockError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Recording,
    Analysis,
    HistoryQuestion,
    Speakers,
    Index,
    Export,
    Mutation,
    Maintenance,
    Installation,
    Selection,
    /// Live notes write into one session folder; archive and installation must wait for them.
    LiveNotes,
}
impl Kind {
    /// Identify kinds requiring every other admitted operation to have finished.
    pub(crate) fn exclusive(self) -> bool {
        matches!(
            self,
            Self::Maintenance | Self::Installation | Self::Selection
        )
    }
    /// History questions yield to capture and retain the ordinary analysis exclusion.
    fn conflicts(self, other: Self) -> bool {
        self == other
            || self.exclusive()
            || other.exclusive()
            || matches!(
                (self, other),
                (Self::HistoryQuestion, Self::Recording | Self::Analysis)
                    | (Self::Recording | Self::Analysis, Self::HistoryQuestion)
            )
    }
}

const BUSY: &str = "Another operation is still running. Wait for it to finish or cancel it first. / Ein anderer Vorgang läuft noch. Bitte zuerst abschließen oder abbrechen.";
const QUESTION_CANCELLING: &str = "The meeting question is stopping for recording. Try again shortly. / Die Meetingfrage wird für die Aufnahme beendet. Bitte gleich erneut versuchen.";
pub(crate) const RECORDING_PRIORITY: &str = "Recording and transcription have priority. Ask again when they finish. / Aufnahme und Transkription haben Vorrang. Stelle die Frage danach erneut.";

struct Active {
    kind: Kind,
    history_control: Option<AnalysisControl>,
}

#[derive(Default)]
pub(crate) struct Gate(Arc<Mutex<Vec<Active>>>);
pub(crate) struct Lease {
    gate: Arc<Mutex<Vec<Active>>>,
    kind: Kind,
    history_control: Option<AnalysisControl>,
}

impl Lease {
    pub(crate) fn history_control(&self) -> Option<&AnalysisControl> {
        self.history_control.as_ref()
    }
}

impl Gate {
    /// Reserve admission without waiting for mutex contention or conflicting work. The returned
    /// lease must span actual work, cancellation and cleanup.
    pub(crate) fn try_acquire(&self, kind: Kind) -> Result<Lease, String> {
        let mut active = self.0.try_lock().map_err(|error| match error {
            TryLockError::WouldBlock => BUSY.to_owned(),
            TryLockError::Poisoned(_) => "Operation control is unavailable; saved work is preserved. / Vorgangssteuerung ist gesperrt; gesicherte Daten bleiben erhalten.".into(),
        })?;
        if kind == Kind::Recording {
            if let Some(control) = active.iter().find_map(|work| work.history_control.as_ref()) {
                control.cancel();
                return Err(QUESTION_CANCELLING.into());
            }
        }
        if kind == Kind::HistoryQuestion && active.iter().any(|work| work.kind == Kind::Recording) {
            return Err(RECORDING_PRIORITY.into());
        }
        if active.iter().any(|work| kind.conflicts(work.kind)) {
            return Err(BUSY.into());
        }
        let history_control = (kind == Kind::HistoryQuestion).then(AnalysisControl::default);
        active.push(Active {
            kind,
            history_control: history_control.clone(),
        });
        Ok(Lease {
            gate: self.0.clone(),
            kind,
            history_control,
        })
    }
}
impl Drop for Lease {
    /// Release only this lease kind while keeping other admitted operations reserved.
    fn drop(&mut self) {
        if let Ok(mut active) = self.gate.lock() {
            if let Some(index) = active.iter().position(|work| work.kind == self.kind) {
                active.swap_remove(index);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify synthetic leases exclude all exclusive work until the held operation is dropped.
    #[test]
    fn destructive_work_refuses_every_active_kind_and_releases_on_exit() {
        for kind in [
            Kind::Recording,
            Kind::Analysis,
            Kind::HistoryQuestion,
            Kind::Speakers,
            Kind::Index,
            Kind::Export,
            Kind::Mutation,
            Kind::Maintenance,
            Kind::Installation,
            Kind::Selection,
            Kind::LiveNotes,
        ] {
            let gate = Gate::default();
            let lease = gate.try_acquire(kind).unwrap();
            for exclusive in [Kind::Maintenance, Kind::Installation, Kind::Selection] {
                assert!(
                    gate.try_acquire(exclusive).is_err(),
                    "{kind:?}/{exclusive:?}"
                );
            }
            drop(lease);
            assert!(gate.try_acquire(Kind::Maintenance).is_ok());
        }
    }

    /// Verify distinct nonexclusive work coexists while duplicate kinds and mutex contention
    /// return busy.
    #[test]
    fn independent_non_destructive_work_can_coexist_and_contention_returns_busy() {
        let gate = Gate::default();
        let recording = gate.try_acquire(Kind::Recording).unwrap();
        let analysis = gate.try_acquire(Kind::Analysis).unwrap();
        let speakers = gate.try_acquire(Kind::Speakers).unwrap();
        let index = gate.try_acquire(Kind::Index).unwrap();
        assert!(gate.try_acquire(Kind::Recording).is_err());
        let locked = gate.0.lock().unwrap();
        assert!(gate
            .try_acquire(Kind::Maintenance)
            .err()
            .unwrap()
            .contains("running"));
        drop(locked);
        drop((recording, analysis, speakers, index));
        assert!(gate.try_acquire(Kind::Installation).is_ok());
    }

    #[test]
    fn recording_cancels_an_active_history_question_and_waits_for_its_lease() {
        use std::{sync::mpsc, thread, time::Duration};

        let gate = Arc::new(Gate::default());
        let worker_gate = Arc::clone(&gate);
        let (started, ready) = mpsc::channel();
        let (finish, release) = mpsc::channel();
        let worker = thread::spawn(move || {
            let lease = worker_gate.try_acquire(Kind::HistoryQuestion).unwrap();
            started
                .send(lease.history_control().unwrap().clone())
                .unwrap();
            release.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        let control = ready.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            gate.try_acquire(Kind::Recording).err().unwrap(),
            QUESTION_CANCELLING
        );
        assert!(control.is_cancelled());
        assert!(gate.try_acquire(Kind::Recording).is_err());
        assert!(gate.try_acquire(Kind::Analysis).is_err());
        finish.send(()).unwrap();
        worker.join().unwrap();

        let recording = gate.try_acquire(Kind::Recording).unwrap();
        assert_eq!(
            gate.try_acquire(Kind::HistoryQuestion).err().unwrap(),
            RECORDING_PRIORITY
        );
        let ordinary_analysis = gate.try_acquire(Kind::Analysis).unwrap();
        drop((recording, ordinary_analysis));
        assert!(gate.try_acquire(Kind::HistoryQuestion).is_ok());
    }

    /// Verify unwinding releases synthetic analysis admission without releasing concurrent
    /// recording admission.
    #[test]
    fn unwinding_releases_only_the_owned_operation() {
        let gate = Gate::default();
        let recording = gate.try_acquire(Kind::Recording).unwrap();
        let result = std::panic::catch_unwind(|| {
            let _analysis = gate.try_acquire(Kind::Analysis).unwrap();
            panic!("synthetic worker failure");
        });
        assert!(result.is_err());
        assert!(gate.try_acquire(Kind::Analysis).is_ok());
        assert!(gate.try_acquire(Kind::Maintenance).is_err());
        drop(recording);
        assert!(gate.try_acquire(Kind::Maintenance).is_ok());
    }
}
