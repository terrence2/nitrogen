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

#[macro_export]
macro_rules! make_frame_graph_pass {
    (Compute() {
        $owner:ident, $gpu:ident, $encoder:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {{
        let _cpass = $encoder.begin_compute_pass();
        $(
            let _cpass = $pass_item_name.$pass_name(_cpass);
        )*
    }};
    (Any() {
        $owner:ident, $gpu:ident, $encoder:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {{
        $(
            $encoder = $pass_item_name.$pass_name($encoder);
        )*
    }};
    (Render(Screen) {
        $owner:ident, $gpu:ident, $encoder:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {
        // FIXME: Check if the color attachment is sub-optimal and needs to be re-created
        let color_attachment = $gpu.get_next_framebuffer()?;

        {
            let _rpass = $encoder.begin_render_pass(&$crate::wgpu::RenderPassDescriptor {
                color_attachments: &[$crate::GPU::color_attachment(&color_attachment.output.view)],
                depth_stencil_attachment: Some($gpu.depth_stencil_attachment()),
            });
            $(
                let _rpass = $pass_item_name.$pass_name(
                    _rpass,
                    $(
                        &$pass_item_input_name
                    ),*
                );
            )*
        }
    };
    (Render($pass_target_buffer:ident, $pass_target_func:ident) {
        $owner:ident, $gpu:ident, $encoder:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {{
        let (color_attachments, depth_stencil_attachment) = $pass_target_buffer.$pass_target_func();
        let render_pass_desc_ref = $crate::wgpu::RenderPassDescriptor {
            color_attachments: &color_attachments,
            depth_stencil_attachment,
        };
        let _rpass = $encoder.begin_render_pass(&render_pass_desc_ref);
        $(
            let _rpass = $pass_item_name.$pass_name(
                _rpass,
                $(
                    &$pass_item_input_name
                ),*
            );
        )*
    }};
}

#[macro_export]
macro_rules! make_frame_graph {
    (
        $name:ident {
            buffers: { $($buffer_name:ident: $buffer_type:ty),* };
            renderers: [
                $( $renderer_name:ident: $renderer_type:ty { $($input_buffer_name:ident),* } ),*
            ];
            passes: [
                $( $pass_name:ident: $pass_type:ident($($pass_args:ident),*) {
                    $($pass_item_name:ident ( $($pass_item_input_name:ident),* ) ),*
                } ),*
            ];
        }
    ) => {
        pub struct $name {
            $(
                $buffer_name: $buffer_type
            ),*,
            $(
                $renderer_name: $renderer_type
            ),*
        }

        impl $name {
            #[allow(clippy::too_many_arguments)]
            pub fn new(
                _legion: &mut ::legion::world::World,
                gpu: &mut $crate::GPU,
                $(
                    $buffer_name: $buffer_type
                ),*
            ) -> ::failure::Fallible<Self> {
                Ok(Self {
                    $(
                        $renderer_name: <$renderer_type>::new(
                            gpu,
                            $(
                                &$input_buffer_name
                            ),*
                        )?
                    ),*,
                    $(
                        $buffer_name
                    ),*
                })
            }

            $(
                pub fn $buffer_name(&mut self) -> &mut $buffer_type {
                    &mut self.$buffer_name
                }
            )*
            $(
                pub fn $renderer_name(&mut self) -> &mut $renderer_type {
                    &mut self.$renderer_name
                }
            )*

            pub fn run(&mut self, gpu: &mut $crate::GPU, tracker: UploadTracker) -> ::failure::Fallible<()> {
                $(
                    let $buffer_name = &self.$buffer_name;
                )*
                $(
                    let $renderer_name = &self.$renderer_name;
                )*

                let mut encoder = gpu
                    .device()
                    .create_command_encoder(&$crate::wgpu::CommandEncoderDescriptor {
                        label: Some("frame-encoder"),
                    });
                tracker.dispatch_uploads(&mut encoder);
                $(
                    $crate::make_frame_graph_pass!($pass_type($($pass_args),*) {
                        self, gpu, encoder, $pass_name, $($pass_item_name ( $($pass_item_input_name),* )),*
                    });
                )*
                gpu.queue_mut().submit(vec![encoder.finish()]);

                Ok(())
            }
        }

        impl ::command::CommandHandler for $name {
            fn handle_command(&mut self, command: &::command::Command) {
                $(
                    if command.target() == stringify!($buffer_name) {
                        self.$buffer_name.handle_command(command);
                    }
                )*
                $(
                    if command.target() == stringify!($renderer_name) {
                        self.$renderer_name.handle_command(command);
                    }
                )*
            }
        }
    };
}

#[cfg(test)]
mod test {
    use crate::{UploadTracker, GPU};
    use failure::Fallible;
    use input::InputSystem;
    use legion::prelude::*;
    use std::cell::RefCell;

    pub struct TestBuffer {
        render_target: wgpu::TextureView,
        update_count: usize,
        compute_count: RefCell<usize>,
        render_count: RefCell<usize>,
        screen_count: RefCell<usize>,
        any_count: RefCell<usize>,
    }
    impl TestBuffer {
        fn new(gpu: &GPU) -> Self {
            let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Uint,
                usage: wgpu::TextureUsage::all(),
            });
            let render_target = texture.create_view(&wgpu::TextureViewDescriptor {
                label: None,
                format: Some(wgpu::TextureFormat::Rgba8Uint),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: 0,
                array_layer_count: None,
            });
            Self {
                render_target,
                update_count: 0,
                compute_count: RefCell::new(0),
                render_count: RefCell::new(0),
                screen_count: RefCell::new(0),
                any_count: RefCell::new(0),
            }
        }
        fn update(&mut self, _tracker: &mut UploadTracker) {
            self.update_count += 1;
        }
        fn example_compute_pass<'a>(&self, cpass: wgpu::ComputePass<'a>) -> wgpu::ComputePass<'a> {
            *self.compute_count.borrow_mut() += 1;
            cpass
        }
        fn example_render_pass<'a>(&self, rpass: wgpu::RenderPass<'a>) -> wgpu::RenderPass<'a> {
            *self.render_count.borrow_mut() += 1;
            rpass
        }
        fn example_render_pass_attachments(
            &self,
        ) -> (
            [wgpu::RenderPassColorAttachmentDescriptor; 1],
            Option<wgpu::RenderPassDepthStencilAttachmentDescriptor>,
        ) {
            (
                [wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &self.render_target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                        store: true,
                    },
                }],
                None,
            )
        }
        fn example_any_pass(&self, encoder: wgpu::CommandEncoder) -> wgpu::CommandEncoder {
            *self.any_count.borrow_mut() += 1;
            encoder
        }
    }

    pub struct TestRenderer {
        render_count: RefCell<usize>,
    }
    impl TestRenderer {
        fn new(_gpu: &GPU, _foo: &TestBuffer) -> Fallible<Self> {
            Ok(Self {
                render_count: RefCell::new(0),
            })
        }
        fn draw<'a>(
            &self,
            rpass: wgpu::RenderPass<'a>,
            test_buffer: &'a TestBuffer,
        ) -> wgpu::RenderPass<'a> {
            *self.render_count.borrow_mut() += 1;
            *test_buffer.screen_count.borrow_mut() += 1;
            rpass
        }
    }

    make_frame_graph!(
        FrameGraph {
            buffers: {
                test_buffer: TestBuffer
            };
            renderers: [
                test_renderer: TestRenderer { test_buffer }
            ];
            passes: [
                example_render_pass: Render(test_buffer, example_render_pass_attachments) {
                    test_buffer()
                },
                example_compute_pass: Compute() {
                    test_buffer()
                },
                example_any_pass: Any() {
                    test_buffer()
                },
                draw: Render(Screen) {
                    test_renderer ( test_buffer )
                }
            ];
        }
    );

    #[test]
    fn test_basic() -> Fallible<()> {
        let mut legion = World::default();
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;
        let test_buffer = TestBuffer::new(&gpu);
        let mut frame_graph = FrameGraph::new(&mut legion, &mut gpu, test_buffer)?;

        for _ in 0..3 {
            let mut upload_tracker = Default::default();
            frame_graph.test_buffer().update(&mut upload_tracker);
            frame_graph.run(&mut gpu, upload_tracker)?;
        }

        assert_eq!(frame_graph.test_buffer().update_count, 3);
        assert_eq!(*frame_graph.test_buffer().compute_count.borrow(), 3);
        assert_eq!(*frame_graph.test_buffer().screen_count.borrow(), 3);
        assert_eq!(*frame_graph.test_buffer().render_count.borrow(), 3);
        assert_eq!(*frame_graph.test_buffer().any_count.borrow(), 3);
        assert_eq!(*frame_graph.test_renderer().render_count.borrow(), 3);
        Ok(())
    }
}
