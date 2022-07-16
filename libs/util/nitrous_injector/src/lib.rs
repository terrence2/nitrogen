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
mod component;
mod component_injector;
mod injector_common;
mod resource;
mod resource_injector;

use crate::injector_common::make_augment_method;

use proc_macro::TokenStream;
use syn::{parse2, ItemFn};

/// Adds a derivation of the nitrous::ScriptResource trait and associated methods.
/// These methods proxy to various _inner versions, which are built using #[method],
/// #[getter], and #[setter] attributes on the impl block, built by using
/// #[inject_nitrous] on an impl.
#[proc_macro_derive(NitrousResource, attributes(property))]
pub fn derive_nitrous_resource(input: TokenStream) -> TokenStream {
    let ast = resource::parse(input);
    let model = resource::analyze(ast);
    let ir = resource::lower(model);
    resource::codegen(ir)
}

/// Adds a derivation of the nitrous::ScriptComponent trait and associated methods.
/// These methods proxy to various _inner versions, which are built using #[method],
/// #[getter], and #[setter] attributes on the impl block, built by using
/// #[inject_nitrous] on an impl.
#[proc_macro_derive(NitrousComponent, attributes(Name, property))]
pub fn derive_nitrous_component(input: TokenStream) -> TokenStream {
    let ast = component::parse(input);
    let model = component::analyze(ast);
    let ir = component::lower(model);
    component::codegen(ir)
}

/// Add to the top of a Resource impl block to collect all tagged methods and build
/// call and attributes for Nitrous. Note that this is not the external trait,
/// which is built from #[derive(NitrousResource)] above the struct.
#[proc_macro_attribute]
pub fn inject_nitrous_resource(
    args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let ast = resource_injector::parse(args, item);
    let model = resource_injector::analyze(ast);
    let ir = resource_injector::lower(model);
    resource_injector::codegen(ir)
}

/// Add to the top of a Component impl block to collect all tagged methods and build
/// call and attributes for Nitrous. Note that this is not the external trait,
/// which is built from #[derive(NitrousComponent)] above the struct.
#[proc_macro_attribute]
pub fn inject_nitrous_component(
    args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let ast = component_injector::parse(args, item);
    let model = component_injector::analyze(ast);
    let ir = component_injector::lower(model);
    component_injector::codegen(ir)
}

/// A tag for #[nitrous_injector] indicating to include this function as a method.
#[proc_macro_attribute]
pub fn method(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = proc_macro2::TokenStream::from(input);

    let output: proc_macro2::TokenStream = {
        let item: ItemFn = parse2(input).unwrap();
        make_augment_method(item)
    };

    proc_macro::TokenStream::from(output)
}

/// Just a tag for the injector.
#[proc_macro_attribute]
pub fn getter(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}

/// Just a tag for the injector.
#[proc_macro_attribute]
pub fn setter(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
}
