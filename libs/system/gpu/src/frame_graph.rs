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
pub use crate::frame_state_tracker::FrameStateTracker;

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
                let mut frame = gpu.begin_frame()?;
                {
                    frame.apply_all_buffer_to_buffer_uploads(self.tracker.drain_b2b_uploads());

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

    pub struct TestBuffer;
    impl TestBuffer {
        fn precompute<'a>(&self, cpass: wgpu::ComputePass<'a>) -> wgpu::ComputePass<'a> {
            cpass
        }
    }

    pub struct TestRenderer;
    impl TestRenderer {
        fn new(_gpu: &GPU, _foo: &TestBuffer) -> Fallible<Self> {
            Ok(Self)
        }
        fn draw<'a>(
            &self,
            rpass: wgpu::RenderPass<'a>,
            _foo: &'a TestBuffer,
        ) -> wgpu::RenderPass<'a> {
            rpass
        }
    }

    make_frame_graph!(
        FrameGraph {
            buffers: {
                foo: TestBuffer
            };
            precompute: { foo };
            renderers: [
                bar: TestRenderer { foo }
            ];
        }
    );

    #[test]
    fn test_basic() -> Fallible<()> {
        let input = InputSystem::new(vec![])?;
        let mut gpu = GPU::new(&input, Default::default())?;
        let foo = Arc::new(RefCell::new(TestBuffer));
        let mut frame_graph = FrameGraph::new(&mut gpu, &foo)?;
        frame_graph.run(&mut gpu)?;
        let _tracker = frame_graph.tracker_mut();
        Ok(())
    }
}
