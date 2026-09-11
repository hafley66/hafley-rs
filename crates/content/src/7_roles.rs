//! Animation action-role vocabulary and catalog-derived role bindings.
//!
//! One [`ActionRole`] names a semantic clip the shared fighter phase machine can
//! request. [`generate_role_bindings`] derives each role's exact catalog identity
//! by matching the generated catalog's action names; no numeric identity is
//! authored here. A role whose exact source clip is absent is `source: None`;
//! an explicit local substitute is a distinct `fallback`, never conflated with a
//! retained source action. Neither character declares a fallback in this cut.
//!
//! The runtime-facing types are always compiled; only the derivation from
//! [`CatalogEvidence`](crate::CatalogEvidence) requires the `ingest` feature.
use serde::{Deserialize, Serialize};
#[cfg(feature = "ingest")]
use std::collections::BTreeSet;

/// Semantic clip identity requested by the shared fighter phase machine. The
/// variants are character-independent; membership is derived from a character's
/// generated catalog, never authored as an ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ActionRole {
    Idle,
    WalkSlow,
    WalkMiddle,
    WalkFast,
    Dash,
    Run,
    Brake,
    Turn,
    JumpSquat,
    CrouchEnter,
    CrouchHold,
    CrouchExit,
    Jump,
    Fall,
    AirJump,
    Landing,
    LandingLight,
    AirAttack,
    LandingRecovery,
}

impl ActionRole {
    /// Declaration order; the generated binding table follows it.
    pub const ALL: [ActionRole; 19] = [
        ActionRole::Idle,
        ActionRole::WalkSlow,
        ActionRole::WalkMiddle,
        ActionRole::WalkFast,
        ActionRole::Dash,
        ActionRole::Run,
        ActionRole::Brake,
        ActionRole::Turn,
        ActionRole::JumpSquat,
        ActionRole::CrouchEnter,
        ActionRole::CrouchHold,
        ActionRole::CrouchExit,
        ActionRole::Jump,
        ActionRole::Fall,
        ActionRole::AirJump,
        ActionRole::Landing,
        ActionRole::LandingLight,
        ActionRole::AirAttack,
        ActionRole::LandingRecovery,
    ];

    /// Stable readable name, also used as the generated Rust variant path.
    pub fn name(self) -> &'static str {
        match self {
            ActionRole::Idle => "Idle",
            ActionRole::WalkSlow => "WalkSlow",
            ActionRole::WalkMiddle => "WalkMiddle",
            ActionRole::WalkFast => "WalkFast",
            ActionRole::Dash => "Dash",
            ActionRole::Run => "Run",
            ActionRole::Brake => "Brake",
            ActionRole::Turn => "Turn",
            ActionRole::JumpSquat => "JumpSquat",
            ActionRole::CrouchEnter => "CrouchEnter",
            ActionRole::CrouchHold => "CrouchHold",
            ActionRole::CrouchExit => "CrouchExit",
            ActionRole::Jump => "Jump",
            ActionRole::Fall => "Fall",
            ActionRole::AirJump => "AirJump",
            ActionRole::Landing => "Landing",
            ActionRole::LandingLight => "LandingLight",
            ActionRole::AirAttack => "AirAttack",
            ActionRole::LandingRecovery => "LandingRecovery",
        }
    }

    /// Exact retained catalog subaction name this role reads. This is the
    /// mechanical derivation key; a character lacking that exact name gets a
    /// missing role rather than a renamed substitute.
    pub fn source_action(self) -> &'static str {
        match self {
            ActionRole::Idle => "Wait1",
            ActionRole::WalkSlow => "WalkSlow",
            ActionRole::WalkMiddle => "WalkMiddle",
            ActionRole::WalkFast => "WalkFast",
            ActionRole::Dash => "Dash",
            ActionRole::Run => "Run",
            ActionRole::Brake => "RunBrake",
            ActionRole::Turn => "TurnRun",
            ActionRole::JumpSquat => "JumpSquat",
            ActionRole::CrouchEnter => "Squat",
            ActionRole::CrouchHold => "SquatWait",
            ActionRole::CrouchExit => "SquatRv",
            ActionRole::Jump => "JumpF",
            ActionRole::Fall => "Fall",
            ActionRole::AirJump => "JumpAerialF",
            ActionRole::Landing => "LandingHeavy",
            ActionRole::LandingLight => "LandingLight",
            ActionRole::AirAttack => "AttackAirF",
            ActionRole::LandingRecovery => "LandingAirF",
        }
    }

    /// Uppercase constant identifier emitted into the generated Rust module.
    pub fn const_ident(self) -> &'static str {
        match self {
            ActionRole::Idle => "IDLE",
            ActionRole::WalkSlow => "WALK_SLOW",
            ActionRole::WalkMiddle => "WALK_MIDDLE",
            ActionRole::WalkFast => "WALK_FAST",
            ActionRole::Dash => "DASH",
            ActionRole::Run => "RUN",
            ActionRole::Brake => "BRAKE",
            ActionRole::Turn => "TURN",
            ActionRole::JumpSquat => "JUMP_SQUAT",
            ActionRole::CrouchEnter => "CROUCH_ENTER",
            ActionRole::CrouchHold => "CROUCH_HOLD",
            ActionRole::CrouchExit => "CROUCH_EXIT",
            ActionRole::Jump => "JUMP",
            ActionRole::Fall => "FALL",
            ActionRole::AirJump => "AIR_JUMP",
            ActionRole::Landing => "LANDING",
            ActionRole::LandingLight => "LANDING_LIGHT",
            ActionRole::AirAttack => "AIR_ATTACK",
            ActionRole::LandingRecovery => "LANDING_RECOVERY",
        }
    }
}

/// One role's resolved catalog identity. `source` is the exact retained action;
/// `fallback` is a distinct, explicitly local substitute. A missing role keeps
/// `source: None` and does not silently borrow a fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleBinding {
    pub role: ActionRole,
    pub source: Option<usize>,
    pub fallback: Option<usize>,
}

impl RoleBinding {
    /// The clip a selector consumes: exact source first, explicit fallback
    /// second, `None` when the role is unavailable.
    pub fn selected(self) -> Option<usize> {
        self.source.or(self.fallback)
    }
}

/// Every role for one character in [`ActionRole::ALL`] order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleBindings {
    pub runtime: String,
    pub roles: Vec<RoleBinding>,
}

impl RoleBindings {
    pub fn get(&self, role: ActionRole) -> Option<RoleBinding> {
        self.roles.iter().copied().find(|binding| binding.role == role)
    }

    /// Exact retained source action for `role`, if the character has it.
    pub fn source(&self, role: ActionRole) -> Option<usize> {
        self.get(role).and_then(|binding| binding.source)
    }

    /// Explicit local substitute for `role`, separate from a source action.
    pub fn fallback(&self, role: ActionRole) -> Option<usize> {
        self.get(role).and_then(|binding| binding.fallback)
    }

    /// The clip a selector consumes for `role`.
    pub fn selected(&self, role: ActionRole) -> Option<usize> {
        self.get(role).and_then(RoleBinding::selected)
    }

    /// Roles with no exact retained source clip, regardless of fallback.
    pub fn missing(&self) -> Vec<ActionRole> {
        self.roles
            .iter()
            .filter(|binding| binding.source.is_none())
            .map(|binding| binding.role)
            .collect()
    }

    /// Roles with neither a source clip nor a fallback.
    pub fn unavailable(&self) -> Vec<ActionRole> {
        self.roles
            .iter()
            .filter(|binding| binding.selected().is_none())
            .map(|binding| binding.role)
            .collect()
    }
}

/// One explicitly declared local substitute: bind `role` to `action` when the
/// character retains no exact source clip for it. This is character policy, not
/// a derived source relationship.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoleFallback<'a> {
    pub role: ActionRole,
    pub action: &'a str,
}

/// Why role binding derivation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleError {
    DuplicateFallback { role: ActionRole },
    FallbackForBoundRole { role: ActionRole },
    FallbackUnknownAction { role: ActionRole, action: String },
}

impl std::fmt::Display for RoleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoleError::DuplicateFallback { role } => {
                write!(f, "duplicate fallback for role {}", role.name())
            }
            RoleError::FallbackForBoundRole { role } => write!(
                f,
                "role {} has a retained source action and must not declare a fallback",
                role.name()
            ),
            RoleError::FallbackUnknownAction { role, action } => write!(
                f,
                "fallback {} for role {} is not a retained catalog action",
                action,
                role.name()
            ),
        }
    }
}

impl std::error::Error for RoleError {}

/// Derive one binding per [`ActionRole::ALL`] entry from the generated catalog
/// evidence. Membership comes only from exact action-name matches; declared
/// fallbacks are validated and kept separate from source identities.
#[cfg(feature = "ingest")]
pub fn generate_role_bindings(
    evidence: &crate::CatalogEvidence,
    fallbacks: &[RoleFallback<'_>],
) -> Result<RoleBindings, RoleError> {
    let mut declared = BTreeSet::new();
    for fallback in fallbacks {
        if !declared.insert(fallback.role) {
            return Err(RoleError::DuplicateFallback { role: fallback.role });
        }
    }
    let mut roles = Vec::with_capacity(ActionRole::ALL.len());
    for role in ActionRole::ALL {
        let source = evidence
            .entries
            .iter()
            .find(|entry| entry.name == role.source_action())
            .map(|entry| entry.id);
        let fallback = match fallbacks.iter().find(|fallback| fallback.role == role) {
            None => None,
            Some(fallback) => {
                if source.is_some() {
                    return Err(RoleError::FallbackForBoundRole { role });
                }
                let entry = evidence
                    .entries
                    .iter()
                    .find(|entry| entry.name == fallback.action)
                    .ok_or_else(|| RoleError::FallbackUnknownAction {
                        role,
                        action: fallback.action.into(),
                    })?;
                Some(entry.id)
            }
        };
        roles.push(RoleBinding { role, source, fallback });
    }
    Ok(RoleBindings { runtime: evidence.runtime.clone(), roles })
}

fn option_literal(value: Option<usize>) -> String {
    match value {
        Some(id) => format!("Some({id})"),
        None => "None".into(),
    }
}

/// Render the source-free generated Rust module for one character's bindings.
/// The module exposes a named `Option<usize>` constant per role plus the full
/// const table; consumers select by role name, never by a handwritten ID.
pub fn role_bindings_source(bindings: &RoleBindings) -> String {
    let mut output = String::from(concat!(
        "// Generated by smash-import from the committed catalog. Do not edit.\n",
        "#![allow(dead_code)]\n\n",
        "use game_content::{ActionRole, RoleBinding, RoleBindings};\n\n",
    ));
    for binding in &bindings.roles {
        output.push_str(&format!(
            "pub const {}: Option<usize> = {};\n",
            binding.role.const_ident(),
            option_literal(binding.source),
        ));
    }
    output.push('\n');
    for binding in &bindings.roles {
        output.push_str(&format!(
            "pub const {}_FALLBACK: Option<usize> = {};\n",
            binding.role.const_ident(),
            option_literal(binding.fallback),
        ));
    }
    output.push('\n');
    output.push_str(&format!(
        "pub const BINDINGS: [RoleBinding; {}] = [\n",
        bindings.roles.len(),
    ));
    for binding in &bindings.roles {
        output.push_str(&format!(
            "    RoleBinding {{ role: ActionRole::{}, source: {}, fallback: {}_FALLBACK }},\n",
            binding.role.name(),
            binding.role.const_ident(),
            binding.role.const_ident(),
        ));
    }
    output.push_str("];\n\n");
    output.push_str(&format!(
        "pub fn bindings() -> RoleBindings {{\n    RoleBindings {{ runtime: {runtime:?}.to_string(), roles: BINDINGS.to_vec() }}\n}}\n",
        runtime = bindings.runtime,
    ));
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Missing roles stay distinct from fallbacks, and `unavailable` is the
    /// intersection with neither.
    #[test]
    fn missing_and_fallback_are_distinct() {
        let bindings = RoleBindings {
            runtime: "probe".into(),
            roles: vec![
                RoleBinding { role: ActionRole::Idle, source: Some(0), fallback: None },
                RoleBinding { role: ActionRole::Fall, source: None, fallback: Some(4) },
                RoleBinding { role: ActionRole::LandingLight, source: None, fallback: None },
            ],
        };
        assert_eq!(bindings.source(ActionRole::Idle), Some(0));
        assert_eq!(bindings.selected(ActionRole::Fall), Some(4));
        assert_eq!(bindings.fallback(ActionRole::Fall), Some(4));
        assert_eq!(bindings.missing(), vec![ActionRole::Fall, ActionRole::LandingLight]);
        assert_eq!(bindings.unavailable(), vec![ActionRole::LandingLight]);
    }

    /// The emitter renders named option constants and a named const table.
    #[test]
    fn emitted_source_names_roles_without_numeric_literals_in_consumers() {
        let bindings = RoleBindings {
            runtime: "probe".into(),
            roles: vec![
                RoleBinding { role: ActionRole::Idle, source: Some(3), fallback: None },
                RoleBinding { role: ActionRole::LandingLight, source: None, fallback: None },
            ],
        };
        let source = role_bindings_source(&bindings);
        assert!(source.contains("pub const IDLE: Option<usize> = Some(3);"));
        assert!(source.contains("pub const LANDING_LIGHT: Option<usize> = None;"));
        assert!(source.contains("RoleBinding { role: ActionRole::Idle, source: IDLE, fallback: IDLE_FALLBACK }"));
        assert!(source.contains("runtime: \"probe\".to_string()"));
    }
}
