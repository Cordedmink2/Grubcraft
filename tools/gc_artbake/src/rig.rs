//! Shared skeletons (docs/33-animation-engine.md): a skinned model's joints,
//! and its clips stored as joint poses that every model on the same skeleton
//! shares, instead of finished bone matrices of its own.
//!
//! A joint's pose is its rotation, translation and uniform scale relative to
//! its nearest ancestor joint (a root joint's, relative to the scene). The
//! model then needs only its fixes: `pre`, which turns the skeleton's space
//! into the model's (its size and turn), and per bone `post`, which turns a
//! stored vertex into that joint's rest (the inverse rest pose and the bind
//! pose difference). The bone matrix of a frame is `pre * global * post`,
//! which is exactly the matrix the bone rows hold for a model without a
//! skeleton.

use std::collections::HashMap;

use gc_assets::anim::{
    Act, AnimTables, FIX_FLOATS, FitRecord, JointRecord, LOOPS, Mask, MoveRecord, NO_KIND, ROOT,
    Role, Side, SkeletonRecord, Weapon,
};
use gc_assets::model::{ClipKind, ClipRecord, MAX_ROWS, NAME_BYTES, f16_bits};
use glam::{Mat4, Quat, Vec3};

/// A joint pose must rebuild its matrix this closely (in the source's units,
/// against the matrix's largest entry), or the model keeps bone rows: a rig
/// with shear or a squashed joint cannot be stored as rotation, translation
/// and one scale.
const FIT_TOLERANCE: f32 = 1e-3;

/// A skinned model's joints, in skin order (the bone order its vertices use).
/// Models share a skeleton, and so its moves, only when their rigs are equal:
/// the same joints, parents and rest pose, since a move's poses carry its
/// bones' lengths and its root the rig's own placement.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Rig {
    pub names: Vec<String>,
    /// The nearest ancestor that is also a joint.
    pub parent: Vec<Option<usize>>,
    /// Each joint's rest relative to its parent joint (a root's, to the
    /// scene), rounded to `REST_STEP`.
    pub rest: Vec<[i32; 16]>,
}

/// Rest poses this close count as the same rig.
const REST_STEP: f32 = 1e-4;

impl Rig {
    /// The joints `joints` (node indices) of a scene whose nodes have parents
    /// `node_parent` and names `node_name`.
    pub fn new(
        joints: &[usize],
        node_parent: &[Option<usize>],
        node_name: &[String],
        node_rest: &[Mat4],
    ) -> Rig {
        let joint_of: HashMap<usize, usize> =
            joints.iter().enumerate().map(|(j, &n)| (n, j)).collect();
        let parent = joints
            .iter()
            .map(|&n| {
                let mut at = node_parent[n];
                while let Some(p) = at {
                    if let Some(&j) = joint_of.get(&p) {
                        return Some(j);
                    }
                    at = node_parent[p];
                }
                None
            })
            .collect::<Vec<Option<usize>>>();
        let rest = joints
            .iter()
            .zip(&parent)
            .map(|(&n, p)| {
                let local = match p {
                    Some(p) => node_rest[joints[*p]].inverse() * node_rest[n],
                    None => node_rest[n],
                };
                local
                    .to_cols_array()
                    .map(|v| (v / REST_STEP).round() as i32)
            })
            .collect();
        Rig {
            names: joints.iter().map(|&n| node_name[n].clone()).collect(),
            parent,
            rest,
        }
    }

    /// The joints at and under `j`.
    fn subtree(&self, j: usize) -> Vec<usize> {
        (0..self.names.len())
            .filter(|&k| {
                let mut at = Some(k);
                while let Some(a) = at {
                    if a == j {
                        return true;
                    }
                    at = self.parent[a];
                }
                false
            })
            .collect()
    }
}

/// One joint in one frame, relative to its parent joint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct JointPose {
    pub r: Quat,
    pub t: Vec3,
    pub s: f32,
}

impl JointPose {
    /// As the frames table stores it: eight half floats, or `None` if a
    /// value is beyond them (or not a number).
    fn packed(&self) -> Option<gc_assets::anim::JointPose> {
        let (r, t) = (self.r, self.t);
        let all = [r.x, r.y, r.z, r.w, t.x, t.y, t.z, self.s];
        if all.iter().any(|v| !v.is_finite() || v.abs() > 65_504.0) {
            return None;
        }
        let h = |a: f32, b: f32| u32::from(f16_bits(a)) | (u32::from(f16_bits(b)) << 16);
        Some([h(r.x, r.y), h(r.z, r.w), h(t.x, t.y), h(t.z, self.s)])
    }

    /// Back from [`JointPose::packed`].
    #[cfg(test)]
    pub fn unpacked(p: &gc_assets::anim::JointPose) -> JointPose {
        use gc_assets::model::f16_value;
        let lo = |w: u32| f16_value(w as u16);
        let hi = |w: u32| f16_value((w >> 16) as u16);
        JointPose {
            r: Quat::from_xyzw(lo(p[0]), hi(p[0]), lo(p[1]), hi(p[1])),
            t: Vec3::new(lo(p[2]), hi(p[2]), lo(p[3])),
            s: hi(p[3]),
        }
    }

    fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(Vec3::splat(self.s), self.r, self.t)
    }
}

/// The poses of a frame whose joints sit at `globals` (in the source's
/// space), or `None` if one cannot be stored as rotation, translation and
/// one scale.
pub(crate) fn poses(rig: &Rig, globals: &[Mat4]) -> Option<Vec<JointPose>> {
    globals
        .iter()
        .enumerate()
        .map(|(j, g)| {
            let local = match rig.parent[j] {
                Some(p) => globals[p].inverse() * *g,
                None => *g,
            };
            let (s, r, t) = local.to_scale_rotation_translation();
            let pose = JointPose {
                r: r.normalize(),
                t,
                s: (s.x + s.y + s.z) / 3.0,
            };
            // Judge the rotation and scale part only, against its own size:
            // the translation is kept exactly, and a long bone must not hide
            // a squash.
            let linear = |m: Mat4| glam::Mat3::from_mat4(m).to_cols_array();
            let size = linear(local).iter().fold(0.0f32, |m, v| m.max(v.abs()));
            let error = linear(pose.matrix() - local)
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()));
            (size > 0.0 && error <= FIT_TOLERANCE * size).then_some(pose)
        })
        .collect()
}

/// What a move is for and how it plays (see `gc_assets::anim`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Tags {
    pub act: Act,
    pub weapon: Weapon,
    pub side: Side,
    pub mask: Mask,
    /// `LOOPS`, `ADDITIVE`.
    pub flags: u8,
}

impl Tags {
    /// The tags of a plain clip kind's move.
    pub fn of_kind(kind: ClipKind) -> Tags {
        Tags {
            act: kind.act(),
            weapon: Weapon::Any,
            side: Side::Any,
            mask: Mask::All,
            flags: if kind.loops() { LOOPS } else { 0 },
        }
    }
}

/// A clip as joint poses.
pub(crate) struct PosedClip {
    /// The plain clip kind it plays, or `None` for a move only the move
    /// picker finds.
    pub kind: Option<ClipKind>,
    /// The source animation's name.
    pub name: String,
    pub tags: Tags,
    pub frames: Vec<Vec<JointPose>>,
}

/// Turn `frames` into changes from the first frame, as an additive move is
/// stored: the pose pass plays it as `delta * rotation`, `translation +
/// delta` and `scale * delta`, scaled by the layer's weight.
pub(crate) fn make_additive(frames: &mut [Vec<JointPose>]) {
    let Some(first) = frames.first().cloned() else {
        return;
    };
    for frame in frames.iter_mut() {
        for (p, p0) in frame.iter_mut().zip(&first) {
            *p = JointPose {
                r: (p.r * p0.r.inverse()).normalize(),
                t: p.t - p0.t,
                s: p.s / p0.s,
            };
        }
    }
}

/// A model posed from a shared skeleton.
pub(crate) struct Posed {
    pub rig: Rig,
    /// Skeleton space to model space.
    pub pre: Mat4,
    /// Per bone (the joints, then the still bone), stored vertex to the
    /// joint's rest.
    pub post: Vec<Mat4>,
    pub clips: Vec<PosedClip>,
}

impl Posed {
    /// A frame's bone matrices (stored vertex to pose, in the model's space),
    /// as the pose pass builds them.
    #[cfg(test)]
    pub fn matrices(&self, frame: &[JointPose]) -> Vec<Mat4> {
        fn global(rig: &Rig, frame: &[JointPose], j: usize) -> Mat4 {
            let local = frame[j].matrix();
            match rig.parent[j] {
                Some(p) => global(rig, frame, p) * local,
                None => local,
            }
        }
        (0..frame.len())
            .map(|j| self.pre * global(&self.rig, frame, j) * self.post[j])
            .chain([Mat4::IDENTITY])
            .collect()
    }
}

/// What the engine makes of a joint, from its name (Quaternius and Mixamo
/// rigs).
fn role(name: &str) -> Role {
    let n = name.to_ascii_lowercase();
    let n = n
        .trim_start_matches("mixamorig:")
        .trim_start_matches("mixamorig_");
    match n {
        "hips" | "pelvis" => Role::Hips,
        "abdomen" | "spine" => Role::Spine,
        "chest" | "spine2" => Role::Chest,
        "neck" => Role::Neck,
        "head" => Role::Head,
        "upperleg.l" | "leftupleg" | "thigh.l" => Role::UpperLegL,
        "lowerleg.l" | "leftleg" | "shin.l" => Role::LowerLegL,
        "foot.l" | "leftfoot" => Role::FootL,
        "upperleg.r" | "rightupleg" | "thigh.r" => Role::UpperLegR,
        "lowerleg.r" | "rightleg" | "shin.r" => Role::LowerLegR,
        "foot.r" | "rightfoot" => Role::FootR,
        "shoulder.l" | "leftshoulder" => Role::ShoulderL,
        "shoulder.r" | "rightshoulder" => Role::ShoulderR,
        _ => Role::None,
    }
}

/// The name of the joint across the body from `name`, if it is a sided one.
fn mirror_name(name: &str) -> Option<String> {
    for (a, b) in [(".L", ".R"), ("_L", "_R"), (".l", ".r"), ("_l", "_r")] {
        for (from, to) in [(a, b), (b, a)] {
            if let Some(stem) = name.strip_suffix(from) {
                return Some(format!("{stem}{to}"));
            }
        }
    }
    for (from, to) in [("Left", "Right"), ("Right", "Left")] {
        if name.contains(from) {
            return Some(name.replacen(from, to, 1));
        }
    }
    None
}

/// A skeleton's joints, with the masks that cover each.
fn skeleton_records(rig: &Rig) -> Vec<JointRecord> {
    let roles: Vec<Role> = rig.names.iter().map(|n| role(n)).collect();
    let find = |r: Role| roles.iter().position(|&x| x == r);
    let mirror = |j: usize| {
        mirror_name(&rig.names[j])
            .and_then(|m| rig.names.iter().position(|n| *n == m))
            .unwrap_or(j)
    };
    let mut joints: Vec<JointRecord> = (0..rig.names.len())
        .map(|j| JointRecord {
            parent: rig.parent[j].map_or(ROOT, |p| p as u8),
            masks: 0,
            mirror: mirror(j) as u8,
            role: roles[j] as u8,
        })
        .collect();
    let mut set = |mask: Mask, js: &[usize]| {
        for &j in js {
            joints[j].masks |= 1 << mask as u8;
        }
    };
    set(Mask::All, &(0..rig.names.len()).collect::<Vec<_>>());
    if let Some(hips) = find(Role::Hips) {
        // The legs carry the hips and whatever is above them (the root that
        // moves the whole body), so a walk under a swing still bobs.
        let mut legs = vec![hips];
        let mut at = rig.parent[hips];
        while let Some(a) = at {
            legs.push(a);
            at = rig.parent[a];
        }
        for r in [Role::UpperLegL, Role::UpperLegR] {
            if let Some(j) = find(r) {
                legs.extend(rig.subtree(j));
            }
        }
        set(Mask::Legs, &legs);
    }
    if let Some(j) = find(Role::Spine).or(find(Role::Chest)) {
        set(Mask::Upper, &rig.subtree(j));
    }
    if let Some(j) = find(Role::Neck).or(find(Role::Head)) {
        set(Mask::Head, &rig.subtree(j));
    }
    if let Some(j) = find(Role::ShoulderL) {
        set(Mask::LeftArm, &rig.subtree(j));
    }
    if let Some(j) = find(Role::ShoulderR) {
        set(Mask::RightArm, &rig.subtree(j));
    }
    joints
}

fn name_bytes(name: &str) -> [u8; NAME_BYTES] {
    let mut out = [0; NAME_BYTES];
    // Cut at a character boundary.
    let mut end = name.len().min(NAME_BYTES);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    out[..end].copy_from_slice(&name.as_bytes()[..end]);
    out
}

/// The three rows of a 3 × 4 matrix.
fn rows(m: &Mat4) -> [f32; FIX_FLOATS] {
    let mut out = [0.0; FIX_FLOATS];
    for r in 0..3 {
        out[r * 4..r * 4 + 4].copy_from_slice(&m.row(r).to_array());
    }
    out
}

/// Builds a pack's anim tables, storing each skeleton and each move once
/// however many models share them. Each skeleton's frames gather in its own
/// block, laid out in the frames table by [`AnimBuilder::finish`].
#[derive(Default)]
pub(crate) struct AnimBuilder {
    skeletons: HashMap<Rig, u16>,
    /// Per skeleton, its frames so far.
    blocks: Vec<Vec<gc_assets::anim::JointPose>>,
    /// Frames already stored, by skeleton: their first row.
    frames: HashMap<(u16, Vec<gc_assets::anim::JointPose>), u32>,
    /// Moves by skeleton, the plain clip kind they stand for, tags and first
    /// row: a tagged move keeps a record of its own even on the frames of a
    /// plain clip, so the picker's library holds it.
    moves: HashMap<(u16, u8, Tags, u32), u32>,
}

impl AnimBuilder {
    /// Add model `model`'s skeleton, moves and fixes; returns its clips.
    pub fn add(
        &mut self,
        t: &mut AnimTables,
        model: u32,
        posed: &Posed,
    ) -> Result<Vec<ClipRecord>, String> {
        let skeleton = match self.skeletons.get(&posed.rig) {
            Some(&s) => s,
            None => {
                let s = u16::try_from(t.skeletons.len()).map_err(|_| "too many skeletons")?;
                let joints = skeleton_records(&posed.rig);
                t.skeletons.push(SkeletonRecord {
                    name: name_bytes(posed.rig.names.first().map_or("", |n| n.as_str())),
                    first_joint: t.joints.len() as u32,
                    joints: joints.len() as u32,
                    first_pose: 0,
                    rows: 0,
                });
                t.joints.extend(joints);
                self.blocks.push(Vec::new());
                self.skeletons.insert(posed.rig.clone(), s);
                s
            }
        };
        t.fits.push(FitRecord {
            model,
            skeleton: u32::from(skeleton),
            first_fix: t.fixes.len() as u32,
            _pad: 0,
        });
        t.fixes.push(rows(&posed.pre));
        t.fixes.extend(posed.post.iter().map(rows));

        let joints = posed.rig.names.len();
        let mut clips = Vec::with_capacity(posed.clips.len());
        for clip in &posed.clips {
            let packed = clip
                .frames
                .iter()
                .flatten()
                .map(JointPose::packed)
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| format!("move {:?} is too large for half floats", clip.name))?;
            let block = &mut self.blocks[skeleton as usize];
            let first_row =
                *self
                    .frames
                    .entry((skeleton, packed))
                    .or_insert_with_key(|(_, packed)| {
                        let row = (block.len() / joints) as u32;
                        block.extend_from_slice(packed);
                        row
                    });
            let kind = clip.kind.map_or(NO_KIND, |k| k as u8);
            let key = (skeleton, kind, clip.tags, first_row);
            let index = match self.moves.get(&key) {
                Some(&k) => k,
                None => {
                    let k = t.moves.len() as u32;
                    t.moves.push(MoveRecord {
                        name: name_bytes(&clip.name),
                        skeleton,
                        kind,
                        act: clip.tags.act as u8,
                        weapon: clip.tags.weapon as u8,
                        side: clip.tags.side as u8,
                        mask: clip.tags.mask as u8,
                        flags: clip.tags.flags,
                        first_row,
                        frames: clip.frames.len() as u32,
                        fps: crate::SAMPLE_FPS,
                    });
                    self.moves.insert(key, k);
                    k
                }
            };
            // A move only the picker finds is not one of the model's clips.
            let Some(kind) = clip.kind else {
                continue;
            };
            let m = &t.moves[index as usize];
            clips.push(ClipRecord {
                kind: kind as u8,
                _pad: [0; 3],
                first_row: m.first_row,
                frames: m.frames,
                fps: m.fps,
            });
        }
        Ok(clips)
    }

    /// Lay every skeleton's block of frames out in `t`.
    pub fn finish(self, t: &mut AnimTables) -> Result<(), String> {
        for (s, block) in t.skeletons.iter_mut().zip(self.blocks) {
            s.first_pose = t.frames.len() as u32;
            s.rows = (block.len() / s.joints as usize) as u32;
            if s.rows as usize > MAX_ROWS {
                return Err(format!(
                    "skeleton {:?} has {} frames, more than {MAX_ROWS}",
                    gc_assets::model::name_str(&s.name),
                    s.rows
                ));
            }
            t.frames.extend(block);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(names: &[&str], parent: &[Option<usize>]) -> Rig {
        Rig {
            names: names.iter().map(ToString::to_string).collect(),
            parent: parent.to_vec(),
            rest: vec![[0; 16]; names.len()],
        }
    }

    #[test]
    fn a_joint_pose_rebuilds_its_frame() {
        let r = rig(&["Hips", "UpperLeg.L"], &[None, Some(0)]);
        let hips = Mat4::from_scale_rotation_translation(
            Vec3::splat(2.0),
            Quat::from_rotation_y(0.7),
            Vec3::new(0.0, 1.0, 0.5),
        );
        let leg =
            hips * Mat4::from_rotation_translation(Quat::from_rotation_x(-0.4), Vec3::X * 0.2);
        let p = poses(&r, &[hips, leg]).expect("plain rotations fit");
        let rebuilt = p[0].matrix() * p[1].matrix();
        assert!(rebuilt.abs_diff_eq(leg, 1e-5));
    }

    #[test]
    fn a_squashed_joint_keeps_bone_rows() {
        let r = rig(&["Body"], &[None]);
        assert!(poses(&r, &[Mat4::from_scale(Vec3::new(1.0, 0.5, 1.0))]).is_none());
        // A long bone does not hide a squash, nor a small rig's scale one.
        let long = Mat4::from_scale_rotation_translation(
            Vec3::new(1.0, 0.9, 1.0),
            Quat::IDENTITY,
            Vec3::new(0.0, 100.0, 0.0),
        );
        assert!(poses(&r, &[long]).is_none());
        assert!(poses(&r, &[Mat4::from_scale(Vec3::new(0.01, 0.009, 0.01))]).is_none());
        assert!(poses(&r, &[Mat4::from_scale(Vec3::splat(0.01))]).is_some());
    }

    #[test]
    fn a_tagged_move_on_a_clips_frames_shares_them_but_keeps_its_record() {
        let r = rig(&["Body"], &[None]);
        let frames = vec![vec![JointPose {
            r: Quat::IDENTITY,
            t: Vec3::ZERO,
            s: 1.0,
        }]];
        let clip = |kind: Option<ClipKind>, tags| PosedClip {
            kind,
            name: "Attack".into(),
            tags,
            frames: frames.clone(),
        };
        let posed = Posed {
            rig: r,
            pre: Mat4::IDENTITY,
            post: vec![Mat4::IDENTITY; 2],
            clips: vec![
                clip(Some(ClipKind::Attack), Tags::of_kind(ClipKind::Attack)),
                clip(None, Tags::of_kind(ClipKind::Attack)),
            ],
        };
        let (mut t, mut b) = (AnimTables::default(), AnimBuilder::default());
        assert_eq!(b.add(&mut t, 0, &posed).unwrap().len(), 1);
        b.finish(&mut t).unwrap();
        assert_eq!(t.moves.len(), 2);
        assert_eq!(t.moves[1].kind, NO_KIND);
        assert_eq!(t.moves[0].first_row, t.moves[1].first_row);
        assert_eq!(t.frames.len(), 1);
    }

    #[test]
    fn masks_and_mirrors_follow_the_joint_names() {
        let r = rig(
            &[
                "Root",
                "Hips",
                "Abdomen",
                "Neck",
                "Head",
                "Shoulder.L",
                "Shoulder.R",
                "UpperLeg.L",
                "UpperLeg.R",
            ],
            &[
                None,
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(2),
                Some(2),
                Some(1),
                Some(1),
            ],
        );
        let joints = skeleton_records(&r);
        let bits = |m: Mask| -> Vec<usize> {
            (0..r.names.len())
                .filter(|&j| joints[j].masks & (1 << m as u8) != 0)
                .collect()
        };
        assert_eq!(bits(Mask::All), (0..9).collect::<Vec<_>>());
        assert_eq!(bits(Mask::Legs), vec![0, 1, 7, 8]);
        assert_eq!(bits(Mask::Upper), vec![2, 3, 4, 5, 6]);
        assert_eq!(bits(Mask::Head), vec![3, 4]);
        assert_eq!(bits(Mask::LeftArm), vec![5]);
        assert_eq!(joints[5].mirror, 6);
        assert_eq!(joints[6].mirror, 5);
        assert_eq!(joints[4].mirror, 4);
        assert_eq!(gc_assets::anim::depths(&joints).unwrap()[4], 4);
        assert_eq!(joints[7].role, Role::UpperLegL as u8);
    }
}
