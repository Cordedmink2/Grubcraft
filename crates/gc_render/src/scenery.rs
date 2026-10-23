//! Scenery models on the GPU: trees, bushes, grass and rocks, drawn by the
//! thousand. Unlike soldiers they are never skinned: each model is stored
//! once in vertex colours (no textures), and every instance is placed,
//! turned, scaled and tinted in the vertex shader, which also sets it on
//! the ground and sways it in the wind (`vs_scenery` in
//! `shaders/scenery.wgsl`). One instanced draw per model and level of
//! detail; the shadow pass draws the casters at a coarse level.
//!
//! The caller (`gc_game::scenery`) decides what stands where and which level
//! of detail each instance gets, and hands the renderer a frame's instances
//! sorted into batches with [`crate::Renderer::set_scenery_frame`].

use bytemuck::{Pod, Zeroable};
use gc_assets::scenery::{LODS, POSITION_SCALE, SWAY_MAX, Scenery, SceneryVertex};

use crate::Target;

/// Fixed capacity of the scenery instance buffer; never reallocated.
pub const MAX_SCENERY_INSTANCES: usize = 65_536;

/// What surface detail a piece of scenery's shader paints over its baked
/// vertex colours (`SceneryInstance::kind`; the same numbers are in
/// scenery.wgsl).
pub mod surface_kind {
    /// Nothing: bones, gathering spots, banners.
    pub const NONE: u32 = 0;
    /// Trees with broadleaf bark (and leaves): oaks, birch, willow.
    pub const BROADLEAF: u32 = 1;
    /// Trees with conifer bark (and needles), and dead wood.
    pub const CONIFER: u32 = 2;
    /// Bushes and grass: leaf and blade mottling.
    pub const FOLIAGE: u32 = 3;
    /// Rocks, from the same Rock035 as the cliffs.
    pub const STONE: u32 = 4;
    /// Rocks with moss on their tops.
    pub const MOSSY_STONE: u32 = 5;
}

/// One piece of scenery to draw. 28 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct SceneryInstance {
    /// Base on the map, in tiles; the shader sets it on the ground there.
    pub pos: [f32; 2],
    /// Unit vector the model's front (+Z as baked) turns to.
    pub facing: [f32; 2],
    /// Size relative to the model as baked.
    pub scale: f32,
    /// Colour multiplier in sRGB (128 is unchanged, 255 about double), and
    /// in `a` the instance's wind phase (0 to 255 for one full cycle), so
    /// neighbours don't sway in step.
    pub tint: [u8; 4],
    /// Which surface detail to paint: one of [`detail`].
    pub kind: u32,
}

/// A run of instances drawn with one model at one level of detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneryBatch {
    pub model: u16,
    /// Level of detail: 0 is full, up to `gc_assets::scenery::LODS - 1`.
    pub lod: u8,
    /// Whether the run casts shadows (trees and big rocks; not grass).
    pub shadow: bool,
    pub first: u32,
    pub count: u32,
}

/// World shader: the shared part (`Globals`, terrain height, lighting, fog of
/// war) followed by the scenery entry points.
pub(crate) fn shader_source() -> String {
    format!(
        "{}\nconst SWAY_MAX: f32 = {:?};\nconst POSITION_SCALE: f32 = {:?};\n{}",
        crate::rock::world_source(),
        SWAY_MAX,
        POSITION_SCALE,
        include_str!("shaders/scenery.wgsl")
    )
}

/// Where a model's meshes lie in the shared buffers.
#[derive(Clone, Copy)]
struct ModelGpu {
    first_vertex: i32,
    /// First index and index count of each level.
    lods: [[u32; 2]; LODS],
}

struct ModelsGpu {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    models: Vec<ModelGpu>,
}

/// One draw: a run of instances of one model at one level.
#[derive(Clone, Copy)]
struct Draw {
    model: usize,
    lod: usize,
    shadow: bool,
    first: u32,
    end: u32,
}

pub(crate) struct SceneryGpu {
    pipeline: wgpu::RenderPipeline,
    shadow_pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    models: Option<ModelsGpu>,
    draws: Vec<Draw>,
    /// Instances left out this frame for want of room.
    dropped: u32,
}

pub(crate) fn make(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    hdr: crate::Hdr,
) -> SceneryGpu {
    let source = shader_source();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("scenery.wgsl"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let vertex_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<SceneryVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &wgpu::vertex_attr_array![0 => Sint16x4, 1 => Snorm8x4, 2 => Unorm8x4],
    };
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<SceneryInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            3 => Float32x2, 4 => Float32x2, 5 => Float32, 6 => Unorm8x4, 7 => Uint32
        ],
    };
    let pipeline = |label, vs, target: Target<'_>| {
        let (targets, fs) = target.color();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers: &[Some(vertex_layout.clone()), Some(instance_layout.clone())],
            },
            // Leaves and blades may be single planes.
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(target.depth_state()),
            multisample: target.multisample(),
            fragment: fs.map(|fs| wgpu::FragmentState {
                module: &shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    SceneryGpu {
        pipeline: pipeline("scenery", "vs_scenery", Target::Color(hdr, "fs_scenery")),
        shadow_pipeline: pipeline("scenery shadow", "vs_scenery_depth", Target::Shadow),
        instances: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scenery instances"),
            size: (MAX_SCENERY_INSTANCES * size_of::<SceneryInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        models: None,
        draws: Vec::new(),
        dropped: 0,
    }
}

impl SceneryGpu {
    /// The fixed instance buffer's size, for the static GPU memory total.
    pub fn instance_bytes(&self) -> u64 {
        self.instances.size()
    }

    /// Bytes of the model buffers, once uploaded.
    pub fn model_bytes(&self) -> u64 {
        self.models
            .as_ref()
            .map_or(0, |m| m.vertices.size() + m.indices.size())
    }

    /// Instances left undrawn this frame for want of room.
    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Upload every model's vertices and indices once.
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scenery: &Scenery<'_>,
    ) -> Result<(), String> {
        let buffer = |label, bytes: &[u8], usage| {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                // Buffers must be a multiple of 4 bytes.
                size: (bytes.len() as u64).next_multiple_of(4).max(4),
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut padded = bytes.to_vec();
            padded.resize(b.size() as usize, 0);
            queue.write_buffer(&b, 0, &padded);
            b
        };
        let limit = device.limits().max_buffer_size;
        for (what, bytes) in [
            ("vertices", size_of_val(scenery.vertices) as u64),
            ("indices", size_of_val(scenery.indices) as u64),
        ] {
            if bytes > limit {
                return Err(format!(
                    "scenery {what} ({bytes} bytes) exceed this GPU's {limit}"
                ));
            }
        }
        self.models = Some(ModelsGpu {
            vertices: buffer(
                "scenery vertices",
                bytemuck::cast_slice(scenery.vertices),
                wgpu::BufferUsages::VERTEX,
            ),
            indices: buffer(
                "scenery indices",
                bytemuck::cast_slice(scenery.indices),
                wgpu::BufferUsages::INDEX,
            ),
            models: scenery
                .models
                .iter()
                .map(|r| ModelGpu {
                    first_vertex: r.first_vertex as i32,
                    lods: r.lods,
                })
                .collect(),
        });
        self.draws.clear();
        Ok(())
    }

    /// Take the frame's instances and batches; the next render draws them.
    /// Without models uploaded, nothing is drawn (and nothing counted).
    pub fn set_frame(
        &mut self,
        queue: &wgpu::Queue,
        instances: &[SceneryInstance],
        batches: &[SceneryBatch],
    ) {
        self.draws.clear();
        self.dropped = 0;
        let Some(models) = &self.models else {
            return;
        };
        let count = instances.len().min(MAX_SCENERY_INSTANCES);
        self.dropped = (instances.len() - count) as u32;
        if count > 0 {
            queue.write_buffer(
                &self.instances,
                0,
                bytemuck::cast_slice(&instances[..count]),
            );
        }
        for b in batches {
            let end = b.first.saturating_add(b.count).min(count as u32);
            let model = usize::from(b.model);
            if b.first >= end || model >= models.models.len() {
                continue;
            }
            self.draws.push(Draw {
                model,
                lod: usize::from(b.lod).min(LODS - 1),
                shadow: b.shadow,
                first: b.first,
                end,
            });
        }
    }

    /// Draw the frame's scenery into the world pass or (with `shadow`) the
    /// shadow map, whose bind group 0 the caller has set. The sun only needs
    /// a silhouette, so it draws no level finer than 2, and only the
    /// batches that cast shadows. Returns draw calls, instances and
    /// triangles drawn.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, shadow: bool) -> (u32, u32, u64) {
        let (mut calls, mut drawn, mut tris) = (0, 0, 0);
        let Some(models) = self.models.as_ref().filter(|_| !self.draws.is_empty()) else {
            return (calls, drawn, tris);
        };
        pass.set_pipeline(if shadow {
            &self.shadow_pipeline
        } else {
            &self.pipeline
        });
        pass.set_vertex_buffer(0, models.vertices.slice(..));
        pass.set_vertex_buffer(1, self.instances.slice(..));
        pass.set_index_buffer(models.indices.slice(..), wgpu::IndexFormat::Uint16);
        for d in self.draws.iter().filter(|d| d.shadow || !shadow) {
            let model = &models.models[d.model];
            let lod = if shadow { d.lod.max(SHADOW_LOD) } else { d.lod };
            let [first, len] = model.lods[lod];
            pass.draw_indexed(first..first + len, model.first_vertex, d.first..d.end);
            calls += 1;
            drawn += d.end - d.first;
            tris += u64::from(len / 3) * u64::from(d.end - d.first);
        }
        (calls, drawn, tris)
    }
}

/// The finest level the sun's pass draws.
const SHADOW_LOD: usize = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenery_layouts_are_stable() {
        // The vertex attribute offsets in `make` and the shader's inputs
        // depend on these.
        assert_eq!(size_of::<SceneryInstance>(), 28);
        assert_eq!(size_of::<SceneryVertex>(), 16);
    }

    #[test]
    fn the_scenery_shader_has_its_entry_points() {
        let source = shader_source();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        let names: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for want in ["vs_scenery", "vs_scenery_depth", "fs_scenery"] {
            assert!(names.contains(&want), "no entry point {want}");
        }
    }
}
