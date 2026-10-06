//! One event-driven, full native evidence observer per worker session.
mod artifact_collect;
mod artifact_git;
pub mod artifacts;
pub mod capture;
pub mod completion_due;
pub mod consume;
pub mod control;
pub mod decisions;
pub mod deferrals;
pub mod dispatch;
pub mod gate_approval;
pub mod hook;
pub mod hook_queue;
pub mod launch;
pub mod liveness;
pub mod preparation;
mod prepared_result;
pub mod questions;
pub mod queue;
pub mod recovery;
pub mod redelivery;
pub mod reminder;
pub mod reminder_queue;
pub mod request_context;
pub mod runtime;
pub mod snapshot;
pub mod source_store;
pub mod state;
pub mod tags;
pub mod uncertainty;
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
    if args == ["__review-artifact-baseline"] {
        return Some(artifact_collect::cli());
    }
    if args.len() == 2 && args[0] == "__review-history-repair" {
        return Some(capture::historical::repair(&args[1]).map(|value| println!("{value}")));
    }
    if args.first().is_some_and(|arg| arg == "observe") {
        return observe_route(args);
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
        "__gate-followup" => gate_approval::run_followups(&args[1]),
        "__gate-answers" => gate_approval::apply_answers(&args[1]),
        "__review-hook-events" => hook_queue::run(&args[1]),
        "__review-wake" => dispatch::run(&args[1]),
        "__checkpoint-worker" => events::run(&args[1]),
        "__checkpoint-claims" => claims::run(&args[1]),
        "__checkpoint-tags" => tags::run(&args[1]),
        "__review-resume" => runtime::resume(&args[1]),
        "__review-retry" => runtime::retry_pending(&args[1]),
        "__review-liveness" => liveness::run(&args[1]),
        "__review-liveness-watch" => liveness::watch(&args[1]),
        "__review-liveness-sample" => liveness::run(&args[1]),
        "__review-recover" => recovery::run(&args[1]).map(|value| println!("{value}")),
        "__review-questions" => questions::deliver(&args[1]),
        "__review-rethink" => uncertainty::resume(&args[1]),
        _ => return None,
    };
    Some(result.inspect_err(|e| {
        let _ = hook::gap(&args[1], e);
    }))
}
pub mod checkpoint;
pub mod checkpoint_claim;
pub mod claims;
pub mod delivery;
pub mod events;
pub mod lifecycle;
pub mod pause;
pub mod steering;
mod steering_conflict;

fn observe_route(args: &[String]) -> Option<crate::Result<()>> {
    match args.get(1).map(String::as_str) {
        Some("report") => Some(workspace::submit(args)),
        Some("select") => Some(preparation::submit(args)),
        Some("read-source") if args.len() == 3 => {
            Some(source_store::print_text(std::path::Path::new(&args[2])))
        }
        Some("migrate-sources") if args.len() == 3 => {
            Some(source_store::migrate(&args[2]).map(|v| println!("{v}")))
        }
        _ => None,
    }
}
