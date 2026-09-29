use crate::{Domain, Error, Project};
use std::{
    fs::{DirBuilder, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, symlink},
    process::{Command, Output},
};

const GUIDANCE: &str = include_str!("../../../assets/sandbox/cx-project-guidance.md");

impl Project {
    /// Reserve the new directory exclusively. A failed initialization is left
    /// visible for repair; it never deletes or overwrites another project.
    pub fn initialize(&self) -> Result<(), Error> {
        let mode = if self.domain == Domain::Mine {
            0o700
        } else {
            0o2770
        };
        DirBuilder::new()
            .mode(mode)
            .create(&self.root)
            .map_err(|e| {
                Error::new(
                    if e.kind() == std::io::ErrorKind::AlreadyExists {
                        "PROJECT_EXISTS"
                    } else {
                        "PROJECT_CREATE"
                    },
                    "project_init",
                    e,
                )
            })?;
        self.write_guidance()?;
        self.git(&["init", "--quiet", "--template=", "--initial-branch=main"])?;
        if self.domain != Domain::Mine {
            self.git(&["config", "core.sharedRepository", "group"])?;
        }
        for (key, fallback) in [("user.name", "kah"), ("user.email", "kah@localhost")] {
            let output = Command::new("git")
                .args(["config", "--global", "--get", key])
                .output()
                .map_err(init_error)?;
            let value = if output.status.success() {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            } else {
                fallback.into()
            };
            self.git(&["config", key, &value])?;
        }
        self.git(&["add", "--", "AGENTS.md", "CLAUDE.md"])?;
        self.git(&[
            "commit",
            "--quiet",
            "--no-gpg-sign",
            "-m",
            "chore: initialize project guidance",
        ])?;
        Ok(())
    }

    fn write_guidance(&self) -> Result<(), Error> {
        let guidance = GUIDANCE
            .replace("selected CX route", "selected kah route")
            .replace(
                "selected bind at /workspace",
                "selected project at its original absolute path",
            )
            .replace(
                "`git -c safe.directory=/workspace -C /workspace ...`",
                "`git -c safe.directory=\"$PWD\" ...` from the project root",
            );
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(if self.domain == Domain::Mine {
                0o600
            } else {
                0o660
            })
            .open(self.root.join("AGENTS.md"))
            .map_err(init_error)?;
        file.write_all(guidance.as_bytes()).map_err(init_error)?;
        symlink("AGENTS.md", self.root.join("CLAUDE.md")).map_err(init_error)?;
        Ok(())
    }

    fn git(&self, args: &[&str]) -> Result<Output, Error> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .map_err(init_error)?;
        if !output.status.success() {
            return Err(Error::new(
                "PROJECT_GIT",
                "project_init",
                String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(output)
    }
}

fn init_error(error: std::io::Error) -> Error {
    Error::new("PROJECT_INIT_IO", "project_init", error)
}
