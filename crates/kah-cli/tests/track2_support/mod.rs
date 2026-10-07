mod config_audit;
mod fixture;
mod journey;
mod launcher;
pub use launcher::launch;
mod perturb;
mod proof;
mod proof_contract;
mod proof_hooks;
mod proof_inputs;
mod proof_journey;
mod proof_probe;
mod proof_replay;
mod scenarios;
mod startup;
mod telemetry;
mod trust;
mod usage;
#[allow(unused_imports)]
pub use perturb::write_run_store;
use serde_json::json;
use std::path::PathBuf;
#[allow(unused_imports)]
pub use trust::consent as private_consent;
#[allow(unused_imports)]
pub use trust::preseed_claude;

pub fn entrypoint(mode: &str) {
    let root = PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
    let run = std::panic::catch_unwind(|| journey::run(&root, mode));
    let result=run.unwrap_or_else(|_| json!({"passed":false,"reason":"TRACK2_PANIC","stage":"journey","retained":root.join("helper.log")}));
    fixture::save(&root.join("result.json"), &result);
}
