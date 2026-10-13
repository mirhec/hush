//! Actual DX12/WARP rendering, with no account, database, keyring, tray or network.
//! Run explicitly on Windows: HUSH_WINDOWS_GRAPHICS_TEST=1 cargo test --test windows_graphics

#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn main() {
    if std::env::var("HUSH_WINDOWS_GRAPHICS_TEST").as_deref() != Ok("1") {
        println!("Windows rendering test skipped; set HUSH_WINDOWS_GRAPHICS_TEST=1 to run.");
        return;
    }
    use eframe::{egui, egui_wgpu::WgpuSetup, wgpu};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    // A broken event loop or blocking driver call must fail CI rather than hang it.
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(45));
        eprintln!("Timed out waiting for a rendered Windows software frame");
        std::process::exit(2);
    });

    struct Demo {
        app: hush::ui::HushApp,
        frames: usize,
        captured: Arc<AtomicBool>,
    }
    impl eframe::App for Demo {
        fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
            self.app.logic(ctx, frame);
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = screenshot {
                assert!(image.width() >= 360 && image.height() >= 480);
                let colors: std::collections::HashSet<_> = image.pixels.iter().collect();
                assert!(
                    colors.len() > 32,
                    "Software renderer produced a blank frame"
                );
                self.captured.store(true, Ordering::Relaxed);
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            self.frames += 1;
            if self.frames == 3 {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            }
            ctx.request_repaint();
        }
        fn ui(&mut self, root: &mut egui::Ui, frame: &mut eframe::Frame) {
            self.app.ui(root, frame);
        }
    }

    let mut options = hush::graphics::native_options();
    options.viewport = egui::ViewportBuilder::default().with_inner_size([440., 640.]);
    let WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup else {
        panic!("Expected the application's DX12 setup");
    };
    // Force the same software adapter that an unaccelerated VM relies on.
    // A hardware-capable CI host must not mask a missing WARP fallback.
    setup.native_adapter_selector = Some(Arc::new(|adapters, surface| {
        adapters
            .iter()
            .find(|adapter| {
                let info = adapter.get_info();
                info.backend == wgpu::Backend::Dx12
                    && info.device_type == wgpu::DeviceType::Cpu
                    && surface.is_none_or(|surface| adapter.is_surface_supported(surface))
            })
            .cloned()
            .ok_or_else(|| "No Windows DX12 software adapter (WARP) available".into())
    }));
    let captured = Arc::new(AtomicBool::new(false));
    let result = captured.clone();
    eframe::run_native(
        "Hush Windows software rendering test",
        options,
        Box::new(move |cc| {
            let adapter = cc
                .wgpu_render_state
                .as_ref()
                .expect("DX12 render state")
                .adapter
                .get_info();
            println!(
                "Rendering with {} ({:?}, {:?})",
                adapter.name, adapter.backend, adapter.device_type
            );
            Ok(Box::new(Demo {
                app: hush::ui::HushApp::new(cc, None, None, Default::default(), true, false),
                frames: 0,
                captured,
            }))
        }),
    )
    .expect("Hush must render without OpenGL or a hardware GPU");
    assert!(
        result.load(Ordering::Relaxed),
        "No rendered frame was captured"
    );
    println!("Windows DX12/WARP demo frame: OK");
}
