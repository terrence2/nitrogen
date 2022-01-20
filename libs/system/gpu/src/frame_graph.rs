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
    (Any() {
        $owner:ident, $gpu:ident, $encoder:ident, $need_rebuild:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {{
        $(
            $encoder = $pass_item_name.$pass_name($encoder)?;
        )*
    }};
    (Compute() {
        $owner:ident, $gpu:ident, $encoder:ident, $need_rebuild:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {{
        let _cpass = $encoder.begin_compute_pass(&$crate::wgpu::ComputePassDescriptor {
            label: Some("compute-pass")
        });
        $(
            let _cpass = $pass_item_name.$pass_name(_cpass, $($pass_item_input_name),*)?;
        )*
    }};
    (Render($pass_target_buffer:ident, $pass_target_func:ident) {
        $owner:ident, $gpu:ident, $encoder:ident, $need_rebuild:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {{
        let (color_attachments, depth_stencil_attachment) = $pass_target_buffer.$pass_target_func();
        let render_pass_desc_ref = $crate::wgpu::RenderPassDescriptor {
            label: Some(concat!("non-screen-render-pass-", stringify!($pass_target_buffer))),
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
            )?;
        )*
    }};
    (Render(Screen) {
        $surface_texture:ident, $gpu:ident, $encoder:ident, $need_rebuild:ident, $pass_name:ident, $($pass_item_name:ident ( $($pass_item_input_name:ident),* )),*
     }
    ) => {
        let view = $surface_texture.texture.create_view(&::wgpu::TextureViewDescriptor::default());
        let _rpass = $encoder.begin_render_pass(&$crate::wgpu::RenderPassDescriptor {
            label: Some("screen-render-pass"),
            color_attachments: &[$crate::Gpu::color_attachment(&view)],
            depth_stencil_attachment: Some($gpu.depth_stencil_attachment()),
        });
        $(
            let _rpass = $pass_item_name.$pass_name(
                _rpass,
                $(
                    &$pass_item_input_name
                ),*
            )?;
        )*
    };
}

#[macro_export]
macro_rules! make_frame_graph {
    (
        $name:ident {
            buffers: { $($buffer_name:ident: $buffer_type:ty),* };
            passes: [
                $( $pass_name:ident: $pass_type:ident($($pass_args:ident),*) {
                    $($pass_item_name:ident ( $($pass_item_input_name:ident),* ) ),*
                } ),*
            ];
        }
    ) => {
        ::paste::paste! {

            #[derive(Clone, Debug)]
            pub struct $name {
                $(
                    $buffer_name: ::std::sync::Arc<::parking_lot::RwLock<$buffer_type>>
                ),*,
            }

            impl $name {
                #[allow(clippy::too_many_arguments)]
                pub fn new(
                    $(
                        $buffer_name: ::std::sync::Arc<::parking_lot::RwLock<$buffer_type>>
                    ),*
                ) -> ::anyhow::Result<Self> {
                    Ok(Self {
                        $(
                            $buffer_name
                        ),*
                    })
                }

                $(
                    #[allow(unused)]
                    pub fn $buffer_name(&self) -> ::parking_lot::RwLockReadGuard<$buffer_type> {
                        self.$buffer_name.read()
                    }

                    #[allow(unused)]
                    pub fn [< $buffer_name _mut >] (&mut self) -> ::parking_lot::RwLockWriteGuard<$buffer_type> {
                        self.$buffer_name.write()
                    }
                )*

                pub fn run(&mut self, gpu: std::sync::Arc<::parking_lot::RwLock<$crate::Gpu>>, tracker: $crate::UploadTracker) -> ::anyhow::Result<bool> {
                    $(
                        let $buffer_name = &self.$buffer_name.read();
                    )*
                    let mut gpu = gpu.write();

                    let mut encoder = gpu
                        .device()
                        .create_command_encoder(&$crate::wgpu::CommandEncoderDescriptor {
                            label: Some("frame-encoder"),
                        });

                    let surface_texture = if let Some(surface_texture) = gpu.get_next_framebuffer()? {
                        surface_texture
                    } else {
                        return Ok(true);
                    };

                    {
                        tracker.dispatch_uploads(&mut encoder);
                        $(
                            let mut _need_rebuild = false;
                            $crate::make_frame_graph_pass!($pass_type($($pass_args),*) {
                                surface_texture, gpu, encoder, _need_rebuild, $pass_name, $($pass_item_name ( $($pass_item_input_name),* )),*
                            });
                        )*
                    };
                    gpu.queue_mut().submit(vec![encoder.finish()]);
                    surface_texture.present();

                    Ok(false)
                }
            }
        }
    };
}

#[cfg(test)]
mod test {
    use crate::{Gpu, TestResources, UploadTracker};
    use anyhow::Result;
    use parking_lot::RwLock;
    use std::{cell::RefCell, sync::Arc};

    #[derive(Debug)]
    pub struct TestBuffer {
        render_target: wgpu::TextureView,
        update_count: usize,
        compute_count: RefCell<usize>,
        render_count: RefCell<usize>,
        screen_count: RefCell<usize>,
        any_count: RefCell<usize>,
    }
    impl TestBuffer {
        fn new(gpu: &Gpu) -> Self {
            let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Uint,
                usage: wgpu::TextureUsages::all(),
            });
            let render_target = texture.create_view(&wgpu::TextureViewDescriptor {
                label: None,
                format: Some(wgpu::TextureFormat::Rgba8Uint),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
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
        #[allow(clippy::unnecessary_wraps)]
        fn example_compute_pass<'a>(
            &self,
            cpass: wgpu::ComputePass<'a>,
        ) -> Result<wgpu::ComputePass<'a>> {
            *self.compute_count.borrow_mut() += 1;
            Ok(cpass)
        }
        #[allow(clippy::unnecessary_wraps)]
        fn example_render_pass<'a>(
            &self,
            rpass: wgpu::RenderPass<'a>,
        ) -> Result<wgpu::RenderPass<'a>> {
            *self.render_count.borrow_mut() += 1;
            Ok(rpass)
        }
        fn example_render_pass_attachments(
            &self,
        ) -> (
            [wgpu::RenderPassColorAttachment; 1],
            Option<wgpu::RenderPassDepthStencilAttachment>,
        ) {
            (
                [wgpu::RenderPassColorAttachment {
                    view: &self.render_target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                        store: true,
                    },
                }],
                None,
            )
        }
        #[allow(clippy::unnecessary_wraps)]
        fn example_any_pass(&self, encoder: wgpu::CommandEncoder) -> Result<wgpu::CommandEncoder> {
            *self.any_count.borrow_mut() += 1;
            Ok(encoder)
        }
    }

    #[derive(Debug)]
    pub struct TestRenderer {
        render_count: RefCell<usize>,
    }
    impl TestRenderer {
        #[allow(clippy::unnecessary_wraps)]
        fn new(_gpu: &Gpu, _foo: &TestBuffer) -> Result<Self> {
            Ok(Self {
                render_count: RefCell::new(0),
            })
        }

        #[allow(clippy::unnecessary_wraps)]
        fn draw<'a>(
            &self,
            rpass: wgpu::RenderPass<'a>,
            test_buffer: &'a TestBuffer,
        ) -> Result<wgpu::RenderPass<'a>> {
            *self.render_count.borrow_mut() += 1;
            *test_buffer.screen_count.borrow_mut() += 1;
            Ok(rpass)
        }
    }

    make_frame_graph!(
        FrameGraph {
            buffers: {
                test_buffer: TestBuffer,
                test_renderer: TestRenderer
            };
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
    fn test_basic() -> Result<()> {
        let TestResources { gpu, window, .. } = Gpu::for_test_unix()?;

        let test_buffer = Arc::new(RwLock::new(TestBuffer::new(&gpu.read())));
        let test_renderer = Arc::new(RwLock::new(TestRenderer::new(
            &gpu.read(),
            &test_buffer.read(),
        )?));
        let mut frame_graph = FrameGraph::new(test_buffer, test_renderer)?;

        let mut skip = 0;
        for _ in 0..3 {
            let mut upload_tracker = Default::default();
            frame_graph.test_buffer_mut().update(&mut upload_tracker);
            let need_rebuild = frame_graph.run(gpu.clone(), upload_tracker)?;
            if need_rebuild {
                skip += 1;
                gpu.write()
                    .on_display_config_changed(window.read().config())?;
            }
        }

        assert!(skip < 3);
        assert_eq!(frame_graph.test_buffer().update_count, 3);
        assert_eq!(*frame_graph.test_buffer().compute_count.borrow(), 3 - skip);
        assert_eq!(*frame_graph.test_buffer().screen_count.borrow(), 3 - skip);
        assert_eq!(*frame_graph.test_buffer().render_count.borrow(), 3 - skip);
        assert_eq!(*frame_graph.test_buffer().any_count.borrow(), 3 - skip);
        assert_eq!(*frame_graph.test_renderer().render_count.borrow(), 3 - skip);
        Ok(())
    }
}
