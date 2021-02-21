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
mod injector;

use crate::injector::{make_derive_nitrous_module, make_inject_attribute};

use proc_macro::TokenStream;
use syn::{parse2, DeriveInput, ItemImpl};

/// Adds a derivation of the nitrous::Module trait and associated methods.
/// These methods proxy to various _inner versions, which are built
/// using #[method], #[getter], and #[setter] attributes on the impl block.
#[proc_macro_derive(NitrousModule)]
pub fn derive_nitrous_module(input: TokenStream) -> TokenStream {
    let input = proc_macro2::TokenStream::from(input);

    let output: proc_macro2::TokenStream = {
        let item: DeriveInput = parse2(input).unwrap();
        make_derive_nitrous_module(item)
    };

    proc_macro::TokenStream::from(output)
}

/// Add to the top of an impl block to collect all tagged methods and build
/// call and attributes for Nitrous. Note that this is not the
/// external trait, which is built from #[derive(NitrousModule)] above the struct.
#[proc_macro_attribute]
pub fn inject_nitrous_module(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let input = proc_macro2::TokenStream::from(input);

    let output: proc_macro2::TokenStream = {
        let item: ItemImpl = parse2(input).unwrap();
        make_inject_attribute(item)
    };

    proc_macro::TokenStream::from(output)
}

/// Just a tag for the injector.
#[proc_macro_attribute]
pub fn method(
    _attr: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    input
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
