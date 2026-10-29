//! Local export entries receipted against the file's root-scope bindings,
//! ECMA-262 16.2.1.6.1 ParseModule step 10: import-bound -> indirect, else the declaration's span.

use oxc_ast::ast as ts;
use oxc_semantic::{Scoping, SemanticBuilder};

use super::ts_resolve::{to_local_span, ModuleFacts};

pub(crate) fn seat_local_exports(program: &ts::Program<'_>, facts: &mut ModuleFacts) {
    if facts.local_exports.is_empty() {
        return;
    }
    let scoping = SemanticBuilder::new().build(program).semantic.into_scoping();
    let wrapped_default = default_wrap(program);
    let exported: Vec<String> = facts.local_exports.keys().cloned().collect();
    for name in exported {
        let (written, span) = facts.local_exports[&name].clone();
        let local = match wrapped_default {
            Some(inner) if name == "default" => inner,
            _ => written.as_str(),
        };
        // Step 10.a.ii: the local name is an import binding, so the entry reads
        // through to the module the import names.
        if let Some(import) = facts.imports.get(local) {
            let import = (import.module.clone(), import.imported.clone());
            facts.local_exports.remove(&name);
            facts.indirect_exports.insert(name, import);
            continue;
        }
        let Some(symbol) = root_binding(&scoping, local) else {
            continue;
        };
        let declared = to_local_span(scoping.symbol_span(symbol));
        if declared != span || local != written {
            facts.local_exports.insert(name, (local.to_string(), declared));
        }
    }
}

/// `export default wrap(Y)`: the default export is Y seen through one call
/// (`memo(Y)`, `connect(a)(Y)`, `forwardRef(Y)`).
fn default_wrap<'a>(program: &'a ts::Program<'a>) -> Option<&'a str> {
    program.body.iter().find_map(|statement| match statement {
        ts::Statement::ExportDefaultDeclaration(declaration) => {
            declaration.declaration.as_expression().and_then(wrapped)
        }
        _ => None,
    })
}

fn wrapped<'a>(expression: &'a ts::Expression<'a>) -> Option<&'a str> {
    let ts::Expression::CallExpression(call) = expression.get_inner_expression() else {
        return None;
    };
    call.arguments.iter().find_map(|argument| match argument.as_expression()?.get_inner_expression() {
        ts::Expression::Identifier(identifier) => Some(identifier.name.as_str()),
        _ => None,
    })
}

fn root_binding(scoping: &Scoping, name: &str) -> Option<oxc_syntax::symbol::SymbolId> {
    scoping
        .get_bindings(scoping.root_scope_id())
        .iter()
        .find_map(|(_, symbol)| (scoping.symbol_name(*symbol) == name).then_some(*symbol))
}
