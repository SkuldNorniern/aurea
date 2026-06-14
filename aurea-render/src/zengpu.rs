//! ZenGPU 2D renderer backend (feature `zengpu`, aurea G4 / Rung 1).
//!
//! Implements [`Renderer`] by recording draw calls into a [`DisplayList`] (the
//! same [`CpuDrawingContext`] the CPU rasterizer uses), then in `end_frame`
//! lowering the list to [`RenderBatches`] and presenting it through ZenGPU's
//! `Vulkan2dSurface`.
//!
//! Unlike [`crate::cpu::CpuRasterizer`], this backend presents **directly** to
//! its own swapchain on the supplied window — it does not publish a CPU
//! framebuffer for the platform to blit. It is therefore driven at the window
//! level (the caller owns the window and its handles); wiring it into `Canvas`
//! backend selection is a follow-up that must reconcile the canvas blit path.

use crate::batch::RenderBatches;
use crate::cpu::CpuDrawingContext;
use crate::display_list::DisplayList;
use crate::renderer::{DrawingContext, Renderer};
use crate::surface::{Surface, SurfaceInfo};
use crate::types::Rect;
use aurea_foundation::{AureaError, AureaResult};

use zengpu_hal::{DeviceRequest, Format, PresentMode, SurfaceConfig, WindowHandles};
use zengpu_vulkan::instance::VulkanInstance;
use zengpu_vulkan::{
    CircleInstance as VkCircle, Frame2d, RectInstance as VkRect, Vulkan2dSurface, VulkanDevice,
};

// The batch-layer and ZenGPU instance types are both `#[repr(C)]` with the same
// `[f32; 4] + [f32; 4]` fields, so a frame's primitives can be reinterpreted
// from one to the other with no per-frame copy. Guard the layout assumptions.
const _: () =
    assert!(std::mem::size_of::<crate::batch::RectInstance>() == std::mem::size_of::<VkRect>());
const _: () = assert!(
    std::mem::size_of::<crate::batch::CircleInstance>() == std::mem::size_of::<VkCircle>()
);

/// A [`Renderer`] that lowers the display list to instanced rects and presents
/// them through ZenGPU's Vulkan backend.
pub struct ZenGpuRenderer {
    // `device` and `_instance` own GPU resources the surface borrows; they must
    // outlive `surface` and are dropped after it (struct field order: surface
    // is declared first so it drops first).
    surface: Vulkan2dSurface,
    #[allow(dead_code)]
    device: VulkanDevice,
    _instance: VulkanInstance,
    display_list: DisplayList,
    /// Reused across frames so steady-state `end_frame` does no allocation.
    batches: RenderBatches,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
}

impl ZenGpuRenderer {
    /// Create a renderer presenting to the window described by `handles`.
    /// `width`/`height` are the logical surface size; `scale_factor` maps to
    /// physical pixels (matching [`CpuRasterizer`](crate::cpu::CpuRasterizer)).
    pub fn new(
        handles: &WindowHandles,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> AureaResult<Self> {
        let instance = VulkanInstance::new_with_surface().map_err(gpu_err)?;
        let adapter = instance
            .request_vulkan_adapter()
            .ok_or(AureaError::ElementOperationFailed)?;
        let device = adapter
            .open_with_surface(DeviceRequest::default())
            .map_err(gpu_err)?;

        let scale = scale_factor.max(1.0);
        let config = SurfaceConfig {
            format: Format::Bgra8Unorm,
            width: ((width as f32 * scale).round() as u32).max(1),
            height: ((height as f32 * scale).round() as u32).max(1),
            present_mode: PresentMode::Fifo,
        };
        let surface = instance
            .create_2d_surface(handles, &device, config)
            .map_err(gpu_err)?;

        Ok(Self {
            surface,
            device,
            _instance: instance,
            display_list: DisplayList::new(),
            batches: RenderBatches::default(),
            logical_width: width,
            logical_height: height,
            scale_factor: scale,
        })
    }

    /// Swapchain extent in physical pixels.
    pub fn size(&self) -> (u32, u32) {
        self.surface.size()
    }
}

impl Renderer for ZenGpuRenderer {
    fn init(&mut self, _surface: Surface, info: SurfaceInfo) -> AureaResult<()> {
        self.logical_width = info.width;
        self.logical_height = info.height;
        self.scale_factor = info.scale_factor.max(1.0);
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> AureaResult<()> {
        self.logical_width = width;
        self.logical_height = height;
        self.display_list.clear();
        let scale = self.scale_factor;
        let pw = ((width as f32 * scale).round() as u32).max(1);
        let ph = ((height as f32 * scale).round() as u32).max(1);
        self.surface.resize(pw, ph).map_err(gpu_err)
    }

    fn begin_frame(&mut self) -> AureaResult<Box<dyn DrawingContext>> {
        self.display_list.clear();
        let mut ctx = CpuDrawingContext::new(
            &mut self.display_list as *mut DisplayList,
            self.logical_width,
            self.logical_height,
        );
        ctx.set_scale_factor(self.scale_factor);
        Ok(Box::new(ctx))
    }

    fn end_frame(&mut self) -> AureaResult<()> {
        self.batches.lower_into(&self.display_list);
        let clear = self.batches.clear.map(|c| {
            [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                c.a as f32 / 255.0,
            ]
        });
        // Zero-copy reinterpret: layout identity is asserted at the top of the
        // module, so the batch primitives upload directly with no per-frame Vec.
        let rects: &[VkRect] = unsafe {
            std::slice::from_raw_parts(
                self.batches.rects.as_ptr() as *const VkRect,
                self.batches.rects.len(),
            )
        };
        let circles: &[VkCircle] = unsafe {
            std::slice::from_raw_parts(
                self.batches.circles.as_ptr() as *const VkCircle,
                self.batches.circles.len(),
            )
        };
        self.surface
            .present(Frame2d { clear, rects, circles })
            .map_err(gpu_err)
    }

    fn cleanup(&mut self) {
        self.display_list.clear();
    }

    fn set_damage(&mut self, _damage: Option<Rect>) {
        // The GPU painter redraws the full frame each present; damage is unused.
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}

fn gpu_err(_e: zengpu_hal::GpuError) -> AureaError {
    AureaError::ElementOperationFailed
}
