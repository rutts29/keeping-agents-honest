//! Controller-supplied audit scope; no scope preserves the strict global audit.
use super::{
    audit_scope::{Resource, Scope, report},
    processes,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};
pub fn run() -> Value {
    let resources = resources();
    let Some(path) = std::env::var_os("KAH_FIXTURE_AUDIT_SCOPE") else {
        let processes = resources
            .iter()
            .filter(|r| r.identity["kind"] == "process")
            .map(|r| r.identity.clone())
            .collect::<Vec<_>>();
        let containers = resources
            .iter()
            .filter(|r| r.identity["kind"] == "container")
            .map(|r| r.identity["id"].clone())
            .collect::<Vec<_>>();
        return json!({"fixtureProcesses":processes,"fixtureContainers":containers,"passed":resources.is_empty()});
    };
    let scope: Scope =
        serde_json::from_slice(&fs::read(path).expect("FIXTURE_AUDIT_SCOPE stage=cleanup"))
            .expect("FIXTURE_AUDIT_SCOPE stage=cleanup");
    assert!(
        !scope.owner.is_empty()
            && scope
                .roots
                .iter()
                .chain(&scope.active_foreign)
                .all(|p| p.is_absolute()),
        "FIXTURE_AUDIT_SCOPE stage=cleanup"
    );
    report(&scope, resources)
}
fn resources() -> Vec<Resource> {
    let mut result = processes::leftovers()
        .into_iter()
        .map(process_resource)
        .collect::<Vec<_>>();
    for id in processes::fixture_containers().expect("FIXTURE_AUDIT_DOCKER stage=cleanup") {
        let uid = kah_run::access::service_uid().unwrap();
        let output = Command::new("/usr/bin/docker")
            .args([
                "--host",
                &format!("unix:///run/user/{uid}/docker.sock"),
                "inspect",
                "--format",
                "{{json .Config.Labels}}",
                &id,
            ])
            .output()
            .unwrap();
        let labels: Value = if output.status.success() {
            serde_json::from_slice(&output.stdout).unwrap_or(Value::Null)
        } else {
            Value::Null
        };
        result.push(Resource {
            identity: json!({"kind":"container","id":id,"labels":labels}),
            paths: labels["dev.kah.project"]
                .as_str()
                .map(PathBuf::from)
                .into_iter()
                .collect(),
            owner: None,
        });
    }
    result
}
fn process_resource(p: processes::Process) -> Resource {
    let env = environment(p.pid);
    let mut paths = vec![PathBuf::from(&p.cwd)];
    paths.extend(
        env.iter()
            .filter(|(key, _)| {
                matches!(key.as_str(), "HOME" | "XDG_STATE_HOME" | "KAH_TRACK2_ROOT")
            })
            .map(|(_, value)| PathBuf::from(value)),
    );
    Resource {
        identity: json!({"kind":"process","pid":p.pid,"parent":p.parent,"group":p.group,"cwd":p.cwd}),
        paths,
        owner: env
            .into_iter()
            .find(|(key, _)| key == "KAH_GATE_AUDIT_OWNER")
            .map(|(_, value)| value),
    }
}
fn environment(pid: u32) -> Vec<(String, String)> {
    fs::read(format!("/proc/{pid}/environ"))
        .unwrap_or_default()
        .split(|b| *b == 0)
        .filter_map(|v| std::str::from_utf8(v).ok()?.split_once('='))
        .filter(|(key, _)| {
            matches!(
                *key,
                "HOME" | "XDG_STATE_HOME" | "KAH_TRACK2_ROOT" | "KAH_GATE_AUDIT_OWNER"
            )
        })
        .map(|(key, value)| (key.into(), value.into()))
        .collect()
}
