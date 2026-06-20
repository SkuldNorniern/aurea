//! A vertex buffer that grows (doubling) to fit the largest instance stream
//! seen so far, replacing the production painter's hand-rolled
//! `InstanceBuffer`/`alloc_mapped_vertex_buffer` with `GpuDevice::{create_buffer,
//! write_buffer}` (host-visible, persistently mapped via `MemoryUsage::CpuToGpu`).

use std::array::from_fn;

use zengpu_hal::{BufferDesc, BufferHandle, BufferUsage, GpuDevice, MemoryUsage, Result};
use zengpu_vulkan::VulkanDevice;

const BUFFER_RING: usize = 3;

struct BufferSlot {
    handle: Option<BufferHandle>,
    capacity: u64,
}

pub struct GrowableBuffer {
    slots: [BufferSlot; BUFFER_RING],
    next_slot: usize,
    usage: BufferUsage,
}

impl GrowableBuffer {
    pub fn new(usage: BufferUsage) -> Self {
        Self {
            slots: from_fn(|_| BufferSlot {
                handle: None,
                capacity: 0,
            }),
            next_slot: 0,
            usage: usage | BufferUsage::VERTEX,
        }
    }

    /// Upload `data`, growing the backing buffer if it doesn't fit. Returns
    /// the buffer to bind for this frame's draws, or `None` if `data` is empty.
    pub fn upload(&mut self, device: &VulkanDevice, data: &[u8]) -> Result<Option<BufferHandle>> {
        if data.is_empty() {
            return Ok(None);
        }
        let slot_index = self.next_slot;
        self.next_slot = (self.next_slot + 1) % BUFFER_RING;
        let slot = &mut self.slots[slot_index];
        let needed = data.len() as u64;
        if needed > slot.capacity {
            if let Some(old) = slot.handle.take() {
                device.destroy_buffer(old);
            }
            let mut capacity = slot.capacity.max(1);
            while capacity < needed {
                capacity *= 2;
            }
            slot.handle = Some(device.create_buffer(BufferDesc {
                size: capacity,
                usage: self.usage,
                memory: MemoryUsage::CpuToGpu,
            })?);
            slot.capacity = capacity;
        }
        let handle = slot.handle.expect("capacity > 0 implies handle is set");
        device.write_buffer(handle, 0, data)?;
        Ok(Some(handle))
    }

    pub fn destroy(&mut self, device: &VulkanDevice) {
        for slot in &mut self.slots {
            if let Some(handle) = slot.handle.take() {
                device.destroy_buffer(handle);
            }
            slot.capacity = 0;
        }
        self.next_slot = 0;
    }
}
