use crate::bridged_type::{BridgedType, OnlyEncoding, TypePosition, HostLang};
use crate::parse::TypeDeclarations;
use crate::SWIFT_BRIDGE_PREFIX;
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std::fmt::{Debug, Formatter};
use syn::spanned::Spanned;
use syn::{LitStr, Path, Type};
use quote::{format_ident, quote_spanned};

pub(crate) use self::struct_field::StructField;
pub(crate) use self::struct_field::StructFields;
use self::struct_field::UnnamedStructField;

mod struct_field;

#[derive(Clone)]
pub(crate) struct SharedStruct {
    pub name: Ident,
    pub swift_repr: StructSwiftRepr,
    pub fields: StructFields,
    pub swift_name: Option<LitStr>,
    pub already_declared: bool,
    pub is_tuple: bool,
}

impl SharedStruct {
    pub(crate) fn to_swift_type(&self, type_pos: TypePosition, types: &TypeDeclarations) -> String {
        if self.is_tuple {
            return self.combine_field_types_swift_name_with_type_pos(type_pos, types);
        }
        match self.swift_name.as_ref() {
            Some(ty) => ty.value(),
            None => self.name.to_string(),
        }
    }

    pub(crate) fn swift_name_string(&self, types: &TypeDeclarations) -> String {
        if self.is_tuple {
            return self.combine_field_types_swift_name(types);
        }
        match self.swift_name.as_ref() {
            Some(ty) => ty.value(),
            None => self.name.to_string(),
        }
    }

    pub(crate) fn ffi_name_string(&self, types: &TypeDeclarations) -> String {
        if self.is_tuple {
            return format!("{}${}${}", SWIFT_BRIDGE_PREFIX, self.name.to_string(), self.combine_field_types_string(types));            
        }
        let name = self.swift_name_string(types);

        format!("{}${}", SWIFT_BRIDGE_PREFIX, name)
    }

    pub(crate) fn ffi_name_tokens(&self) -> TokenStream {
        let name = Ident::new(
            &format!("{}{}", SWIFT_BRIDGE_PREFIX, &self.name),
            self.name.span(),
        );

        quote! {
            #name
        }
    }

    /// __swift_bridge__Option_SomeStruct
    pub fn ffi_option_name_tokens(&self) -> TokenStream {
        let name = Ident::new(
            &format!("{}Option_{}", SWIFT_BRIDGE_PREFIX, self.name),
            self.name.span(),
        );
        quote! { #name }
    }

    /// __swift_bridge__$Option$SomeStruct
    pub fn ffi_option_name_string(&self, types: &TypeDeclarations) -> String {
        format!(
            "{}$Option${}",
            SWIFT_BRIDGE_PREFIX,
            self.swift_name_string(types)
        )
    }

    /// Some if the struct has a single variant.
    /// TODO: If all of the struct's fields have an `OnlyEncoding`, then the struct has exactly
    ///  one encoding as well.
    pub fn only_encoding(&self, types: &TypeDeclarations) -> Option<OnlyEncoding> {
        let has_fields = !self.fields.is_empty();
        if has_fields || self.already_declared {
            return None;
        }

        let struct_name = &self.name;
        let empty_fields = self.fields.empty_field_wrapper();

        Some(OnlyEncoding {
            swift: format!("{}()", self.swift_name_string(types)),
            rust: quote! {#struct_name #empty_fields},
        })
    }
}

impl SharedStruct {
    /// Convert the FFI representation of this struct into its Rust struct format.
    pub(crate) fn convert_ffi_repr_to_rust(
        &self,
        rust_val: &TokenStream,
        swift_bridge_path: &Path,
        types: &TypeDeclarations,
    ) -> TokenStream {
        let struct_name = &self.name;

        let converted_fields: Vec<TokenStream> = self
            .fields
            .normalized_fields()
            .iter()
            .map(|norm_field| {
                let maybe_name_and_colon = norm_field.maybe_name_and_colon();
                let access_field = norm_field.append_field_accessor(&quote! {val});

                let ty = BridgedType::new_with_type(&norm_field.ty, types).unwrap();
                let converted_field = ty.convert_ffi_expression_to_rust_type(
                    &access_field,
                    norm_field.ty.span(),
                    swift_bridge_path,
                    types,
                );

                quote! {
                    #maybe_name_and_colon #converted_field
                }
            })
            .collect();

        let converted_fields = self.wrap_fields(&converted_fields);

        if self.fields.is_empty() {
            quote! {
                #struct_name #converted_fields
            }
        } else {
            quote! {
                { let val = #rust_val; #struct_name #converted_fields }
            }
        }
    }

    pub(crate) fn generate_into_ffi_repr_method(&self, expression: &TokenStream,
        types: &TypeDeclarations,
        swift_bridge_path: &Path,
        span: Span,) -> TokenStream{
        let struct_name = &self.name;
        let struct_ffi_name = format_ident!("{}{}", SWIFT_BRIDGE_PREFIX, struct_name);

        let converted_fields: Vec<TokenStream> = self
            .fields
            .normalized_fields()
            .iter()
            .map(|norm_field| {
                let maybe_name_and_colon = norm_field.maybe_name_and_colon();
                let access_field = norm_field.append_field_accessor(&quote! {val});

                let ty = BridgedType::new_with_type(&norm_field.ty, types).unwrap();
                let converted_field = ty.convert_rust_expression_to_ffi_type(
                    &access_field,
                    swift_bridge_path,
                    types,
                    span,
                );

                quote! {
                    #maybe_name_and_colon #converted_field
                }
            })
            .collect();

        let converted_fields = self.wrap_fields(&converted_fields);

        let ffi_name = self.ffi_name_tokens();

        let convert_rust_to_ffi = if self.fields.is_empty() {
            quote! {
                #ffi_name { _private: 123 }
            }
        } else {
            quote! {
                { let val = #expression; #ffi_name #converted_fields }
            }
        };

        quote!{
            impl #struct_name {
                #[doc(hidden)]
                #[inline(always)]
                pub fn into_ffi_repr(self) -> #struct_ffi_name {
                    #convert_rust_to_ffi
                }
            }
        }
    }

    pub(crate) fn convert_swift_to_ffi_repr(
        &self,
        expression: &str,
        types: &TypeDeclarations,
    ) -> String {
        let struct_name = &self.ffi_name_string(types);

        let converted_fields: Vec<String> = self
            .fields
            .normalized_fields()
            .iter()
            .map(|norm_field| {
                let field_name = norm_field.ffi_field_name();
                let ty = BridgedType::new_with_type(&norm_field.ty, types).unwrap();
                let access_field = ty.convert_swift_expression_to_ffi_type(
                    &format!("val.{field_name}", field_name = field_name),
                    types,
                    TypePosition::SharedStructField,
                );

                format!(
                    "{field_name}: {access_field}",
                    field_name = field_name,
                    access_field = access_field
                )
            })
            .collect();
        let converted_fields = converted_fields.join(", ");

        if self.fields.is_empty() {
            format!("{struct_name}(_private: 123)", struct_name = &struct_name,)
        } else {
            format!(
                "{{ let val = {expression}; return {struct_name}({converted_fields}); }}()",
                struct_name = &struct_name,
                expression = expression,
                converted_fields = converted_fields
            )
        }
    }

    pub(crate) fn convert_ffi_expression_to_swift(
        &self,
        expression: &str,
        types: &TypeDeclarations,
    ) -> String {
        let struct_name = &self.swift_name_string(types);

        let converted_fields: Vec<String> = self
            .fields
            .normalized_fields()
            .iter()
            .map(|norm_field| {
                let field_name = norm_field.ffi_field_name();

                let ty = BridgedType::new_with_type(&norm_field.ty, types).unwrap();
                let access_field = ty.convert_ffi_value_to_swift_value(
                    &format!("val.{field_name}", field_name = field_name),
                    TypePosition::SharedStructField,
                    types,
                );

                format!(
                    "{field_name}: {access_field}",
                    field_name = field_name,
                    access_field = access_field
                )
            })
            .collect();
        let converted_fields = converted_fields.join(", ");

        if self.fields.is_empty() {
            format!("{struct_name}()", struct_name = &struct_name,)
        } else {
            format!(
                "{{ let val = {expression}; return {struct_name}({converted_fields}); }}()",
                expression = expression,
                struct_name = &struct_name,
                converted_fields = converted_fields
            )
        }
    }

    fn wrap_fields(&self, fields: &[TokenStream]) -> TokenStream {
        match &self.fields {
            StructFields::Named(_) => {
                quote! {
                    { #(#fields),* }
                }
            }
            StructFields::Unnamed(_) => {
                quote! {
                    ( #(#fields),* )
                }
            }
            StructFields::Unit => {
                debug_assert_eq!(fields.len(), 0);
                quote! {}
            }
        }
    }

    pub fn tuple_from(types: &Vec<Type>) -> Option<Self>{
        let unnamed_fields = types.iter().enumerate().map(|(idx, ty)|
            UnnamedStructField{
                ty: ty.clone(),
                idx: idx
            }
        ).collect();
        Some(SharedStruct {
            name: format_ident!("tuple"),
            swift_repr: StructSwiftRepr::Structure,
            fields: StructFields::Unnamed(unnamed_fields),
            swift_name: None,
            already_declared: false, 
            is_tuple: true,
        })
    }

    fn combine_field_types_swift_name_with_type_pos(&self, type_pos: TypePosition, types: &TypeDeclarations) -> String {
        match &self.fields {
            StructFields::Named(_) => todo!(),
            StructFields::Unnamed(unnamed_fiels) => {
                let names: Vec<String> = unnamed_fiels.iter().enumerate().map(|(_idx, field)|BridgedType::new_with_type(&field.ty, types).unwrap().to_swift_type(type_pos, types)).collect();
                let names = names.join(", ");
                let names = "(".to_string() + &names;
                let names = names + ")";
                return names;
            },
            StructFields::Unit => todo!(),
        }
    }

    fn combine_field_types_swift_name(&self, types: &TypeDeclarations) -> String {
        match &self.fields {
            StructFields::Named(_) => todo!(),
            StructFields::Unnamed(unnamed_fiels) => {
                let names: Vec<String> = unnamed_fiels.iter().enumerate().map(|(idx, field)|BridgedType::new_with_type(&field.ty, types).unwrap().to_swift_type(TypePosition::FnArg(HostLang::Rust, idx), types)).collect();
                let names = names.join(", ");
                let names = "(".to_string() + &names;
                let names = names + ")";
                return names;
            },
            StructFields::Unit => todo!(),
        }
    }

    fn combine_field_types_string(&self, types: &TypeDeclarations) -> String {
        match &self.fields {
            StructFields::Named(_) => todo!(),
            StructFields::Unnamed(unnamed_fields) => unnamed_fields.iter().map(|field|BridgedType::new_with_type(&field.ty, types).unwrap().to_rust_type_path(types).to_string()).fold("".to_string(), |sum, s| sum+&s),
            StructFields::Unit => todo!(),
        }
    }

    fn combine_field_types_tokens(&self, _swift_bridge_path: &Path, types: &TypeDeclarations) -> Vec<TokenStream> {
        match &self.fields {
            StructFields::Named(_) => todo!(),
            StructFields::Unnamed(unnamed_fields) => unnamed_fields.iter().map(|field|BridgedType::new_with_type(&field.ty, types).unwrap().to_rust_type_path(types)).collect(),
            StructFields::Unit => todo!(),
        }
    }

    pub fn generate_prefixed_type_name_tokens(&self, swift_bridge_path: &Path, types: &TypeDeclarations) -> TokenStream {
        if self.is_tuple {
            let combined_types = self.combine_field_types_string(types);
            let ty_name = format_ident!("{}_{}", self.name, combined_types);
            let prefixed_ty_name = Ident::new(
                &format!("{}{}", SWIFT_BRIDGE_PREFIX, ty_name),
                ty_name.span(),
            );
        
            let prefixed_ty_name = if self.already_declared {
                quote! { <super:: #ty_name as #swift_bridge_path::SharedStruct>::FfiRepr }
            } else {
                quote! { #prefixed_ty_name }
            };
        
            return prefixed_ty_name;
        }
        let ty_name = &self.name;
        
        let prefixed_ty_name = Ident::new(
            &format!("{}{}", SWIFT_BRIDGE_PREFIX, ty_name),
            ty_name.span(),
        );
    
        let prefixed_ty_name = if self.already_declared {
            quote! { <super:: #ty_name as #swift_bridge_path::SharedStruct>::FfiRepr }
        } else {
            quote! { #prefixed_ty_name }
        };
    
        prefixed_ty_name   
    }

    pub fn convert_ffi_expression_to_rust_type(
        &self,
        value: &TokenStream,
        span: Span,
        _swift_bridge_path: &Path,
        _types: &TypeDeclarations,
    ) -> TokenStream {
        if self.is_tuple {
            let fields: Vec<TokenStream> = self
            .fields
            .normalized_fields()
            .iter()
            .map(|norm_field| {
                let access_field = norm_field.append_field_accessor(&quote! {#value});
                access_field
            })
            .collect();
            return quote_spanned!{
                span => ( #(#fields),* )
            };
        }
        quote_spanned! {span=>
            #value.into_rust_repr()
        }
    }

    pub fn convert_rust_expression_to_ffi_type(&self, expression: &TokenStream, types: &TypeDeclarations) -> TokenStream{
        if let Some(_only) = self.only_encoding(types) {
            return quote! { {#expression;} };
        }
        if self.is_tuple {
            let combined_types = self.combine_field_types_string(types);
            let ty_name = format_ident!("{}_{}", self.name, combined_types);
            let prefixed_ty_name = Ident::new(
                &format!("{}{}", SWIFT_BRIDGE_PREFIX, ty_name),
                ty_name.span(),
            );
            return quote!{
                let val = #expression;
                #prefixed_ty_name(val.0, val.1)
            };
        }
        quote! {
            #expression.into_ffi_repr()
        }
    }

    pub(crate) fn generate_custom_rust_ffi_type(&self, swift_bridge_path: &Path, types: &TypeDeclarations) -> Option<TokenStream>{
        if self.is_tuple {
            let combined_types_string = self.combine_field_types_string(types);
            let combined_types_tokens = self.combine_field_types_tokens(swift_bridge_path, types);
            let ty_name = format_ident!("{}_{}", self.name, combined_types_string);
            let prefixed_ty_name = Ident::new(
                &format!("{}{}", SWIFT_BRIDGE_PREFIX, ty_name),
                ty_name.span(),
            );
            return Some(quote!{
                #[repr(C)]
                #[doc(hidden)]
                pub struct #prefixed_ty_name ( #(#combined_types_tokens),* );
            });
        }
        None
    }

    pub(crate) fn convert_ffi_expression_to_swift_type(&self, expression: &str, type_pos: TypePosition, types: &TypeDeclarations) -> String {
        if let Some(only) = self.only_encoding(types) {
            return format!("{{ let _ = {}; return {} }}()", expression, only.swift);
        }
        if self.is_tuple {
            let converted_fields: Vec<String> = match &self.fields {
                StructFields::Named(_) => todo!(),
                StructFields::Unnamed(unnamed_fields) => unnamed_fields.iter().enumerate().map(|(idx, field)|{
                    let ty = BridgedType::new_with_type(&field.ty, types).unwrap();
                    let converted_field = ty.convert_ffi_value_to_swift_value(&format!("val._{idx}"), type_pos, types);
                    converted_field
                }).collect(),
                StructFields::Unit => todo!(),
            };
            let converted_fields = converted_fields.join(", ");

            return format!("let val = {}; return ({converted_fields});", expression);
        }
        format!("{}.intoSwiftRepr()", expression)
    }
    pub fn convert_swift_expression_to_ffi_type(
        &self,
        expression: &str,
        type_pos: TypePosition,
        types: &TypeDeclarations,
    ) -> String {
        if self.is_tuple {  
            let converted_fields: Vec<String> = match &self.fields {
                StructFields::Unnamed(unnamed_fields) => unnamed_fields.iter().enumerate().map(|(idx, field)|{
                    let ty = BridgedType::new_with_type(&field.ty, types).unwrap();
                    let converted_field = ty.convert_swift_expression_to_ffi_type(&format!("{expression}.{idx}"), types, type_pos);
                    format!("_{idx}: ")+&converted_field
                }).collect(),
                _ => todo!()
            };
            let converted_fields = converted_fields.join(", ");
            return format!("{}${}${}({})", SWIFT_BRIDGE_PREFIX, self.name, self.combine_field_types_string(types), converted_fields);
        }
        if let Some(_only) = self.only_encoding(types) {
            return format!("{{ let _ = {}; }}()", expression);
        }
        format!("{}.intoFfiRepr()", expression)
    }
    pub fn generate_custom_c_ffi_type(&self, types: &TypeDeclarations) -> Option<String> {
        if self.is_tuple {
            let combined_types = self.combine_field_types_string(types);
            let fields: Vec<String> = match &self.fields {
                StructFields::Unnamed(unnamed_fields) => unnamed_fields.iter().enumerate().map(|(idx, field)|{
                    let field = BridgedType::new_with_type(&field.ty, types).unwrap().to_c(types);
                    return format!("{} _{}", field, idx);
                }).collect(),
                _ => todo!(),
            };
            let fields = fields.join("; ");
            let fields = fields + ";";
            let c_decl = format!("typedef struct __swift_bridge__$tuple${combined_types} {{ {fields} }} __swift_bridge__$tuple${combined_types};");
            return Some(c_decl);
        }
        None
    }
    pub fn contains_owned_string_recursive(&self, types: &TypeDeclarations) -> bool {
        if self.is_tuple {
            return match &self.fields {
                StructFields::Named(_) => todo!(),
                StructFields::Unnamed(unnamed_fields) => unnamed_fields.iter().map(|field|{
                    return BridgedType::new_with_type(&field.ty, types).unwrap();
                }).any(|ty|ty.contains_owned_string_recursive(types)),
                StructFields::Unit => todo!(),
            };
        }
        false
    }
}

impl PartialEq for SharedStruct {
    fn eq(&self, other: &Self) -> bool {
        self.name.to_string() == other.name.to_string()
            && self.swift_repr == other.swift_repr
            && self.fields == other.fields
            && self.swift_name.as_ref().map(|l| l.value())
                == other.swift_name.as_ref().map(|l| l.value())
            && self.already_declared == other.already_declared
    }
}

impl Debug for SharedStruct {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedStruct")
            .field("name", &self.name.to_string())
            .field("swift_repr", &self.swift_repr)
            .field("fields", &self.fields)
            .field("swift_name", &self.swift_name.as_ref().map(|l| l.value()))
            .field("already_declared", &self.already_declared)
            .finish()
    }
}

/// Whether to create a class or a structure when creating the Swift representation of a shared
/// struct.
///
/// https://docs.swift.org/swift-book/LanguageGuide/ClassesAndStructures.html
#[derive(Debug, Copy, Clone, PartialEq)]
pub(crate) enum StructSwiftRepr {
    Class,
    /// # Invariants
    ///
    /// (These invariants aren't implemented yet)
    ///
    /// - Cannot be owned by Swift it it contains one or more fields that need to run destructors.
    ///   - Since Swift struct cannot run de-initializers on structs. Only on classes.
    /// - Can always be passed to Swift by immutable reference
    ///   - Since this means Swift does not need to run any de-initializers, which it cannot do
    ///     for structs.
    Structure,
}
