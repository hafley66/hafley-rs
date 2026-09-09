use base64::Engine;
use brawllib_rs::high_level_fighter::HighLevelSubaction;

type Error = Box<dyn std::error::Error>;

/// Offline Rukaidata HTML decode. Retains upstream poses and scripts.
#[tracing::instrument(target = "game_content::ingest", skip_all)]
pub fn decode_file(path: &std::path::Path) -> Result<HighLevelSubaction, Error> {
    decode_html(&std::fs::read_to_string(path)?)
}

#[tracing::instrument(target = "game_content::ingest", skip_all, fields(html_bytes = html.len()))]
pub fn decode_html(html: &str) -> Result<HighLevelSubaction, Error> {
    let (_, rest) = html.split_once("const fighter_subaction_data = \"")
        .ok_or("missing subaction data")?;
    let (payload, _) = rest.split_once('"').ok_or("unterminated subaction data")?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(payload)?;
    let (action, used): (HighLevelSubaction, usize) =
        bincode::serde::decode_from_slice(&bytes, bincode::config::standard())?;
    if used != bytes.len() {
        return Err("schema must consume the entire payload".into());
    }
    tracing::debug!(target = "game_content::ingest", action = %action.name,
        frames = action.frames.len(), payload_bytes = used);
    Ok(action)
}
