//! Per-monitor GPU surface.
//!
//! Milestone 0 declares the `wgpu` backend only. No device, surface, or frame
//! is created here.

/// Name of the GPU backend. Does not create a device or submit frames.
pub fn backend_name() -> &'static str {
    let _declared = wgpu::Backends::empty();
    "wgpu"
}

#[cfg(test)]
mod tests {
    #[test]
    fn declares_wgpu_without_creating_a_device() {
        assert_eq!(super::backend_name(), "wgpu");
        assert!(wgpu::Backends::empty().is_empty());
    }
}
