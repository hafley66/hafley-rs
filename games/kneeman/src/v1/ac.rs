//! The Armored Core overlay (plans/ac-overlay.md): pick up an AC Core and your BODY is
//! replaced — heavy thruster movement plus a c-stick arm gun — while your character's own
//! slots (tilts, jabs, specials, grabs) keep working underneath. This is the item-intercepts-
//! input-lanes idea: the transformation claims the c-stick lane (like aiming items do) and
//! the air-jump lane (boost), and composes with everything it didn't claim.
//!
//! Composition rules (why this file is small):
//! - `ArmWeapon` is DATA. Each weapon resolves to a plain projectile `ItemKind` that is not
//!   AC-coupled: a future ground bazooka pickup fires the same `Rocket`, a turret the same
//!   `PlasmaBall`. The mech owns a trigger, never the bullets.
//! - Movement numbers live in `Tune` (`ac_*` knobs) and act inside the existing Air arm of
//!   the FSM; there is no AC state machine.
//! - The transformation itself is one `Badge` bit. No new fighter mode, no deep links.

use crate::v1::item::{Item, ItemKind, item_logic};
use crate::v1::stage::ToolKind;
use crate::v1::{FxKind, SimState, Tune, Vector2, push_fx};

/// Which arm the core rolled at attach. Stored on the fighter as a plain `u8`
/// (`Fighter.arm`), meaningless unless `Badge::AcCore` is set.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ArmWeapon {
    MachineGun = 0,
    Bazooka = 1,
    EnergyCannon = 2,
}

pub const ARM_WEAPONS: u8 = 3;

/// Frames of mech wear armed on `Badge::AcCore` attach (10s @ 60Hz): plans/ac-ship-backlog.md
/// item 1. The fuel meter is the mid-stock way out of the armored core; a KO is the other --
/// every badge clears on respawn (`fighter.rs::respawn`). Ticks down in
/// `fighters::tick_badge_meters`, set in `acts::attach_badge`.
pub const AC_GAS_FRAMES: i64 = 600;

impl ArmWeapon {
    pub fn from_u8(x: u8) -> Self {
        match x % ARM_WEAPONS {
            0 => ArmWeapon::MachineGun,
            1 => ArmWeapon::Bazooka,
            _ => ArmWeapon::EnergyCannon,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ArmWeapon::MachineGun => "MG",
            ArmWeapon::Bazooka => "BZK",
            ArmWeapon::EnergyCannon => "ENG",
        }
    }
}

/// One weapon's trigger data. `shot` is the decoupling seam: any item/hazard that wants
/// this ammunition spawns the same kind and gets the same flight/hit rules for free.
#[derive(Copy, Clone)]
pub struct ArmSpec {
    pub shot: ItemKind,
    pub cadence: u8, // frames between shots while the c-stick stays deflected
    pub speed: f32,  // muzzle speed (px/s)
    pub range: i64,  // projectile lifetime (frames); for the rocket this is also its fuse
    pub weak: bool,  // laser-bolt weak flag (the machine-gun spray tax)
}

pub const fn arm_spec(w: ArmWeapon) -> ArmSpec {
    match w {
        ArmWeapon::MachineGun => ArmSpec {
            shot: ItemKind::LaserBolt,
            cadence: 5,
            speed: 900.0,
            range: 40,
            weak: true, // sprays fast, stings little
        },
        ArmWeapon::Bazooka => ArmSpec {
            shot: ItemKind::Rocket,
            cadence: 45,
            speed: 520.0,
            range: 70,
            weak: false,
        },
        ArmWeapon::EnergyCannon => ArmSpec {
            shot: ItemKind::PlasmaBall,
            cadence: 55,
            speed: 380.0,
            range: 90,
            weak: false,
        },
    }
}

/// Fire the AC's arm along `aim` (unit c-stick). The trigger half of `fire_gun`, minus the
/// hand-item bookkeeping: no ammo (the core is the mag), cooldown lives on the FIGHTER
/// (`arm_cd`, set here, ticked in the FSM), projectile rides the normal item slots so every
/// existing flight/hit/ricochet rule applies untouched.
pub(crate) fn ac_fire(n: &mut SimState, idx: usize, aim: Vector2, _t: &Tune) {
    let f = n.fighters[idx];
    let w = ArmWeapon::from_u8(f.arm);
    let spec = arm_spec(w);
    let Some(slot) = n.items.iter().position(|x| !x.active()) else {
        return; // field full: the trigger clicks, no cooldown spent
    };
    let dir = aim.normalize_or_zero();
    let dir = if dir == Vector2::ZERO {
        Vector2::new(f.facing, 0.0)
    } else {
        dir
    };
    // shoulder muzzle: above the hand line, pushed out along the shot
    let muzzle = f.pos + Vector2::new(f.facing * 14.0, -66.0) + dir * 26.0;
    n.items[slot] = Item {
        kind: spec.shot,
        pos: muzzle,
        vel: dir * spec.speed,
        owner: idx as i8,
        gas: spec.weak as i64 as f32, // LaserBolt reads 1.0 = weak autofire; others ignore
        gas_max: 1.0,
        timer: spec.range,
        facing: if dir.x == 0.0 {
            f.facing
        } else {
            dir.x.signum()
        },
        tool: ToolKind::TrailPen,
        stroke: 0,
        thrown: false,
        mount: -1,
        hp: crate::v1::items::hurt::item_hp(spec.shot), // projectile shots opt out (0.0)
        cell: None,
    };
    debug_assert!(item_logic(spec.shot).land != crate::v1::item::Land::Settle);
    n.fighters[idx].arm_cd = spec.cadence;
    push_fx(n, FxKind::Muzzle, muzzle);
}
