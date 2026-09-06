use super::{EntityId, RuleSetId, StateId};
use crate::v1::Vector2;
use serde::{Deserialize, Serialize};

pub const SCRIPT_SCALARS: usize = 8;
pub const SCRIPT_INTS: usize = 8;
pub const SCRIPT_VECTORS: usize = 4;
pub const SCRIPT_REFS: usize = 4;

/// Bounded cross-frame residue for one interpreted instance. Authored names compile to these
/// banks; no content-specific Rust field is required for ammo, charge, owner, target, etc.
#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ScriptState {
    pub state: StateId,
    pub frame: u16,
    pub scalars: [f32; SCRIPT_SCALARS],
    pub ints: [i32; SCRIPT_INTS],
    pub vectors: [Vector2; SCRIPT_VECTORS],
    pub refs: [EntityId; SCRIPT_REFS],
    pub flags: u32,
}

impl ScriptState {
    pub const EMPTY: Self = Self {
        state: StateId(0),
        frame: 0,
        scalars: [0.0; SCRIPT_SCALARS],
        ints: [0; SCRIPT_INTS],
        vectors: [Vector2::ZERO; SCRIPT_VECTORS],
        refs: [EntityId::NONE; SCRIPT_REFS],
        flags: 0,
    };

    pub fn read(&self, field: Field) -> Result<Value, StateError> {
        match field {
            Field::State => Ok(Value::State(self.state)),
            Field::Frame => Ok(Value::Int(self.frame as i32)),
            Field::Scalar(i) => self
                .scalars
                .get(i as usize)
                .copied()
                .map(Value::Scalar)
                .ok_or(StateError::OutOfRange(field)),
            Field::Int(i) => self
                .ints
                .get(i as usize)
                .copied()
                .map(Value::Int)
                .ok_or(StateError::OutOfRange(field)),
            Field::Vector(i) => self
                .vectors
                .get(i as usize)
                .copied()
                .map(Value::Vector)
                .ok_or(StateError::OutOfRange(field)),
            Field::Ref(i) => self
                .refs
                .get(i as usize)
                .copied()
                .map(Value::Entity)
                .ok_or(StateError::OutOfRange(field)),
            Field::Flag(i) if i < 32 => Ok(Value::Bool(self.flags & (1_u32 << i) != 0)),
            Field::Flag(_) => Err(StateError::OutOfRange(field)),
        }
    }

    pub fn write(&mut self, field: Field, value: Value) -> Result<(), StateError> {
        match (field, value) {
            (Field::State, Value::State(v)) => self.state = v,
            (Field::Frame, Value::Int(v)) if (0..=u16::MAX as i32).contains(&v) => {
                self.frame = v as u16
            }
            (Field::Scalar(i), Value::Scalar(v)) => {
                *self
                    .scalars
                    .get_mut(i as usize)
                    .ok_or(StateError::OutOfRange(field))? = v
            }
            (Field::Int(i), Value::Int(v)) => {
                *self
                    .ints
                    .get_mut(i as usize)
                    .ok_or(StateError::OutOfRange(field))? = v
            }
            (Field::Vector(i), Value::Vector(v)) => {
                *self
                    .vectors
                    .get_mut(i as usize)
                    .ok_or(StateError::OutOfRange(field))? = v
            }
            (Field::Ref(i), Value::Entity(v)) => {
                *self
                    .refs
                    .get_mut(i as usize)
                    .ok_or(StateError::OutOfRange(field))? = v
            }
            (Field::Flag(i), Value::Bool(v)) if i < 32 => {
                let mask = 1_u32 << i;
                if v {
                    self.flags |= mask;
                } else {
                    self.flags &= !mask;
                }
            }
            _ => return Err(StateError::TypeMismatch { field, value }),
        }
        Ok(())
    }

    pub fn transition(&mut self, state: StateId) {
        self.state = state;
        self.frame = 0;
    }
}

impl Default for ScriptState {
    fn default() -> Self {
        Self::EMPTY
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Field {
    State,
    Frame,
    Scalar(u8),
    Int(u8),
    Vector(u8),
    Ref(u8),
    Flag(u8),
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Value {
    None,
    Bool(bool),
    Int(i32),
    Scalar(f32),
    Vector(Vector2),
    Entity(EntityId),
    State(StateId),
    RuleSet(RuleSetId),
}

impl Value {
    pub fn add(self, rhs: Self) -> Option<Self> {
        match (self, rhs) {
            (Self::Int(a), Self::Int(b)) => a.checked_add(b).map(Self::Int),
            (Self::Scalar(a), Self::Scalar(b)) => Some(Self::Scalar(a + b)),
            (Self::Vector(a), Self::Vector(b)) => Some(Self::Vector(a + b)),
            _ => None,
        }
    }

    pub fn mul(self, rhs: Self) -> Option<Self> {
        match (self, rhs) {
            (Self::Int(a), Self::Int(b)) => a.checked_mul(b).map(Self::Int),
            (Self::Scalar(a), Self::Scalar(b)) => Some(Self::Scalar(a * b)),
            (Self::Vector(a), Self::Scalar(b)) | (Self::Scalar(b), Self::Vector(a)) => {
                Some(Self::Vector(a * b))
            }
            _ => None,
        }
    }

    pub fn clamp(self, lo: Self, hi: Self) -> Option<Self> {
        match (self, lo, hi) {
            (Self::Int(v), Self::Int(lo), Self::Int(hi)) => Some(Self::Int(v.clamp(lo, hi))),
            (Self::Scalar(v), Self::Scalar(lo), Self::Scalar(hi)) => {
                Some(Self::Scalar(v.clamp(lo, hi)))
            }
            _ => None,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum StateError {
    OutOfRange(Field),
    TypeMismatch { field: Field, value: Value },
    InvalidArithmetic { field: Field, rhs: Value },
    InvalidClamp { field: Field, lo: Value, hi: Value },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_state_is_bounded_and_roundtrips() {
        let mut state = ScriptState::EMPTY;
        state.write(Field::Scalar(0), Value::Scalar(12.5)).unwrap();
        state
            .write(Field::Ref(0), Value::Entity(EntityId(7)))
            .unwrap();
        state.write(Field::Flag(3), Value::Bool(true)).unwrap();
        let bytes = bincode::serialize(&state).unwrap();
        let back: ScriptState = bincode::deserialize(&bytes).unwrap();
        assert_eq!(back, state);
        assert_eq!(back.read(Field::Scalar(0)), Ok(Value::Scalar(12.5)));
        assert_eq!(back.read(Field::Flag(3)), Ok(Value::Bool(true)));
    }

    #[test]
    fn transition_resets_only_machine_clock() {
        let mut state = ScriptState::EMPTY;
        state.frame = 19;
        state.scalars[0] = 4.0;
        state.transition(StateId(8));
        assert_eq!(state.state, StateId(8));
        assert_eq!(state.frame, 0);
        assert_eq!(state.scalars[0], 4.0);
    }
}
