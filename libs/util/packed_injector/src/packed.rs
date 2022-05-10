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
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse2, Data, DeriveInput, Generics, Ident, ItemFn, Type};

pub(crate) type Ast = DeriveInput;

pub(crate) struct ResourceModel {
    ident: Ident,
    generics: Generics,
    fieldset: Vec<(Ident, Type)>,
    methods: Vec<ItemFn>,
    ast: DeriveInput,
}

pub(crate) fn parse(input: TokenStream) -> Ast {
    parse2(TokenStream2::from(input)).expect("parse result")
}

pub(crate) fn analyze(ast: Ast) -> ResourceModel {
    let mut fieldset = Vec::new();
    match &ast.data {
        Data::Struct(ds) => {
            for field in &ds.fields {
                if let Some(ident) = &field.ident {
                    fieldset.push((ident.to_owned(), field.ty.to_owned()));
                }
            }
        }
        _ => panic!("packed_struct must be used on a _struct_"),
    }
    ResourceModel {
        ident: ast.ident.clone(),
        generics: ast.generics.clone(),
        fieldset,
        methods: Vec::new(),
        ast,
    }
}

pub(crate) fn lower(mut model: ResourceModel) -> ResourceModel {
    for (ident, ty) in &model.fieldset {
        let method: ItemFn = parse2(quote! {
            #[allow(unused)]
            pub fn #ident(&self) -> #ty {
                self.#ident
            }
        })
        .unwrap();
        model.methods.push(method);
    }
    model
}

pub(crate) fn codegen(model: ResourceModel) -> TokenStream {
    let ast = model.ast;
    let methods = model.methods;
    let ident = model.ident;
    let (_impl_generics, ty_generics, where_clause) = model.generics.split_for_impl();
    proc_macro::TokenStream::from(quote! {
        #[repr(C, packed)]
        #[derive(::packed_struct::zerocopy::FromBytes, ::packed_struct::zerocopy::AsBytes)]
        #ast

        impl #ident #ty_generics #where_clause {
            #(#methods)*

            #[allow(unused)]
            pub fn overlay(buf: &[u8]) -> ::packed_struct::anyhow::Result<&#ident> {
                ::packed_struct::zerocopy::LayoutVerified::<&[u8], #ident>::new(buf)
                    .map(|v| v.into_ref())
                    .ok_or_else(|| ::packed_struct::anyhow::anyhow!("cannot overlay"))
            }

            #[allow(unused)]
            pub fn overlay_prefix(buf: &[u8]) -> ::packed_struct::anyhow::Result<&#ident> {
                ::packed_struct::zerocopy::LayoutVerified::<&[u8], #ident>::new(&buf[0..::std::mem::size_of::<#ident>()])
                    .map(|v| v.into_ref())
                    .ok_or_else(|| ::packed_struct::anyhow::anyhow!("cannot overlay"))
            }

            #[allow(unused)]
            pub fn overlay_slice(buf: &[u8]) -> ::packed_struct::anyhow::Result<&[#ident]> {
                ::packed_struct::zerocopy::LayoutVerified::<&[u8], [#ident]>::new_slice(buf)
                    .map(|v| v.into_slice())
                    .ok_or_else(|| ::packed_struct::anyhow::anyhow!("cannot overlay slice"))
            }
        }
    })
}
