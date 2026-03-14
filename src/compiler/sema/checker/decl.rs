use crate::compiler::ast::Span;
use crate::compiler::ast::{ImportItem, Program, Statement};
use crate::compiler::sema::checker::{ClassInfo, SemanticAnalyzer, SemanticErrorKind};
use crate::compiler::sema::ty::Type;
use std::collections::HashMap;

impl SemanticAnalyzer {
    pub fn collect_definitions(&mut self, program: &Program) {
        let saved_file = self.current_file.clone();
        self.current_file = program.file_path.clone();
        for stmt in &program.statements {
            let (actual_stmt, is_exported) = match stmt {
                Statement::Export { decl, .. } => (&**decl, true),
                _ => (stmt, false),
            };

            if let Statement::ClassDeclaration {
                name,
                name_span,
                fields,
                methods,
                constructor: _,
                span,
                doc,
            } = actual_stmt
            {
                if self.classes.contains_key(name) || self.scope.lookup_local(name).is_some() {
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                }
                let mut field_map = HashMap::new();
                let mut static_field_map = HashMap::new();
                for f in fields {
                    if field_map.contains_key(&f.name) || static_field_map.contains_key(&f.name) {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(f.name.clone()),
                            f.name_span,
                        );
                    }
                    let ty = self.resolve_type(f.ty.clone());
                    if f.is_static {
                        static_field_map.insert(
                            f.name.clone(),
                            (ty, f.name_span, f.doc.as_ref().map(|d| d.content())),
                        );
                    } else {
                        field_map.insert(
                            f.name.clone(),
                            (ty, f.name_span, f.doc.as_ref().map(|d| d.content())),
                        );
                    }
                }
                let mut method_map = HashMap::new();
                let mut static_method_map = HashMap::new();
                for m in methods {
                    if method_map.contains_key(&m.name)
                        || static_method_map.contains_key(&m.name)
                        || field_map.contains_key(&m.name)
                        || static_field_map.contains_key(&m.name)
                    {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(m.name.clone()),
                            m.name_span,
                        );
                    }
                    let param_tys = m
                        .params
                        .iter()
                        .map(|(_, ty)| self.resolve_type(ty.clone()))
                        .collect();
                    let ret_ty = self.resolve_type(m.return_ty.clone());
                    if m.is_static {
                        static_method_map.insert(
                            m.name.clone(),
                            (
                                param_tys,
                                ret_ty,
                                m.doc.as_ref().map(|d| d.content()),
                                m.name_span,
                            ),
                        );
                    } else {
                        method_map.insert(
                            m.name.clone(),
                            (
                                param_tys,
                                ret_ty,
                                m.doc.as_ref().map(|d| d.content()),
                                m.name_span,
                            ),
                        );
                    }
                }
                self.classes.insert(
                    name.clone(),
                    ClassInfo {
                        name: name.clone(),
                        fields: field_map,
                        static_fields: static_field_map,
                        methods: method_map,
                        static_methods: static_method_map,
                        is_exported,
                        defined_in: self.current_file.clone(),
                        span: *span,
                        doc: doc.as_ref().map(|d| d.content()),
                    },
                );
            } else if let Statement::FunctionDeclaration {
                name,
                name_span,
                params,
                return_ty,
                body: _,
                is_async: _,
                span: _,
                doc,
            } = actual_stmt
            {
                if self.classes.contains_key(name) || self.scope.lookup_local(name).is_some() {
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                }
                let param_tys = params
                    .iter()
                    .map(|(_, ty)| self.resolve_type(ty.clone()))
                    .collect();
                let ret_ty = self.resolve_type(return_ty.clone());
                self.scope.insert(
                    name.clone(),
                    Type::Function(param_tys, Box::new(ret_ty)),
                    false,
                    true, // function declarations are constant
                    is_exported,
                    *name_span,
                    self.current_file.clone(),
                    doc.as_ref().map(|d| d.content()),
                );
            } else if let Statement::VarDeclaration {
                name,
                name_span,
                ty,
                value: _,
                is_const,
                span: _,
                doc,
            } = actual_stmt
            {
                if self.classes.contains_key(name) || self.scope.lookup_local(name).is_some() {
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                }
                // In pass 1, we try to use the declared type if available.
                // Otherwise we use Unknown, and it will be properly inferred in pass 2.
                let var_ty = ty
                    .as_ref()
                    .map(|t| self.resolve_type(t.clone()))
                    .unwrap_or(Type::Unknown);
                self.scope.insert(
                    name.clone(),
                    var_ty,
                    false,
                    *is_const,
                    is_exported,
                    *name_span,
                    self.current_file.clone(),
                    doc.as_ref().map(|d| d.content()),
                );
            } else if let Statement::Enum(decl) = actual_stmt {
                if self.classes.contains_key(&decl.name)
                    || self.scope.lookup_local(&decl.name).is_some()
                {
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(decl.name.clone()),
                        decl.name_span,
                    );
                }

                self.scope.insert(
                    decl.name.clone(),
                    Type::Enum(decl.name.clone()),
                    false,
                    true,
                    is_exported,
                    decl.name_span,
                    self.current_file.clone(),
                    decl.doc.as_ref().map(|d| d.content()),
                );
            } else if let Statement::Import {
                path,
                path_span,
                item,
                ..
            } = actual_stmt
            {
                self.load_import(path.clone(), *path_span);
                match item {
                    ImportItem::Named(names) => {
                        for (name, name_span) in names {
                            let sym_info = self
                                .scope
                                .lookup(name)
                                .map(|s| (s.is_exported, s.defined_in.clone(), s.span));
                            let class_info = self
                                .classes
                                .get(name)
                                .map(|c| (c.is_exported, self.current_file.clone(), c.span)); // simplified defined_in for classes

                            let export_check = if let Some(info) = sym_info {
                                Some(info)
                            } else {
                                class_info
                            };

                            if let Some((is_exported, def_file, def_span)) = export_check {
                                if !is_exported {
                                    self.error(
                                        SemanticErrorKind::ExportRequired(name.clone()),
                                        *name_span,
                                    );
                                }
                                self.record_definition(*name_span, def_file, def_span);

                                // Insert placeholder with correct is_exported flag if it was a symbol
                                // (Classes are already in self.classes)
                                if self.scope.lookup_local(name).is_none()
                                    && self.classes.get(name).is_none()
                                {
                                    // Placeholder insert for variables/functions
                                    self.scope.insert(
                                        name.clone(),
                                        Type::Unknown,
                                        false,
                                        false,
                                        is_exported,
                                        *name_span,
                                        self.current_file.clone(),
                                        None,
                                    );
                                }
                            } else {
                                self.error(
                                    SemanticErrorKind::UndefinedImport(name.clone(), path.clone()),
                                    *name_span,
                                );
                            }
                        }
                    }
                    ImportItem::Namespace((_ns, ns_span)) => {
                        if let Ok(abs_p) = self.resolve_import_path(path) {
                            self.record_definition(
                                *ns_span,
                                abs_p.to_string_lossy().to_string(),
                                Span::new(1, 1),
                            );
                        }
                    }
                }
            }
        }
        self.current_file = saved_file;
    }
}
