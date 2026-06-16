/// Host-visible instance buffer that grows (doubling) to fit the largest
/// batch seen so far, reused across frames to avoid per-frame allocation.
pub struct InstanceBuffer {
    pub buffer: wgpu::Buffer,
    capacity: usize,
    elem_size: usize,
    label: &'static str,
}

impl InstanceBuffer {
    pub fn new(device: &wgpu::Device, label: &'static str, elem_size: usize) -> Self {
        let capacity = 1;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (elem_size * capacity) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            capacity,
            elem_size,
            label,
        }
    }

    /// Upload `data` (tightly-packed instance bytes), growing the buffer first
    /// if it can't fit. Empty slices are a no-op.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let needed = data.len() / self.elem_size;
        if needed > self.capacity {
            let capacity = needed.next_power_of_two();
            self.buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: (self.elem_size * capacity) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.capacity = capacity;
        }
        queue.write_buffer(&self.buffer, 0, data);
    }
}
