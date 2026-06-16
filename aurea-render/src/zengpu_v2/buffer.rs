//! A vertex buffer that grows (doubling) to fit the largest instance stream
//! seen so far, replacing the production painter's hand-rolled
//! `InstanceBuffer`/`alloc_mapped_vertex_buffer` with `GpuDevice::{create_buffer,
//! write_buffer}` (host-visible, persistently mapped via `MemoryUsage::CpuToGpu`).

use zengpu_hal::{BufferDesc, BufferHandle, BufferUsage, GpuDevice, MemoryUsage, Result};
use zengpu_vulkan::VulkanDevice;

pub struct GrowableBuffer {
    handle: Option<BufferHandle>,
    capacity: u64,
    usage: BufferUsage,
}

impl GrowableBuffer {
    pub fn new(usage: BufferUsage) -> Self {
        Self { handle: None, capacity: 0, usage: usage | BufferUsage::VERTEX }
    }

    /// Upload `data`, growing the backing buffer if it doesn't fit. Returns
    /// the buffer to bind for this frame's draws, or `None` if `data` is empty.
    pub fn upload(&mut self, device: &VulkanDevice, data: &[u8]) -> Result<Option<BufferHandle>> {
        if data.is_empty() {
            return Ok(None);
        }
        let needed = data.len() as u64;
        if needed > self.capacity {
            if let Some(old) = self.handle.take() {
                device.destroy_buffer(old);
            }
            let mut capacity = self.capacity.max(1);
            while capacity < needed {
                capacity *= 2;
            }
            self.handle = Some(device.create_buffer(BufferDesc {
                size: capacity,
                usage: self.usage,
                memory: MemoryUsage::CpuToGpu,
            })?);
            self.capacity = capacity;
        }
        let handle = self.handle.expect("capacity > 0 implies handle is set");
        device.write_buffer(handle, 0, data)?;
        Ok(Some(handle))
    }

    pub fn destroy(&mut self, device: &VulkanDevice) {
        if let Some(handle) = self.handle.take() {
            device.destroy_buffer(handle);
        }
        self.capacity = 0;
    }
}
