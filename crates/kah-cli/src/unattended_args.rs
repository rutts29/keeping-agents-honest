use kah_core::unattended::Limits;
use kah_sandbox::Domain;

#[derive(Default)]
pub struct RunArguments {
    pub online: bool,
    pub agent: Option<String>,
    pub task: String,
    pub parent: Option<String>,
    pub test: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub observer_model: Option<String>,
    pub observer_effort: Option<String>,
    pub jev_model: Option<String>,
    pub domain: Option<Domain>,
    pub limits: Limits,
}
impl RunArguments {
    pub fn parse(input: Vec<String>) -> Result<Self, String> {
        let mut result = Self::default();
        let mut args = input.into_iter();
        while let Some(arg) = args.next() {
            if arg == "--online" {
                result.online = true;
                continue;
            }
            if arg.starts_with("--") {
                let value = args
                    .next()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or("RUN_OPTION_VALUE stage=arguments")?;
                match arg.as_str() {
                    "--agent" if matches!(value.as_str(), "codex" | "claude") => {
                        result.agent = Some(value)
                    }
                    "--model" => result.model = Some(value),
                    "--parent" => result.parent = Some(value),
                    "--effort" => result.effort = Some(value),
                    "--observer-model" => result.observer_model = Some(value),
                    "--observer-effort" => result.observer_effort = Some(value),
                    "--jev-model" => result.jev_model = Some(value),
                    "--allow-host" => return Err("RUN_NETWORK_POLICY stage=arguments: use --online; the fetch broker was retired".into()),
                    "--test" => result.test = value,
                    "--domain" => {
                        result.domain = Some(
                            Domain::ALL
                                .into_iter()
                                .find(|d| d.as_str() == value)
                                .ok_or("RUN_DOMAIN stage=arguments")?,
                        )
                    }
                    "--budget-tokens" => result.limits.tokens = Some(number(&value)?),
                    "--time-limit" => result.limits.seconds = Some(number(&value)?),
                    _ => return Err(format!("RUN_OPTION stage=arguments: {arg}")),
                }
            } else if result.task.is_empty() {
                result.task = arg;
            } else {
                return Err(
                    "RUN_TASK_ARGUMENTS stage=arguments: quote the task as one argument".into(),
                );
            }
        }
        if result.task.trim().is_empty() || result.test.trim().is_empty() {
            return Err(
                "RUN_TASK_AND_TEST_REQUIRED stage=arguments: kah run --test COMMAND TASK".into(),
            );
        }
        Ok(result)
    }
}
fn number(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| "RUN_LIMIT stage=arguments: expected a positive integer".into())
}
