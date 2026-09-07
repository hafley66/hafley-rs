//! `boop tag ...`: the shared tag table, the one place a tag is written or
//! read. Every search here hits `agent_tag`, never a message body.

use anyhow::Result;

use boop::identity;
use boop_store::tags::Tag;

use crate::cli::db::open_store;
use crate::cli::line;
use crate::TagFormat;

/// Apply every named tag to one source. An unnamed source is the caller's own
/// route when the whoami ladder answers, and `cli` when it does not.
pub(crate) fn run_tag_add(tags: &[String], source: Option<&str>) -> Result<()> {
    let source = match source {
        Some(source) => source.to_owned(),
        None => caller_source(),
    };
    let store = open_store()?;
    let now = now_ms();
    for tag in tags {
        let applied = store.tag_apply(tag, &source, now)?;
        line(&format!("{} {} {}", applied.tag, applied.uses, source));
    }
    Ok(())
}

/// The recently used tags, newest use first. Five by default: the list a
/// prompt offers without reading anyone's prose.
pub(crate) fn run_tag_recent(limit: usize, format: TagFormat) -> Result<()> {
    emit(&open_store()?.tags_recent(limit)?, format)
}

pub(crate) fn run_tag_search(query: &str, limit: usize, format: TagFormat) -> Result<()> {
    emit(&open_store()?.tags_search(query, limit)?, format)
}

pub(crate) fn run_tag_list(format: TagFormat) -> Result<()> {
    emit(&open_store()?.tags_list()?, format)
}

pub(crate) fn run_tag_of(source: &str) -> Result<()> {
    for tag in open_store()?.tags_for(source)? {
        line(&tag);
    }
    Ok(())
}

pub(crate) fn run_tag_sources(tag: &str) -> Result<()> {
    for source in open_store()?.sources_for(tag)? {
        line(&source);
    }
    Ok(())
}

pub(crate) fn run_tag_rm(tag: &str, source: &str) -> Result<()> {
    let removed = open_store()?.tag_unlink(tag, source)?;
    match removed {
        true => line(&format!("{tag} off {source}")),
        false => line(&format!("{source} carried no {tag}")),
    }
    Ok(())
}

pub(crate) fn run_tag_backfill() -> Result<()> {
    let tagged = open_store()?.tags_backfill_favorites()?;
    line(&format!("{tagged} favorites tagged"));
    Ok(())
}

/// One row per tag: tab-separated `tag uses last_used_iso`, or the rows as one
/// JSON array.
fn emit(tags: &[Tag], format: TagFormat) -> Result<()> {
    match format {
        TagFormat::Json => line(&serde_json::to_string_pretty(tags)?),
        TagFormat::Text => {
            for tag in tags {
                line(&format!(
                    "{}\t{}\t{}",
                    tag.tag,
                    tag.uses,
                    iso(tag.last_used_ts)
                ));
            }
        }
    }
    Ok(())
}

/// The route the caller stands in, spelled the way every other source is.
fn caller_source() -> String {
    let identity = identity::resolve_as(None);
    if let Some(lane) = identity.lane.filter(|lane| !lane.is_empty()) {
        return format!("lane:{lane}");
    }
    match identity.session.filter(|session| !session.is_empty()) {
        Some(session) => format!("session:{session}"),
        None => "cli".to_owned(),
    }
}

fn iso(ms: i64) -> String {
    use time::format_description::well_known::Rfc3339;
    let Ok(stamp) = time::OffsetDateTime::from_unix_timestamp_nanos(ms as i128 * 1_000_000) else {
        return ms.to_string();
    };
    stamp.format(&Rfc3339).unwrap_or_else(|_| ms.to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_millis() as i64)
        .unwrap_or(0)
}
