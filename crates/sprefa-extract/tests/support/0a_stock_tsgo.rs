//! Stock executable selection and portable paths in complete-stream snapshots.
use serde_json::Value;

pub fn executable() -> String {
    sprefa_extract::lang::ts_lib::typescript_executable(None)
        .expect("npm ci in ts7 installs the stock native compiler")
        .to_string_lossy()
        .into_owned()
}

pub fn normalize(value: Value) -> Value {
    let mut serialized = serde_json::to_string(&value).unwrap();
    if let Some(executable) = sprefa_extract::lang::ts_lib::typescript_executable(None) {
        serialized = serialized.replace(executable.parent().unwrap().to_str().unwrap(), "$TS_LIB");
    }
    serialized = serialized.replace(env!("CARGO_MANIFEST_DIR"), "$CRATE");
    serde_json::from_str(&serialized).unwrap()
}
