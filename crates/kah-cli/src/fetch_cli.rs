use crate::Result;
use std::{io::Write, path::PathBuf};
pub fn run(args: &[String]) -> Result<()> {
    if args.len() != 2 {
        return Err("FETCH_ARGUMENTS stage=arguments: kah __fetch HTTPS_URL".into());
    }
    let directory = PathBuf::from(
        std::env::var_os("KAH_FETCH_DIR").ok_or("FETCH_NOT_ENABLED stage=fetch_client")?,
    );
    let bytes = kah_run::fetch_exchange::request(&directory, &args[1])?;
    std::io::stdout()
        .write_all(&bytes)
        .map_err(|e| format!("FETCH_OUTPUT stage=fetch_client: {e}"))?;
    Ok(())
}
