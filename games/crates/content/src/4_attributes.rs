//! Numeric rows emitted by Rukaidata's attributes page. Offline only.
use std::collections::BTreeMap;

pub fn attributes(html: &str) -> Result<BTreeMap<String, f32>, String> {
    let mut values = BTreeMap::new();
    for row in html.split("<tr>").skip(1) {
        let mut cells = row.split("<td>").skip(1);
        let name = cells.next().and_then(|s| s.split_once("</td>"))
            .ok_or("attribute name cell missing")?.0.trim_end_matches(':');
        let value = cells.next().and_then(|s| s.split_once("</td>"))
            .ok_or("attribute value cell missing")?.0.parse::<f32>()
            .map_err(|_| format!("invalid numeric attribute {name}"))?;
        if !value.is_finite() || name.is_empty() || values.insert(name.to_owned(), value).is_some() {
            return Err(format!("invalid or duplicate attribute {name}"));
        }
    }
    if values.is_empty() { return Err("no attribute rows".into()); }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_rows_reject_missing_duplicate_and_nonfinite_data() {
        let row = "<tr><td>speed:</td></td><td>2.3</td></tr>";
        assert_eq!(attributes(row).unwrap(), BTreeMap::from([("speed".into(), 2.3)]));
        for invalid in ["", "<tr><td>speed:</td>", &row.repeat(2), "<tr><td>speed:</td><td>NaN</td></tr>"] {
            assert!(attributes(invalid).is_err());
        }
    }
}
