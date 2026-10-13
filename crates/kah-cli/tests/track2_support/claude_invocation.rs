//! Fixture-local shim merges settings without replacing kah's invocation hooks.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};
pub fn install(home: &Path, native: &Path) {
    let path = home.join(".local/bin/claude");
    let settings = r#"{"model":"claude-sonnet-5-5","effortLevel":"low","fastMode":false}"#;
    let script = format!(
        "#!/bin/bash\nset -eu\nargs=(\"$@\")\nfound=false\nfor ((i=0; i<${{#args[@]}}; i++)); do\n  if [ \"${{args[i]}}\" = -- ]; then break; fi\n  if [ \"${{args[i]}}\" = --settings ]; then\n    ((i+=1))\n    args[i]=$(/usr/bin/jq -ce --argjson fixture '{settings}' '. + $fixture' <<<\"${{args[i]}}\")\n    found=true\n  fi\ndone\nif ! $found; then args=(--settings '{settings}' \"${{args[@]}}\"); fi\nexport CLAUDE_CONFIG_DIR={} DISABLE_AUTOUPDATER=1\nexec {} \"${{args[@]}}\"\n",
        quote(Path::new(super::claude_login::CONFIG_DIR)),
        quote(native)
    );
    fs::write(&path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}
fn quote(path: &Path) -> String {
    format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"))
}
/// Runs a freshly written executable. Parallel tests fork while the file is
/// still open for writing; the forked child keeps that descriptor until its
/// own exec, so the kernel can answer ETXTBSY. No test can close a descriptor
/// that another thread's child holds. Retry only that error, at most 3 tries.
pub fn output_retrying_text_file_busy(mut build: impl FnMut() -> Command) -> Output {
    const ETXTBSY: i32 = 26;
    let mut tries = 1;
    loop {
        match build().output() {
            Err(error) if error.raw_os_error() == Some(ETXTBSY) && tries < 3 => {}
            Ok(out)
                if !out.status.success()
                    && String::from_utf8_lossy(&out.stderr).contains("Text file busy")
                    && tries < 3 => {}
            other => return other.unwrap(),
        }
        tries += 1;
    }
}
