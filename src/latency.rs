//! Low-latency optimization primitives.
//!
//! Core pinning and pre-allocated buffer pools to hit <500µs pipeline targets.

use core_affinity::CoreId;
use tracing::info;

/// Pin the current thread to a specific CPU core to minimize cache misses.
pub fn pin_to_core(core_id: usize) {
    if core_affinity::set_for_current(CoreId { id: core_id }) {
        info!("Successfully pinned thread to core {}", core_id);
    } else {
        info!("Failed to pin thread to core {}", core_id);
    }
}

/// Pre-allocated buffer pool to avoid heap allocations in the hot path.
pub struct BufferPool {
    pool: Vec<Vec<u8>>,
}

impl BufferPool {
    pub fn new(capacity: usize, buffer_size: usize) -> Self {
        let mut pool = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            pool.push(vec![0u8; buffer_size]);
        }
        Self { pool }
    }

    pub fn get(&mut self) -> Option<Vec<u8>> {
        self.pool.pop()
    }

    pub fn release(&mut self, buf: Vec<u8>) {
        self.pool.push(buf);
    }
}
