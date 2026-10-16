//! Cold launch progress is native pane activity, not elapsed time alone.
use super::{runtime, state::State};
use serde_json::{Value, json};

/// True while the owned observer pane shows a native turn running or a new
/// native phase. A slow but working launch keeps its binding window open.
pub(super) fn observed(state: &State, last: &mut Option<Value>) -> bool {
    let Some(view) = view(state) else {
        return false;
    };
    let phase = json!([view["agent"], view["agent_status"], view["agent_session"]]);
    let changed = last.as_ref() != Some(&phase);
    *last = Some(phase);
    changed || view["agent_status"] == "working"
}

fn view(state: &State) -> Option<Value> {
    let (socket, _) = runtime::transport(state).ok()?;
    let pane = state.observer.pane.as_deref()?;
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane]).ok()?;
    Some(view["result"]["pane"].clone())
}
