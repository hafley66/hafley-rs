use crate::query::Capture;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub id: usize,
    pub parent: Option<usize>,
    pub start: usize,
    pub end: usize,
    pub definitions: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub role: String,
    pub capture: Capture,
    pub scope: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnresolvedReason {
    NoDefinition,
    DefinitionAfterReference,
    AmbiguousExternal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub name: String,
    pub capture: Capture,
    pub scope: usize,
    pub definition: Option<usize>,
    pub external: Option<ExternalDefinition>,
    pub unresolved: Option<UnresolvedReason>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalDefinition {
    pub path: String,
    pub name: String,
    pub start: usize,
    pub role: String,
    pub resolution: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScopeGraph {
    pub scopes: Vec<Scope>,
    pub definitions: Vec<Definition>,
    pub references: Vec<Reference>,
}

pub fn resolve(captures: impl IntoIterator<Item = Capture>, source_len: usize) -> ScopeGraph {
    let mut captures = captures.into_iter().collect::<Vec<_>>();
    captures.sort_by_key(|capture| (capture.start, capture.end, capture.label.clone()));
    captures.dedup_by(|left, right| {
        left.start == right.start && left.end == right.end && left.label == right.label
    });
    let member_names = captures
        .iter()
        .filter(|capture| capture.label == "_")
        .map(|capture| (capture.start, capture.end))
        .collect::<Vec<_>>();

    let scope_captures = captures
        .iter()
        .filter(|capture| capture.label == "local.scope")
        .cloned()
        .collect::<Vec<_>>();
    let mut scopes = vec![Scope {
        id: 0,
        parent: None,
        start: 0,
        end: source_len,
        definitions: Vec::new(),
    }];
    for capture in &scope_captures {
        scopes.push(Scope {
            id: scopes.len(),
            parent: None,
            start: capture.start,
            end: capture.end,
            definitions: Vec::new(),
        });
    }
    for child in 1..scopes.len() {
        let parent = (0..scopes.len())
            .filter(|candidate| *candidate != child)
            .filter(|candidate| {
                scopes[*candidate].start <= scopes[child].start
                    && scopes[*candidate].end >= scopes[child].end
                    && (scopes[*candidate].start < scopes[child].start
                        || scopes[*candidate].end > scopes[child].end)
            })
            .min_by_key(|candidate| scopes[*candidate].end - scopes[*candidate].start)
            .unwrap_or(0);
        scopes[child].parent = Some(parent);
    }

    let definition_captures = captures
        .iter()
        .filter(|capture| {
            capture.label.starts_with("local.definition.")
                && !capture
                    .ancestor_kinds
                    .iter()
                    .any(|kind| kind == "import_header")
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut definitions = Vec::new();
    for capture in definition_captures {
        let scope = innermost_scope(&scopes, capture.start, capture.end);
        let role = capture.label["local.definition.".len()..].to_string();
        let definition = Definition {
            name: capture.text.clone(),
            role,
            capture,
            scope,
        };
        let index = definitions.len();
        scopes[scope].definitions.push(index);
        definitions.push(definition);
    }

    let mut references = Vec::new();
    for capture in captures
        .iter()
        .filter(|capture| capture.label == "local.reference")
    {
        if capture
            .ancestor_kinds
            .iter()
            .any(|kind| kind == "import_header" || kind == "package_header")
            || member_names.contains(&(capture.start, capture.end))
            || definitions.iter().any(|definition| {
                definition.capture.start == capture.start && definition.capture.end == capture.end
            })
        {
            continue;
        }
        let scope = innermost_scope(&scopes, capture.start, capture.end);
        let mut search_scope = Some(scope);
        let mut definition = None;
        let mut later_definition = false;
        while let Some(scope_id) = search_scope {
            let matching = scopes[scope_id]
                .definitions
                .iter()
                .copied()
                .filter(|index| definitions[*index].name == capture.text)
                .collect::<Vec<_>>();
            if let Some(found) = matching
                .iter()
                .copied()
                .filter(|index| definitions[*index].capture.start <= capture.start)
                .max_by_key(|index| definitions[*index].capture.start)
            {
                definition = Some(found);
                break;
            }
            later_definition |= !matching.is_empty();
            search_scope = scopes[scope_id].parent;
        }
        references.push(Reference {
            name: capture.text.clone(),
            capture: capture.clone(),
            scope,
            definition,
            external: None,
            unresolved: definition.is_none().then_some(if later_definition {
                UnresolvedReason::DefinitionAfterReference
            } else {
                UnresolvedReason::NoDefinition
            }),
        });
    }
    ScopeGraph {
        scopes,
        definitions,
        references,
    }
}

fn innermost_scope(scopes: &[Scope], start: usize, end: usize) -> usize {
    scopes
        .iter()
        .filter(|scope| scope.start <= start && scope.end >= end)
        .min_by_key(|scope| scope.end - scope.start)
        .map(|scope| scope.id)
        .unwrap_or(0)
}
