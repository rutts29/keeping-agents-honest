//! Matrix flags are real native findings; absent Jev answers are never invented.
use super::*;
use kah_cli::commit_measure::{self, Bundle, confirmation::Answers};
pub fn run(f: &fixture::Fixture, id: &str, pane: &str, bundle: &Bundle) {
    let directory = f.evidence.join(id).join("confirmation");
    let count = commit_measure::confirmation::count(
        bundle,
        &Answers {
            stages: Default::default(),
        },
        &directory,
    )
    .unwrap();
    if count["flags"] == 0 {
        return;
    }
    let output = f.project.join(format!("{id}-confirmation.json"));
    let before = usage::snapshot(&f.home, &f.project);
    let prompt = format!(
        "[kah-observer] Confirm the REAL native matrix flags for case {id}; no Jev answers exist yet. Read {}. {} Write the JSON array to {}. No probabilities or Jev calls.",
        directory.join("packet.json").display(),
        fs::read_to_string(directory.join("prompt.txt")).unwrap(),
        output.display(),
    );
    herdr(&["agent", "prompt", pane, &prompt]);
    journey::wait(f, pane, &output);
    let after = usage::snapshot(&f.home, &f.project);
    assert_eq!(after["session"], before["session"]);
    fs::copy(&output, directory.join("native-answer.json")).unwrap();
    fs::copy(
        after["transcript"].as_str().unwrap(),
        f.evidence.join("native-transcript.jsonl"),
    )
    .unwrap();
    save(
        &directory.join("native-result.json"),
        &json!({
            "session":after["session"],"usage":usage::delta(&before,&after),
            "source":"fresh matrix flags only; real Jev confirmation pending authorization",
            "actual_jev_calls":0,"confirmation_side_jev_calls":0
        }),
    );
}
