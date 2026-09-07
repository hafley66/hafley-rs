use std::{io::Write, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let trace: serde_json::Value =
        serde_json::from_slice(&std::fs::read(args.get(1).ok_or("trace path required")?)?)?;
    let frames = trace["frames"].as_array().ok_or("frames required")?;
    let fps = trace["fps"].as_u64().ok_or("fps required")?;
    let output = args.get(2).ok_or("output MP4 path required")?;
    let instance = wgpu::Instance::default();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    eprintln!(
        "GPU_HOST adapter={:?} frames={} fps={fps}",
        adapter.get_info(),
        frames.len()
    );
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("trace capsule presentation"),
        source: wgpu::ShaderSource::Wgsl(include_str!("0_capsules.wgsl").into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 32,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let pixels = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 640 * 480 * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 640 * 480 * 4,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: pixels.as_entire_binding(),
            },
        ],
    });
    let mut encoder_process = std::process::Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
            "640x480",
            "-framerate",
            &fps.to_string(),
            "-i",
            "pipe:0",
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            output,
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    let mut pipe = encoder_process
        .stdin
        .take()
        .ok_or("encoder stdin missing")?;
    let mut contact_frames = 0;
    for frame in frames {
        let hit = frame["hit"].as_bool().ok_or("hit required")?;
        contact_frames += usize::from(hit);
        let values = [
            frame["a"][0].as_f64().ok_or("a.x")? as f32,
            frame["a"][1].as_f64().ok_or("a.y")? as f32,
            frame["a"][2].as_f64().ok_or("a.z")? as f32,
            frame["radius"].as_f64().ok_or("radius")? as f32,
            frame["b"][0].as_f64().ok_or("b.x")? as f32,
            frame["b"][1].as_f64().ok_or("b.y")? as f32,
            frame["b"][2].as_f64().ok_or("b.z")? as f32,
            if hit { 1.0 } else { 0.0 },
        ];
        // This host is an orthographic XY fixture viewer. Reject unsupported depth.
        assert_eq!([values[2], values[6]], [0.0, 0.0]);
        let mut bytes = [0u8; 32];
        for (index, value) in values.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        queue.write_buffer(&uniform, 0, &bytes);
        let mut commands = device.create_command_encoder(&Default::default());
        {
            let mut pass = commands.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(80, 60, 1);
        }
        commands.copy_buffer_to_buffer(&pixels, 0, &readback, 0, 640 * 480 * 4);
        queue.submit([commands.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })?;
        rx.recv()??;
        {
            let mapped = readback.slice(..).get_mapped_range();
            assert!(
                mapped.as_chunks::<4>().0.contains(&[67, 162, 206, 255]),
                "target absent"
            );
            let expected = if hit {
                [255, 101, 68, 255]
            } else {
                [245, 195, 76, 255]
            };
            assert!(
                mapped.as_chunks::<4>().0.contains(&expected),
                "moving capsule absent"
            );
            pipe.write_all(&mapped)?;
        }
        readback.unmap();
    }
    drop(pipe);
    assert!(encoder_process.wait()?.success(), "MP4 encoder failed");
    eprintln!(
        "GPU_HOST_OK frames={} contact_frames={contact_frames}",
        frames.len()
    );
    Ok(())
}
