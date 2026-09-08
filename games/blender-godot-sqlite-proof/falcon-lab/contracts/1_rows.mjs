// Target-specific row adapters. The shared layout contains only names, kinds,
// offsets and lengths; no Rust/Godot type expressions.
export function emitRows(layouts, capacity) {
  const rust = [];
  const gd = ['extends RefCounted', '', `const STRIDE = ${capacity + 3}`, ''];
  for (const [name, layout] of Object.entries(layouts)) {
    const read = layout.fields.map(f => `            ${f.name}: ${f.length === 1 ? `row.values[${f.offset}]` : `std::array::from_fn(|i| row.values[${f.offset} + i])`},`).join('\n');
    const write = layout.fields.map(f => f.length === 1
      ? `        row.values[${f.offset}] = self.${f.name};`
      : `        row.values[${f.offset}..${f.offset + f.length}].copy_from_slice(&self.${f.name});`).join('\n');
    rust.push(`impl ${name} {
    pub const KIND: i64 = ${layout.kind};
    pub fn from_row(row: &Row) -> Option<Self> {
        if row.kind != Self::KIND { return None; }
        Some(Self {
${read}
        })
    }
    pub fn write_row(&self, row: &mut Row) {
        assert_eq!(row.kind, Self::KIND);
${write}
    }
    pub fn into_row(self, tick: i64, entity: i64) -> Row {
        let mut row = Row { tick, kind: Self::KIND, entity, values: [0.0; ${capacity}] };
        self.write_row(&mut row);
        row
    }
}`);
    const method = name.replace(/[A-Z]/g, (c, i) => (i ? '_' : '') + c.toLowerCase());
    gd.push(`static func ${method}(rows: PackedFloat64Array, entity: int = 0) -> Dictionary:`,
      '\tassert(rows.size() % STRIDE == 0, "Invalid packed row length")',
      '\tfor start in range(0, rows.size(), STRIDE):',
      `\t\tif rows[start + 1] == ${layout.kind} and rows[start + 2] == entity:`,
      '\t\t\treturn {',
      ...layout.fields.map(f => `\t\t\t\t"${f.name}": ${f.length === 1 ? `rows[start + ${3 + f.offset}]` : `rows.slice(start + ${3 + f.offset}, start + ${3 + f.offset + f.length})`},`),
      '\t\t\t}', '\treturn {}', '');
  }
  rust.push(`pub fn pack_rows(rows: &[Row]) -> Vec<f64> {
    rows.iter().flat_map(|row| [row.tick as f64, row.kind as f64, row.entity as f64].into_iter().chain(row.values)).collect()
}`);
  return { rust, gdscript: gd.join('\n') + '\n' };
}
