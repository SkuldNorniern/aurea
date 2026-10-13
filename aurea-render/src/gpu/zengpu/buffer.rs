//! A vertex buffer that grows (doubling) to fit the largest instance stream
//! seen so far, replacing the production painter's hand-rolled
//! `InstanceBuffer`/`alloc_mapped_vertex_buffer` with `GpuDevice::{create_buffer,
//! write_buffer}` (host-visible, persistently mapped via `MemoryUsage::CpuToGpu`).

use std::array::from_fn;

use zengpu::native::NativeDevice;
use zengpu_hal::{BufferDesc, BufferHandle, BufferUsage, GpuDevice, GpuError, MemoryUsage, Result};

const BUFFER_RING: usize = 3;

struct BufferSlot {
    handle: Option<BufferHandle>,
    capacity: u64,
}

impl BufferSlot {
    fn grow(
        &mut self,
        needed: u64,
        create: impl FnOnce(u64) -> Result<BufferHandle>,
        destroy: impl FnOnce(BufferHandle),
    ) -> Result<()> {
        if needed <= self.capacity && self.handle.is_some() {
            return Ok(());
        }
        let capacity = needed
            .checked_next_power_of_two()
            .ok_or(GpuError::OutOfMemory(MemoryUsage::CpuToGpu))?;
        let fresh = create(capacity)?;
        let previous = self.handle.replace(fresh);
        self.capacity = capacity;
        if let Some(previous) = previous {
            destroy(previous);
        }
        Ok(())
    }
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

    /// Reserve the next slot without writing or advancing the ring. May run
    /// before frame acquisition; failed growth preserves its old allocation.
    pub fn prepare(&mut self, device: &NativeDevice, len: usize) -> Result<()> {
        if len == 0 {
            return Ok(());
        }
        let slot = &mut self.slots[self.next_slot];
        let needed = len as u64;
        // Allocation failure preserves the previous valid buffer for retry.
        slot.grow(
            needed,
            |capacity| {
                device.create_buffer(BufferDesc {
                    size: capacity,
                    usage: self.usage,
                    memory: MemoryUsage::CpuToGpu,
                })
            },
            |old| device.destroy_buffer(old),
        )
    }

    /// Upload after the frame fence completes. Empty data uses no ring slot.
    pub fn upload(&mut self, device: &NativeDevice, data: &[u8]) -> Result<Option<BufferHandle>> {
        if data.is_empty() {
            return Ok(None);
        }
        self.prepare(device, data.len())?;
        let slot = &mut self.slots[self.next_slot];
        let handle = slot.handle.expect("capacity > 0 implies handle is set");
        device.write_buffer(handle, 0, data)?;
        self.next_slot = (self.next_slot + 1) % BUFFER_RING;
        Ok(Some(handle))
    }

    pub fn destroy(&mut self, device: &NativeDevice) {
        for slot in &mut self.slots {
            if let Some(handle) = slot.handle.take() {
                device.destroy_buffer(handle);
            }
            slot.capacity = 0;
        }
        self.next_slot = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_growth_preserves_the_old_buffer_and_retry_replaces_it_once() {
        let old = BufferHandle::from_parts(1, 0);
        let new = BufferHandle::from_parts(2, 0);
        let mut slot = BufferSlot {
            handle: Some(old),
            capacity: 16,
        };
        assert!(
            slot.grow(
                33,
                |_| Err(GpuError::OutOfMemory(MemoryUsage::CpuToGpu)),
                |_| panic!("destroyed on failure")
            )
            .is_err()
        );
        assert_eq!(slot.handle, Some(old));
        assert_eq!(slot.capacity, 16);
        slot.grow(8, |_| panic!("should fit"), |_| panic!("should survive"))
            .expect("reuse");
        let mut destroyed = Vec::new();
        slot.grow(
            33,
            |capacity| {
                assert_eq!(capacity, 64);
                Ok(new)
            },
            |handle| destroyed.push(handle),
        )
        .expect("retry");
        assert_eq!(destroyed, [old]);
        assert_eq!(slot.handle, Some(new));
        assert_eq!(slot.capacity, 64);
    }
}
