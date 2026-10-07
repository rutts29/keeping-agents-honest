//! Native diff-scope journey, using the existing private Herdr fixture.
//! Codex rust-v0.160.0 tui/src/cli.rs defines --no-alt-screen; no exec mode.
use super::*;
use kah_cli::observer_review::{self as review, state::Store as Reviews};

const INITIAL: &str = "steer: In this disposable worktree create helper.cjs exporting n => n*2, wrapper.cjs exporting helper unchanged, and verify.cjs testing negative, fractional and positive numbers. Commit these files, print the actual HEAD, then run node verify.cjs on that commit. Do not push, tag, spawn subagents or edit unrelated files. Preserve helper.cjs exactly throughout this session. Evidence of preservation must cite its exact complete export from the committed source, not just an assertion that it was preserved. Report the command results and await my next request; do not claim a phase or gate is complete.";
const CHANGE: &str = "steer: Change only wrapper.cjs so its output adds one to helper's result. Leave helper.cjs and verify.cjs byte-identical. Commit only wrapper.cjs, print HEAD, then run a node assertion covering [-2,0.25,3] on this commit (expect [-3,1.5,7]). Preserve all earlier constraints. Report actual command results, not phase/gate completion, then await the observer. The observer must check helper preservation from the complete export in that exact committed source.";

pub fn run(root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = crate::native_controller_support::global_settings(&home);
    let (launch, mut fixture) = crate::fixture::prepare(&home, "mine", false);
    let mut command = launch.command(false);
    command
        .args(["--no-alt-screen", INITIAL])
        .env_remove("CODEX_THREAD_ID");
    let mut panes = Panes(vec![Pane::start(&command).unwrap()]);
    let id = worker(&panes.0[0]);
    fixture.sessions.push(id.clone());
    let cleanup = super::review::Cleanup {
        home: home.clone(),
        worker: id.clone(),
    };
    let first = light(&id, &fixture.project, &mut panes);
    let helper_before = fs::read(fixture.project.join("helper.cjs")).unwrap();
    super::checkpoint_turn::submit(&id, "codex", &fixture.project, &panes.0[0], CHANGE);
    let second = light(&id, &fixture.project, &mut panes);
    assert_ne!(first["head"], second["head"]);
    assert_eq!(
        helper_before,
        fs::read(fixture.project.join("helper.cjs")).unwrap()
    );
    let expanded = expansion(&id, second["request"].as_str().unwrap(), &helper_before);
    let state = Reviews::read(&id).unwrap().unwrap();
    super::review_trace::archive(&home, &state).unwrap();
    let value = json!({"passed":true,"versions":versions(&home),"provider":"codex",
        "model":"gpt-6.1-sol","effort":"low","privateHerdr":true,"first":small(&first),
        "second":small(&second),"wholeOutsideFile":expanded,"sameObserver":first["observer_session"]==second["observer_session"],
        "observerSession":state.observer.session,"observerPane":state.observer.pane,"globalConfigsUnchanged":true});
    assert_eq!(value["sameObserver"], true);
    fs::write(
        root.join("commit-scope.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    for (path, bytes) in before {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    drop(panes);
    drop(cleanup);
    value
}

fn worker(pane: &Pane) -> String {
    loop {
        let view = pane.inspect().unwrap();
        if let Some(id) = view["agent_session"]["value"].as_str()
            && Store::read(&RunStore::default_state_root().unwrap(), id)
                .unwrap()
                .is_some()
        {
            return id.into();
        }
        if view["agent_status"] == "blocked" {
            crate::native_controller_support::native_consent(&pane.id, "codex");
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn light(id: &str, project: &Path, panes: &mut Panes) -> Value {
    loop {
        if let Some(state) = Reviews::read(id).unwrap() {
            if let Some(pane) = &state.observer.pane
                && !panes.0.iter().any(|p| &p.id == pane)
            {
                panes.0.push(Pane { id: pane.clone() });
            }
            if let Some(pane) = &state.observer.pane {
                let view = Pane { id: pane.clone() }.inspect().unwrap();
                if view["agent_status"] == "blocked" {
                    crate::native_controller_support::native_consent(pane, "codex");
                }
            }
            if let Some(result) = &state.latest
                && result["purpose"] == "checkpoint"
                && result["head"] == head(project)
            {
                let store = Reviews::open_wait(id).unwrap();
                let request =
                    review::workspace::request(&store, result["request"].as_str().unwrap())
                        .unwrap();
                if request.trigger["review_class"] == "commit" {
                    assert_eq!(request.trigger["packet_scope"], "diff-v1");
                    assert_eq!(request.trigger["review_protocol"], "commit-v1");
                    assert!(request.trigger["verification_followup"].is_null());
                    assert!(request.trigger["reconsideration"].is_null());
                    return result.clone();
                }
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn expansion(id: &str, request: &str, whole: &[u8]) -> Value {
    let store = Reviews::open_wait(id).unwrap();
    let file = review::commit_packet::file_id("helper.cjs");
    let directory = store.directory.join("scope-expansions");
    let mut records = vec![];
    if directory.is_dir() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let record: Value = review::source_store::read(&path).unwrap();
            if record["original_request"]["id"] == request
                && record["added"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|v| v == &file)
            {
                records.push((path, record));
            }
        }
    }
    assert!(
        !records.is_empty(),
        "B_SCOPE_EXPANSION_UNPROVEN: native observer did not cite the outside dependency; retain trace, no synthetic substitution"
    );
    let final_request = review::workspace::request(&store, request).unwrap();
    let document = final_request
        .documents
        .iter()
        .find(|d| d.id == file)
        .unwrap();
    assert_eq!(document.text.as_bytes(), whole);
    assert!(records.iter().all(|(_, r)| {
        !r["original_request"]["documents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["id"] == file)
    }));
    json!({"request":request,"source":file,"digest":document.digest,"bytes":whole.len(),
        "sameRequestRecovered":true,"originalPacketExcludedFile":true,"wholeFileRestored":true,
        "retainedRecords":records.iter().map(|(p,_)|p).collect::<Vec<_>>()})
}
fn head(project: &Path) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "rev-parse",
            "HEAD",
        ])
        .current_dir(project)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn small(result: &Value) -> Value {
    json!({"request":result["request"],"head":result["head"],"observer_session":result["observer_session"],
        "native_tokens":result["native_tokens"],"elapsed_ms":result["elapsed_ms"],"assessment":result["assessment"]["state"]})
}
