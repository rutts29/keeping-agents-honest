use super::policy_settings;
use crate::Arguments;
use kah_sandbox::{Domain, Project};

#[test]
fn the_overlay_model_is_the_native_model_choice() {
    // app-server probes read the overlay model; prompt text after the native
    // `--` and other options' values are not model choices.
    let project = Project {
        domain: Domain::Mine,
        root: "/fixture".into(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    for (args, expected) in [
        (vec!["--model=chosen"], Some("chosen")),
        (vec!["--model", "chosen"], Some("chosen")),
        (
            vec!["--model", "good", "--", "--", "--model=bad"],
            Some("good"),
        ),
        (vec!["--", "--image", "--model=bad"], None),
    ] {
        let args = Arguments::parse(args.iter().map(|s| s.to_string()).collect()).unwrap();
        let settings = policy_settings(&project, &args);
        assert_eq!(settings["model"].as_str(), expected, "{args:?}");
    }
}
