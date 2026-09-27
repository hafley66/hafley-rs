//! Rust receiver typing projected from the shared tree-sitter parse.

use std::collections::HashMap;

use super::df_syntax_rows::Span;
use super::receiver_rows::{ReceiverBinding, ReceiverOutcome};

#[derive(Clone, Debug, PartialEq, Eq)]
enum TypeBinding {
    Named(String),
    Unknown,
}

pub fn receiver_rows_from_tree(tree: &tree_sitter::Tree, source: &[u8]) -> Vec<ReceiverBinding> {
    let mut tables = Tables::default();
    tables.collect(tree.root_node(), source);
    let mut walk = ReceiverWalk {
        source,
        tables,
        impl_stack: Vec::new(),
        scopes: Vec::new(),
        out: Vec::new(),
    };
    walk.visit(tree.root_node());
    walk.out
}

#[derive(Default)]
struct Tables {
    rets: HashMap<String, String>,
    assoc_rets: HashMap<(String, String), String>,
    fields: HashMap<(String, String), String>,
}

impl Tables {
    fn collect(&mut self, node: tree_sitter::Node<'_>, source: &[u8]) {
        match node.kind() {
            "function_item" => {
                if let Some(name) = node.child_by_field_name("name") {
                    if let Some(ty) = node
                        .child_by_field_name("return_type")
                        .and_then(|ty| principal_type(ty, source))
                    {
                        self.rets.entry(text(name, source).to_owned()).or_insert(ty);
                    }
                }
            }
            "impl_item" => {
                if let Some(owner) = node
                    .child_by_field_name("type")
                    .and_then(|ty| principal_type(ty, source))
                {
                    if let Some(body) = node.child_by_field_name("body") {
                        for method in named_children(body)
                            .into_iter()
                            .filter(|item| item.kind() == "function_item")
                        {
                            let (Some(name), Some(ret)) = (
                                method.child_by_field_name("name"),
                                method
                                    .child_by_field_name("return_type")
                                    .and_then(|ty| principal_type(ty, source)),
                            ) else {
                                continue;
                            };
                            let method_name = text(name, source).to_owned();
                            let ret = if ret == "Self" { owner.clone() } else { ret };
                            self.rets
                                .entry(method_name.clone())
                                .or_insert_with(|| ret.clone());
                            self.assoc_rets
                                .entry((owner.clone(), method_name))
                                .or_insert(ret);
                        }
                    }
                }
            }
            "struct_item" => {
                if let (Some(name), Some(body)) = (
                    node.child_by_field_name("name"),
                    node.child_by_field_name("body"),
                ) {
                    let owner = text(name, source).to_owned();
                    for field in named_children(body)
                        .into_iter()
                        .filter(|field| field.kind() == "field_declaration")
                    {
                        if let (Some(name), Some(ty)) = (
                            field.child_by_field_name("name"),
                            field
                                .child_by_field_name("type")
                                .and_then(|ty| principal_type(ty, source)),
                        ) {
                            self.fields
                                .entry((owner.clone(), text(name, source).to_owned()))
                                .or_insert(ty);
                        }
                    }
                }
            }
            _ => {}
        }
        for child in named_children(node) {
            self.collect(child, source);
        }
    }
}

struct ReceiverWalk<'a> {
    source: &'a [u8],
    tables: Tables,
    impl_stack: Vec<String>,
    scopes: Vec<HashMap<String, TypeBinding>>,
    out: Vec<ReceiverBinding>,
}

impl ReceiverWalk<'_> {
    fn visit(&mut self, node: tree_sitter::Node<'_>) {
        match node.kind() {
            "impl_item" => {
                let owner = node
                    .child_by_field_name("type")
                    .and_then(|ty| principal_type(ty, self.source));
                if let Some(owner) = owner {
                    self.impl_stack.push(owner);
                    for child in named_children(node) {
                        self.visit(child);
                    }
                    self.impl_stack.pop();
                } else {
                    self.visit_children(node);
                }
                return;
            }
            "function_item" => {
                self.scopes.push(HashMap::new());
                self.seed_params(node);
                self.visit_children(node);
                self.scopes.pop();
                return;
            }
            "closure_expression" => {
                self.scopes.push(HashMap::new());
                if let Some(params) = node.child_by_field_name("parameters") {
                    for param in named_children(params) {
                        if param.kind() == "identifier" {
                            self.insert(text(param, self.source).to_owned(), TypeBinding::Unknown);
                        }
                    }
                }
                self.visit_children(node);
                self.scopes.pop();
                return;
            }
            "let_declaration" => {
                let binding = self.local_binding(node);
                for child in named_children(node) {
                    self.visit(child);
                }
                if let Some((name, ty)) = binding {
                    self.insert(name, ty);
                }
                return;
            }
            "call_expression" => {
                if let Some(function) = node.child_by_field_name("function") {
                    let function = if function.kind() == "generic_function" {
                        function.child_by_field_name("function").unwrap_or(function)
                    } else {
                        function
                    };
                    if function.kind() == "field_expression" {
                        if let Some(method) = function.child_by_field_name("field") {
                            let receiver = function.child_by_field_name("value");
                            let outcome = receiver
                                .map(|receiver| self.receiver_outcome(receiver))
                                .unwrap_or(ReceiverOutcome::Inferred);
                            self.out.push(ReceiverBinding {
                                call_site: span(method, self.source),
                                outcome,
                            });
                        }
                    } else if let Some(path) = self.simple_path(function) {
                        if path.len() == 1 && path[0] != "self" && self.lookup(&path[0]).is_some() {
                            self.out.push(ReceiverBinding {
                                call_site: span(function, self.source),
                                outcome: ReceiverOutcome::Shadowed,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
        self.visit_children(node);
    }

    fn visit_children(&mut self, node: tree_sitter::Node<'_>) {
        for child in named_children(node) {
            self.visit(child);
        }
    }

    fn local_binding(&self, node: tree_sitter::Node<'_>) -> Option<(String, TypeBinding)> {
        let pattern = node.child_by_field_name("pattern")?;
        let name_node = match pattern.kind() {
            "identifier" => pattern,
            "let_pattern" | "ref_pattern" | "mut_pattern" => named_children(pattern)
                .into_iter()
                .filter(|child| child.kind() == "identifier")
                .last()?,
            _ => return None,
        };
        let name = text(name_node, self.source).to_owned();
        let explicit = node
            .child_by_field_name("type")
            .and_then(|ty| receiver_type(ty, self.source));
        let inferred = node
            .child_by_field_name("value")
            .and_then(|value| self.init_type(value));
        let ty = explicit
            .or(inferred)
            .map_or(TypeBinding::Unknown, TypeBinding::Named);
        Some((name, ty))
    }

    fn init_type(&self, init: tree_sitter::Node<'_>) -> Option<String> {
        let mut current = init;
        while matches!(
            current.kind(),
            "parenthesized_expression"
                | "reference_expression"
                | "try_expression"
                | "await_expression"
        ) {
            current = named_children(current)
                .into_iter()
                .find(|child| child.kind() != "type_arguments")?;
        }
        match current.kind() {
            "call_expression" => {
                let function = current.child_by_field_name("function")?;
                let function = if function.kind() == "generic_function" {
                    function.child_by_field_name("function").unwrap_or(function)
                } else {
                    function
                };
                if function.kind() == "field_expression" {
                    let receiver = function.child_by_field_name("value")?;
                    let method = function.child_by_field_name("field")?;
                    let owner = self.expr_type(receiver)?;
                    return self
                        .tables
                        .assoc_rets
                        .get(&(owner, text(method, self.source).to_owned()))
                        .cloned();
                }
                let path = self.simple_path(function)?;
                let name = path.last()?.clone();
                if path.len() == 1 {
                    return self.tables.rets.get(&name).cloned();
                }
                let owner = path[..path.len() - 1]
                    .iter()
                    .rev()
                    .find(|segment| segment.chars().next().is_some_and(char::is_uppercase))?;
                let owner = self.resolve_self(owner)?;
                self.tables
                    .assoc_rets
                    .get(&(owner.clone(), name.clone()))
                    .cloned()
                    .or_else(|| (name == "new").then_some(owner))
            }
            _ => self.expr_type(current),
        }
    }

    fn expr_type(&self, expr: tree_sitter::Node<'_>) -> Option<String> {
        match expr.kind() {
            "identifier" | "scoped_identifier" | "self" => {
                let path = self.simple_path(expr)?;
                if path == ["self"] {
                    return self.impl_stack.last().cloned();
                }
                if path.len() != 1 {
                    return None;
                }
                match self.lookup(&path[0])? {
                    TypeBinding::Named(ty) => self.resolve_self(ty),
                    TypeBinding::Unknown => None,
                }
            }
            "field_expression" => {
                let base = self.expr_type(expr.child_by_field_name("value")?)?;
                let field = expr.child_by_field_name("field")?;
                self.tables
                    .fields
                    .get(&(base, text(field, self.source).to_owned()))
                    .and_then(|ty| self.resolve_self(ty))
            }
            "call_expression"
            | "parenthesized_expression"
            | "reference_expression"
            | "try_expression"
            | "await_expression" => self.init_type(expr),
            _ => None,
        }
    }

    fn receiver_outcome(&self, expr: tree_sitter::Node<'_>) -> ReceiverOutcome {
        let mut current = expr;
        loop {
            match current.kind() {
                "reference_expression" | "parenthesized_expression" | "unary_expression" => {
                    let Some(inner) = named_children(current).into_iter().last() else {
                        return ReceiverOutcome::Inferred;
                    };
                    current = inner;
                }
                "identifier" | "scoped_identifier" | "self" => {
                    let Some(path) = self.simple_path(current) else {
                        return ReceiverOutcome::Inferred;
                    };
                    if path == ["self"] {
                        return self
                            .impl_stack
                            .last()
                            .cloned()
                            .map_or(ReceiverOutcome::Inferred, ReceiverOutcome::Named);
                    }
                    let last = path.last().unwrap();
                    let binding = self.lookup(last);
                    let bound = binding.and_then(|binding| match binding {
                        TypeBinding::Named(ty) => self.resolve_self(ty),
                        TypeBinding::Unknown => None,
                    });
                    return match bound {
                        Some(ty) => ReceiverOutcome::Named(ty),
                        None if binding.is_none()
                            && last.chars().next().is_some_and(char::is_uppercase) =>
                        {
                            ReceiverOutcome::Named(last.clone())
                        }
                        None => ReceiverOutcome::Inferred,
                    };
                }
                "field_expression" => {
                    let Some(base) = current.child_by_field_name("value") else {
                        return ReceiverOutcome::Inferred;
                    };
                    let Some(base_ty) = self.base_type(base) else {
                        return ReceiverOutcome::Inferred;
                    };
                    let Some(field) = current.child_by_field_name("field") else {
                        return ReceiverOutcome::Inferred;
                    };
                    return self
                        .tables
                        .fields
                        .get(&(base_ty, text(field, self.source).to_owned()))
                        .and_then(|ty| self.resolve_self(ty))
                        .map_or(ReceiverOutcome::Inferred, ReceiverOutcome::Named);
                }
                _ => {
                    return self
                        .expr_type(current)
                        .map_or(ReceiverOutcome::Inferred, ReceiverOutcome::Named);
                }
            }
        }
    }

    fn base_type(&self, expr: tree_sitter::Node<'_>) -> Option<String> {
        let path = self.simple_path(expr)?;
        if path == ["self"] {
            return self.impl_stack.last().cloned();
        }
        if path.len() != 1 {
            return None;
        }
        match self.lookup(&path[0])? {
            TypeBinding::Named(ty) => self.resolve_self(ty),
            TypeBinding::Unknown => None,
        }
    }

    fn seed_params(&mut self, function: tree_sitter::Node<'_>) {
        let generic_bounds = self.generic_bounds(function);
        let Some(params) = function.child_by_field_name("parameters") else {
            return;
        };
        for param in named_children(params) {
            if param.kind() != "parameter" {
                continue;
            }
            let (Some(pattern), Some(ty)) = (
                param.child_by_field_name("pattern"),
                param.child_by_field_name("type"),
            ) else {
                continue;
            };
            if pattern.kind() != "identifier" {
                continue;
            }
            if let Some(ty) = receiver_type(ty, self.source) {
                let ty = generic_bounds
                    .get(&ty)
                    .and_then(|bounds| match bounds.as_slice() {
                        [trait_name] => Some(trait_name.clone()),
                        _ => None,
                    })
                    .unwrap_or(ty);
                self.insert(
                    text(pattern, self.source).to_owned(),
                    TypeBinding::Named(ty),
                );
            }
        }
    }

    fn generic_bounds(&self, function: tree_sitter::Node<'_>) -> HashMap<String, Vec<String>> {
        let mut out = HashMap::new();
        if let Some(params) = function.child_by_field_name("type_parameters") {
            for param in named_children(params) {
                if param.kind() != "type_parameter" {
                    continue;
                }
                let Some(name) = param.child_by_field_name("name") else {
                    continue;
                };
                if let Some(bounds) = param.child_by_field_name("bounds") {
                    out.entry(text(name, self.source).to_owned())
                        .or_insert_with(Vec::new)
                        .extend(named_children(bounds).into_iter().map(|bound| {
                            text(bound, self.source)
                                .trim()
                                .rsplit("::")
                                .next()
                                .unwrap_or_default()
                                .to_owned()
                        }));
                }
            }
        }
        for clause in named_children(function)
            .into_iter()
            .filter(|child| child.kind() == "where_clause")
        {
            for predicate in named_children(clause) {
                if predicate.kind() != "where_predicate" {
                    continue;
                }
                let (Some(left), Some(bounds)) = (
                    predicate.child_by_field_name("left"),
                    predicate.child_by_field_name("bounds"),
                ) else {
                    continue;
                };
                out.entry(text(left, self.source).to_owned())
                    .or_insert_with(Vec::new)
                    .extend(named_children(bounds).into_iter().map(|bound| {
                        text(bound, self.source)
                            .trim()
                            .rsplit("::")
                            .next()
                            .unwrap_or_default()
                            .to_owned()
                    }));
            }
        }
        out
    }

    fn simple_path(&self, node: tree_sitter::Node<'_>) -> Option<Vec<String>> {
        let path = std::str::from_utf8(&self.source[node.byte_range()]).ok()?;
        Some(
            path.trim()
                .trim_start_matches("::")
                .split("::")
                .map(str::trim)
                .filter(|segment| !segment.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }

    fn resolve_self(&self, ty: &str) -> Option<String> {
        if ty == "Self" {
            self.impl_stack.last().cloned()
        } else {
            Some(ty.to_owned())
        }
    }

    fn insert(&mut self, name: String, binding: TypeBinding) {
        let Some(frame) = self.scopes.last_mut() else {
            return;
        };
        if frame.get(&name).is_some_and(|current| current != &binding) {
            frame.insert(name, TypeBinding::Unknown);
        } else {
            frame.insert(name, binding);
        }
    }

    fn lookup(&self, name: &str) -> Option<&TypeBinding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }
}

fn named_children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

fn principal_type(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "reference_type" | "pointer_type" | "parenthesized_type" => node
            .child_by_field_name("type")
            .or_else(|| named_children(node).into_iter().last())
            .and_then(|child| principal_type(child, source)),
        "generic_type" => {
            let head = node.child_by_field_name("type")?;
            let name = text(head, source).rsplit("::").next()?.to_owned();
            if matches!(name.as_str(), "Result" | "Option") {
                node.child_by_field_name("type_arguments")
                    .and_then(|args| {
                        named_children(args)
                            .into_iter()
                            .find(|arg| is_type_node(arg.kind()))
                    })
                    .and_then(|arg| principal_type(arg, source))
            } else {
                Some(name)
            }
        }
        "trait_object" | "abstract_type" => {
            let bounds = named_children(node);
            if bounds.len() != 1 {
                return None;
            }
            let bound = bounds[0];
            if bound.kind() == "function_type" {
                return bound
                    .child_by_field_name("trait")
                    .and_then(|trait_path| principal_type(trait_path, source));
            }
            principal_type(bound, source)
        }
        "dynamic_type" => node
            .child_by_field_name("trait")
            .and_then(|trait_path| principal_type(trait_path, source)),
        "array_type" | "slice_type" | "tuple_type" | "function_type" | "unit_type" => None,
        _ => Some(text(node, source).trim().rsplit("::").next()?.to_owned()),
    }
}

fn receiver_type(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "reference_type" | "pointer_type" | "parenthesized_type" => node
            .child_by_field_name("type")
            .or_else(|| named_children(node).into_iter().last())
            .and_then(|inner| receiver_type(inner, source)),
        "generic_type"
            if node.child_by_field_name("type").is_some_and(|head| {
                text(head, source)
                    .rsplit("::")
                    .next()
                    .is_some_and(|name| name == "Box")
            }) =>
        {
            node.child_by_field_name("type_arguments")
                .and_then(|args| {
                    named_children(args)
                        .into_iter()
                        .find(|arg| is_type_node(arg.kind()))
                })
                .and_then(|inner| receiver_type(inner, source))
        }
        _ => principal_type(node, source),
    }
}

fn is_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "type_identifier"
            | "primitive_type"
            | "scoped_type_identifier"
            | "generic_type"
            | "reference_type"
            | "pointer_type"
            | "tuple_type"
            | "array_type"
            | "slice_type"
            | "function_type"
            | "trait_object"
            | "abstract_type"
    )
}

fn span(node: tree_sitter::Node<'_>, source: &[u8]) -> Span {
    let start = syn_compatible_byte(source, node.start_byte());
    let end = syn_compatible_byte(source, node.end_byte());
    Span {
        start,
        len: end - start,
    }
}

fn syn_compatible_byte(source: &[u8], offset: usize) -> u32 {
    let line_start = source[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |newline| newline + 1);
    let column = std::str::from_utf8(&source[line_start..offset])
        .expect("Rust source is UTF-8")
        .chars()
        .count();
    (line_start + column) as u32
}

fn text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> &'a str {
    std::str::from_utf8(&source[node.byte_range()]).expect("Rust source is UTF-8")
}
