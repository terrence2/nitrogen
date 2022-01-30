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
mod injector;
mod resource;

use crate::injector::make_augment_method;

use proc_macro::TokenStream;
use syn::{parse2, ItemFn};

/// Adds a derivation of the nitrous::ScriptResource trait and associated methods.
/// These methods proxy to various _inner versions, which are built using #[method],
/// #[getter], and #[setter] attributes on the impl block, built by using
/// #[inject_nitrous] on an impl.
#[proc_macro_derive(NitrousResource)]
pub fn derive_nitrous_resource(input: TokenStream) -> TokenStream {
    let ast = resource::parse(input);
    let model = resource::analyze(ast);
    let ir = resource::lower(model);
    let rust = resource::codegen(ir);
    rust
}

/// Adds a derivation of the nitrous::ScriptComponent trait and associated methods.
/// These methods proxy to various _inner versions, which are built using #[method],
/// #[getter], and #[setter] attributes on the impl block, built by using
/// #[inject_nitrous] on an impl.
#[proc_macro_derive(NitrousComponent, attributes(Name))]
pub fn derive_nitrous_component(input: TokenStream) -> TokenStream {
    let ast = component::parse(input);
    let model = component::analyze(ast);
    let ir = component::lower(model);
    let rust = component::codegen(ir);
    rust
}

/// Add to the top of an impl block to collect all tagged methods and build
/// call and attributes for Nitrous. Note that this is not the external trait,
/// which is built from #[derive(Nitrous___)] above the struct.
#[proc_macro_attribute]
pub fn inject_nitrous(
    args: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let ast = injector::parse(args, item);
    let model = injector::analyze(ast);
    let ir = injector::lower(model);
    let rust = injector::codegen(ir);
    rust
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
