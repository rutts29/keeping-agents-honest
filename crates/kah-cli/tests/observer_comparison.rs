use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "summarize freshly rerun Jev/Luna live journey evidence; gate only"]
fn compare_live_judge_backends() {
    let directory =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/gates/evidence/phase-3");
    let mut rows = vec![];
    for backend in ["jev", "luna"] {
        let continuity: Value = serde_json::from_slice(
            &fs::read(directory.join(format!("continuity-{backend}.json"))).unwrap(),
        )
        .unwrap();
        let drift: Value = serde_json::from_slice(
            &fs::read(directory.join(format!("drift-{backend}.json"))).unwrap(),
        )
        .unwrap();
        if continuity["passed"] != true || drift["passed"] != true {
            rows.push(json!({"backend":backend,"continuityPassed":continuity["passed"],"driftPassed":drift["passed"],"comparisonComplete":false}));
            assert_eq!(backend, "jev", "the required Luna journeys must pass");
            continue;
        }
        let judgments = continuity["finalState"]["judgments"]
            .as_array()
            .unwrap()
            .iter()
            .chain(drift["judgments"].as_array().unwrap())
            .collect::<Vec<_>>();
        assert!(
            judgments
                .iter()
                .all(|j| j["backend"] == backend && j["fallback_reason"].is_null())
        );
        let labels = classifications(&continuity);
        let correct = labels.iter().filter(|v| v["correct"] == true).count();
        rows.push(json!({
            "backend":backend,"questionSetVersion":kah_observer::judge::questions::VERSION,
            "journeysPassed":4,"falseReminders":continuity["intentionalScopeChangeReminders"],
            "missedDrift":u32::from(drift["reminders"]!=1),"driftReminders":drift["reminders"],
            "labels":labels,"labeledQuestionsCorrect":correct,"labeledQuestionsTotal":labels.len(),
            "judgeMetrics":metrics(&judgments),"observerTotalInputTokens":number(&continuity["finalState"]["usage"],"input_tokens")+number(&drift["usage"],"input_tokens"),
            "observerTotalOutputTokens":number(&continuity["finalState"]["usage"],"output_tokens")+number(&drift["usage"],"output_tokens"),
            "observerTotalElapsedMs":number(&continuity["finalState"]["usage"],"elapsed_ms")+number(&drift["usage"],"elapsed_ms"),
            "continuityWallMs":continuity["wallElapsedMs"],"driftWallMs":drift["wallElapsedMs"]
        }));
    }
    let report = json!({"rows":rows,"scope":"Small live fixtures, not a broad accuracy or calibration benchmark. Both conditions include separate source-linked Luna text extraction. Codex-login dollar cost is unavailable; Jev cost is a published-rate estimate. Latencies are per judgment; total observer time includes text extraction.","passed":true});
    fs::write(
        directory.join("backend-comparison.json"),
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
    eprintln!("{}", serde_json::to_string_pretty(&report).unwrap());
}

fn classifications(continuity: &Value) -> Vec<Value> {
    let mut labels = vec![];
    for (stage, expected) in [(1, "correction"), (2, "scope_expansion")] {
        let latest = continuity["stages"][stage]["judgments"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        let kind = &latest["answers"]["steer_kind"];
        let steering = &latest["answers"]["is_steering"];
        labels.push(json!({"stage":stage,"question":"steer_kind","expected":expected,"actual":kind["choice"],"confidence":kind["confidence"],"correct":kind["choice"]==expected}));
        labels.push(json!({"stage":stage,"question":"is_steering","expected":true,"probability":steering["noul"],"correct":steering["noul"].as_f64().unwrap()>=0.8}));
    }
    labels
}
fn metrics(judgments: &[&Value]) -> Value {
    let mut latency = judgments
        .iter()
        .map(|j| number(&j["metrics"], "elapsed_ms"))
        .collect::<Vec<_>>();
    latency.sort();
    let input = judgments
        .iter()
        .map(|j| number(&j["metrics"], "input_tokens"))
        .sum::<u64>();
    let output = judgments
        .iter()
        .map(|j| number(&j["metrics"], "output_tokens"))
        .sum::<u64>();
    let costs = judgments
        .iter()
        .map(|j| j["metrics"]["estimated_cost_usd"].as_f64())
        .collect::<Option<Vec<_>>>();
    let calls = judgments
        .iter()
        .map(|j| j["metrics"]["calls"].as_u64().unwrap_or(1))
        .sum::<u64>();
    json!({"judgments":judgments.len(),"calls":calls,"inputTokens":input,"outputTokens":output,"estimatedCostUsd":costs.map(|v|v.iter().sum::<f64>()),"p50LatencyMs":latency[(latency.len()-1)/2],"p95LatencyMs":latency[(latency.len()*95).div_ceil(100)-1],"latenciesMs":latency})
}
fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap()
}
