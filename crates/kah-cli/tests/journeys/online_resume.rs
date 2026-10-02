use super::{fixture::Fixture, operations};
use crate::client::Client;
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn verify(home: &Path, f: &Fixture, id: &str) -> Value {
    let args = kah_cli::Arguments::parse(vec!["--".into(), "resume".into(), id.into()]).unwrap();
    let launch = kah_cli::Launch::prepare(home, &home.join("repos/mine"), args).unwrap();
    assert_eq!(launch.project.root, f.project);
    assert_eq!(launch.overlay, f.overlay);
    let mut client = Client::start_command(launch.command(true), &f.stderr);
    let resume=client.request("thread/resume",json!({"threadId":id,"cwd":f.project,"permissions":":danger-full-access","approvalPolicy":"never"}));
    assert_eq!(resume["model"], "gpt-6.1-sol");
    let config = client.request(
        "config/read",
        json!({"cwd":f.project,"includeLayers":false}),
    );
    assert_eq!(config["config"]["web_search"], "live");
    let turn=client.turn(id,&format!("Use native exec_command once: test ! -e '{}' && echo HOST_HIDDEN; curl --max-time 12 -Is https://example.com; echo ONLINE_CURL_EXIT=$?. This is the resumed explicitly-online acceptance test. Report and stop; no other tools.",f.canary.display()));
    assert_eq!(turn["status"], "completed");
    let operations = operations(&client.events);
    let text = serde_json::to_string(&operations).unwrap();
    assert!(
        text.contains("HOST_HIDDEN") && text.contains("ONLINE_CURL_EXIT=0"),
        "{text}"
    );
    json!({"priorOnlineChoiceRestored":true,"sameStableOverlay":true,"model":resume["model"],"operations":operations,"passed":true})
}
