//! Shared skeletons and their tagged moves (see
//! `docs/33-animation-engine.md`). Optional tables in a model pack: a pack
//! without them (the building packs, and model packs baked before them)
//! loads as before, its models posed from their own bone rows
//! ([`crate::model`]).
//!
//! - `anim/skeletons`: one [`SkeletonRecord`] per skeleton.
//! - `anim/joints`: every skeleton's [`JointRecord`]s, back to back, in the
//!   order of the bones its models' vertices name.
//! - `anim/moves`: one [`MoveRecord`] per move, each on one skeleton.
//! - `anim/frames`: the joint poses of every frame of every move, a block
//!   per skeleton. A frame row is one [`JointPose`] per joint of its skeleton
//!   (eight half floats: its rotation, a unit quaternion x y z w, then its
//!   translation and uniform scale, both relative to its parent), so a row
//!   costs 16 bytes a joint. Rows count from the start of their skeleton's
//!   block, which is how joining packs leaves every move's rows alone.
//! - `anim/fits`: one [`FitRecord`] per model posed from a skeleton, sorted
//!   by model.
//! - `anim/fixes`: per fitted model, the matrix that turns the skeleton's
//!   space into the model's (its size and turn), then one per bone that turns
//!   the model's stored vertices into that bone's rest (its bind pose
//!   difference), each the three rows of a 3 × 4 matrix.
//! - `anim/pairs`, optional even beside the others: one [`PairRecord`] per
//!   matched kill, the two halves of one move made for two men. A pack with
//!   no kills leaves it out.

use alloc::vec::Vec;
use core::fmt;

use bytemuck::{Pod, Zeroable};

use crate::model::{MAX_BONES, Models, NAME_BYTES};
use crate::{Pack, TableError, read_table};

/// Bumped whenever a record's layout or meaning changes.
pub const ANIM_SCHEMA: u32 = 1;
pub const SKELETONS_PATH: &str = "anim/skeletons";
pub const JOINTS_PATH: &str = "anim/joints";
pub const MOVES_PATH: &str = "anim/moves";
pub const FRAMES_PATH: &str = "anim/frames";
pub const FITS_PATH: &str = "anim/fits";
pub const FIXES_PATH: &str = "anim/fixes";
pub const PAIRS_PATH: &str = "anim/pairs";

/// The anim tables that come all together, in the order
/// [`AnimTables::write`] writes them (`anim/pairs` may follow).
const PATHS: [&str; 6] = [
    SKELETONS_PATH,
    JOINTS_PATH,
    MOVES_PATH,
    FRAMES_PATH,
    FITS_PATH,
    FIXES_PATH,
];

/// Most joints a skeleton may have: a model's bones, less the still one.
pub const MAX_JOINTS: u32 = MAX_BONES;
/// Floats in one fix (a 3 × 4 matrix, row by row).
pub const FIX_FLOATS: usize = 12;
/// No parent: a root joint.
pub const ROOT: u8 = u8::MAX;

/// One joint in one frame: the bits of eight half floats, two to a word
/// (low half first): rotation x y, z w, translation x y, z and scale.
pub type JointPose = [u32; 4];

/// The parts of a body a layered move can play on, by joint set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Mask {
    /// Every joint.
    All = 0,
    /// The hips and legs: footwork under a swing.
    Legs = 1,
    /// From the waist up.
    Upper = 2,
    /// The neck and head.
    Head = 3,
    /// The left arm, from the shoulder.
    LeftArm = 4,
    /// The right arm, from the shoulder.
    RightArm = 5,
}

impl Mask {
    pub const ALL: [Mask; 6] = [
        Mask::All,
        Mask::Legs,
        Mask::Upper,
        Mask::Head,
        Mask::LeftArm,
        Mask::RightArm,
    ];

    pub fn from_u8(v: u8) -> Option<Mask> {
        Mask::ALL.get(v as usize).copied()
    }
}

/// What a joint is to the engine: the joints look-at turns and foot IK
/// bends. Found by the baker from the joint names (Quaternius and Mixamo
/// names), or set in the model's config.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Role {
    None = 0,
    Hips = 1,
    Spine = 2,
    Chest = 3,
    Neck = 4,
    Head = 5,
    UpperLegL = 6,
    LowerLegL = 7,
    FootL = 8,
    UpperLegR = 9,
    LowerLegR = 10,
    FootR = 11,
    ShoulderL = 12,
    ShoulderR = 13,
}

impl Role {
    pub const ALL: [Role; 14] = [
        Role::None,
        Role::Hips,
        Role::Spine,
        Role::Chest,
        Role::Neck,
        Role::Head,
        Role::UpperLegL,
        Role::LowerLegL,
        Role::FootL,
        Role::UpperLegR,
        Role::LowerLegR,
        Role::FootR,
        Role::ShoulderL,
        Role::ShoulderR,
    ];

    pub fn from_u8(v: u8) -> Option<Role> {
        Role::ALL.get(v as usize).copied()
    }
}

/// One skeleton. 40 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct SkeletonRecord {
    /// Zero-padded UTF-8: the rig's name, for tools.
    pub name: [u8; NAME_BYTES],
    pub first_joint: u32,
    pub joints: u32,
    /// Its block of the frames table: the first joint pose, and how many
    /// rows (`joints` poses each) it holds.
    pub first_pose: u32,
    pub rows: u32,
}

/// One joint. 4 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct JointRecord {
    /// The parent joint, or [`ROOT`].
    pub parent: u8,
    /// One bit per [`Mask`] that covers this joint (`Mask::All` always).
    pub masks: u8,
    /// The joint that mirrors this one across the body (itself on the
    /// middle line).
    pub mirror: u8,
    /// See [`Role`].
    pub role: u8,
}

/// What a move is for: the fight picks moves by these, falling back to the
/// model's plain clip kinds when its skeleton has none tagged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Act {
    Idle = 0,
    Walk = 1,
    Run = 2,
    Guard = 3,
    Attack = 4,
    Parry = 5,
    Block = 6,
    Hit = 7,
    Stagger = 8,
    Knockdown = 9,
    GetUp = 10,
    Death = 11,
    Cheer = 12,
    Shoot = 13,
    /// A step in a fight: forward, back or to a side (by [`Side`]).
    Step = 14,
    /// A flinch played on top of whatever else (an additive layer).
    Flinch = 15,
    /// The killer's half of a matched kill, played with its victim's
    /// [`Act::Killed`] half. Both halves carry a contact, so both land it at
    /// [`BLOW_AT`] of the move; the game pairs them through
    /// `gc_game::models::Clips::kill_pair`, never by picking these alone.
    Kill = 16,
    /// The victim's half of a matched kill.
    Killed = 17,
}

impl Act {
    pub const ALL: [Act; 18] = [
        Act::Idle,
        Act::Walk,
        Act::Run,
        Act::Guard,
        Act::Attack,
        Act::Parry,
        Act::Block,
        Act::Hit,
        Act::Stagger,
        Act::Knockdown,
        Act::GetUp,
        Act::Death,
        Act::Cheer,
        Act::Shoot,
        Act::Step,
        Act::Flinch,
        Act::Kill,
        Act::Killed,
    ];

    pub fn from_u8(v: u8) -> Option<Act> {
        Act::ALL.get(v as usize).copied()
    }
}

/// Which way a move goes or a blow came from, seen by the man playing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Side {
    Any = 0,
    Front = 1,
    Back = 2,
    Left = 3,
    Right = 4,
    High = 5,
    Low = 6,
}

impl Side {
    pub const ALL: [Side; 7] = [
        Side::Any,
        Side::Front,
        Side::Back,
        Side::Left,
        Side::Right,
        Side::High,
        Side::Low,
    ];

    pub fn from_u8(v: u8) -> Option<Side> {
        Side::ALL.get(v as usize).copied()
    }

    /// The side seen in a mirror.
    pub fn mirrored(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            s => s,
        }
    }
}

/// The weapon a move is made for (`Any` for the body's own moves: walking,
/// dying, a Brood claw).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Weapon {
    Any = 0,
    SwordShield = 1,
    Spear = 2,
    TwoHanded = 3,
    Polearm = 4,
    AxeMace = 5,
    Bow = 6,
    Claws = 7,
}

impl Weapon {
    pub const ALL: [Weapon; 8] = [
        Weapon::Any,
        Weapon::SwordShield,
        Weapon::Spear,
        Weapon::TwoHanded,
        Weapon::Polearm,
        Weapon::AxeMace,
        Weapon::Bow,
        Weapon::Claws,
    ];

    pub fn from_u8(v: u8) -> Option<Weapon> {
        Weapon::ALL.get(v as usize).copied()
    }
}

/// How far into an attack move its blow lands, as a share of the move: the
/// game's swing lands its blow there, and the baker retimes a move made with
/// its blow elsewhere to match. Packs carry it baked in, so changing it
/// needs a re-bake of every pack with timed attacks (and a baker version
/// bump, so CI flags the stale ones).
pub const BLOW_AT: f32 = 0.45;

/// [`MoveRecord::flags`]: the move repeats (walking) rather than playing
/// once and holding (dying).
pub const LOOPS: u8 = 1;
/// [`MoveRecord::flags`]: the move is stored as a change from its first
/// frame and played on top of another.
pub const ADDITIVE: u8 = 2;
/// [`MoveRecord::flags`]: the move may be played mirrored (its skeleton is
/// symmetric).
pub const MIRRORS: u8 = 4;

/// No plain clip kind: [`MoveRecord::kind`] of a move that stands in for
/// none.
pub const NO_KIND: u8 = u8::MAX;

/// One move on one skeleton. 48 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct MoveRecord {
    /// Zero-padded UTF-8: the source animation's name.
    pub name: [u8; NAME_BYTES],
    pub skeleton: u16,
    /// The plain [`ClipKind`](crate::model::ClipKind) this move stands in
    /// for, or [`NO_KIND`].
    pub kind: u8,
    /// See [`Act`], [`Weapon`], [`Side`] and [`Mask`].
    pub act: u8,
    pub weapon: u8,
    pub side: u8,
    pub mask: u8,
    /// [`LOOPS`], [`ADDITIVE`], [`MIRRORS`].
    pub flags: u8,
    /// The row of its first frame in its skeleton's block.
    pub first_row: u32,
    pub frames: u32,
    /// Frames per second the move was sampled at.
    pub fps: f32,
}

/// The tags of a move in a checked pack (the loader rejects any other).
impl MoveRecord {
    pub fn name(&self) -> &str {
        crate::model::name_str(&self.name)
    }

    pub fn act(&self) -> Act {
        Act::from_u8(self.act).unwrap_or(Act::Idle)
    }

    pub fn weapon(&self) -> Weapon {
        Weapon::from_u8(self.weapon).unwrap_or(Weapon::Any)
    }

    pub fn side(&self) -> Side {
        Side::from_u8(self.side).unwrap_or(Side::Any)
    }

    pub fn mask(&self) -> Mask {
        Mask::from_u8(self.mask).unwrap_or(Mask::All)
    }

    pub fn loops(&self) -> bool {
        self.flags & LOOPS != 0
    }

    pub fn additive(&self) -> bool {
        self.flags & ADDITIVE != 0
    }
}

/// A model posed from a skeleton. 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Pod, Zeroable)]
pub struct FitRecord {
    pub model: u32,
    pub skeleton: u32,
    /// The first of the model's fixes: its skeleton-to-model matrix, then one
    /// per bone (the model's bones, the still bone last).
    pub first_fix: u32,
    /// What the model holds, a [`Weapon`] (0, any, in packs baked before
    /// it): the picker prefers moves made for it.
    pub weapon: u8,
    pub _pad: [u8; 3],
}

/// A matched kill: the killer's half (an [`Act::Kill`] move) and the
/// victim's ([`Act::Killed`]), as long as each other, sampled alike and
/// meeting at `contact`; the two may be on different skeletons. Unlike an
/// attack a kill is not retimed, so it plays as made. 16 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct PairRecord {
    pub kill: u32,
    pub killed: u32,
    /// How far apart the two men stand, origin to origin, the victim facing
    /// the killer, in the killer's skeleton's space (the killer's size, from his
    /// first fix, turns it into tiles).
    pub reach: f32,
    /// How far through both halves (0 to 1, as `MoveClip::at` counts) the
    /// blow lands.
    pub contact: f32,
}

impl FitRecord {
    pub fn weapon(&self) -> Weapon {
        Weapon::from_u8(self.weapon).unwrap_or(Weapon::Any)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimError {
    Table(&'static str, TableError),
    /// Some tables are there and some are not.
    Partial,
    /// A skeleton, joint, move, fit or pair points outside its tables or
    /// breaks their rules.
    Bad(&'static str, usize),
}

impl fmt::Display for AnimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AnimError::Table(p, e) => write!(f, "{p}: {e}"),
            AnimError::Partial => write!(f, "model pack has only some of the anim tables"),
            AnimError::Bad(what, k) => write!(f, "anim {what} {k} is malformed"),
        }
    }
}

impl core::error::Error for AnimError {}

/// The skeletons and moves in a pack, borrowed in place.
#[derive(Clone, Copy, Debug, Default)]
pub struct Anim<'a> {
    pub skeletons: &'a [SkeletonRecord],
    pub joints: &'a [JointRecord],
    pub moves: &'a [MoveRecord],
    /// Every skeleton's block of joint poses.
    pub frames: &'a [JointPose],
    pub fits: &'a [FitRecord],
    pub fixes: &'a [[f32; FIX_FLOATS]],
    pub pairs: &'a [PairRecord],
}

impl<'a> Anim<'a> {
    /// Read and check every table, against the pack's `models`. A pack with
    /// none of them has no skeletons (an empty `Anim`).
    pub fn from_pack(pack: &Pack<'a>, models: &Models<'_>) -> Result<Anim<'a>, AnimError> {
        let present = PATHS.iter().filter(|p| pack.get_path(p).is_some()).count();
        if present == 0 {
            return match pack.get_path(PAIRS_PATH) {
                Some(_) => Err(AnimError::Partial),
                None => Ok(Anim::default()),
            };
        }
        if present < PATHS.len() {
            return Err(AnimError::Partial);
        }
        fn table<'a, T: Pod>(pack: &Pack<'a>, path: &'static str) -> Result<&'a [T], AnimError> {
            let entry = pack.get_path(path).ok_or(AnimError::Partial)?;
            read_table(ANIM_SCHEMA, entry.bytes).map_err(|e| AnimError::Table(path, e))
        }
        let a = Anim {
            skeletons: table(pack, SKELETONS_PATH)?,
            joints: table(pack, JOINTS_PATH)?,
            moves: table(pack, MOVES_PATH)?,
            frames: table(pack, FRAMES_PATH)?,
            fits: table(pack, FITS_PATH)?,
            fixes: table(pack, FIXES_PATH)?,
            pairs: match pack.get_path(PAIRS_PATH) {
                Some(entry) => read_table(ANIM_SCHEMA, entry.bytes)
                    .map_err(|e| AnimError::Table(PAIRS_PATH, e))?,
                None => &[],
            },
        };
        a.check(models)?;
        Ok(a)
    }

    fn check(&self, models: &Models<'_>) -> Result<(), AnimError> {
        for (k, s) in self.skeletons.iter().enumerate() {
            let bad = AnimError::Bad("skeleton", k);
            let end = s.first_joint as usize + s.joints as usize;
            let Some(joints) = self.joints.get(s.first_joint as usize..end) else {
                return Err(bad);
            };
            let poses = s.rows as usize * s.joints as usize;
            if s.joints == 0
                || s.joints > MAX_JOINTS
                || s.rows as usize > crate::model::MAX_ROWS
                || s.first_pose as usize + poses > self.frames.len()
                || depths(joints).is_none()
            {
                return Err(bad);
            }
            for joint in joints {
                if joint.masks & 1 == 0
                    || joint.masks >> Mask::ALL.len() != 0
                    || joint.mirror as usize >= joints.len()
                    || Role::from_u8(joint.role).is_none()
                {
                    return Err(bad);
                }
            }
        }
        for (k, m) in self.moves.iter().enumerate() {
            if m.skeleton as usize >= self.skeletons.len()
                || Act::from_u8(m.act).is_none()
                || Weapon::from_u8(m.weapon).is_none()
                || Side::from_u8(m.side).is_none()
                || Mask::from_u8(m.mask).is_none()
                || (m.kind != NO_KIND && crate::model::ClipKind::from_u8(m.kind).is_none())
                || m.frames == 0
                || m.fps.is_nan()
                || m.fps <= 0.0
                || m.first_row as usize + m.frames as usize
                    > self.skeletons[m.skeleton as usize].rows as usize
            {
                return Err(AnimError::Bad("move", k));
            }
        }
        for (k, f) in self.fits.iter().enumerate() {
            let bad = AnimError::Bad("fit", k);
            if k > 0 && self.fits[k - 1].model >= f.model {
                return Err(bad);
            }
            let (Some(model), Some(skeleton)) = (
                models.models.get(f.model as usize),
                self.skeletons.get(f.skeleton as usize),
            ) else {
                return Err(bad);
            };
            // The model's bones are the skeleton's joints plus the still one.
            if model.bones != skeleton.joints + 1
                || f.first_fix as usize + 1 + model.bones as usize > self.fixes.len()
                || Weapon::from_u8(f.weapon).is_none()
            {
                return Err(bad);
            }
        }
        for (k, p) in self.pairs.iter().enumerate() {
            let half = |m: u32, act: Act| self.moves.get(m as usize).filter(|r| r.act() == act);
            let (Some(kill), Some(killed)) = (half(p.kill, Act::Kill), half(p.killed, Act::Killed))
            else {
                return Err(AnimError::Bad("pair", k));
            };
            if kill.frames != killed.frames
                || kill.fps != killed.fps
                || !(p.reach.is_finite() && p.reach > 0.0)
                || !(p.contact > 0.0 && p.contact < 1.0)
            {
                return Err(AnimError::Bad("pair", k));
            }
        }
        Ok(())
    }

    /// What model `model` holds: [`Weapon::Any`] unless its fit names one.
    pub fn weapon_of(&self, model: usize) -> Weapon {
        self.fit(model).map_or(Weapon::Any, FitRecord::weapon)
    }

    /// The fit of `model`, if it is posed from a skeleton.
    pub fn fit(&self, model: usize) -> Option<&'a FitRecord> {
        let k = self
            .fits
            .binary_search_by_key(&(model as u32), |f| f.model)
            .ok()?;
        Some(&self.fits[k])
    }

    /// The joints of skeleton `s`.
    pub fn joints_of(&self, s: usize) -> &'a [JointRecord] {
        let r = &self.skeletons[s];
        &self.joints[r.first_joint as usize..(r.first_joint + r.joints) as usize]
    }

    /// The moves on skeleton `s`.
    pub fn moves_of(&self, s: usize) -> impl Iterator<Item = (usize, &'a MoveRecord)> + 'a {
        let moves = self.moves;
        moves
            .iter()
            .enumerate()
            .filter(move |(_, m)| m.skeleton as usize == s)
    }
}

/// Each joint's depth (joints between it and its root, a root 0), or `None`
/// if a parent is not one of `joints` or the parents loop.
pub fn depths(joints: &[JointRecord]) -> Option<Vec<u8>> {
    joints
        .iter()
        .map(|j| {
            let mut depth = 0u8;
            let mut parent = j.parent;
            while parent != ROOT {
                depth += 1;
                if depth as usize > joints.len() {
                    return None;
                }
                parent = joints.get(parent as usize)?.parent;
            }
            Some(depth)
        })
        .collect()
}

/// Everything the anim tables hold, for writing them (or joining packs).
#[derive(Default)]
pub struct AnimTables {
    pub skeletons: Vec<SkeletonRecord>,
    pub joints: Vec<JointRecord>,
    pub moves: Vec<MoveRecord>,
    pub frames: Vec<JointPose>,
    pub fits: Vec<FitRecord>,
    pub fixes: Vec<[f32; FIX_FLOATS]>,
    pub pairs: Vec<PairRecord>,
}

impl AnimTables {
    /// Add `a`, whose fits name models counted from `first_model` here.
    pub fn append(&mut self, a: &Anim<'_>, first_model: u32) {
        let (skeletons, joints, poses, fixes, moves) = (
            self.skeletons.len() as u32,
            self.joints.len() as u32,
            self.frames.len() as u32,
            self.fixes.len() as u32,
            self.moves.len() as u32,
        );
        self.skeletons
            .extend(a.skeletons.iter().map(|s| SkeletonRecord {
                first_joint: s.first_joint + joints,
                first_pose: s.first_pose + poses,
                ..*s
            }));
        self.joints.extend_from_slice(a.joints);
        self.moves.extend(a.moves.iter().map(|m| MoveRecord {
            skeleton: m.skeleton + skeletons as u16,
            ..*m
        }));
        self.frames.extend_from_slice(a.frames);
        self.fits.extend(a.fits.iter().map(|f| FitRecord {
            model: f.model + first_model,
            skeleton: f.skeleton + skeletons,
            first_fix: f.first_fix + fixes,
            ..*f
        }));
        self.fixes.extend_from_slice(a.fixes);
        self.pairs.extend(a.pairs.iter().map(|p| PairRecord {
            kill: p.kill + moves,
            killed: p.killed + moves,
            ..*p
        }));
    }

    pub fn is_empty(&self) -> bool {
        self.skeletons.is_empty()
    }

    /// Write the tables, unless there are none (a pack without skeletons
    /// carries no anim tables at all).
    pub fn write(&self, pack: &mut crate::PackWriter) {
        use crate::{Kind, table_bytes};
        if self.is_empty() {
            return;
        }
        let tables = [
            table_bytes(ANIM_SCHEMA, &self.skeletons),
            table_bytes(ANIM_SCHEMA, &self.joints),
            table_bytes(ANIM_SCHEMA, &self.moves),
            table_bytes(ANIM_SCHEMA, &self.frames),
            table_bytes(ANIM_SCHEMA, &self.fits),
            table_bytes(ANIM_SCHEMA, &self.fixes),
        ];
        for (path, bytes) in PATHS.into_iter().zip(&tables) {
            pack.add(path, Kind::Table, bytes);
        }
        if !self.pairs.is_empty() {
            pack.add(
                PAIRS_PATH,
                Kind::Table,
                &table_bytes(ANIM_SCHEMA, &self.pairs),
            );
        }
    }
}
