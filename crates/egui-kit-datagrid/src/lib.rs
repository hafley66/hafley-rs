//! A practical virtual-scrolling datagrid adapter for `egui-kit`.
//!
//! The base kit stays small; this crate owns the specialist `egui_table` dependency. The table
//! delegate remains caller-owned so applications decide row models, selection, sorting, editing,
//! and intents.

use std::hash::Hash;

use egui::{Id, Response, Ui};
use egui_table::{AutoSizeMode, Table};

pub use egui_table::{
    CellInfo, Column, HeaderCellInfo, HeaderRow, PrefetchInfo, TableDelegate, TableState,
};
pub use egui_table::{SplitScroll, SplitScrollDelegate};

/// Render a virtual-scrolling datagrid with caller-owned cell rendering.
pub fn data_grid<D>(
    ui: &mut Ui,
    id: impl Hash,
    row_count: u64,
    columns: Vec<Column>,
    headers: impl IntoIterator<Item = HeaderRow>,
    delegate: &mut D,
) -> Response
where
    D: TableDelegate,
{
    Table::new()
        .id_salt(Id::new(id))
        .num_rows(row_count)
        .columns(columns)
        .headers(headers.into_iter().collect::<Vec<_>>())
        .auto_size_mode(AutoSizeMode::OnParentResize)
        .show(ui, delegate)
}

/// Fixed-width column. Set `range(width..=width)` so auto-sizing will not flex it.
pub fn fixed_column(width: f32) -> Column {
    Column::new(width).range(width..=width)
}

/// Flexible column with a minimum width and no upper bound.
pub fn flex_column(width: f32, minimum: f32) -> Column {
    Column::new(width).range(minimum..=f32::INFINITY)
}
