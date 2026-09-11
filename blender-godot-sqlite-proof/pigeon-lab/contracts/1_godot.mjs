// Godot target policy. Shared models supply every field name and type.
export function emitGodot(contract) {
  const rust = ['use godot::prelude::*;', 'use crate::fixture::sql_viewer::boundary::contracts::*;'];
  const gd = ['extends RefCounted', ''];
  for (const [name, value] of Object.entries(contract.constants).filter(([,v]) => v.type === 'string')) {
    gd.push(`const ${name} = ${JSON.stringify(value.value)}`);
  }
  gd.push('');
  const scalarType = { int32: 'int', int64: 'int', uint32: 'int', uint64: 'int', float32: 'float', float64: 'float', boolean: 'bool' };
  const variantType = { int: 'TYPE_INT', float: 'TYPE_FLOAT', bool: 'TYPE_BOOL', Array: 'TYPE_ARRAY', PackedFloat64Array: 'TYPE_PACKED_FLOAT64_ARRAY', PackedVector3Array: 'TYPE_PACKED_VECTOR3_ARRAY', PackedColorArray: 'TYPE_PACKED_COLOR_ARRAY' };
  const bounds = f => [['minimum', '>='], ['maximum', '<=']].filter(([key]) => f[key] !== undefined);
  function gdType(f) {
    return f.packed ?? (f.nullable || contract.unions[f.ref] ? 'Variant' : f.ref ?? (f.maxLength ? 'Array' : scalarType[f.type]));
  }
  function variant(f, expr) {
    if (f.packed) return `${expr}.to_variant()`;
    if (f.nullable) return `match ${expr} { Some(value) => ${variant(f.item, 'value')}, None => Variant::nil() }`;
    if (f.ref) return `${expr}.to_dictionary().to_variant()`;
    if (f.maxLength && f.item.type === 'int32') return `${expr}.iter().map(|v| i64::from(*v)).collect::<Array<i64>>().to_variant()`;
    if (['uint64', 'uint32'].includes(f.type)) return `i64::try_from(${expr}).expect("Godot integer range").to_variant()`;
    if (f.type) return `${expr}.to_variant()`;
    throw Error(`unsupported Godot field ${f.name}`);
  }
  for (const name of contract.godot) {
    const fields = contract.models[name];
    const native = fields.some(f => f.packed);
    for (const f of fields.filter(f => f.packed)) {
      const expected = { PackedFloat64Array: 'float64', PackedVector3Array: 'Position', PackedColorArray: 'Rgba' }[f.packed];
      if (!f.maxLength || !expected || (f.item.type ?? f.item.ref) !== expected) throw Error(`invalid Godot packed mapping: ${name}.${f.name}`);
    }
    const target = native ? `Godot${name}` : name;
    if (native) rust.push(`pub struct ${target} {\n${fields.map(f => `    pub ${f.name}: ${f.packed ?? f.rust},`).join('\n')}\n}`);
    rust.push(`impl ${target} {\n    pub fn to_dictionary(&self) -> VarDictionary {\n        let mut out = VarDictionary::new();\n${fields.map(f => `        out.set(${JSON.stringify(f.name)}, ${variant(f, `self.${f.name}`)});`).join('\n')}\n        out\n    }\n}`);
    if (fields.every(f => f.packed || scalarType[f.type])) {
      rust.push(`impl ${target} {\n    pub fn from_dictionary(value: &VarDictionary) -> Self {\n        let out = Self {\n${fields.map(f => `            ${f.name}: value.get(${JSON.stringify(f.name)}).expect("missing ${f.name}").try_to::<${f.packed ?? f.rust}>().expect("invalid ${f.name}"),`).join('\n')}\n        };\n${fields.filter(f => f.maxLength).map(f => `        assert!(out.${f.name}.len() <= ${f.maxLength}, "oversized ${f.name}");`).join('\n')}\n${fields.flatMap(f => bounds(f).map(([key, op]) => `        assert!((out.${f.name} as f64) ${op} ${Number.isInteger(f[key]) ? f[key] + '.0' : f[key]}, "out of range ${f.name}");`)).join('\n')}\n        out\n    }\n}`);
    }
    gd.push(`class ${name}:`, ...fields.map(f => `\tvar ${f.name}: ${gdType(f)}`), '',
      `\tstatic func from_wire(data: Dictionary) -> ${name}:`, `\t\tvar out := ${name}.new()`);
    for (const f of fields) {
      gd.push(`\t\tassert(data.has("${f.name}"), "Missing ${name}.${f.name}")`);
      const expected = variantType[gdType(f)];
      for (const [key, op] of bounds(f)) gd.push(`\t\tassert(data["${f.name}"] ${op} ${f[key]}, "Out of range ${name}.${f.name}")`);
      if (f.nullable) gd.push(`\t\tassert(data["${f.name}"] == null or typeof(data["${f.name}"]) == ${variantType[scalarType[f.item.type]]}, "Invalid ${name}.${f.name}")`);
      if (f.maxLength) gd.push(`\t\tassert(data["${f.name}"].size() <= ${f.maxLength}, "Oversized ${name}.${f.name}")`);
      if (expected) gd.push(`\t\tassert(typeof(data["${f.name}"]) == ${expected}, "Invalid ${name}.${f.name}")`);
      gd.push(`\t\tout.${f.name} = ${f.ref ? `${f.ref}.from_wire(data["${f.name}"])` : `data["${f.name}"]`}`);
    }
    gd.push('\t\treturn out', '', '\tfunc to_wire() -> Dictionary:', '\t\treturn {',
      ...fields.map(f => `\t\t\t"${f.name}": ${f.ref ? `${f.name}.to_wire()` : f.name},`), '\t\t}', '');
  }
  for (const [name, variants] of Object.entries(contract.unions)) {
    const variantName = v => v.name[0].toUpperCase() + v.name.slice(1);
    const first = contract.models[variants[0].ref];
    const common = first.filter(f => f.type && variants.every(v => contract.models[v.ref].some(p => p.name === f.name && p.type === f.type)));
    rust.push(`impl ${name} {\n    pub fn to_dictionary(&self) -> VarDictionary {\n        match self {\n${variants.map(v => `            Self::${variantName(v)}(value) => value.to_dictionary(),`).join('\n')}\n        }\n    }\n${common.map(f => `    pub fn ${f.name}(&self) -> ${f.rust} {\n        match self {\n${variants.map(v => `            Self::${variantName(v)}(value) => value.${f.name},`).join('\n')}\n        }\n    }`).join('\n')}\n}`);
    gd.push(`class ${name}:`, '\tstatic func from_wire(data: Dictionary) -> Variant:');
    for (const v of variants) {
      gd.push(`\t\tif ${contract.models[v.ref].map(f => `data.has("${f.name}")`).join(' and ')}:`, `\t\t\treturn ${v.ref}.from_wire(data)`);
    }
    gd.push('\t\tassert(false, "Unknown status shape")', '\t\treturn null', '');
  }
  return { rust: rust.join('\n\n') + '\n', gdscript: gd.join('\n') + '\n' };
}
