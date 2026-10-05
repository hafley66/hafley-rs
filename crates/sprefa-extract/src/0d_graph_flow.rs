use super::*;

// Return edges retain their caller-local storage orientation. The walk follows values.
pub(super) const SQL: &str = "SELECT \"_row\", \
    CASE WHEN \"kind\" = 'ret_to_call_res' THEN \"to_blob\" ELSE \"from_blob\" END, \
    CASE WHEN \"kind\" = 'ret_to_call_res' THEN printf('%d:%d', \"to__start\", \"to__end\") ELSE printf('%d:%d', \"from__start\", \"from__end\") END, \
    CASE WHEN \"kind\" = 'ret_to_call_res' THEN \"from_blob\" ELSE \"to_blob\" END, \
    CASE WHEN \"kind\" = 'ret_to_call_res' THEN printf('%d:%d', \"from__start\", \"from__end\") ELSE printf('%d:%d', \"to__start\", \"to__end\") END, '~', NULL \
    FROM \"flow_edge\" \
    UNION ALL \
    SELECT \"_row\", \"_content_id\", printf('%d:%d', \"from__start\", \"from__end\"), \
    \"_content_id\", printf('%d:%d', \"to__start\", \"to__end\"), '~', NULL \
    FROM \"edge\" WHERE \"family\" = 'df' AND \"_content_id\" IS NOT NULL";

/// Normalize path seeds to content identity; digest seeds already name it.
pub(super) fn flow_seed(
    seed: &str,
    root: Option<&Path>,
) -> Result<BTreeSet<Node>, Box<dyn std::error::Error>> {
    let (blob, span) = seed
        .rsplit_once('@')
        .ok_or("flow seed must be PATH@START:END or BLOB@START:END")?;
    let (start, end) = span
        .split_once(':')
        .ok_or("flow seed must be PATH@START:END or BLOB@START:END")?;
    let start: u32 = start.parse()?;
    let end: u32 = end.parse()?;
    if start >= end {
        return Err(
            "flow seed requires START < END (zero-based byte offsets, END exclusive)".into(),
        );
    }
    let digest = blob
        .strip_prefix("blake3:")
        .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .or_else(|| {
            blob.strip_prefix("git:")
                .filter(|hex| hex.len() == 40 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        });
    let blob = if digest.is_some() {
        blob.to_ascii_lowercase()
    } else {
        let path = Path::new(blob);
        let path = match root {
            Some(root) if !path.is_absolute() => root.join(path),
            _ => path.to_path_buf(),
        };
        let mut bytes = Vec::new();
        fs::File::open(sprefa_extract::io_path(&path))
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|error| format!("flow seed input '{blob}': {error}"))?;
        if end as usize > bytes.len() {
            return Err(format!("flow seed END {end} exceeds input length {}", bytes.len()).into());
        }
        sprefa_extract::content_id_of(&bytes).to_string()
    };
    Ok(BTreeSet::from([(blob, Some(format!("{start}:{end}")))]))
}
