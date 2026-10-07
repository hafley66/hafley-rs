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

struct Source {
    path: String,
    bytes: Vec<u8>,
    offsets: Vec<u32>,
}

#[derive(Default)]
struct Location {
    file: Option<String>,
    line: Option<u32>,
    col: Option<u32>,
    text: Option<String>,
    reason: Option<String>,
}

fn location(node: &Node, sources: &BTreeMap<String, Result<Source, String>>) -> Location {
    let result = (|| -> Result<Location, String> {
        let source = sources
            .get(&node.0)
            .ok_or_else(|| "digest has no loaded file fact".to_string())?
            .as_ref()
            .map_err(Clone::clone)?;
        let span = node.1.as_deref().ok_or("flow endpoint has no span")?;
        let (start, end) = span.split_once(':').ok_or("invalid flow endpoint span")?;
        let start: u32 = start.parse().map_err(|_| "invalid flow endpoint start")?;
        let end: u32 = end.parse().map_err(|_| "invalid flow endpoint end")?;
        if start > end {
            return Err("flow endpoint START exceeds END".to_string());
        }
        let bytes = source.bytes.get(start as usize..end as usize)
            .ok_or("flow endpoint span exceeds loaded source")?;
        let text = std::str::from_utf8(bytes)
            .map_err(|_| "flow endpoint span is not valid UTF-8")?;
        let (line, col) = line_col(&source.offsets, start);
        Ok(Location {
            file: Some(source.path.clone()),
            line: Some(line),
            col: Some(col),
            text: Some(text.to_string()),
            reason: None,
        })
    })();
    result.unwrap_or_else(|reason| Location { reason: Some(reason), ..Location::default() })
}

/// Digest identity stays in the existing fields. File facts provide the exact
/// source mapping; missing or ambiguous mappings carry a reason on that endpoint.
pub(super) fn locate(
    connection: &Connection,
    rows: &mut [FlatFact],
    root: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut files: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut statement = connection.prepare("SELECT digest, path FROM file")?;
    for file in statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))? {
        let (digest, path) = file?;
        files.entry(digest).or_default().insert(path);
    }
    let mut sources: BTreeMap<String, Result<Source, String>> = BTreeMap::new();
    for row in rows.iter() {
        let FlatFact::GraphPath { from_path, to_path, .. } = row else { continue };
        for digest in [from_path, to_path] {
            if sources.contains_key(digest) {
                continue;
            }
            let source = (|| -> Result<Source, String> {
                let paths = files.get(digest).ok_or("digest has no loaded file fact")?;
                if paths.len() != 1 {
                    return Err(format!("digest maps to {} loaded paths; source path is ambiguous", paths.len()));
                }
                let path = Path::new(paths.first().expect("a mapped digest has a path"));
                let absolute = match root {
                    Some(root) if !path.is_absolute() => root.join(path),
                    _ => sprefa_extract::io_path(path),
                };
                let bytes = fs::read(&absolute).map_err(|error| format!("source read: {error}"))?;
                if sprefa_extract::content_id_of(&bytes).to_string() != *digest {
                    return Err("source content does not match loaded file digest".to_string());
                }
                Ok(Source {
                    path: path.to_str().ok_or("source path is not valid UTF-8")?.to_string(),
                    offsets: newline_offsets(&bytes),
                    bytes,
                })
            })();
            sources.insert(digest.clone(), source);
        }
    }
    for row in rows {
        let FlatFact::GraphPath {
            from_path, from_name, to_path, to_name,
            from_file, from_line, from_col, from_text, from_reason,
            to_file, to_line, to_col, to_text, to_reason, ..
        } = row else { continue };
        let from = location(&(from_path.clone(), from_name.clone()), &sources);
        let to = location(&(to_path.clone(), to_name.clone()), &sources);
        (*from_file, *from_line, *from_col, *from_text, *from_reason) =
            (from.file, from.line, from.col, from.text, from.reason);
        (*to_file, *to_line, *to_col, *to_text, *to_reason) =
            (to.file, to.line, to.col, to.text, to.reason);
    }
    Ok(())
}
