use crate::Result;
use kah_core::run_store::RunStore;
use kah_observer::review;
pub fn run(args: &[String]) -> Result<()> {
    if args.len() == 4 && matches!(args[2].as_str(), "--confirm" | "--dismiss" | "--promote") {
        review::apply(&args[1], args[2].trim_start_matches("--"), &args[3])?;
        println!("Recorded user decision {} for {}.", args[2], args[3]);
        return Ok(());
    }
    if !args.iter().any(|s| s == "--review") {
        return Err("REVIEW_ARGUMENTS stage=arguments: kah ledger SESSION --confirm|--dismiss|--promote ITEM".into());
    }
    let id = args.iter().skip(1).find(|s| !s.starts_with("--"));
    let sessions = crate::inspection_cli::load_sessions(&RunStore::default_state_root()?, id)?;
    let values = sessions
        .iter()
        .map(|s| review::list(&s.binding.session))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if args.iter().any(|s| s == "--json") {
        println!("{}", serde_json::to_string_pretty(&values)?);
        return Ok(());
    }
    for value in values {
        println!("Session {}", value["session"].as_str().unwrap_or("unknown"));
        for item in value["items"].as_array().unwrap() {
            println!(
                "{} [{}]\n{}\nActions: {}",
                item["id"].as_str().unwrap(),
                item["status"].as_str().unwrap(),
                item["text"].as_str().unwrap(),
                item["actions"]
            );
        }
    }
    Ok(())
}
