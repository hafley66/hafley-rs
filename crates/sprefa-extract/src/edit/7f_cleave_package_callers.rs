use super::*;

pub(super) fn package_callers(
    cx: &MoveCx,
    arm: &dyn Cleave,
    src: &str,
    item: &str,
    known: &[String],
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for rel in cx.files() {
        if rel == src
            || known.contains(rel)
            || cleave_for(rel).map(|other| other.name()) != Some(arm.name())
        {
            continue;
        }
        let spelled = arm.spell_module(cx, rel, src);
        let Some(text) = cx.text(rel) else {
            continue;
        };
        if !text.contains(&format!("{spelled}::{item}"))
            && !text.contains(&format!("{spelled}::{{"))
        {
            continue;
        }
        let facts = FileFacts::open(cx, rel, false)?;
        if facts
            .specifiers
            .iter()
            .any(|row| row.name == item && module_key(item, &row.module) == spelled)
        {
            out.push(rel.clone());
        }
    }
    Ok(out)
}
