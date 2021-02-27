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
                LLType::Boolean => parse2(quote! { args[#i].to_bool()? }).unwrap(),
                LLType::Integer => parse2(quote! { args[#i].to_int()? }).unwrap(),
                LLType::Float => parse2(quote! { args[#i].to_float()? }).unwrap(),
                LLType::StrRef => parse2(quote! { args[#i].to_str()? }).unwrap(),
                LLType::String => parse2(quote! { args[#i].to_str()?.to_owned() }).unwrap(),
                LLType::Value => parse2(quote! { args[#i].clone() }).unwrap(),
                LLType::Unit => parse2(quote! { Value::True() }).unwrap(),
            };
            arg_items.push(expr);
        }

        let toks = match ret {
            LLRetType::Nothing => {
                quote! { #name => { self.#item( #(#arg_items),* ); Ok(::nitrous::Value::True()) } }
            }
            LLRetType::Raw(llty) => match llty {
                LLType::Boolean => {
                    quote! { #name => { Ok(::nitrous::Value::Boolean(self.#item( #(#arg_items),* ))) } }
                }
                LLType::Integer => {
                    quote! { #name => { Ok(::nitrous::Value::Integer(self.#item( #(#arg_items),* ))) } }
                }
                LLType::Float => {
                    quote! { #name => { Ok(::nitrous::Value::Float(self.#item( #(#arg_items),* ))) } }
                }
                LLType::String => {
                    quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_items),* ))) } }
                }
                LLType::StrRef => {
                    quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_items),* ).to_owned())) } }
                }
                LLType::Value => {
                    quote! { #name => { Ok(self.#item( #(#arg_items),* )) } }
                }
                LLType::Unit => {
                    quote! { #name => { self.#item( #(#arg_items),* ); Ok(::nitrous::Value::True()) } }
                }
            },
            LLRetType::FallibleRaw(llty) => match llty {
                LLType::Boolean => {
                    quote! { #name => { Ok(::nitrous::Value::Boolean(self.#item( #(#arg_items),* )?)) } }
                }
                LLType::Integer => {
                    quote! { #name => { Ok(::nitrous::Value::Integer(self.#item( #(#arg_items),* )?)) } }
                }
                LLType::Float => {
                    quote! { #name => { Ok(::nitrous::Value::Float(self.#item( #(#arg_items),* )?)) } }
                }
                LLType::String => {
                    quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_items),* )?)) } }
                }
                LLType::StrRef => {
                    quote! { #name => { Ok(::nitrous::Value::String(self.#item( #(#arg_items),* )?.to_owned())) } }
                }
                LLType::Value => {
                    quote! { #name => { self.#item( #(#arg_items),* ) } }
                }
                LLType::Unit => {
                    quote! { #name => { self.#item( #(#arg_items),* )?; Ok(::nitrous::Value::True()) } }
                }
            },
        };

        let call_arm: Arm = parse2(toks).unwrap();
        method_arms.push(call_arm);

        let lookup_arm: Arm =
            parse2(quote! { #name => ::nitrous::Value::Method(module, #name.to_owned()) }).unwrap();
        get_arms.push(lookup_arm);
    }

    for item in visitor.getters {
        let name = format!("{}", item);
        let arm: Arm = parse2(quote! { #name => self.#item() }).unwrap();
        get_arms.push(arm);
    }

    for item in visitor.setters {
        let name = format!("{}", item);
        let arm: Arm = parse2(quote! { #name => self.#item(value) }).unwrap();
        put_arms.push(arm);
    }

    quote! {
        impl #impl_generics #ty #ty_generics #where_clause {
            fn __call_method_inner__(&mut self, name: &str, args: &[::nitrous::Value]) -> ::failure::Fallible<::nitrous::Value> {
                match name {
                    // Note: this first case makes the trailing comma valid if there are no
                    //       actual arms found above.
                    "" => {
                        log::warn!("Empty name passed to call_method on {}", stringify!(#ty));
                        ::failure::bail!("Empty name passed to call_method on {}", stringify!(#ty))
                    }
                    #(#method_arms),*,
                    _ => {
                        log::warn!("Unknown call_method '{}' passed to {}", name, stringify!(#ty));
                        ::failure::bail!("Unknown call_method '{}' passed to {}", name, stringify!(#ty))
                    }
                }
            }

            fn __get_inner__(&self, module: ::std::sync::Arc<::parking_lot::RwLock<dyn ::nitrous::Module>>, name: &str) -> ::failure::Fallible<::nitrous::Value> {
                Ok(match name {
                    // Note: this first case makes the trailing comma valid if there are no
                    //       actual arms found above.
                    "" => {
                        log::warn!("Empty name passed to get on {}", stringify!(#ty));
                        ::failure::bail!("Empty name passed to get on {}", stringify!(#ty))
                    }
                    #(#get_arms),*,
                    _ => {
                        log::warn!("Unknown get '{}' passed to {}", name, stringify!(#ty));
                        ::failure::bail!("Unknown get '{}' passed to {}", name, stringify!(#ty))
                    }
                })
            }

            fn __put_inner__(&mut self, module: ::std::sync::Arc<::parking_lot::RwLock<dyn ::nitrous::Module>>, name: &str, value: ::nitrous::Value) -> ::failure::Fallible<()> {
                match name {
                    // Note: this first case makes the trailing comma valid if there are no
                    //       actual arms found above.
                    "" => {
                        log::warn!("Empty name passed to put on {}", stringify!(#ty));
                        ::failure::bail!("Empty name passed to put on {}", stringify!(#ty))
                    }
                    #(#put_arms),*,
                    _ => {
                        log::warn!("Unknown put '{}' passed to {}", name, stringify!(#ty));
                        ::failure::bail!("Unknown put '{}' passed to {}", name, stringify!(#ty))
                    }
                }
            }
        }

        #item
    }
}

#[derive(Debug)]
enum LLType {
    Boolean,
    Integer,
    Float,
    String,
    StrRef,
    Value,
    Unit,
}

impl LLType {
    fn from_type(ty: &Type) -> Self {
        if let Type::Path(p) = ty {
            Self::from_type_path(p)
        } else if let Type::Reference(r) = ty {
            match Self::from_type(&r.elem) {
                LLType::StrRef => LLType::StrRef,
                v => panic!(
                    "nitrous LLType only support references to str, not: {:#?}",
                    v
                ),
            }
        } else if let Type::Tuple(tt) = ty {
            if tt.elems.is_empty() {
                LLType::Unit
            } else {
                panic!("nitrous LLType only supports the unit tuple type")
            }
        } else {
            panic!(
                "nitrous LLType only supports path and ref types, not: {:#?}",
                ty
            );
        }
    }

    fn type_path_name(p: &TypePath) -> String {
        assert_eq!(
            p.path.segments.len(),
            1,
            "nitrous LLType paths must have length 1 (e.g. builtins)"
        );
        p.path.segments.first().unwrap().ident.to_string()
    }

    fn from_type_path(p: &TypePath) -> Self {
        match Self::type_path_name(p).as_str() {
            "bool" => LLType::Boolean,
            "i64" => LLType::Integer,
            "f64" => LLType::Float,
            "str" => LLType::StrRef,
            "String" => LLType::String,
            "Value" => LLType::Value,
            _ => panic!(
                "nitrous LLType only supports basic types, not: {:#?}",
                p.path.segments.first().unwrap()
            ),
        }
    }
}

#[derive(Debug)]
enum LLRetType {
    Nothing,
    Raw(LLType),
    FallibleRaw(LLType),
}

impl LLRetType {
    fn from_return_type(ret_ty: &ReturnType) -> Self {
        match ret_ty {
            ReturnType::Default => LLRetType::Nothing,
            ReturnType::Type(_, ref ty) => {
                if let Type::Path(p) = ty.borrow() {
                    match LLType::type_path_name(p).as_str() {
                        "Fallible" => {
                            if let PathArguments::AngleBracketed(ab_generic_args) =
                                &p.path.segments.first().unwrap().arguments
                            {
                                let fallible_arg = ab_generic_args.args.first().unwrap();
                                if let GenericArgument::Type(ty_inner) = fallible_arg {
                                    LLRetType::FallibleRaw(LLType::from_type(ty_inner))
                                } else {
                                    panic!("Fallible parameter must be a type");
                                }
                            } else {
                                panic!("Fallible must have an angle bracketed argument");
                            }
                        }
                        _ => Self::Raw(LLType::from_type(ty.borrow())),
                    }
                } else {
                    panic!("nitrous LLRetType only supports Fallible, None, Value, and basic types")
                }
            }
        }
    }
}

#[derive(Debug)]
struct ArgDef {
    name: Ident,
    ty: LLType,
}

struct CollectorVisitor {
    methods: Vec<(Ident, Vec<ArgDef>, LLRetType)>,
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
                                    ty: LLType::from_type(pat_type.ty.borrow()),
                                }
                            } else {
                                panic!("only identifier patterns supported as nitrous method args")
                            }
                        }
                    })
                    .collect::<Vec<_>>();
                let ret = LLRetType::from_return_type(&node.sig.output);
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
