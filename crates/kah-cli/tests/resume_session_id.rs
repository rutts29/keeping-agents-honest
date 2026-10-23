//! Which argument names the saved session to resume: the scanner honors the
//! native option table and the native `--`.
use kah_cli::{
    native_options::{Take, takes},
    resume::session_id,
};

const ID: &str = "00000000-0000-4000-8000-000000000001";

fn find(args: &[&str]) -> Option<String> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    session_id(&args).map(str::to_owned)
}

#[test]
fn a_resume_or_fork_subcommand_names_its_session() {
    for args in [
        vec!["resume", ID],
        vec!["fork", ID],
        vec!["-p", "work", "resume", ID],
        vec!["--model", "chosen", "resume", ID],
        vec![
            "-m", "chosen", "--config", "a=b", "-i", "shot.png", "resume", ID,
        ],
        vec!["--profile=work", "-C", "/tmp", "--search", "resume", ID],
        vec!["-mchosen", "resume", ID],
    ] {
        assert_eq!(find(&args).as_deref(), Some(ID), "{args:?}");
    }
}

#[test]
fn a_session_id_that_is_a_value_or_follows_the_terminator_is_not_a_target() {
    for args in [
        vec!["-p", "resume", ID],
        vec!["--profile", "resume", ID],
        vec!["--", "resume", ID],
        vec!["--search", "--", "resume", ID],
        vec!["fix the build", "resume", ID],
        vec!["-s", "fork", ID],
        vec!["resume", "--last"],
        vec!["resume", "not-a-session-id"],
    ] {
        assert_eq!(find(&args), None, "{args:?}");
    }
}

#[test]
fn the_option_table_tells_where_a_value_lives() {
    for (arg, take) in [
        ("--profile", Take::Next),
        ("-p", Take::Next),
        ("-m", Take::Next),
        ("--model", Take::Next),
        ("--profile=work", Take::Inline),
        ("-pwork", Take::Inline),
        ("-m=chosen", Take::Inline),
        ("--last", Take::No),
        ("--search", Take::No),
        ("--", Take::No),
        ("-", Take::No),
        ("resume", Take::No),
        ("-h", Take::No),
    ] {
        assert_eq!(takes(arg), take, "{arg}");
    }
}
