//! ZenGPU 2D renderer backend (feature `zengpu`, aurea G4 / Rung 1).

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

const _: () =
    assert!(std::mem::size_of::<crate::batch::RectInstance>() == std::mem::size_of::<VkRect>());
const _: () = assert!(
    std::mem::size_of::<crate::batch::CircleInstance>() == std::mem::size_of::<VkCircle>()
);

/// A [`Renderer`] that lowers the display list to instanced rects and presents them through
/// ZenGPU's Vulkan backend.
pub struct ZenGpuRenderer {
    surface: Vulkan2dSurface,
    #[allow(dead_code)]
    device: VulkanDevice,
    _instance: VulkanInstance,
    display_list: DisplayList,
    batches: RenderBatches,
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
}

impl ZenGpuRenderer {
    /// Create a renderer presenting to the window described by `handles`.
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
    }

    fn display_list(&self) -> Option<&DisplayList> {
        Some(&self.display_list)
    }
}

fn gpu_err(_e: zengpu_hal::GpuError) -> AureaError {
    AureaError::ElementOperationFailed
}
