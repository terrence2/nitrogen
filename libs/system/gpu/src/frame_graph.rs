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
    (Render(Screen) {
        $owner:ident, $gpu:ident, $encoder:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {
        let color_attachment = $gpu.get_next_framebuffer()?;

        {
            let _rpass = $encoder.begin_render_pass(&$crate::wgpu::RenderPassDescriptor {
                color_attachments: &[$crate::GPU::color_attachment(&color_attachment.view)],
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
            tracker: $crate::FrameStateTracker,
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
                $(
                    let $renderer_name = &self.$renderer_name;
                )*

                let mut encoder = gpu
                    .device()
                    .create_command_encoder(&$crate::wgpu::CommandEncoderDescriptor {
                        label: Some("frame-encoder"),
                    });
                self.tracker.dispatch_uploads(&mut encoder);
                $(
                    $crate::make_frame_graph_pass!($pass_type($($pass_args),*) {
                        self, gpu, encoder, $pass_name, $($pass_item_name ( $($pass_item_input_name),* )),*
                    });
                )*
                gpu.queue_mut().submit(&[encoder.finish()]);
                self.tracker.reset();

                Ok(())
            }

            pub fn tracker_mut(&mut self) -> &mut $crate::FrameStateTracker {
                &mut self.tracker
            }
        }
    };
}

#[cfg(test)]
mod test {
    use crate::GPU;
    use failure::Fallible;
    use input::InputSystem;
    use std::{cell::RefCell, sync::Arc};

    pub struct TestBuffer {
        render_target: wgpu::TextureView,
        update_count: usize,
        compute_count: RefCell<usize>,
        render_count: RefCell<usize>,
        screen_count: RefCell<usize>,
    }
    impl TestBuffer {
        fn new(gpu: &GPU) -> Arc<RefCell<Self>> {
            let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth: 1,
                },
                array_layer_count: 1,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Uint,
                usage: wgpu::TextureUsage::all(),
            });
            let render_target = texture.create_view(&wgpu::TextureViewDescriptor {
                format: wgpu::TextureFormat::Rgba8Uint,
                dimension: wgpu::TextureViewDimension::D2,
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                array_layer_count: 1,
            });
            Arc::new(RefCell::new(Self {
                render_target,
                update_count: 0,
                compute_count: RefCell::new(0),
                render_count: RefCell::new(0),
                screen_count: RefCell::new(0),
            }))
        }
        fn update(&mut self) {
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
                    load_op: wgpu::LoadOp::Clear,
                    store_op: wgpu::StoreOp::Store,
                    clear_color: wgpu::Color::GREEN,
                }],
                None,
            )
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
                    test_buffer ( )
                },
                example_compute_pass: Compute() {
                    test_buffer ( )
                },
                draw: Render(Screen) {
                    test_renderer ( test_buffer )
                }
            ];
        }
    );

    #[test]
    fn test_basic() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;
        let test_buffer = TestBuffer::new(&gpu);
        let mut frame_graph = FrameGraph::new(&mut gpu, &test_buffer)?;

        for _ in 0..3 {
            test_buffer.borrow_mut().update();
            frame_graph.run(&mut gpu)?;
        }

        assert_eq!(test_buffer.borrow().update_count, 3);
        assert_eq!(*test_buffer.borrow().compute_count.borrow(), 3);
        assert_eq!(*test_buffer.borrow().screen_count.borrow(), 3);
        assert_eq!(*test_buffer.borrow().render_count.borrow(), 3);
        assert_eq!(*frame_graph.test_renderer.render_count.borrow(), 3);

        let _tracker = frame_graph.tracker_mut();
        Ok(())
    }
}
