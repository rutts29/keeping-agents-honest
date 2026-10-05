//! Abort evidence publication if a native updater changed the launch binary.
use std::{path::PathBuf, process::Command};
#[derive(serde::Serialize)]
pub struct Version {
    binary: PathBuf,
    target: PathBuf,
    #[serde(rename = "version")]
    text: String,
}
impl Version {
    pub fn capture(agent: &str) -> Self {
        let binary = PathBuf::from(std::env::var_os("HOME").unwrap())
            .join(".local/bin")
            .join(agent);
        let target = binary.canonicalize().unwrap();
        let output = Command::new(&binary).arg("--version").output().unwrap();
        assert!(output.status.success());
        let text = String::from_utf8(output.stdout).unwrap().trim().to_owned();
        assert_eq!(target, binary.canonicalize().unwrap());
        Self {
            binary,
            target,
            text,
        }
    }
    pub fn verify(&self, reported: &serde_json::Value) {
        assert_eq!(
            self.binary.canonicalize().unwrap(),
            self.target,
            "native updater changed the fixture binary; rerun before claiming an exact version"
        );
        assert!(
            self.matches(reported),
            "reported native version differs from the captured binary"
        );
    }
    pub fn matches(&self, reported: &serde_json::Value) -> bool {
        self.binary.canonicalize().is_ok_and(|p| p == self.target)
            && reported.as_str() == Some(self.text.as_str())
    }
}
