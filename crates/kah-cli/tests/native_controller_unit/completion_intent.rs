use super::*;
use kah_core::{
    intent_history::History,
    run_store::RunStore,
    unattended::{CompletionEvidence, RunControl, RunStatus},
};
use kah_observer::session::{State, Store};
use serde_json::Value;
use std::{fs, path::PathBuf};

struct Fixture {
    base: PathBuf,
    state: State,
    projection: ActiveProjection,
    proof: Value,
}
impl Fixture {
    fn new(persist: bool) -> Self {
        let base = std::env::temp_dir().join(format!(
            "kah-completion-intent-{}",
            crate::relay_store::nonce()
        ));
        let mut state: State = serde_json::from_str(include_str!(
            "../../../../docs/gates/evidence/phase3-claude/b2/completion-intent-replay/session.json"
        ))
        .unwrap();
        state.binding.project = base.join("project");
        fs::create_dir_all(&state.binding.project).unwrap();
        let projection:ActiveProjection=serde_json::from_str(include_str!("../../../../docs/gates/evidence/phase3-claude/b2/completion-intent-replay/projection.json")).unwrap();
        let proof: Value = serde_json::from_str(include_str!(
            "../../../../docs/gates/evidence/phase3-claude/b2/completion-intent-replay/report.json"
        ))
        .unwrap();
        if persist {
            let history=History::from_jsonl(include_str!("../../../../docs/gates/evidence/phase3-claude/b2/completion-intent-replay/history.jsonl")).unwrap();
            drop(
                RunStore::create(
                    &base,
                    &state.binding.project,
                    state.run_id.as_ref().unwrap(),
                    &history,
                    &projection,
                )
                .unwrap(),
            );
        } else {
            state.run_id = None;
        }
        let mut store = Store::open_wait(&base, &state.binding.session).unwrap();
        store.save(&state).unwrap();
        drop(store);
        Self {
            base,
            state,
            projection,
            proof,
        }
    }
    fn control(&self) -> RunControl {
        let mut control =
            RunControl::new(self.proof["goal"].as_str().unwrap(), Default::default()).unwrap();
        control.start().unwrap();
        control
            .checkpoint(self.proof["tests"]["commit"].as_str().unwrap().to_owned())
            .unwrap();
        control
    }
    fn complete_stub(&self, status: &Status, control: &mut RunControl) {
        if let Status::Ready(projection) = status {
            assert_eq!(projection.goal.text, self.projection.goal.text);
            assert_eq!(projection.revision, 2);
            control
                .complete(&CompletionEvidence {
                    ledger_user_sequence: 0,
                    commit: self.proof["tests"]["commit"].as_str().unwrap().into(),
                    tests_passed: self.proof["tests"]["success"].as_bool().unwrap(),
                    outcomes_met: true,
                })
                .unwrap();
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.base).unwrap();
    }
}

#[test]
fn retained_revision_two_with_goal_completes_after_empty_no_gap_sync() {
    let f = Fixture::new(true);
    let mut control = f.control();
    let gap = resolve(
        &f.base,
        &f.state.binding,
        Outcome {
            gap: Some("OBSERVER_SOURCE_PENDING".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(waiting(&gap).unwrap().contains("observer intake gap"));
    f.complete_stub(&gap, &mut control);
    assert_eq!(control.status(), RunStatus::Running);
    for projection in [
        None,
        Some({
            let mut p = f.projection.clone();
            p.goal.text.clear();
            p
        }),
    ] {
        let status = resolve(
            &f.base,
            &f.state.binding,
            Outcome {
                projection,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(waiting(&status).is_none());
        let mut resumed = f.control();
        f.complete_stub(&status, &mut resumed);
        assert_eq!(resumed.status(), RunStatus::Completed);
    }
}
#[test]
fn paused_completion_defers_then_completes_after_unpause() {
    let f = Fixture::new(true);
    let marker = Store::directory(&f.base, &f.state.binding.session).join("observer-paused");
    fs::write(&marker, b"fixture pause").unwrap();
    let mut control = f.control();
    let paused = resolve(
        &f.base,
        &f.state.binding,
        Outcome {
            projection: Some(f.projection.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(waiting(&paused), Some("observer paused"));
    f.complete_stub(&paused, &mut control);
    assert_eq!(control.status(), RunStatus::Running);
    fs::remove_file(marker).unwrap();
    let status = resolve(&f.base, &f.state.binding, Outcome::default()).unwrap();
    f.complete_stub(&status, &mut control);
    assert_eq!(control.status(), RunStatus::Completed);
}
#[test]
fn truly_missing_authoritative_intent_still_fails() {
    let f = Fixture::new(false);
    let error = resolve(&f.base, &f.state.binding, Outcome::default())
        .err()
        .unwrap();
    assert!(
        error
            .to_string()
            .starts_with("RUN_INTENT_MISSING stage=completion")
    );
}
