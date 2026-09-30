use crate::Result;
use kah_sandbox::{Domain, LaunchOptions};

#[derive(Debug, Default)]
pub struct Arguments {
    pub project: Option<(Domain, String)>,
    pub options: LaunchOptions,
    pub native: Vec<String>,
    pub effort: Option<String>,
    pub help: bool,
    pub no_observer: bool,
    pub observer_model: Option<String>,
    pub observer_effort: Option<String>,
    pub observer_judge: Option<String>,
    pub jev_model: Option<String>,
}

impl Arguments {
    pub fn parse(input: Vec<String>) -> Result<Self> {
        let mut result = Self::default();
        let mut args = input.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--online" => result.options.online = true,
                "--yolo" => result.options.yolo = true,
                "--no-observer" => result.no_observer = true,
                "--observer-model" => result.observer_model = Some(value(&mut args, &arg)?),
                "--observer-effort" => result.observer_effort = Some(value(&mut args, &arg)?),
                "--observer-judge" => result.observer_judge = Some(value(&mut args, &arg)?),
                "--jev-model" => result.jev_model = Some(value(&mut args, &arg)?),
                "--help" | "-h" => result.help = true,
                "--" => {
                    result.native.extend(args);
                    break;
                }
                "--model" | "-m" => result
                    .native
                    .extend(["--model".into(), value(&mut args, &arg)?]),
                "--effort" => result.effort = Some(value(&mut args, &arg)?),
                "mine" | "research" | "untrusted" if result.project.is_none() => {
                    let domain = Domain::ALL.into_iter().find(|d| d.as_str() == arg).unwrap();
                    result.project = Some((domain, value(&mut args, &arg)?));
                }
                _ => {
                    result.native.push(arg);
                    result.native.extend(args);
                    break;
                }
            }
        }
        Ok(result)
    }

    pub fn validate_native(&self, domain: Domain) -> Result<()> {
        let mut args = self.native.iter();
        while let Some(arg) = args.next() {
            if arg.starts_with("--remote") {
                return Err("NATIVE_REMOTE_POLICY stage=arguments: a remote app server cannot adopt this launch's isolated configuration".into());
            }
            match arg.as_str() {
                "--" => break,
                "-m" | "--model" | "-i" | "--image" => {
                    next(&mut args, arg)?;
                }
                "-c" | "--config" => {
                    let setting = next(&mut args, arg)?;
                    if domain != Domain::Mine
                        && setting.split_once('=').map(|p| p.0) != Some("model_reasoning_effort")
                    {
                        return Err("NATIVE_CONFIG_POLICY stage=arguments: restricted domains accept only model/effort overrides".into());
                    }
                }
                "resume" | "fork" | "--all" | "--last" | "--no-alt-screen" | "--no-daemon"
                | "--help" => (),
                "agents" | "exec" | "e" | "review" | "app-server" | "exec-server" | "sandbox"
                | "mcp" | "plugin" | "remote-control" | "apply" | "a" | "cloud" => {
                    return Err("NATIVE_COMMAND_POLICY stage=arguments: kah interactive launch accepts prompts, resume and fork; use kah run for unattended work".into());
                }
                _ if domain != Domain::Mine && arg.starts_with('-') => {
                    return Err(format!("NATIVE_FLAG_POLICY stage=arguments: {arg} could bypass restricted policy; use kah's model, effort and online options").into());
                }
                _ => (),
            }
        }
        Ok(())
    }
}

fn value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String> {
    args.next()
        .ok_or_else(|| format!("ARGUMENT_REQUIRED stage=arguments: {option} needs a value").into())
}
fn next<'a>(args: &mut impl Iterator<Item = &'a String>, option: &str) -> Result<&'a str> {
    args.next()
        .map(String::as_str)
        .ok_or_else(|| format!("ARGUMENT_REQUIRED stage=arguments: {option} needs a value").into())
}
