use super::*;
use sprefa_extract::edit::ts7_cleave_facts;

pub(super) trait CleaveFactSource {
    fn load(
        &self,
        cx: &MoveCx,
        src: &str,
        item: &str,
        source: &mut FileFacts,
        imports: &mut Imports,
    ) -> Result<(), String>;
}

pub(super) struct FastFacts;
pub(super) struct SlowTsFacts;

impl CleaveFactSource for FastFacts {
    fn load(
        &self,
        _cx: &MoveCx,
        _src: &str,
        _item: &str,
        _source: &mut FileFacts,
        _imports: &mut Imports,
    ) -> Result<(), String> {
        Ok(())
    }
}

impl CleaveFactSource for SlowTsFacts {
    fn load(
        &self,
        cx: &MoveCx,
        src: &str,
        item: &str,
        source: &mut FileFacts,
        imports: &mut Imports,
    ) -> Result<(), String> {
        let mut candidates: Vec<(String, u32)> = source
            .free
            .iter()
            .map(|(name, span)| (name.clone(), span.start))
            .collect();
        candidates.extend(
            source
                .specifiers
                .iter()
                .map(|row| (row.name.clone(), row.span.start)),
        );
        let imported_names: BTreeSet<_> = source
            .specifiers
            .iter()
            .map(|row| row.name.clone())
            .collect();
        let facts = ts7_cleave_facts::collect_ts_cleave(
            cx.root(),
            src,
            &source.text,
            item,
            &candidates,
            &imported_names,
        )?;
        let symbols: BTreeMap<_, _> = facts
            .items
            .iter()
            .map(|row| (row.name.as_str(), row))
            .collect();
        source
            .decls
            .retain(|decl| symbols.contains_key(decl.name.as_str()));
        for decl in &mut source.decls {
            if let Some(symbol) = symbols.get(decl.name.as_str()) {
                decl.type_only |= symbol.type_only;
            }
        }
        if !source.decls.iter().any(|decl| decl.name == item) {
            return Err(format!(
                "documentSymbol declares no top-level {item} in {src}"
            ));
        }

        let local_names: BTreeSet<_> = source.decls.iter().map(|decl| decl.name.clone()).collect();
        let resolved_local: BTreeSet<_> = facts
            .definitions
            .iter()
            .filter(|definition| definition.path == src)
            .map(|definition| (definition.name.clone(), definition.site))
            .collect();
        source.free.retain(|(name, span)| {
            !local_names.contains(name) || resolved_local.contains(&(name.clone(), span.start))
        });
        source.free.retain(|(name, _)| name != item);

        for definition in facts.definitions {
            for row in source
                .specifiers
                .iter_mut()
                .filter(|row| row.name == definition.name)
            {
                row.type_only |= definition.type_only;
            }
            if !source
                .specifiers
                .iter()
                .any(|row| row.name == definition.name)
                || definition.path == src
            {
                continue;
            }
            imports
                .names
                .retain(|(from, bound, ..)| from != src || bound != &definition.name);
            imports.names.insert(
                0,
                (
                    src.to_string(),
                    definition.name.clone(),
                    definition.path,
                    definition.name,
                    false,
                ),
            );
        }
        let reference_files: BTreeSet<_> = facts
            .references
            .iter()
            .filter(|reference| reference.path != src)
            .map(|reference| reference.path.clone())
            .collect();
        imports.names.retain(|(from, _, target, declared, _)| {
            target != src || declared != item || reference_files.contains(from)
        });
        for reference in facts.references {
            if reference.path == src {
                source
                    .free
                    .push((item.to_string(), span_of(reference.start, reference.end)));
                continue;
            }
            if !cx.contains(&reference.path) {
                continue;
            }
            let other = FileFacts::open(cx, &reference.path, false)?;
            for row in other
                .specifiers
                .iter()
                .filter(|row| row.name == item || row.imported.as_deref() == Some(item))
            {
                if imports
                    .names
                    .iter()
                    .any(|(from, bound, target, declared, _)| {
                        from == &reference.path
                            && bound == &row.name
                            && target == src
                            && declared == item
                    })
                {
                    continue;
                }
                imports.names.push((
                    reference.path.clone(),
                    row.name.clone(),
                    src.to_string(),
                    item.to_string(),
                    false,
                ));
            }
        }
        Ok(())
    }
}
