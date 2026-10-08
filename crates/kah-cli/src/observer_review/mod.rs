//! One event-driven, full native evidence observer per worker session.
mod artifact_collect;
mod artifact_git;
pub mod artifacts;
pub mod capture;
pub mod commit_context;
pub mod commit_packet;
pub mod completion_due;
pub mod consume;
pub mod control;
pub mod decision_deferral_scope;
pub mod decisions;
pub mod deferrals;
pub mod dispatch;
pub mod gate_approval;
pub mod hook;
pub mod hook_queue;
pub mod indexed_sources;
pub mod launch;
pub mod light_commit;
pub mod light_report;
pub mod liveness;
pub mod packet_references;
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
pub mod service;
pub mod snapshot;
pub mod source_store;
pub mod state;
pub mod tags;
pub mod timing;
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
                    let _ = hook::gap_error(&args[1], e.as_ref());
                }),
        );
    }
    if let Some(result) = maintenance(args) {
        return Some(result);
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
        "__review-consume" | "__review-consume-supervise" => {
            consume::supervisor::supervise(&args[1])
        }
        "__review-consume-worker" => consume::run(&args[1]),
        "__gate-followup" => gate_approval::run_followups(&args[1]),
        "__gate-answers" => gate_approval::apply_answers(&args[1]),
        "__review-hook-events" => hook_queue::run(&args[1]),
        "__review-wake" => dispatch::run(&args[1]),
        "__review-kick" => queue::kick(&args[1]),
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
        let _ = hook::gap_error(&args[1], e.as_ref());
    }))
}
fn maintenance(args: &[String]) -> Option<crate::Result<()>> {
    match args {
        [name] if name == "__review-project-context" => Some(capture::project_cli(None)),
        [name, flag, head] if name == "__review-project-context" && flag == "--commit-baseline" => {
            Some(capture::project_cli(Some(head)))
        }
        [name] if name == "__review-artifact-baseline" => Some(artifact_collect::cli()),
        [name, worker] if name == "__review-history-repair" => {
            Some(capture::historical::repair(worker).map(|value| println!("{value}")))
        }
        [name, worker, authorization] if name == "__review-claims-migrate" => {
            Some(queue::claim_migration::cli(worker, authorization))
        }
        [name, worker, authorization] if name == "__review-light-migrate" => {
            Some(queue::migration::cli(worker, authorization))
        }
        [name, worker] if name == "__review-preparation-coverage" => {
            Some(queue::preparation_coverage::run(worker).map(|value| println!("{value}")))
        }
        _ => None,
    }
}
pub mod checkpoint;
pub mod checkpoint_claim;
pub mod claims;
pub mod delivery;
pub mod events;
pub mod failure;
mod failure_snapshot;
pub mod lifecycle;
pub mod pause;
pub mod steering;
mod steering_conflict;

fn observe_route(args: &[String]) -> Option<crate::Result<()>> {
    match args.get(1).map(String::as_str) {
        Some("report") => Some(workspace::submit(args)),
        Some("select") => Some(preparation::submit(args)),
        Some("select-batch") => Some(preparation::batch::submit(args)),
        Some("read-source") if args.len() == 3 => {
            Some(source_store::print_text(std::path::Path::new(&args[2])))
        }
        Some("migrate-sources") if args.len() == 3 => {
            Some(source_store::migrate(&args[2]).map(|v| println!("{v}")))
        }
        _ => None,
    }
}
