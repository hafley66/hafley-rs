//! Receiver typing for the kotlin call arm (the go/rust twins): one
//! `ReceiverBinding` per navigation-call site, a `Shadowed` row per plain
//! call to a scope-bound name, and `MethodOwner` rows for class-body
//! members. Resolve reads the rows plus this module's per-blob plan store
//! (the go `GoBindPlan` twin): the member span -> owner table the `receiver`
//! leg joins through, the (owner, member) -> written type table the one-hop
//! `a.f` upgrade reads cross-file, and the site -> (base, member) chains the
//! walk saw but could not type.
//!
//! Scope bindings, innermost wins, one frame per function body, lambda
//! literal, and class/object body. A class frame is seeded from its val/var
//! class parameters and member properties before any member body walks, so
//! every method sees the whole member set whatever order the file declares
//! them in.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use tree_sitter::Node;

use crate::family::CallF;
use crate::rows::FamilyBundle;
use crate::shape::{ContentId, Span, Strings};
use crate::types::{MethodOwner, ReceiverBinding, ReceiverOutcome};

use super::kotlin::{def_span, kt_first_child, kt_text, node_span};

// ── the per-blob plan store (the go.rs GoBindPlan twin) ─────────────────────

/// One file's resolve-side tables, keyed by content.
pub(crate) struct KtBindPlan {
    /// Member def span -> owner type name: the (T, m) table's owner half.
    owners: HashMap<(u32, u32), String>,
    /// (owner, member) -> the member's written type, generics stripped.
    fields: HashMap<(String, String), String>,
    /// Navigation-call site span -> (base type, member spelled): the
    /// `h.w.run()` chain the walk saw but resolve must type cross-file.
    recv_fields: HashMap<(u32, u32), (String, String)>,
}

impl KtBindPlan {
    /// The owner type of the member def at `span`, when this file declares it.
    pub(crate) fn owner_of(&self, span: Span) -> Option<&str> {
        self.owners.get(&(span.start, span.end())).map(String::as_str)
    }

    /// The written type of member `member` on owner `owner` declared here.
    pub(crate) fn field_type_of(&self, owner: &str, member: &str) -> Option<&str> {
        self.fields
            .get(&(owner.to_string(), member.to_string()))
            .map(String::as_str)
    }

    /// The (base type, member) chain phase 1 recorded for the site at `span`.
    pub(crate) fn recv_field_of(&self, span: Span) -> Option<(String, String)> {
        self.recv_fields.get(&(span.start, span.end())).cloned()
    }

    /// Does this file declare `ty` as a member owner (class/object/interface)?
    pub(crate) fn owns_type(&self, ty: &str) -> bool {
        self.owners.values().any(|owner| owner == ty)
            || self.fields.keys().any(|(owner, _)| owner == ty)
    }

}

static PLAN_CACHE: LazyLock<Mutex<HashMap<ContentId, Arc<KtBindPlan>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn kt_bind_plan_of(blob: &ContentId) -> Option<Arc<KtBindPlan>> {
    let guard = PLAN_CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
    guard.get(blob).cloned()
}

fn kt_bind_plan_store(blob: ContentId, plan: KtBindPlan) {
    let mut guard = PLAN_CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
    guard.insert(blob, Arc::new(plan));
}

// ── the scope walk ───────────────────────────────────────────────────────────

/// One name's binding in a scope frame. `Inferred` covers a name bound by the
/// parse but typeless here (a call-result initializer, a fn-typed param).
#[derive(Clone, Debug, PartialEq, Eq)]
enum KtBinding {
    Named(String),
    Inferred,
    Ambiguous,
}

type Frame = HashMap<String, KtBinding>;

/// The receiver walk: one pass per file, collecting one row per site and
/// `MethodOwner` rows for the aux sink, plus the plan for the resolve phase.
struct ReceiverWalk<'a> {
    src: &'a [u8],
    strings: &'a mut Strings,
    scopes: Vec<Frame>,
    /// Enclosing class/object/companion owner names; `this` and the implicit
    /// `this` read the innermost.
    owners: Vec<String>,
    /// The innermost function's type parameters: name -> single bound.
    bounds: Vec<HashMap<String, String>>,
    out: Vec<ReceiverBinding>,
    owners_out: Vec<MethodOwner>,
    plan: KtBindPlan,
}

/// Phase-1 entry: `MethodOwner` + `ReceiverBinding` rows for one file, and
/// the blob's plan for the resolve phase.
pub(crate) fn collect_receivers(
    root: Node,
    src: &[u8],
    blob: ContentId,
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    let mut walk = ReceiverWalk {
        src,
        strings,
        scopes: Vec::new(),
        owners: Vec::new(),
        bounds: Vec::new(),
        out: Vec::new(),
        owners_out: Vec::new(),
        plan: KtBindPlan {
            owners: HashMap::new(),
            fields: HashMap::new(),
            recv_fields: HashMap::new(),
        },
    };
    walk.walk(root);
    sink.aux.method_owners.append(&mut walk.owners_out);
    sink.aux.receivers.append(&mut walk.out);
    if !walk.plan.owners.is_empty()
        || !walk.plan.fields.is_empty()
        || !walk.plan.recv_fields.is_empty()
    {
        kt_bind_plan_store(blob, walk.plan);
    }
}

impl<'a> ReceiverWalk<'a> {
    fn walk(&mut self, node: Node) {
        match node.kind() {
            "class_declaration" | "object_declaration" => self.class_decl(node),
            "companion_object" => self.companion(node),
            "function_declaration" => self.function(node),
            "lambda_literal" => self.lambda(node),
            "property_declaration" => self.property(node),
            "call_expression" => {
                self.call_site(node);
                self.walk_children(node);
            }
            _ => self.walk_children(node),
        }
    }

    fn walk_children(&mut self, node: Node) {
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        for child in children {
            self.walk(child);
        }
    }

    fn class_decl(&mut self, node: Node) {
        let Some(name) = kt_first_child(node, "type_identifier")
            .map(|n| kt_text(n, self.src).to_string())
        else {
            self.walk_children(node);
            return;
        };
        self.owners.push(name.clone());
        self.scopes.push(Frame::default());
        // val/var class parameters are members: seed them (and record their
        // written types) so every member body sees the whole frame.
        if let Some(pc) = kt_first_child(node, "primary_constructor") {
            let mut cursor = pc.walk();
            let params: Vec<Node> = pc
                .children(&mut cursor)
                .filter(|n| n.kind() == "class_parameter")
                .collect();
            for param in params {
                self.seed_class_param(param, &name);
            }
        }
        if let Some(body) = kt_first_child(node, "class_body") {
            self.seed_member_properties(body, &name);
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        for child in children {
            if child.kind() == "class_body" {
                self.class_body(child, &name);
            } else {
                self.walk(child);
            }
        }
        self.scopes.pop();
        self.owners.pop();
    }

    fn companion(&mut self, node: Node) {
        // Members answer for the enclosing class (calls are `Outer.m()`), so
        // the owner stack is untouched; only a fresh scope frame opens.
        let owner = self.owners.last().cloned();
        self.scopes.push(Frame::default());
        if let (Some(body), Some(owner)) = (kt_first_child(node, "class_body"), owner.as_deref()) {
            self.seed_member_properties(body, owner);
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        for child in children {
            if child.kind() == "class_body" {
                if let Some(owner) = owner.as_deref() {
                    self.class_body(child, owner);
                    continue;
                }
            }
            self.walk(child);
        }
        self.scopes.pop();
    }

    /// One class_body's members: `MethodOwner` rows, the plan's owner table,
    /// then the bodies (nested decls push their own frames via their arms).
    fn class_body(&mut self, body: Node, owner: &str) {
        let mut cursor = body.walk();
        let members: Vec<Node> = body.children(&mut cursor).collect();
        for member in members {
            match member.kind() {
                "function_declaration" => {
                    let span = def_span(member);
                    self.plan
                        .owners
                        .insert((span.start, span.end()), owner.to_string());
                    self.push_owner(span, Some(owner));
                    self.walk(member);
                }
                "property_declaration" => {
                    let span = node_span(member);
                    self.plan
                        .owners
                        .insert((span.start, span.end()), owner.to_string());
                    self.push_owner(span, Some(owner));
                    self.walk(member);
                }
                _ => self.walk(member),
            }
        }
    }

    /// Seed the class frame with the body's member properties, order-free.
    fn seed_member_properties(&mut self, body: Node, owner: &str) {
        let mut cursor = body.walk();
        let props: Vec<Node> = body
            .children(&mut cursor)
            .filter(|n| n.kind() == "property_declaration")
            .collect();
        for prop in props {
            if let Some((name, written)) = property_head(prop, self.src) {
                if let Some(ty) = &written {
                    self.plan
                        .fields
                        .insert((owner.to_string(), name.clone()), ty.clone());
                }
                let binding = self.property_binding_value(prop, written);
                self.insert(name, binding);
            }
        }
    }

    fn seed_class_param(&mut self, param: Node, owner: &str) {
        let Some((name, written)) = class_param_head(param, self.src) else {
            return;
        };
        if let Some(ty) = &written {
            self.plan
                .fields
                .insert((owner.to_string(), name.clone()), ty.clone());
        }
        let binding = written.map(KtBinding::Named).unwrap_or(KtBinding::Inferred);
        self.insert(name, binding);
    }

    fn function(&mut self, node: Node) {
        self.scopes.push(Frame::default());
        let bounds = fn_bounds(node, self.src);
        self.bounds.push(bounds);
        if let Some(params) = kt_first_child(node, "function_value_parameters") {
            let mut cursor = params.walk();
            let params: Vec<Node> = params
                .children(&mut cursor)
                .filter(|n| n.kind() == "parameter")
                .collect();
            for param in params {
                self.seed_param(param);
            }
        }
        self.walk_children(node);
        self.bounds.pop();
        self.scopes.pop();
    }

    /// A typed parameter binds its written type; a parameter typed by the
    /// function's OWN type parameter binds that param's single bound.
    fn seed_param(&mut self, param: Node) {
        let mut cursor = param.walk();
        let kids: Vec<Node> = param.children(&mut cursor).collect();
        let Some(name) = kids
            .iter()
            .find(|n| n.kind() == "simple_identifier")
            .map(|n| kt_text(*n, self.src).to_string())
        else {
            return;
        };
        let written = kids
            .iter()
            .find(|n| is_type_node(n.kind()))
            .and_then(|ty| written_ty(*ty, self.src));
        let binding = match written {
            Some(ty) => match self.bounds.last().and_then(|b| b.get(&ty)) {
                Some(bound) => KtBinding::Named(bound.clone()),
                None if self.bounds.last().is_some_and(|b| b.contains_key(&ty)) => {
                    KtBinding::Inferred
                }
                None => KtBinding::Named(ty),
            },
            None => KtBinding::Inferred,
        };
        self.insert(name, binding);
    }

    fn lambda(&mut self, node: Node) {
        self.scopes.push(Frame::default());
        let mut seeded = false;
        if let Some(lp) = kt_first_child(node, "lambda_parameters") {
            let mut cursor = lp.walk();
            let params: Vec<Node> = lp
                .children(&mut cursor)
                .filter(|n| n.kind() == "variable_declaration")
                .collect();
            for vd in params {
                if let Some((name, written)) = property_head(vd, self.src) {
                    let binding = written.map(KtBinding::Named).unwrap_or(KtBinding::Inferred);
                    self.insert(name, binding);
                    seeded = true;
                }
            }
        }
        if !seeded {
            // The implicit single parameter, df's own `it` convention.
            self.insert("it".to_string(), KtBinding::Inferred);
        }
        self.walk_children(node);
        self.scopes.pop();
    }

    fn property(&mut self, node: Node) {
        let head = property_head(node, self.src);
        // The initializer is read in the OUTER scope: the binding lands only
        // after its own expression walked.
        self.walk_children(node);
        if let Some((name, written)) = head {
            let binding = self.property_binding_value(node, written);
            self.insert(name, binding);
        }
    }

    /// A property's binding: the written type, else a constructor-call
    /// initializer's uppercase callee, else the parse could not type it.
    fn property_binding_value(&self, node: Node, written: Option<String>) -> KtBinding {
        if let Some(ty) = written {
            return KtBinding::Named(ty);
        }
        let mut cursor = node.walk();
        let init = node
            .children(&mut cursor)
            .find(|n| n.kind() == "call_expression");
        if let Some(call) = init {
            let mut cc = call.walk();
            let lead = call.children(&mut cc).find(|n| n.kind() != "call_suffix");
            if let Some(lead) = lead {
                if lead.kind() == "simple_identifier" {
                    let callee = kt_text(lead, self.src);
                    if callee.chars().next().is_some_and(char::is_uppercase) {
                        return KtBinding::Named(callee.to_string());
                    }
                }
            }
        }
        KtBinding::Inferred
    }

    // ── call sites ───────────────────────────────────────────────────────────

    /// One `ReceiverBinding` per navigation-call site (the site span IS the
    /// lead navigation_expression, what `kt_walk_call_sites` keyed), a
    /// `Shadowed` row per plain call to a scope-bound name, and an implicit-
    /// `this` row for a plain unbound lowercase call inside a class body.
    fn call_site(&mut self, call: Node) {
        let mut cursor = call.walk();
        let Some(lead) = call
            .children(&mut cursor)
            .find(|n| n.kind() != "call_suffix")
        else {
            return;
        };
        match lead.kind() {
            "navigation_expression" => {
                let site = node_span(lead);
                let outcome = self.nav_outcome(lead, site);
                self.out.push(ReceiverBinding {
                    call_site: site,
                    outcome,
                });
            }
            "simple_identifier" => {
                let ident = kt_text(lead, self.src);
                if self.lookup(ident).is_some() {
                    self.out.push(ReceiverBinding {
                        call_site: node_span(lead),
                        outcome: ReceiverOutcome::Shadowed,
                    });
                } else if ident.chars().next().is_some_and(char::is_lowercase) {
                    // The implicit receiver: a bare member call inside a class
                    // body is `this.m()`.
                    if let Some(owner) = self.owners.last().cloned() {
                        self.out.push(ReceiverBinding {
                            call_site: node_span(lead),
                            outcome: ReceiverOutcome::Named(self.strings.intern(&owner)),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// The receiver is the lead's named children MINUS the final
    /// navigation_suffix (the callee). Its shape picks the outcome.
    fn nav_outcome(&mut self, lead: Node, site: Span) -> ReceiverOutcome {
        let mut cursor = lead.walk();
        let kids: Vec<Node> = lead
            .children(&mut cursor)
            .filter(|n| n.is_named())
            .collect();
        let Some(recv) = kids.first() else {
            return ReceiverOutcome::Inferred;
        };
        match recv.kind() {
            "simple_identifier" => self.ident_outcome(*recv),
            "this_expression" => self.this_outcome(),
            "navigation_expression" => {
                // The one-hop `a.f`: phase 1 records the chain, resolve
                // upgrades it through the corpus's written member types.
                let (base, member, extra) = nav_parts(*recv, self.src);
                if !extra {
                    if let (Some(base), Some(member)) = (base, member) {
                        if let ReceiverOutcome::Named(base_ty) = self.ident_outcome(base) {
                            let base_name = self.strings.lookup(base_ty).to_string();
                            self.plan
                                .recv_fields
                                .insert((site.start, site.end()), (base_name, member));
                        }
                    }
                }
                ReceiverOutcome::Inferred
            }
            _ => ReceiverOutcome::Inferred,
        }
    }

    /// A receiver identifier: its scope binding, else an uppercase spelling
    /// names the object/companion itself, else the parse could not type it.
    fn ident_outcome(&mut self, ident: Node) -> ReceiverOutcome {
        let text = kt_text(ident, self.src);
        let found = self.lookup(text).cloned();
        match found {
            Some(KtBinding::Named(ty)) => ReceiverOutcome::Named(self.strings.intern(&ty)),
            Some(KtBinding::Ambiguous) => ReceiverOutcome::Ambiguous,
            Some(KtBinding::Inferred) => ReceiverOutcome::Inferred,
            None if text.chars().next().is_some_and(char::is_uppercase) => {
                ReceiverOutcome::Named(self.strings.intern(text))
            }
            None => ReceiverOutcome::Inferred,
        }
    }

    fn this_outcome(&mut self) -> ReceiverOutcome {
        let owner = self.owners.last().cloned();
        match owner {
            Some(owner) => ReceiverOutcome::Named(self.strings.intern(&owner)),
            None => ReceiverOutcome::Inferred,
        }
    }

    // ── scope bookkeeping ────────────────────────────────────────────────────

    fn insert(&mut self, name: String, binding: KtBinding) {
        let Some(frame) = self.scopes.last_mut() else {
            return;
        };
        if let Some(existing) = frame.get(&name) {
            if *existing != binding
                && matches!(existing, KtBinding::Named(_))
                && matches!(binding, KtBinding::Named(_))
            {
                frame.insert(name, KtBinding::Ambiguous);
                return;
            }
        }
        frame.insert(name, binding);
    }

    fn lookup(&self, name: &str) -> Option<&KtBinding> {
        self.scopes.iter().rev().find_map(|frame| frame.get(name))
    }

    fn push_owner(&mut self, span: Span, owner: Option<&str>) {
        self.owners_out.push(MethodOwner {
            span,
            self_type: owner.map(|o| self.strings.intern(o)),
            trait_name: None,
        });
    }
}

// ── grammar readers ──────────────────────────────────────────────────────────

fn is_type_node(kind: &str) -> bool {
    matches!(
        kind,
        "user_type" | "nullable_type" | "function_type" | "parenthesized_type" | "type_identifier"
    )
}

/// (name, written type) of a `val x: T` head: a variable_declaration's
/// simple_identifier plus its first type-node child. Also read directly on a
/// lambda's own variable_declaration parameters.
fn property_head(node: Node, src: &[u8]) -> Option<(String, Option<String>)> {
    let vd = if node.kind() == "variable_declaration" {
        node
    } else {
        kt_first_child(node, "variable_declaration")?
    };
    let mut cursor = vd.walk();
    let kids: Vec<Node> = vd.children(&mut cursor).collect();
    let name = kids
        .iter()
        .find(|n| n.kind() == "simple_identifier")
        .map(|n| kt_text(*n, src).to_string())?;
    let written = kids
        .iter()
        .find(|n| is_type_node(n.kind()))
        .and_then(|ty| written_ty(*ty, src));
    Some((name, written))
}

/// (name, written type) of a primary-constructor `val/var x: T` parameter. A
/// bare constructor argument (no binding_pattern_kind) binds nothing.
fn class_param_head(node: Node, src: &[u8]) -> Option<(String, Option<String>)> {
    let mut cursor = node.walk();
    let kids: Vec<Node> = node.children(&mut cursor).collect();
    if !kids.iter().any(|n| n.kind() == "binding_pattern_kind") {
        return None;
    }
    let name = kids
        .iter()
        .find(|n| n.kind() == "simple_identifier")
        .map(|n| kt_text(*n, src).to_string())?;
    let written = kids
        .iter()
        .find(|n| is_type_node(n.kind()))
        .and_then(|ty| written_ty(*ty, src));
    Some((name, written))
}

/// A written type's principal name: the dotted path's last identifier, generic
/// arguments stripped, `T?` unwrapped. A function type names nothing.
fn written_ty(node: Node, src: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => Some(kt_text(node, src).to_string()),
        "nullable_type" | "parenthesized_type" => {
            let mut cursor = node.walk();
            let kids: Vec<Node> = node.children(&mut cursor).collect();
            kids.into_iter()
                .find(|n| n.is_named())
                .and_then(|inner| written_ty(inner, src))
        }
        "user_type" => {
            let mut cursor = node.walk();
            let mut last: Option<String> = None;
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "simple_identifier" | "type_identifier") {
                    last = Some(kt_text(child, src).to_string());
                }
            }
            last
        }
        _ => None,
    }
}

/// The enclosing function's type parameters: name -> single bound text. A
/// multi-bound `<P : A, B>` names no single type and binds nothing.
fn fn_bounds(node: Node, src: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(tps) = kt_first_child(node, "type_parameters") else {
        return out;
    };
    let mut cursor = tps.walk();
    let params: Vec<Node> = tps
        .children(&mut cursor)
        .filter(|n| n.kind() == "type_parameter")
        .collect();
    for tp in params {
        let mut cc = tp.walk();
        let kids: Vec<Node> = tp.children(&mut cc).collect();
        let Some(name) = kids
            .iter()
            .find(|n| n.kind() == "type_identifier")
            .map(|n| kt_text(*n, src).to_string())
        else {
            continue;
        };
        let bounds: Vec<&Node> = kids.iter().filter(|n| n.kind() == "user_type").collect();
        if let [bound] = bounds.as_slice() {
            if let Some(text) = written_ty(**bound, src) {
                out.insert(name, text);
            }
        }
    }
    out
}

/// (base, member, has-extra) of a one-hop `a.f` navigation: the base is a
/// simple_identifier, the single suffix's member a simple_identifier, and
/// nothing else trails. Any other shape is not a one-hop field chain.
fn nav_parts<'a>(nav: Node<'a>, src: &'a [u8]) -> (Option<Node<'a>>, Option<String>, bool) {
    let mut cursor = nav.walk();
    let kids: Vec<Node> = nav
        .children(&mut cursor)
        .filter(|n| n.is_named())
        .collect();
    let mut base: Option<Node> = None;
    let mut member: Option<String> = None;
    let mut suffixes = 0usize;
    for kid in &kids {
        match kid.kind() {
            "navigation_suffix" => {
                suffixes += 1;
                if suffixes == 1 {
                    member = kt_first_child(*kid, "simple_identifier")
                        .map(|n| kt_text(n, src).to_string());
                }
            }
            _ if base.is_none() => base = Some(*kid),
            _ => return (None, None, true),
        }
    }
    let shaped = suffixes == 1
        && base.is_some_and(|b| b.kind() == "simple_identifier")
        && member.is_some();
    if shaped {
        (base, member, false)
    } else {
        (None, None, true)
    }
}
