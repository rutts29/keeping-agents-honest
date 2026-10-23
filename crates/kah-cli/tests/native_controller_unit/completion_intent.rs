use super::*;
use kah_core::{
    intent_history::History,
    run_store::RunStore,
    unattended::{CompletionEvidence, RunControl, RunStatus},
};
use kah_observer::session::{State, Store};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

const GOAL: &str = "Add a word-count command with tests.";

fn binding(project: PathBuf) -> kah_observer::session::Binding {
    kah_observer::session::Binding {
        session: format!("completion-intent-{}", crate::relay_store::nonce()),
        codex_home: project.join("codex"),
        project,
        native: "claude".into(),
        model: "claude-sonnet-5-5".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    }
}
/// Revision 2: the user goal and one later user requirement.
fn history() -> History {
    use kah_core::intent_history::{
        Event, GoalRevision, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
    };
    let user = |record: &str| {
        SourceRef::new(
            SourceProvider::Claude,
            "session",
            record,
            SourceRole::User,
            format!("sha256:{record}"),
        )
    };
    let mut history = History::new(GoalRevision::new(1, GOAL, vec![user("goal")])).unwrap();
    history
        .append(Event::RequirementAdded {
            item: IntentItem::new(
                "count-words",
                IntentKind::Outcome,
                "Count words separated by any whitespace.",
                vec![user("count-words")],
            ),
            approval: None,
        })
        .unwrap();
    history
}

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
        let mut state = State::new(binding(base.join("project")));
        state.run_id = Some("run-completion".into());
        fs::create_dir_all(&state.binding.project).unwrap();
        let history = history();
        let projection = ActiveProjection::from_history(
            &history,
            kah_core::projection::ProjectionConfig::default(),
        )
        .unwrap();
        let proof = json!({"goal":GOAL,"tests":{"commit":"7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a87","success":true}});
        if persist {
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
fn revision_two_with_goal_completes_after_empty_no_gap_sync() {
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
