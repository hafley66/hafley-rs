//! TypeScript 7.0.2 synchronous API transport. Each frame is one MessagePack
//! array carrying a message kind and two binary strings.

use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use rmpv::Value as MpValue;
use serde_json::{json, Value};

pub struct Ts7Api {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    pub snapshot: u64,
    pub project: String,
}

impl Ts7Api {
    pub fn open(root: &Path, file: &Path) -> Result<Self, String> {
        let tsc = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("ts7/node_modules/typescript/bin/tsc");
        if !tsc.is_file() {
            return Err(format!("TypeScript 7.0.2 executable missing: {}", tsc.display()));
        }
        let mut child = Command::new(&tsc)
            .arg("--api")
            .arg("--cwd")
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("start {}: {error}", tsc.display()))?;
        let stdin = child.stdin.take().ok_or("tsc stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("tsc stdout unavailable")?;
        let mut api = Self { child, stdin, stdout, snapshot: 0, project: String::new() };
        api.request("initialize", Value::Null)?;
        let updated = api.request("updateSnapshot", json!({"openFiles": [file]}))?;
        api.snapshot = updated["snapshot"].as_u64().ok_or("updateSnapshot missing snapshot")?;
        let found = api.request("getDefaultProjectForFile", json!({
            "snapshot": api.snapshot,
            "file": file,
        }))?;
        api.project = found["id"].as_str().ok_or("no default project for file")?.to_owned();
        Ok(api)
    }

    pub fn request(&mut self, method: &str, payload: Value) -> Result<Value, String> {
        let bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
        let frame = MpValue::Array(vec![
            MpValue::from(1),
            MpValue::Binary(method.as_bytes().to_vec()),
            MpValue::Binary(bytes),
        ]);
        rmpv::encode::write_value(&mut self.stdin, &frame)
            .map_err(|error| format!("write {method}: {error}"))?;
        self.stdin.flush().map_err(|error| format!("flush {method}: {error}"))?;
        let response = rmpv::decode::read_value(&mut self.stdout)
            .map_err(|error| format!("read {method}: {error}"))?;
        let MpValue::Array(parts) = response else { return Err(format!("{method}: response is not an array")); };
        if parts.len() != 3 || parts[0].as_u64() != Some(4) {
            return Err(format!("{method}: unexpected response frame {parts:?}"));
        }
        let response_method = binary(&parts[1]).map_err(|error| format!("{method}: {error}"))?;
        if response_method != method.as_bytes() {
            return Err(format!("{method}: response method mismatch"));
        }
        let payload = binary(&parts[2]).map_err(|error| format!("{method}: {error}"))?;
        serde_json::from_slice(payload).map_err(|error| format!("decode {method}: {error}"))
    }

    pub fn checker(&mut self, method: &str, fields: Value) -> Result<Value, String> {
        let mut payload = json!({"snapshot": self.snapshot, "project": self.project});
        let Some(fields) = fields.as_object() else { return Err("checker fields must be an object".into()); };
        payload.as_object_mut().unwrap().extend(fields.clone());
        self.request(method, payload)
    }

    pub fn symbol_at(&mut self, file: &Path, position: u32) -> Result<Value, String> {
        self.checker("getSymbolAtPosition", json!({"file": file, "position": position}))
    }

    pub fn type_at(&mut self, file: &Path, position: u32) -> Result<Value, String> {
        self.checker("getTypeAtPosition", json!({"file": file, "position": position}))
    }

    pub fn references_in_file(&mut self, file: &Path, symbol: u64) -> Result<Value, String> {
        self.checker("getReferencesToSymbolInFile", json!({"file": file, "symbol": symbol}))
    }
}

impl Drop for Ts7Api {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn binary(value: &MpValue) -> Result<&[u8], &'static str> {
    match value {
        MpValue::Binary(bytes) => Ok(bytes),
        _ => Err("expected MessagePack binary field"),
    }
}

pub fn utf16_offset(text: &str, byte_offset: usize) -> u32 {
    text[..byte_offset].encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> (Ts7Api, PathBuf, u32) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ts7_api");
        let file = root.join("0_fixture.ts");
        let text = std::fs::read_to_string(&file).unwrap();
        let byte_offset = text.rfind("old").unwrap();
        let position = utf16_offset(&text, byte_offset + 1);
        assert!(position < byte_offset as u32);
        (Ts7Api::open(&root, &file).unwrap(), file, position)
    }

    #[test]
    fn stable_api_get_type_at_position_exchange() {
        let (mut api, file, position) = fixture();
        let ty = api.type_at(&file, position).unwrap();
        assert!(ty["id"].as_u64().is_some(), "{ty}");
    }

    #[test]
    fn stable_api_get_symbol_at_position_exchange() {
        let (mut api, file, position) = fixture();
        let symbol = api.symbol_at(&file, position).unwrap();
        assert_eq!(symbol["name"], "old");
    }

    #[test]
    fn stable_api_get_references_to_symbol_in_file_exchange() {
        let (mut api, file, position) = fixture();
        let symbol = api.symbol_at(&file, position).unwrap();
        let symbol_id = symbol["id"].as_u64().unwrap();
        let refs = api.references_in_file(&file, symbol_id).unwrap();
        assert!(refs.as_array().is_some_and(|rows| rows.len() >= 2), "{refs}");
    }
}
