use crate::{drawn_at_all, ToolGap, TurnKind, TurnRow, Viewport};

pub(crate) fn visible_gap(rows: &[TurnRow], viewport: Viewport) -> Option<ToolGap> {
    for tool in rows.iter().filter(|row| row.kind == TurnKind::Tool) {
        let mut intervals = vec![(tool.start.max(viewport.top), tool.end.min(viewport.bottom))];
        for conversation in rows.iter().filter(|row| drawn_at_all(row.kind)) {
            intervals = intervals
                .into_iter()
                .flat_map(|(start, end)| {
                    if conversation.end < start || conversation.start > end {
                        vec![(start, end)]
                    } else {
                        vec![(start, conversation.start - 1), (conversation.end + 1, end)]
                    }
                })
                .filter(|(start, end)| start <= end)
                .collect();
        }
        if let Some((start_row, end_row)) = intervals.into_iter().find(|(start, end)| start <= end)
        {
            let before_id = rows
                .iter()
                .filter(|row| drawn_at_all(row.kind) && row.end < start_row)
                .max_by_key(|row| row.end)
                .map(|row| row.id.clone());
            let after_id = rows
                .iter()
                .filter(|row| drawn_at_all(row.kind) && row.start > end_row)
                .min_by_key(|row| row.start)
                .map(|row| row.id.clone());
            return Some(ToolGap {
                before_id,
                after_id,
                start_row,
                end_row,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{layout_pinned, Layout, ListedTurn, Mode, Options};

    #[test]
    fn three_conversations_and_a_tool_interval_share_the_viewport() {
        let rows: Vec<_> = [
            ("a", TurnKind::User, 0, 1),
            ("tool", TurnKind::Tool, 2, 3),
            ("b", TurnKind::Agent, 4, 5),
            ("c", TurnKind::Agent, 6, 8),
        ]
        .into_iter()
        .map(|(id, kind, start, end)| TurnRow {
            id: id.into(),
            kind,
            total: end - start + 1,
            start,
            end,
            lines: end - start + 1,
        })
        .collect();
        let listed: Vec<_> = rows
            .iter()
            .map(|row| ListedTurn {
                id: row.id.clone(),
                kind: row.kind,
            })
            .collect();
        for mode in [Mode::Recent, Mode::Relative] {
            let layout = layout_pinned(
                &rows,
                &[],
                &listed,
                Viewport { top: 0, bottom: 8 },
                0,
                &Options {
                    mode,
                    ..Default::default()
                },
            );
            // A recent strip lists only the conversation, so the tool draws no
            // square; a relative strip draws the tool as a tiny square beside it.
            let expected: &[(&str, bool)] = match mode {
                Mode::Recent => &[("a", true), ("b", true), ("c", true)],
                Mode::Relative => &[
                    ("a", true),
                    ("tool", false),
                    ("b", true),
                    ("c", true),
                ],
            };
            assert_eq!(
                layout
                    .squares()
                    .iter()
                    .map(|square| (square.id.as_str(), square.active))
                    .collect::<Vec<_>>(),
                expected
            );
            let gap = match layout {
                Layout::Recent(strip) => strip.gap,
                Layout::Relative(strip) => strip.gap,
            };
            assert_eq!(
                gap,
                Some(ToolGap {
                    before_id: Some("a".into()),
                    after_id: Some("b".into()),
                    start_row: 2,
                    end_row: 3
                })
            );
        }
        let mut overlapping = rows.clone();
        overlapping[0].end = 3;
        assert_eq!(
            visible_gap(&overlapping, Viewport { top: 0, bottom: 8 }),
            None
        );
    }
}
