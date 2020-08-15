// This file is part of Nitrogen.
//
// Nitrogen is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Nitrogen is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with Nitrogen.  If not, see <http://www.gnu.org/licenses/>.
use crate::{UploadTracker, GPU};
use failure::{err_msg, Fallible};
use log::trace;
use std::{
    any::Any,
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard},
};

#[macro_export]
macro_rules! make_frame_graph {
    (
        $name:ident {
            buffers: { $($buffer_name:ident: $buffer_type:ty),* };
            precompute: { $($precompute_name:ident),* };
            renderers: [
                $( $renderer_name:ident: $renderer_type:ty { $($input_buffer_name:ident),* } ),*
            ];
        }
    ) => {
        pub struct $name {
            tracker: $crate::UploadTracker,
            $(
                $buffer_name: ::std::sync::Arc<::std::cell::RefCell<$buffer_type>>
            ),*,
            $(
                $renderer_name: $renderer_type
            ),*
        }

        impl $name {
            #[allow(clippy::too_many_arguments)]
            pub fn new(
                gpu: &mut $crate::GPU,
                $(
                    $buffer_name: &::std::sync::Arc<::std::cell::RefCell<$buffer_type>>
                ),*
            ) -> ::failure::Fallible<Self> {
                Ok(Self {
                    tracker: Default::default(),
                    $(
                        $buffer_name: $buffer_name.to_owned()
                    ),*,
                    $(
                        $renderer_name: <$renderer_type>::new(
                            gpu,
                            $(
                                &$input_buffer_name.borrow()
                            ),*
                        )?
                    ),*
                })
            }

            pub fn run(&mut self, gpu: &mut $crate::GPU) -> ::failure::Fallible<()> {
                $(
                    let $buffer_name = self.$buffer_name.borrow();
                )*
                let mut frame = gpu.begin_frame()?;
                {
                    frame.apply_all_buffer_to_buffer_uploads(self.tracker.drain_b2b_uploads());
                    frame.apply_all_buffer_to_texture_uploads(self.tracker.drain_b2t_uploads());

                    {
                        let _cpass = frame.begin_compute_pass();
                        $(
                            let _cpass = $precompute_name.precompute(_cpass);
                        )*
                    }

                    {
                        let _rpass = frame.begin_render_pass();
                        $(
                            let _rpass = self.$renderer_name.draw(
                                _rpass,
                                $(
                                    &$input_buffer_name
                                ),*
                            );
                        )*
                    }
                }
                frame.finish();

                Ok(())
            }

            pub fn tracker_mut(&mut self) -> &mut $crate::UploadTracker {
                &mut self.tracker
            }
        }
    };
}

pub trait GpuResource {
    fn name(&self) -> &str;
    fn compute<'a>(&'a self, cpass: wgpu::ComputePass<'a>) -> wgpu::ComputePass<'a> {
        cpass
    }
}

pub struct ComputePass {
    name: String,
    steps: Vec<String>,
}

impl ComputePass {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            steps: Vec::new(),
        }
    }

    pub fn add_step(&mut self, step: Arc<RwLock<dyn GpuResource>>) -> &mut Self {
        self.steps.push(step.read().unwrap().name().to_owned());
        self
    }
}

pub trait RenderStep: Any + 'static {
    fn name(&self) -> &str;
    //fn render<'a>(&'a self, rpass: wgpu::RenderPass<'a>) -> wgpu::RenderPass<'a>;
    fn render<'a>(
        &'a self,
        rpass: wgpu::RenderPass<'a>,
        resources: &'a HashMap<&str, RwLockReadGuard<'a, dyn RenderStep>>,
    ) -> wgpu::RenderPass<'a>;
}

pub struct RenderPass {
    name: String,
    steps: Vec<String>,
}

impl RenderPass {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            steps: Vec::new(),
        }
    }

    pub fn add_step(&mut self, step: Arc<RwLock<dyn RenderStep>>) -> &mut Self {
        self.steps.push(step.read().unwrap().name().to_owned());
        self
    }
}

enum PassHolder {
    Compute(ComputePass),
    Render(RenderPass),
}

impl PassHolder {
    fn as_compute_pass_mut(&mut self) -> &mut ComputePass {
        match self {
            Self::Compute(compute_pass) => compute_pass,
            _ => panic!("not an compute pass"),
        }
    }

    fn as_render_pass_mut(&mut self) -> &mut RenderPass {
        match self {
            Self::Render(render_pass) => render_pass,
            _ => panic!("not an render pass"),
        }
    }
}

pub struct FrameGraph {
    resources: HashMap<String, Arc<RwLock<dyn GpuResource>>>,
    render_steps: HashMap<String, Arc<RwLock<dyn RenderStep>>>,
    passes: Vec<PassHolder>,
}

impl FrameGraph {
    pub fn new(
        resources: &[Arc<RwLock<dyn GpuResource>>],
        render_steps: &[Arc<RwLock<dyn RenderStep>>],
    ) -> Self {
        Self {
            resources: resources
                .iter()
                .map(|resource| (resource.read().unwrap().name().to_owned(), resource.clone()))
                .collect(),
            render_steps: render_steps
                .iter()
                .map(|step| (step.read().unwrap().name().to_owned(), step.clone()))
                .collect(),
            passes: Vec::new(),
        }
    }

    pub fn run(&mut self, gpu: &mut GPU, mut upload_tracker: UploadTracker) -> Fallible<()> {
        // Borrow all resources up front so that the read lock guards will live through the entire
        // encoding process, ensuring that we maintain read access and prevent background write
        // locks.
        let resources: HashMap<&str, RwLockReadGuard<dyn GpuResource>> = self
            .resources
            .iter()
            .map(|(k, v)| (k.as_str(), v.read().unwrap()))
            .collect();
        let render_steps: HashMap<&str, RwLockReadGuard<dyn RenderStep>> = self
            .render_steps
            .iter()
            .map(|(k, v)| (k.as_str(), v.read().unwrap()))
            .collect();

        let mut encoder = gpu
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });

        // Buffer-to-buffer uploads.
        for desc in upload_tracker.b2b_uploads.drain(..) {
            encoder.copy_buffer_to_buffer(
                &desc.source,
                desc.source_offset,
                &desc.destination,
                desc.destination_offset,
                desc.copy_size,
            );
        }

        // Buffer-to-texture uploads.
        for desc in upload_tracker.b2t_uploads.drain(..) {
            encoder.copy_buffer_to_texture(
                wgpu::BufferCopyView {
                    buffer: &desc.source,
                    offset: 0,
                    bytes_per_row: desc.target_extent.width * desc.target_element_size,
                    rows_per_image: desc.target_extent.height,
                },
                wgpu::TextureCopyView {
                    texture: &desc.target,
                    mip_level: 0, // TODO: need to scale extent appropriately
                    array_layer: desc.target_array_layer,
                    origin: wgpu::Origin3d::ZERO,
                },
                desc.target_extent,
            );
        }

        let color_attachment = gpu
            .swap_chain
            .get_next_texture()
            .map_err(|_| err_msg("failed to get next swap chain image"))?;

        for pass in self.passes.iter() {
            match pass {
                PassHolder::Compute(compute_pass) => {
                    let mut cpass = encoder.begin_compute_pass();
                    for step_name in &compute_pass.steps {
                        trace!("{}:{}", compute_pass.name, step_name);
                        cpass = resources[step_name.as_str()].compute(cpass);
                    }
                }
                PassHolder::Render(render_pass) => {
                    let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                            attachment: &color_attachment.view,
                            resolve_target: None,
                            load_op: wgpu::LoadOp::Clear,
                            store_op: wgpu::StoreOp::Store,
                            clear_color: wgpu::Color::GREEN,
                        }],
                        depth_stencil_attachment: Some(
                            wgpu::RenderPassDepthStencilAttachmentDescriptor {
                                attachment: &gpu.depth_texture,
                                depth_load_op: wgpu::LoadOp::Clear,
                                depth_store_op: wgpu::StoreOp::Store,
                                clear_depth: 1f32,
                                stencil_load_op: wgpu::LoadOp::Clear,
                                stencil_store_op: wgpu::StoreOp::Store,
                                clear_stencil: 0,
                            },
                        ),
                    });
                    for step_name in &render_pass.steps {
                        trace!("{}:{}", render_pass.name, step_name);
                        rpass = render_steps[step_name.as_str()].render(rpass, &render_steps);
                    }
                }
            }
        }

        gpu.queue.submit(&[encoder.finish()]);

        Ok(())
    }

    pub fn add_compute_pass(&mut self, name: &str) -> &mut ComputePass {
        let index = self.passes.len();
        self.passes
            .push(PassHolder::Compute(ComputePass::new(name)));
        self.passes[index].as_compute_pass_mut()
    }

    pub fn add_render_pass(&mut self, name: &str) -> &mut RenderPass {
        let index = self.passes.len();
        self.passes.push(PassHolder::Render(RenderPass::new(name)));
        self.passes[index].as_render_pass_mut()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::GPU;
    use failure::Fallible;
    use input::InputSystem;
    use std::{cell::RefCell, sync::Arc};

    pub struct TestBuffer {
        update_count: usize,
        compute_count: RefCell<usize>,
    }
    impl GpuResource for TestBuffer {
        fn name(&self) -> &str {
            "test-buffer"
        }
        fn compute<'a>(&'a self, cpass: wgpu::ComputePass<'a>) -> wgpu::ComputePass<'a> {
            *self.compute_count.borrow_mut() += 1;
            cpass
        }
    }
    impl TestBuffer {
        fn new() -> Self {
            Self {
                update_count: 0,
                compute_count: RefCell::new(0),
            }
        }
        fn update(&mut self, _input: i32, _tracker: &mut UploadTracker) {
            self.update_count += 1;
        }
    }

    pub struct TestRenderer {
        render_count: RefCell<usize>,
        test_buffer: Arc<RwLock<TestBuffer>>,
    }
    impl RenderStep for TestRenderer {
        fn name(&self) -> &str {
            "test-renderer"
        }
        fn render<'a>(&self, rpass: wgpu::RenderPass<'a>) -> wgpu::RenderPass<'a> {
            *self.render_count.borrow_mut() += 1;
            rpass
        }
    }
    impl TestRenderer {
        fn new(_gpu: &GPU, test_buffer: Arc<RwLock<TestBuffer>>) -> Fallible<Self> {
            Ok(Self {
                render_count: RefCell::new(0),
                test_buffer,
            })
        }
    }

    #[test]
    fn test_basic() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;
        let test_buffer = Arc::new(RwLock::new(TestBuffer::new()));
        let test_renderer = Arc::new(RwLock::new(TestRenderer::new(&gpu, test_buffer.clone())?));

        let mut buffer_input = 0;

        let mut frame_graph = FrameGraph::new(&[test_buffer.clone()], &[test_renderer.clone()]);
        frame_graph
            .add_compute_pass("precompute")
            .add_step(test_buffer.clone());
        frame_graph
            .add_render_pass("frame")
            .add_step(test_renderer.clone());

        for i in 0..3 {
            let mut upload_tracker = Default::default();
            test_buffer
                .write()
                .unwrap()
                .update(buffer_input, &mut upload_tracker);
            frame_graph.run(&mut gpu, upload_tracker)?;
        }

        assert_eq!(test_buffer.read().unwrap().update_count, 3);
        assert_eq!(*test_buffer.read().unwrap().compute_count.borrow(), 3);
        assert_eq!(*test_renderer.read().unwrap().render_count.borrow(), 3);

        Ok(())
    }
}
