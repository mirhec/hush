//! Native rendering defaults shared by the application and the Windows startup test.

pub fn native_options() -> eframe::NativeOptions {
    #[cfg(target_os = "windows")]
    {
        use eframe::{egui_wgpu::WgpuSetupCreateNew, wgpu};
        let mut setup = WgpuSetupCreateNew::without_display_handle();
        // Windows VMs often expose only Microsoft's OpenGL 1.1 implementation.
        // DX12 enumerates WARP as well as hardware adapters; wgpu prefers hardware
        // but accepts the CPU adapter when no suitable GPU is present.
        setup.instance_descriptor.backends = wgpu::Backends::DX12;
        // FXC is part of Windows. Do not introduce a dxcompiler.dll dependency.
        setup
            .instance_descriptor
            .backend_options
            .dx12
            .shader_compiler = wgpu::Dx12Compiler::Fxc;
        eframe::NativeOptions {
            renderer: eframe::Renderer::Wgpu,
            wgpu_options: eframe::WgpuConfiguration {
                wgpu_setup: setup.into(),
                ..Default::default()
            }
            .with_surface_config(eframe::SurfaceConfig::LOW_LATENCY),
            ..Default::default()
        }
    }
    #[cfg(not(target_os = "windows"))]
    eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    }
}
