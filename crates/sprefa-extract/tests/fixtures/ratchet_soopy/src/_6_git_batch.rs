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
        let (_, bytes) = self.read_spec(&oid.0)?;
        Ok(bytes)
    }

    /// Read an expression such as `commit:path` and return Git's resolved OID.
    pub fn read_spec(&mut self, spec: &str) -> Result<(ObjectId, Arc<[u8]>)> {
        let mut bytes = Vec::new();
        let oid = self.read_spec_into(spec, &mut bytes)?;
        Ok((oid, Arc::from(bytes)))
    }

    /// Read an expression such as `commit:path` into a caller-owned buffer.
    ///
    /// The buffer is resized to the resolved blob's exact size. Reusing it
    /// across calls retains at most the largest blob allocation instead of one
    /// allocation per returned source.
    pub fn read_spec_into(&mut self, spec: &str, bytes: &mut Vec<u8>) -> Result<ObjectId> {
        writeln!(self.input, "{spec}")?;
        self.input.flush()?;
        let mut header = String::new();
        self.output.read_line(&mut header)?;
        let fields: Vec<&str> = header.split_whitespace().collect();
        if fields.len() != 3 || fields[1] != "blob" {
            bail!("git cat-file response for {spec}: {}", header.trim());
        }
        let oid = ObjectId(Arc::from(fields[0]));
        let size: usize = fields[2].parse().context("parse Git blob size")?;
        bytes.clear();
        bytes.resize(size, 0);
        self.output.read_exact(&mut *bytes)?;
        let mut newline = [0];
        self.output.read_exact(&mut newline)?;
        if newline[0] != b'\n' {
            bail!("git cat-file response missing blob terminator");
        }
        Ok(oid)
    }
}

impl Drop for GitBatch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
