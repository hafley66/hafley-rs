use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::ObjectId;

pub struct GitBatch {
    child: Child,
    input: ChildStdin,
    output: BufReader<std::process::ChildStdout>,
}
impl GitBatch {
    pub fn open(root: &std::path::Path) -> Result<Self> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("start git cat-file --batch")?;
        let input = child.stdin.take().context("open cat-file stdin")?;
        let output = BufReader::new(child.stdout.take().context("open cat-file stdout")?);
        Ok(Self {
            child,
            input,
            output,
        })
    }

    pub fn read(&mut self, oid: &ObjectId) -> Result<Arc<[u8]>> {
        writeln!(self.input, "{}", oid.0)?;
        self.input.flush()?;
        let mut header = String::new();
        self.output.read_line(&mut header)?;
        let fields: Vec<&str> = header.split_whitespace().collect();
        if fields.len() != 3 || fields[1] != "blob" {
            bail!("git cat-file response for {}: {}", oid.0, header.trim());
        }
        let size: usize = fields[2].parse().context("parse Git blob size")?;
        let mut bytes = vec![0; size];
        self.output.read_exact(&mut bytes)?;
        let mut newline = [0];
        self.output.read_exact(&mut newline)?;
        if newline[0] != b'\n' {
            bail!("git cat-file response missing blob terminator");
        }
        Ok(Arc::from(bytes))
    }
}

impl Drop for GitBatch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
