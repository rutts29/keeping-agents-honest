//! Jobs whose input stays true while they own it (finding #21) settle on spawn
//! requests: hook::spawn records one before its try_lock, and the running job
//! takes it after each pass.
use kah_cli::observer_review::{hook, job_lease, state::Store};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

const KICK: &str = "__review-kick";
const CHILD: &str = "KAH_JOB_REQUESTS_CHILD";

/// Reruns the test with a private HOME whose `.local/bin/kah` only logs the
/// job name, so a spawn that wins its lock starts no real job.
fn in_private_home(name: &str) -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    if let Some(home) = std::env::var_os(CHILD) {
        return Some(home.into());
    }
    let home = std::env::temp_dir().join(format!("job-requests-{}", kah_cli::relay_store::nonce()));
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(
        bin.join("kah"),
        "#!/bin/sh\necho \"$1\" >> \"$HOME/spawned.log\"\n",
    )
    .unwrap();
    fs::set_permissions(bin.join("kah"), fs::Permissions::from_mode(0o700)).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(CHILD, &home)
        .env("HOME", &home)
        .status()
        .unwrap();
    fs::remove_dir_all(&home).unwrap();
    assert!(status.success());
    None
}

struct Job {
    worker: String,
    root: PathBuf,
}
impl Job {
    fn new(name: &str) -> Self {
        let worker = format!("{name}-{}", std::process::id());
        let root = Store::directory(&worker).unwrap();
        kah_core::run_store::ensure_private_directory(&root).unwrap();
        Self { worker, root }
    }
    fn open(&self, command: &str) -> fs::File {
        fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join(format!("job-{command}.lock")))
            .unwrap()
    }
    fn marker(&self, command: &str) -> PathBuf {
        self.root.join(format!("job-{command}.wake"))
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.root.parent().unwrap());
    }
}

#[test]
fn a_spawn_during_the_pass_causes_exactly_one_more_pass() {
    let job = Job::new("requests-during-pass");
    let lease = job.open(KICK);
    lease.try_lock().unwrap();
    let mut passes = 0;
    job_lease::until_requested(&job.worker, KICK, &lease, || {
        passes += 1;
        if passes == 1 {
            // The running job holds the lock, so this spawn starts nothing.
            hook::spawn(KICK, &job.worker).unwrap();
            assert!(job.marker(KICK).exists());
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(passes, 2);
    assert!(!job.marker(KICK).exists());
}

#[test]
fn the_request_of_the_spawn_that_started_the_job_causes_no_second_pass() {
    let job = Job::new("requests-own-spawn");
    let lease = job.open(KICK);
    job_lease::request(&job.worker, KICK, &lease).unwrap();
    lease.try_lock().unwrap();
    let mut passes = 0;
    job_lease::until_requested(&job.worker, KICK, &lease, || {
        // The request is cleared before the pass reads any input.
        assert!(!job.marker(KICK).exists());
        passes += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(passes, 1);
}

// until_requested is take_request, then until_settled with take_request as
// its recheck. These two tests put a spawn into the window between the
// unlock and the final take, which no pass can reach.
#[test]
fn a_spawn_that_wins_the_lock_after_the_unlock_owns_the_request_alone() {
    let job = Job::new("requests-spawn-wins");
    let lease = job.open(KICK);
    lease.try_lock().unwrap();
    let spawned = job.open(KICK);
    let mut first = 0;
    let mut window = true;
    job_lease::until_settled(
        &lease,
        || {
            first += 1;
            Ok(())
        },
        || {
            if std::mem::take(&mut window) {
                // hook::spawn: the request, then a try_lock that wins.
                job_lease::request(&job.worker, KICK, &spawned).unwrap();
                spawned.try_lock().unwrap();
            }
            job_lease::take_request(&job.worker, KICK)
        },
    )
    .unwrap();
    assert_eq!(first, 1, "the old job gives the request to the new one");
    let mut second = 0;
    job_lease::until_requested(&job.worker, KICK, &spawned, || {
        second += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(second, 1, "one job handles the request, with no spin");
}

#[test]
fn a_spawn_that_loses_the_lock_after_the_unlock_gets_one_more_pass() {
    let job = Job::new("requests-job-wins");
    let lease = job.open(KICK);
    lease.try_lock().unwrap();
    let spawner = job.open(KICK);
    let mut passes = 0;
    let mut window = true;
    job_lease::until_settled(
        &lease,
        || {
            passes += 1;
            if passes == 2 {
                // The spawn's try_lock now fails; it started no job.
                assert!(spawner.try_lock().is_err());
            }
            Ok(())
        },
        || {
            if std::mem::take(&mut window) {
                job_lease::request(&job.worker, KICK, &spawner).unwrap();
            }
            job_lease::take_request(&job.worker, KICK)
        },
    )
    .unwrap();
    assert_eq!(passes, 2);
    assert!(!job.marker(KICK).exists());
}

#[test]
fn only_the_listed_jobs_record_requests() {
    let job = Job::new("requests-listed");
    let tags = job.open("__checkpoint-tags");
    tags.try_lock().unwrap();
    hook::spawn("__checkpoint-tags", &job.worker).unwrap();
    assert!(!job.marker("__checkpoint-tags").exists());
}

#[test]
fn a_job_knows_its_own_lease() {
    let job = Job::new("requests-own-lease");
    let lease = job.open(KICK);
    assert!(job_lease::same(&lease, &job.open(KICK)).unwrap());
    assert!(!job_lease::same(&lease, &job.open("__gate-host")).unwrap());
}

type Pass<'a> = Box<dyn FnMut() -> kah_cli::Result<()> + 'a>;

/// The site's job settles on a spawn of itself made during its pass.
fn settles(
    name: &str,
    command: &str,
    run_with: impl FnOnce(&str, &fs::File, Pass<'_>) -> kah_cli::Result<()>,
) {
    let job = Job::new(name);
    let lease = job.open(command);
    lease.try_lock().unwrap();
    let passes = std::cell::Cell::new(0);
    let pass = || {
        passes.set(passes.get() + 1);
        if passes.get() == 1 {
            hook::spawn(command, &job.worker)?;
        }
        Ok(())
    };
    run_with(&job.worker, &lease, Box::new(pass)).unwrap();
    assert_eq!(
        passes.get(),
        2,
        "{command} stranded a spawn made during its pass"
    );
}

#[test]
fn the_kick_job_settles_on_a_spawn_during_its_pass() {
    settles("kick-settles", KICK, |w, l, p| {
        kah_cli::observer_review::dispatch::kick_with(w, l, p)
    });
}

#[test]
fn the_gate_host_job_settles_on_a_spawn_during_its_pass() {
    settles("gate-host-settles", "__gate-host", |w, l, p| {
        kah_cli::observer_review::gate_host::run_with(w, l, p)
    });
}

#[test]
fn the_gate_followup_job_settles_on_a_spawn_during_its_pass() {
    settles("gate-followup-settles", "__gate-followup", |w, l, p| {
        kah_cli::observer_review::gate_approval::run_followups_with(w, l, p)
    });
}

#[test]
fn the_questions_job_settles_on_a_spawn_during_its_pass() {
    settles("questions-settles", "__review-questions", |w, l, p| {
        kah_cli::observer_review::questions::deliver_with(w, l, p)
    });
}

#[test]
fn an_empty_stop_during_the_claims_pass_causes_one_more_pass() {
    // A Stop with no assistant text writes no claim. Only its spawn request
    // makes the running job check the delivered-change review again.
    let claims = "__checkpoint-claims";
    let job = Job::new("claims-empty-stop");
    let lease = job.open(claims);
    lease.try_lock().unwrap();
    let mut passes = 0;
    kah_cli::observer_review::claims::run_with(&job.worker, &lease, || {
        passes += 1;
        if passes == 1 {
            kah_cli::observer_review::claims::schedule(&job.worker, "2", "")?;
            assert!(!job_lease::has_json(&job.root.join("claim-events"))?);
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(passes, 2, "the empty Stop was dropped");
    assert!(!job.marker(claims).exists());
}

#[test]
fn a_spawn_runs_when_its_request_cannot_be_written() {
    let name = "a_spawn_runs_when_its_request_cannot_be_written";
    let Some(home) = in_private_home(name) else {
        return;
    };
    let job = Job::new("requests-unwritable");
    for kind in ["lock", "log"] {
        fs::write(job.root.join(format!("job-{KICK}.{kind}")), "").unwrap();
    }
    fs::set_permissions(&job.root, fs::Permissions::from_mode(0o500)).unwrap();
    let spawned = hook::spawn(KICK, &job.worker);
    fs::set_permissions(&job.root, fs::Permissions::from_mode(0o700)).unwrap();
    spawned.unwrap();
    assert!(!job.marker(KICK).exists());
    let log = home.join("spawned.log");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !fs::read_to_string(&log).is_ok_and(|l| l.contains(KICK)) {
        assert!(Instant::now() < deadline, "the spawn did not start the job");
        std::thread::sleep(Duration::from_millis(50));
    }
}
