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
use font_common::{FontInterface, GlyphFrame};
use gpu::GPU;

// A glyph cache stores the context required to produce and draw a text layout.
pub struct GlyphCache {
    bind_group: wgpu::BindGroup,
    font: Box<dyn FontInterface>,
}

impl GlyphCache {
    pub fn new(
        font: Box<dyn FontInterface>,
        bind_group_layout: &wgpu::BindGroupLayout,
        gpu: &GPU,
    ) -> Self {
        let (texture_view, sampler) = font.gpu_resources();

        let bind_group = gpu.device().create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glyph-cache-TTF-bind-group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self { bind_group, font }
    }

    pub fn render_height(&self) -> f32 {
        self.font.render_height()
    }

    pub fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph-cache-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        dimension: wgpu::TextureViewDimension::D2,
                        component_type: wgpu::TextureComponentType::Uint,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler { comparison: false },
                    count: None,
                },
            ],
        })
    }

    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    pub fn can_render_char(&self, c: char) -> bool {
        self.font.can_render_char(c)
    }

    pub fn frame_for(&self, c: char) -> &GlyphFrame {
        self.font.frame_for(c)
    }

    pub fn pair_kerning(&self, a: char, b: char, scale: f32) -> f32 {
        self.font.pair_kerning(a, b, scale)
    }
}
