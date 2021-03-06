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
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::borrow::Borrow;
use syn::{
    parse2,
    visit::{self, Visit},
    Arm, DeriveInput, Expr, FnArg, GenericArgument, Ident, ImplItemMethod, ItemFn, ItemImpl, Pat,
    PathArguments, ReturnType, Type, TypePath,
};

pub(crate) fn make_derive_nitrous_module(item: DeriveInput) -> TokenStream2 {
    let ident = item.ident;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    quote! {
        impl #impl_generics ::nitrous::Module for #ident #ty_generics #where_clause {
            fn module_name(&self) -> String {
                stringify!(#ident).to_owned()
            }

            fn call_method(&mut self, name: &str, args: &[::nitrous::Value]) -> ::failure::Fallible<::nitrous::Value> {
                self.__call_method_inner__(name, args)
            }

            fn put(&mut self, module: ::std::sync::Arc<::parking_lot::RwLock<dyn ::nitrous::Module>>, name: &str, value: ::nitrous::Value) -> ::failure::Fallible<()> {
                self.__put_inner__(module, name, value)
            }

            fn get(&self, module: ::std::sync::Arc<::parking_lot::RwLock<dyn ::nitrous::Module>>, name: &str) -> ::failure::Fallible<::nitrous::Value> {
                self.__get_inner__(module, name)
            }
        }
    }
}

pub(crate) fn make_augment_method(item: ItemFn) -> TokenStream2 {
    quote! {
        #[allow(clippy::unnecessary_wraps)]
        #item
    }
}

pub(crate) fn make_inject_attribute(item: ItemImpl) -> TokenStream2 {
    let ty = &item.self_ty;
    let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();

    let mut visitor = CollectorVisitor::new();
    visitor.visit_item_impl(&item);

    let mut method_arms = Vec::new();
    let mut get_arms = Vec::new();
    let mut put_arms = Vec::new();

    for (item, args, ret) in visitor.methods {
        let name = format!("{}", item);

        let mut arg_items = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let expr: Expr = match arg.ty {
                Scalar::Boolean => parse2(quote! { args[#i].to_bool()? }).unwrap(),
                Scalar::Integer => parse2(quote! { args[#i].to_int()? }).unwrap(),
                Scalar::Float => parse2(quote! { args[#i].to_float()? }).unwrap(),
                Scalar::StrRef => parse2(quote! { args[#i].to_str()? }).unwrap(),
                Scalar::String => parse2(quote! { args[#i].to_str()?.to_owned() }).unwrap(),
                Scalar::Value => parse2(quote! { args[#i].clone() }).unwrap(),
                Scalar::Unit => parse2(quote! { Value::True() }).unwrap(),
            };
            arg_items.push(expr);
        }

        let toks = match ret {
            RetType::Nothing => {
                quote! { #name => { self.#item( #(#arg_items),* ); Ok(::nitrous::Value::True()) } }
            }
            RetType::Raw(llty) => match llty {
                Scalar::Boolean => {
                    quote! { #name => { Ok(::nitrous::Value::Boolean(self.#item( #(#arg_items),* ))) } }
                }
                Scalar::Integer => {
                    quote! { #name => { Ok(::nitrous::Value::Integer(self.#item( #(#arg_items),* ))) } }
                }
                Scalar::Float => {
                    quote! { #name => { Ok(::nitrous::Value::Float(::ordered_float::OrderedFloat(self.#item( #(#arg_items),* )))) } }
                }
                Scalar::String => {
                    quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_items),* ))) } }
                }
                Scalar::StrRef => {
                    quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_items),* ).to_owned())) } }
                }
                Scalar::Value => {
                    quote! { #name => { Ok(self.#item( #(#arg_items),* )) } }
                }
                Scalar::Unit => {
                    quote! { #name => { self.#item( #(#arg_items),* ); Ok(::nitrous::Value::True()) } }
                }
            },
            RetType::FallibleRaw(llty) => match llty {
                Scalar::Boolean => {
                    quote! { #name => { Ok(::nitrous::Value::Boolean(self.#item( #(#arg_items),* )?)) } }
                }
                Scalar::Integer => {
                    quote! { #name => { Ok(::nitrous::Value::Integer(self.#item( #(#arg_items),* )?)) } }
                }
                Scalar::Float => {
                    quote! { #name => { Ok(::nitrous::Value::Float(::ordered_float::OrderedFloat(self.#item( #(#arg_items),* )?))) } }
                }
                Scalar::String => {
                    quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_items),* )?)) } }
                }
                Scalar::StrRef => {
                    quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_items),* )?.to_owned())) } }
                }
                Scalar::Value => {
                    quote! { #name => { self.#item( #(#arg_items),* ) } }
                }
                Scalar::Unit => {
                    quote! { #name => { self.#item( #(#arg_items),* )?; Ok(::nitrous::Value::True()) } }
                }
            },
        };

        let call_arm: Arm = parse2(toks).unwrap();
        method_arms.push(call_arm);

        let lookup_arm: Arm =
            parse2(quote! { #name => { Ok(::nitrous::Value::Method(module, #name.to_owned())) } })
                .unwrap();
        get_arms.push(lookup_arm);
    }

    for item in visitor.getters {
        let name = format!("{}", item);
        let arm: Arm = parse2(quote! { { #name => self.#item() } }).unwrap();
        get_arms.push(arm);
    }

    for item in visitor.setters {
        let name = format!("{}", item);
        let arm: Arm = parse2(quote! { #name => { self.#item(value) } }).unwrap();
        put_arms.push(arm);
    }

    quote! {
        impl #impl_generics #ty #ty_generics #where_clause {
            fn __call_method_inner__(&mut self, name: &str, args: &[::nitrous::Value]) -> ::failure::Fallible<::nitrous::Value> {
                match name {
                    #(#method_arms)*
                    _ => {
                        log::warn!("Unknown call_method '{}' passed to {}", name, stringify!(#ty));
                        ::failure::bail!("Unknown call_method '{}' passed to {}", name, stringify!(#ty))
                    }
                }
            }

            fn __get_inner__(&self, module: ::std::sync::Arc<::parking_lot::RwLock<dyn ::nitrous::Module>>, name: &str) -> ::failure::Fallible<::nitrous::Value> {
                match name {
                    #(#get_arms)*
                    _ => {
                        log::warn!("Unknown get '{}' passed to {}", name, stringify!(#ty));
                        ::failure::bail!("Unknown get '{}' passed to {}", name, stringify!(#ty));
                    }
                }
            }

            fn __put_inner__(&mut self, module: ::std::sync::Arc<::parking_lot::RwLock<dyn ::nitrous::Module>>, name: &str, value: ::nitrous::Value) -> ::failure::Fallible<()> {
                match name {
                    #(#put_arms)*
                    _ => {
                        log::warn!("Unknown put '{}' passed to {}", name, stringify!(#ty));
                        ::failure::bail!("Unknown put '{}' passed to {}", name, stringify!(#ty));
                    }
                }
            }
        }

        #item
    }
}

#[derive(Debug)]
enum Scalar {
    Boolean,
    Integer,
    Float,
    String,
    StrRef,
    Value,
    Unit,
}

impl Scalar {
    fn from_type(ty: &Type) -> Self {
        if let Type::Path(p) = ty {
            Self::from_type_path(p)
        } else if let Type::Reference(r) = ty {
            match Self::from_type(&r.elem) {
                Scalar::StrRef => Scalar::StrRef,
                v => panic!(
                    "nitrous Scalar only support references to str, not: {:#?}",
                    v
                ),
            }
        } else if let Type::Tuple(tt) = ty {
            if tt.elems.is_empty() {
                Scalar::Unit
            } else {
                panic!("nitrous Scalar only supports the unit tuple type")
            }
        } else {
            panic!(
                "nitrous Scalar only supports path and ref types, not: {:#?}",
                ty
            );
        }
    }

    fn type_path_name(p: &TypePath) -> String {
        assert_eq!(
            p.path.segments.len(),
            1,
            "nitrous Scalar paths must have length 1 (e.g. builtins)"
        );
        p.path.segments.first().unwrap().ident.to_string()
    }

    fn from_type_path(p: &TypePath) -> Self {
        match Self::type_path_name(p).as_str() {
            "bool" => Scalar::Boolean,
            "i64" => Scalar::Integer,
            "f64" => Scalar::Float,
            "str" => Scalar::StrRef,
            "String" => Scalar::String,
            "Value" => Scalar::Value,
            _ => panic!(
                "nitrous Scalar only supports basic types, not: {:#?}",
                p.path.segments.first().unwrap()
            ),
        }
    }
}

#[derive(Debug)]
enum RetType {
    Nothing,
    Raw(Scalar),
    FallibleRaw(Scalar),
}

impl RetType {
    fn from_return_type(ret_ty: &ReturnType) -> Self {
        match ret_ty {
            ReturnType::Default => RetType::Nothing,
            ReturnType::Type(_, ref ty) => {
                if let Type::Path(p) = ty.borrow() {
                    match Scalar::type_path_name(p).as_str() {
                        "Fallible" => {
                            if let PathArguments::AngleBracketed(ab_generic_args) =
                                &p.path.segments.first().unwrap().arguments
                            {
                                let fallible_arg = ab_generic_args.args.first().unwrap();
                                if let GenericArgument::Type(ty_inner) = fallible_arg {
                                    RetType::FallibleRaw(Scalar::from_type(ty_inner))
                                } else {
                                    panic!("Fallible parameter must be a type");
                                }
                            } else {
                                panic!("Fallible must have an angle bracketed argument");
                            }
                        }
                        _ => Self::Raw(Scalar::from_type(ty.borrow())),
                    }
                } else {
                    panic!("nitrous RetType only supports Fallible, None, Value, and basic types")
                }
            }
        }
    }
}

#[derive(Debug)]
struct ArgDef {
    name: Ident,
    ty: Scalar,
}

struct CollectorVisitor {
    methods: Vec<(Ident, Vec<ArgDef>, RetType)>,
    getters: Vec<Ident>,
    setters: Vec<Ident>,
}

impl CollectorVisitor {
    fn new() -> Self {
        Self {
            methods: Vec::new(),
            getters: Vec::new(),
            setters: Vec::new(),
        }
    }
}

impl<'ast> Visit<'ast> for CollectorVisitor {
    fn visit_impl_item_method(&mut self, node: &'ast ImplItemMethod) {
        for attr in &node.attrs {
            if attr.path.is_ident("method") {
                let args = node
                    .sig
                    .inputs
                    .iter()
                    .filter(|arg| matches!(arg, FnArg::Typed(_)))
                    .map(|arg| match arg {
                        FnArg::Receiver(_) => panic!("already filtered out receivers"),
                        FnArg::Typed(pat_type) => {
                            if let Pat::Ident(ident) = pat_type.pat.borrow() {
                                ArgDef {
                                    name: ident.ident.clone(),
                                    ty: Scalar::from_type(pat_type.ty.borrow()),
                                }
                            } else {
                                panic!("only identifier patterns supported as nitrous method args")
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                let ret = RetType::from_return_type(&node.sig.output);
                self.methods.push((node.sig.ident.clone(), args, ret));
                break;
            } else if attr.path.is_ident("getter") {
                self.getters.push(node.sig.ident.clone());
                break;
            } else if attr.path.is_ident("setter") {
                self.setters.push(node.sig.ident.clone());
                break;
            }
        }
        visit::visit_impl_item_method(self, node);
    }
}
