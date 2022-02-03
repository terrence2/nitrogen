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
use crate::injector_common::*;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse2, visit::Visit, Arm};

pub(crate) fn parse(_args: TokenStream, item: TokenStream) -> Ast {
    parse2(TokenStream2::from(item)).expect("parse result")
}

pub(crate) fn analyze(ast: Ast) -> InjectModel {
    let mut visitor = CollectorVisitor::new();
    visitor.visit_item_impl(&ast);
    InjectModel::new(ast, visitor)
}

fn make_resource_get_arm(_type_name: &str, name: &str) -> Arm {
    parse2(quote! { #name => { Ok(::nitrous::Value::make_resource_method::<Self>(#name)) } })
        .unwrap()
}

pub(crate) fn lower(model: InjectModel) -> Ir {
    let InjectModel {
        item,
        methods,
        // FIXME: generate getter and setter methods
        ..
    } = model;
    let mut ir = Ir::new(item);
    lower_help(&mut ir, make_resource_get_arm);
    lower_methods(methods, &mut ir, make_resource_get_arm);
    ir
}

pub(crate) fn codegen(ir: Ir) -> TokenStream {
    let Ir {
        item,
        method_arms,
        get_arms,
        put_arms,
        names,
        help_items,
    } = ir;
    let ty = &item.self_ty;
    let (impl_generics, _ty_generics, where_clause) = item.generics.split_for_impl();
    let ts2 = quote! {
        impl #impl_generics #ty #where_clause {
            fn __call_method_inner__(&mut self, name: &str, args: &[::nitrous::Value]) -> ::anyhow::Result<::nitrous::Value> {
                match name {
                    #(#method_arms)*
                    _ => {
                        log::warn!("Unknown call_method '{}' passed to {}", name, stringify!(#ty));
                        ::anyhow::bail!("Unknown call_method '{}' passed to {}", name, stringify!(#ty))
                    }
                }
            }

            fn __get_inner__(&self, name: &str) -> ::anyhow::Result<::nitrous::Value> {
                match name {
                    #(#get_arms)*
                    _ => {
                        log::warn!("Unknown get '{}' passed to {}", name, stringify!(#ty));
                        ::anyhow::bail!("Unknown get '{}' passed to {}", name, stringify!(#ty));
                    }
                }
            }

            fn __put_inner__(&mut self, name: &str, value: ::nitrous::Value) -> ::anyhow::Result<()> {
                match name {
                    #(#put_arms)*
                    _ => {
                        log::warn!("Unknown put '{}' passed to {}", name, stringify!(#ty));
                        ::anyhow::bail!("Unknown put '{}' passed to {}", name, stringify!(#ty));
                    }
                }
            }

            fn __names_inner__(&self) -> Vec<&str> {
                vec![#(#names),*]
            }

            fn __show_help__(&self) -> ::anyhow::Result<::nitrous::Value> {
                let items = vec![#(#help_items),*];
                let out = items.join("\n");
                Ok(::nitrous::Value::String(out))
            }
        }

        #item
    };
    TokenStream::from(ts2)
}
