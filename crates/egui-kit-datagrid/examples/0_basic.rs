//! Compile-only datagrid composition example.

use egui::{Id, Ui};
use egui_kit_datagrid::{
    data_grid, fixed_column, flex_column, CellInfo, HeaderCellInfo, HeaderRow, TableDelegate,
};

struct Rows {
    values: Vec<(&'static str, u32)>,
}

impl TableDelegate for Rows {
    fn header_cell_ui(&mut self, ui: &mut Ui, cell: &HeaderCellInfo) {
        ui.label(match cell.col_range.start {
            0 => "Name",
            _ => "Count",
        });
    }

    fn cell_ui(&mut self, ui: &mut Ui, cell: &CellInfo) {
        let Some((name, count)) = self.values.get(cell.row_nr as usize) else {
            return;
        };
        match cell.col_nr {
            0 => ui.label(*name),
            _ => ui.label(count.to_string()),
        };
    }
}

#[allow(dead_code)]
fn render(ui: &mut Ui) {
    let mut rows = Rows {
        values: vec![("PK Freeze", 2), ("Franklin Badge", 1)],
    };
    data_grid(
        ui,
        Id::new("inventory"),
        rows.values.len() as u64,
        vec![flex_column(220.0, 140.0), fixed_column(80.0)],
        [HeaderRow::new(24.0)],
        &mut rows,
    );
}

fn main() {}
