use std::{io::Write, time::Duration};
use wgpu::util::DeviceExt;

pub const WIDTH: u32 = 960;
pub const HEIGHT: u32 = 540;
pub type Vertex = [f32; 6];

pub struct Capture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    readback: wgpu::Buffer,
    encoder: std::process::Child,
    pipe: Option<std::process::ChildStdin>,
    pub frames: usize,
}

impl Capture {
    pub fn new(output: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))?;
        eprintln!("GPU {:?}", adapter.get_info());
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Falcon fixture wireframes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("0_wire.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(WIDTH * HEIGHT * 4),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = std::process::Command::new("ffmpeg")
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
                "960x540",
                "-framerate",
                "60",
                "-i",
                "pipe:0",
                "-an",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                output,
            ])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        let pipe = encoder.stdin.take();
        Ok(Self {
            device,
            queue,
            pipeline,
            texture,
            readback,
            encoder,
            pipe,
            frames: 0,
        })
    }

    pub fn frame(&mut self, vertices: &[Vertex]) -> Result<(), Box<dyn std::error::Error>> {
        self.frame_regions(vertices, [[50, 600, 130, 445], [580, 690, 150, 270]])
    }

    pub fn frame_regions(
        &mut self,
        vertices: &[Vertex],
        regions: [[u32; 4]; 2],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bytes: Vec<u8> = vertices
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: &bytes,
                usage: wgpu::BufferUsages::VERTEX,
            });
        let mut commands = self.device.create_command_encoder(&Default::default());
        let view = self.texture.create_view(&Default::default());
        {
            let mut pass = commands.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.04,
                            b: 0.065,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        commands.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(WIDTH * 4),
                    rows_per_image: Some(HEIGHT),
                },
            },
            self.texture.size(),
        );
        self.queue.submit([commands.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |r| {
                tx.send(r).unwrap();
            });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })?;
        rx.recv()??;
        {
            let data = self.readback.slice(..).get_mapped_range();
            // Colored geometry must survive actual GPU readback on every frame.
            let pixels = data.as_chunks::<4>().0;
            assert!(
                pixels
                    .iter()
                    .enumerate()
                    .filter(|(i, p)| {
                        let x = *i as u32 % WIDTH;
                        let y = *i as u32 / WIDTH;
                        (regions[0][0]..regions[0][1]).contains(&x)
                            && (regions[0][2]..regions[0][3]).contains(&y)
                            && p[0] > 120
                            && p[0] < 220
                            && p[1] < 160
                            && p[2] > 190
                    })
                    .count()
                    > 100,
                "Falcon geometry absent"
            );
            assert!(
                pixels
                    .iter()
                    .enumerate()
                    .filter(|(i, p)| {
                        let x = *i as u32 % WIDTH;
                        let y = *i as u32 / WIDTH;
                        (regions[1][0]..regions[1][1]).contains(&x)
                            && (regions[1][2]..regions[1][3]).contains(&y)
                            && ((p[0] < 100 && p[1] > 180 && p[2] > 180)
                                || (p[0] > 220 && p[1] > 70 && p[1] < 140 && p[2] < 90))
                    })
                    .count()
                    > 30,
                "sandbag geometry absent"
            );
            self.pipe.as_mut().unwrap().write_all(&data)?;
        }
        self.readback.unmap();
        self.frames += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<(), Box<dyn std::error::Error>> {
        drop(self.pipe.take());
        assert!(self.encoder.wait()?.success());
        eprintln!("CAPTURE_OK {} frames", self.frames);
        Ok(())
    }
}

pub fn line(out: &mut Vec<Vertex>, a: [f32; 2], b: [f32; 2], width: f32, color: [f32; 4]) {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let len = dx.hypot(dy).max(0.001);
    let n = [-dy / len * width * 0.5, dx / len * width * 0.5];
    let points = [
        [a[0] + n[0], a[1] + n[1]],
        [b[0] + n[0], b[1] + n[1]],
        [b[0] - n[0], b[1] - n[1]],
        [a[0] - n[0], a[1] - n[1]],
    ];
    for i in [0, 1, 2, 0, 2, 3] {
        out.push([
            points[i][0] / WIDTH as f32 * 2.0 - 1.0,
            1.0 - points[i][1] / HEIGHT as f32 * 2.0,
            color[0],
            color[1],
            color[2],
            color[3],
        ]);
    }
}
