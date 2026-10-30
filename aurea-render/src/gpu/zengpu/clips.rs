//! Device-rasterized path coverage, shared by every draw under the same clip.

use std::mem::{size_of, size_of_val, take};
use std::sync::Arc;

use zengpu::native::NativeDevice;
use zengpu_hal::{
    Bindings, BufferDesc, BufferHandle, BufferUsage, FilterMode, Format, GpuDevice, GpuError,
    MemoryUsage, PipelineHandle, Result, SamplerDesc, SamplerHandle, Scalar, Submission,
    SubmissionStatus, TexDim, TextureDesc, TextureHandle, TextureUsage,
};

use crate::cpu::path::{Edge, tessellate_path_into};
use crate::geometry::FillRule;
use crate::gpu::FramePlan;
use crate::numeric::f32_to_u32_clamped;
use crate::types::{Path, PathCommand, Rect};

const CACHE_BYTES: u64 = 32 << 20;
const CACHE_MASKS: usize = 256;
const SCRATCH_BYTES: usize = 1 << 20;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct ClipBinding {
    pub slot: u32,
    pub rect: [f32; 4],
    pub enabled: bool,
}

impl ClipBinding {
    pub fn scalars(self, viewport: (f32, f32)) -> [Scalar; 7] {
        [
            Scalar::F32(viewport.0),
            Scalar::F32(viewport.1),
            Scalar::F32(self.rect[0]),
            Scalar::F32(self.rect[1]),
            Scalar::F32(self.rect[2]),
            Scalar::F32(self.rect[3]),
            Scalar::U32(u32::from(self.enabled)),
        ]
    }
}

struct Mask {
    paths: Arc<[Path]>,
    rect: Rect,
    texture: TextureHandle,
    slot: u32,
    edges: Option<BufferHandle>,
    work: Option<Submission>,
    frame: u64,
}

impl Mask {
    fn bytes(&self) -> u64 {
        u64::from(f32_to_u32_clamped(self.rect.width))
            * u64::from(f32_to_u32_clamped(self.rect.height))
            * 4
            + size_of::<Self>() as u64
            + size_of_val(&*self.paths) as u64
            + self
                .paths
                .iter()
                .map(|p| (p.commands.capacity() * size_of::<PathCommand>()) as u64)
                .sum::<u64>()
    }

    fn poll(&mut self, device: &NativeDevice) -> Result<bool> {
        if let Some(work) = &self.work {
            if work.poll()? == SubmissionStatus::Pending {
                return Ok(false);
            }
            self.work = None;
            if let Some(edges) = self.edges.take() {
                device.destroy_buffer(edges);
            }
        }
        Ok(true)
    }

    fn destroy(mut self, device: &NativeDevice) {
        if let Some(edges) = self.edges.take() {
            device.destroy_buffer(edges);
        }
        device.destroy_texture(self.texture);
    }
}

#[derive(Default)]
pub(super) struct ClipMasks {
    masks: Vec<Mask>,
    retired: Vec<Mask>,
    pub bindings: Vec<ClipBinding>,
    sampler: Option<SamplerHandle>,
    white: Option<(TextureHandle, u32)>,
    frame: u64,
    edges: Vec<Edge>,
    packed: Vec<f32>,
}

fn same_paths(a: &[Path], b: &[Path]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.fill_rule == b.fill_rule && a.commands == b.commands)
}

fn mask_rect(clip: Option<Rect>, width: u32, height: u32) -> Rect {
    let (w, h) = (width as f32, height as f32);
    let r = clip.unwrap_or(Rect::new(0.0, 0.0, w, h));
    let (x0, y0) = (r.x.floor().clamp(0.0, w), r.y.floor().clamp(0.0, h));
    let (x1, y1) = (
        r.right().ceil().clamp(0.0, w),
        r.bottom().ceil().clamp(0.0, h),
    );
    Rect::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0))
}

fn texture_desc(width: u32, height: u32, storage: bool) -> TextureDesc {
    TextureDesc {
        width,
        height,
        depth: 1,
        format: Format::Rgba8Unorm,
        usage: TextureUsage::SAMPLED
            | TextureUsage::TRANSFER_DST
            | TextureUsage::TRANSFER_SRC
            | if storage {
                TextureUsage::STORAGE
            } else {
                TextureUsage::empty()
            },
        samples: 1,
        dimension: TexDim::D2,
        mip_levels: 1,
        array_layers: 1,
    }
}

impl ClipMasks {
    fn ensure_white(&mut self, device: &NativeDevice) -> Result<u32> {
        if let Some((_, slot)) = self.white {
            return Ok(slot);
        }
        let sampler = match self.sampler {
            Some(sampler) => sampler,
            None => {
                let sampler = device.create_sampler(SamplerDesc {
                    min_filter: FilterMode::Nearest,
                    mag_filter: FilterMode::Nearest,
                    ..SamplerDesc::default()
                })?;
                self.sampler = Some(sampler);
                sampler
            }
        };
        let texture = device.create_texture(texture_desc(1, 1, false))?;
        let uploaded = device.upload_texture_data(texture, &[255; 4]);
        if let Err(error) = uploaded {
            device.destroy_texture(texture);
            return Err(error);
        }
        let Some(slot) = device.bind_texture(texture, sampler) else {
            device.destroy_texture(texture);
            return Err(GpuError::Backend(
                "clip white descriptor unavailable".into(),
            ));
        };
        self.white = Some((texture, slot));
        Ok(slot)
    }

    pub fn prepare(
        &mut self,
        device: &NativeDevice,
        plan: &FramePlan,
        pipeline: Option<PipelineHandle>,
    ) -> Result<()> {
        self.frame = self.frame.wrapping_add(1);
        for mask in &mut self.masks {
            mask.poll(device)?;
        }
        self.poll_retired(device)?;
        self.bindings.clear();
        if plan.order.is_empty() {
            return Ok(());
        }
        let white = self.ensure_white(device)?;
        for i in 0..plan.order.len() {
            let Some(paths) = plan
                .path_clips
                .get(i)
                .and_then(Option::as_ref)
                .filter(|p| !p.is_empty())
            else {
                self.bindings.push(ClipBinding {
                    slot: white,
                    rect: [0.0, 0.0, 1.0, 1.0],
                    enabled: false,
                });
                continue;
            };
            let rect = mask_rect(
                plan.clips.get(i).copied().flatten(),
                plan.viewport_width,
                plan.viewport_height,
            );
            let index = match self.masks.iter().position(|m| {
                m.rect == rect && (Arc::ptr_eq(&m.paths, paths) || same_paths(&m.paths, paths))
            }) {
                Some(index) => index,
                None => {
                    // Small masks can exhaust descriptors before reaching the byte budget.
                    // Retire a previous-frame mask before allocating its replacement.
                    if self.masks.len() >= CACHE_MASKS
                        && let Some(index) = self.oldest_unused()
                    {
                        let mask = self.masks.swap_remove(index);
                        self.retire(device, mask)?;
                    }
                    let pipeline = pipeline
                        .ok_or_else(|| GpuError::Unsupported("GPU path coverage".into()))?;
                    self.create_mask(device, paths, rect, pipeline)?;
                    self.masks.len() - 1
                }
            };
            let mask = &mut self.masks[index];
            mask.frame = self.frame;
            self.bindings.push(ClipBinding {
                slot: mask.slot,
                rect: [rect.x, rect.y, rect.width, rect.height],
                enabled: true,
            });
        }
        let mut bytes: u64 = self.masks.iter().map(Mask::bytes).sum();
        while bytes > CACHE_BYTES || self.masks.len() > CACHE_MASKS {
            let Some(index) = self.oldest_unused() else {
                break;
            };
            let mask = self.masks.swap_remove(index);
            bytes -= mask.bytes();
            self.retire(device, mask)?;
        }
        if self.packed.capacity() > SCRATCH_BYTES / size_of::<f32>() {
            self.packed = Vec::new();
        }
        if self.edges.capacity() > SCRATCH_BYTES / size_of::<Edge>() {
            self.edges = Vec::new();
        }
        Ok(())
    }

    fn oldest_unused(&self) -> Option<usize> {
        self.masks
            .iter()
            .enumerate()
            .filter(|(_, m)| m.frame != self.frame)
            .min_by_key(|(_, m)| m.frame)
            .map(|(index, _)| index)
    }

    fn create_mask(
        &mut self,
        device: &NativeDevice,
        paths: &Arc<[Path]>,
        rect: Rect,
        pipeline: PipelineHandle,
    ) -> Result<()> {
        self.packed.clear();
        for path in paths.iter() {
            tessellate_path_into(path, &mut self.edges);
            self.packed.push(self.edges.len() as f32);
            self.packed.push(if path.fill_rule == FillRule::EvenOdd {
                1.0
            } else {
                0.0
            });
            for e in &self.edges {
                let data = [
                    e.y_min - rect.y,
                    e.y_max - rect.y,
                    e.x_at_y_min - rect.x,
                    e.slope,
                    e.dir as f32,
                ];
                if !data.iter().all(|v| v.is_finite()) {
                    return Err(GpuError::Backend("non-finite clip edge".into()));
                }
                self.packed.extend_from_slice(&data);
            }
        }
        let data = super::backend::as_bytes(&self.packed);
        let edges = device.create_buffer(BufferDesc {
            size: size_of_val(data) as u64,
            usage: BufferUsage::STORAGE,
            memory: MemoryUsage::Upload,
        })?;
        if let Err(error) = device.write_buffer(edges, 0, data) {
            device.destroy_buffer(edges);
            return Err(error);
        }
        let texture = match device.create_texture(texture_desc(
            f32_to_u32_clamped(rect.width),
            f32_to_u32_clamped(rect.height),
            true,
        )) {
            Ok(texture) => texture,
            Err(error) => {
                device.destroy_buffer(edges);
                return Err(error);
            }
        };
        let work = match device.submit(
            0,
            pipeline,
            Bindings {
                buffers: &[edges.index()],
                images: &[texture.index()],
                scalars: &[Scalar::U32(u32::try_from(paths.len()).unwrap_or(u32::MAX))],
                ..Default::default()
            },
            [
                f32_to_u32_clamped(rect.width).div_ceil(8),
                f32_to_u32_clamped(rect.height).div_ceil(8),
                1,
            ],
        ) {
            Ok(work) => work,
            Err(error) => {
                device.destroy_buffer(edges);
                device.destroy_texture(texture);
                return Err(error);
            }
        };
        let mut mask = Mask {
            paths: Arc::clone(paths),
            rect,
            texture,
            slot: 0,
            edges: Some(edges),
            work: Some(work),
            frame: self.frame,
        };
        match device.bind_texture(texture, self.sampler.expect("white sampler exists")) {
            Some(slot) => {
                mask.slot = slot;
                self.masks.push(mask);
                Ok(())
            }
            None => {
                self.retired.push(mask);
                Err(GpuError::Backend("clip descriptor unavailable".into()))
            }
        }
    }

    fn retire(&mut self, device: &NativeDevice, mut mask: Mask) -> Result<()> {
        if mask.poll(device)? {
            mask.destroy(device);
        } else {
            self.retired.push(mask);
        }
        Ok(())
    }

    fn poll_retired(&mut self, device: &NativeDevice) -> Result<()> {
        let mut i = 0;
        while i < self.retired.len() {
            if self.retired[i].poll(device)? {
                self.retired.swap_remove(i).destroy(device);
            } else {
                i += 1;
            }
        }
        Ok(())
    }

    pub fn trim(&mut self, device: &NativeDevice) {
        let _ = self.poll_retired(device);
        let masks = take(&mut self.masks);
        for mask in masks {
            let _ = self.retire(device, mask);
        }
        self.bindings = Vec::new();
        self.packed = Vec::new();
        self.edges = Vec::new();
    }

    /// Called after the backend has waited for its work during teardown.
    pub fn destroy(&mut self, device: &NativeDevice) {
        for mask in self.masks.drain(..).chain(self.retired.drain(..)) {
            mask.destroy(device);
        }
        if let Some((texture, _)) = self.white.take() {
            device.destroy_texture(texture);
        }
        if let Some(sampler) = self.sampler.take() {
            device.destroy_sampler(sampler);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::cpu::CpuRasterizer;
    use crate::gpu::batch::DrawRef;
    use crate::gpu::zengpu::{ZenGpuContext, pipelines::Pipelines};
    use crate::types::{Paint, Point};
    use zengpu_hal::TextureRegion;

    #[test]
    #[allow(clippy::cognitive_complexity)] // Geometry, coverage oracle, and cache lifetime in one fixture.
    fn device_masks_match_cpu_coverage_and_reuse_until_trimmed() {
        let Ok(context) = ZenGpuContext::new() else {
            return;
        };
        let device = context.device();
        let mut pipelines = Pipelines::default();
        let pipeline = pipelines.ensure_clip_mask(device).expect("kernel");
        let mut cache = ClipMasks::default();
        let mut outer = Path::new();
        outer.commands.extend([
            PathCommand::MoveTo(Point::new(7.3, 5.7)),
            PathCommand::CubicTo(
                Point::new(44.2, -3.0),
                Point::new(61.8, 29.1),
                Point::new(44.6, 45.2),
            ),
            PathCommand::QuadTo(Point::new(15.8, 59.9), Point::new(7.3, 5.7)),
            PathCommand::Close,
        ]);
        let mut hole = Path::new();
        hole.fill_rule = FillRule::EvenOdd;
        for r in [
            Rect::new(5.2, 8.7, 51.3, 42.6),
            Rect::new(19.4, 19.2, 15.7, 14.4),
        ] {
            hole.commands.extend([
                PathCommand::MoveTo(Point::new(r.x, r.y)),
                PathCommand::LineTo(Point::new(r.right(), r.y)),
                PathCommand::LineTo(Point::new(r.right(), r.bottom())),
                PathCommand::LineTo(Point::new(r.x, r.bottom())),
                PathCommand::Close,
            ]);
        }
        for (case, paths) in [
            vec![outer.clone()],
            vec![hole.clone()],
            vec![outer, hole],
            vec![Path::new()],
        ]
        .into_iter()
        .enumerate()
        {
            let paths: Arc<[Path]> = paths.into();
            let mut plan = FramePlan {
                viewport_width: 64,
                viewport_height: 64,
                order: vec![DrawRef::Rect(0), DrawRef::Rect(1)],
                clips: vec![Some(Rect::new(3.0, 2.0, 56.0, 54.0)); 2],
                path_clips: vec![Some(Arc::clone(&paths)); 2],
                ..Default::default()
            };
            cache.prepare(device, &plan, Some(pipeline)).expect("mask");
            assert_eq!(cache.bindings[0], cache.bindings[1]);
            let binding = cache.bindings[0];
            let mask = cache.masks.iter().find(|m| m.slot == binding.slot).unwrap();
            let (w, h) = (
                f32_to_u32_clamped(mask.rect.width),
                f32_to_u32_clamped(mask.rect.height),
            );
            let origin = Point::new(mask.rect.x, mask.rect.y);
            let mut expected = vec![255u8; (w * h) as usize];
            for path in paths.iter() {
                let coverage =
                    CpuRasterizer::path_coverage(path, &Paint::new(), origin, (w, h)).unwrap();
                for (want, pixel) in expected.iter_mut().zip(coverage.as_chunks::<4>().0) {
                    *want =
                        u8::try_from((u32::from(*want) * u32::from(pixel[3]) + 127) / 255).unwrap();
                }
            }
            let buffer = device
                .create_buffer(BufferDesc {
                    size: u64::from(w * h * 4),
                    usage: BufferUsage::READBACK | BufferUsage::TRANSFER_DST,
                    memory: MemoryUsage::Readback,
                })
                .unwrap();
            device
                .copy_texture_region_to_buffer(
                    mask.texture,
                    TextureRegion::rect(0, 0, w, h),
                    buffer,
                    0,
                    w * 4,
                )
                .unwrap()
                .wait(Duration::MAX)
                .unwrap();
            let pixels = device.read_buffer(buffer, 0, u64::from(w * h * 4)).unwrap();
            device.destroy_buffer(buffer);
            let worst = pixels
                .as_chunks::<4>()
                .0
                .iter()
                .zip(&expected)
                .map(|(got, want)| got[3].abs_diff(*want))
                .max()
                .unwrap();
            assert!(worst <= 2, "case {case}: coverage differs by {worst}");
            let count = cache.masks.len();
            cache.prepare(device, &plan, Some(pipeline)).unwrap();
            assert_eq!(cache.bindings[0], binding);
            assert_eq!(cache.masks.len(), count, "reuse across frames");
            plan.viewport_width = 32;
            cache.prepare(device, &plan, Some(pipeline)).unwrap();
            assert_ne!(cache.bindings[0], binding, "resize changes mask crop");
        }
        // Moving tiny clips must respect the descriptor budget even when texels
        // alone stay far below the byte limit.
        for i in 0..CACHE_MASKS + 4 {
            let path = Path::rounded_rect(Rect::new(0.0, 0.0, 1.0 + i as f32 / 1000.0, 1.0), 0.0);
            let plan = FramePlan {
                viewport_width: 2,
                viewport_height: 2,
                order: vec![DrawRef::Rect(0)],
                path_clips: vec![Some(vec![path].into())],
                ..Default::default()
            };
            cache.prepare(device, &plan, Some(pipeline)).unwrap();
            assert!(cache.masks.len() <= CACHE_MASKS);
        }
        device.wait_idle().unwrap();
        cache.trim(device);
        assert!(cache.masks.is_empty() && cache.retired.is_empty());
        assert_eq!(cache.packed.capacity(), 0);
        cache.destroy(device);
        pipelines.destroy(device);
    }
}
