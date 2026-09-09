use crate::{Action, Attack, Frame};
use brawllib_rs::high_level_fighter::{CollisionBoxValues, HighLevelSubaction};

#[tracing::instrument(target = "game_content::ingest", skip_all, fields(actions = actions.len()))]
pub fn bake(actions: &[HighLevelSubaction]) -> Vec<Action> {
    actions
        .iter()
        .map(|a| Action {
            iasa: a.iasa,
            landing_lag: a.landing_lag,
            frames: a
                .frames
                .iter()
                .map(|f| Frame {
                    interruptible: f.interruptible,
                    landing_lag: f.landing_lag,
                    x_pos: f.x_pos,
                    y_pos: f.y_pos,
                    hit_boxes: f
                        .hit_boxes
                        .iter()
                        .filter_map(|h| {
                            let CollisionBoxValues::Hit(v) = &h.next_values else {
                                return None;
                            };
                            Some(Attack {
                                id: h.hitbox_id,
                                position: [h.next_pos.x, h.next_pos.y, h.next_pos.z],
                                radius: h.next_size,
                                enabled: v.enabled,
                                aerial: v.aerial,
                                damage: v.damage,
                                kbg: v.kbg as u32,
                                bkb: v.bkb as u32,
                                wdsk: v.wdsk as u32,
                                trajectory: v.trajectory as f32,
                            })
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect()
}
