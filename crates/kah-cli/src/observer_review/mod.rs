//! One event-driven, full native evidence observer per worker session.
pub mod capture;
pub mod consume;
pub mod control;
pub mod dispatch;
pub mod hook;
pub mod launch;
pub mod runtime;
pub mod state;
pub mod workspace;

pub fn command(args: &[String]) -> Option<crate::Result<()>> {
    route(args).map(|result| {
        result.map_err(|error| {
            if error.to_string().contains(" stage=") {
                error
            } else {
                format!("REVIEW_IO stage=command: {error}").into()
            }
        })
    })
}
fn route(args: &[String]) -> Option<crate::Result<()>> {
    if args.first().is_some_and(|s| s == "gate") {
        return Some(events::cli(args));
    }
    if args.len() == 4 && args[0] == "__review-watch" {
        return Some(
            args[2]
                .parse::<u32>()
                .map_err(|e| e.into())
                .and_then(|pid| lifecycle::watch(&args[1], pid, &args[3]))
                .inspect_err(|e| {
                    let _ = hook::gap(&args[1], e);
                }),
        );
    }
    if args == ["__review-project-context"] {
        return Some(capture::project_cli());
    }
    if args.get(..2) == Some(&["observe".into(), "report".into()]) {
        return Some(workspace::submit(args));
    }
    if args.len() != 2 {
        return None;
    }
    let result = match args[0].as_str() {
        "observer" => control::focus(&args[1]),
        "__review-hook" => {
            hook::run(&args[1]);
            Ok(())
        }
        "__review-consume" => consume::run(&args[1]),
        "__review-wake" => dispatch::run(&args[1]),
        "__checkpoint-worker" => events::run(&args[1]),
        "__checkpoint-claims" => claims::run(&args[1]),
        "__review-resume" => runtime::resume(&args[1]),
        _ => return None,
    };
    Some(result.inspect_err(|e| {
        let _ = hook::gap(&args[1], e);
    }))
}
pub mod checkpoint;
pub mod claims;
pub mod delivery;
pub mod events;
pub mod lifecycle;
pub mod pause;
pub mod steering;
