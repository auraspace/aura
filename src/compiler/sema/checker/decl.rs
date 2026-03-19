use crate::compiler::ast::Span;
use crate::compiler::ast::{AccessModifier, ImportItem, Program, Statement};
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
                type_params,
                extends,
                implements,
                fields,
                methods,
                constructor,
                is_abstract,
                span,
                doc,
            } = actual_stmt
            {
                if self.classes.contains_key(name)
                    || self.interfaces.contains_key(name)
                    || self.scope.lookup_local(name).is_some()
                {
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(name.clone()),
                        *name_span,
                    );
                }
                self.push_scope();
                for tp in type_params {
                    self.scope.insert(
                        tp.name.clone(),
                        Type::GenericParam(tp.name.clone()),
                        false,
                        true,
                        false,
                        tp.span,
                        self.current_file.clone(),
                        None,
                    );
                }

                let mut field_map = HashMap::new();
                for f in fields {
                    if field_map.contains_key(&f.name) {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(f.name.clone()),
                            f.name_span,
                        );
                    }
                    let ty = self.resolve_type(f.ty.clone());
                    field_map.insert(
                        f.name.clone(),
                        crate::compiler::sema::checker::FieldInfo {
                            ty,
                            is_static: f.is_static,
                            is_readonly: f.is_readonly,
                            defined_in_class: name.clone(),
                            access: f.access,
                            span: f.name_span,
                            doc: f.doc.as_ref().map(|d| d.content()),
                        },
                    );
                }
                let mut method_map = HashMap::new();
                for m in methods {
                    if method_map.contains_key(&m.name) || field_map.contains_key(&m.name) {
                        self.error(
                            SemanticErrorKind::DuplicateDeclaration(m.name.clone()),
                            m.name_span,
                        );
                    }
                    self.push_scope();
                    for tp in &m.type_params {
                        self.scope.insert(
                            tp.name.clone(),
                            Type::GenericParam(tp.name.clone()),
                            false,
                            true,
                            false,
                            tp.span,
                            self.current_file.clone(),
                            None,
                        );
                    }

                    let mut param_tys = Vec::new();
                    for (_, ty) in &m.params {
                        param_tys.push(self.resolve_type(ty.clone()));
                    }
                    let ret_ty = self.resolve_type(m.return_ty.clone());
                    method_map.insert(
                        m.name.clone(),
                        crate::compiler::sema::checker::MethodInfo {
                            type_params: m.type_params.clone(),
                            params: param_tys,
                            ret_ty,
                            is_static: m.is_static,
                            is_async: m.is_async,
                            is_override: m.is_override,
                            is_abstract: m.is_abstract,
                            defined_in_class: name.clone(),
                            access: m.access,
                            span: m.name_span,
                            doc: m.doc.as_ref().map(|d| d.content()),
                        },
                    );
                    self.pop_scope();

                    if m.is_abstract && !*is_abstract {
                        self.error(
                            crate::compiler::sema::checker::SemanticErrorKind::AbstractMethodInConcreteClass(
                                name.clone(),
                                m.name.clone(),
                            ),
                            m.name_span,
                        );
                    }
                    if m.is_abstract && !matches!(*m.body, Statement::Error) {
                        self.error(
                            crate::compiler::sema::checker::SemanticErrorKind::AbstractMethodWithBody(
                                name.clone(),
                                m.name.clone(),
                            ),
                            m.name_span,
                        );
                    }
                }

                let ctor_info = constructor.as_ref().map(|c| {
                    let mut param_tys = Vec::new();
                    for (_, ty) in &c.params {
                        param_tys.push(self.resolve_type(ty.clone()));
                    }
                    (param_tys, c.access)
                });
                self.pop_scope();

                self.classes.insert(
                    name.clone(),
                    ClassInfo {
                        name: name.clone(),
                        parent: extends.clone(),
                        implements: implements.clone(),
                        type_params: type_params.clone(),
                        fields: field_map,
                        methods: method_map,
                        constructor: ctor_info,
                        is_exported,
                        is_abstract: *is_abstract,
                        defined_in: self.current_file.clone(),
                        span: *span,
                        doc: doc.as_ref().map(|d| d.content()),
                    },
                );

                // Basic validation for parent existence
                if let Some(crate::compiler::ast::TypeExpr::Name(parent_name, _)) = extends {
                    if parent_name == name {
                        self.error(
                            SemanticErrorKind::CircularInheritance(name.clone()),
                            *name_span,
                        );
                    }
                }
            } else if let Statement::Interface(decl) = actual_stmt {
                if self.classes.contains_key(&decl.name)
                    || self.interfaces.contains_key(&decl.name)
                    || self.scope.lookup_local(&decl.name).is_some()
                {
                    self.error(
                        SemanticErrorKind::DuplicateDeclaration(decl.name.clone()),
                        decl.name_span,
                    );
                }
                self.push_scope();
                for tp in &decl.type_params {
                    self.scope.insert(
                        tp.name.clone(),
                        Type::GenericParam(tp.name.clone()),
                        false,
                        true,
                        false,
                        tp.span,
                        self.current_file.clone(),
                        None,
                    );
                }

                let mut field_map = HashMap::new();
                for f in &decl.fields {
                    let ty = self.resolve_type(f.ty.clone());
                    field_map.insert(
                        f.name.clone(),
                        crate::compiler::sema::checker::FieldInfo {
                            ty,
                            is_static: false,
                            is_readonly: f.is_readonly,
                            defined_in_class: decl.name.clone(),
                            access: AccessModifier::Public,
                            span: f.name_span,
                            doc: f.doc.as_ref().map(|d| d.content()),
                        },
                    );
                }
                let mut method_map = HashMap::new();
                for m in &decl.methods {
                    self.push_scope();
                    for tp in &m.type_params {
                        self.scope.insert(
                            tp.name.clone(),
                            Type::GenericParam(tp.name.clone()),
                            false,
                            true,
                            false,
                            tp.span,
                            self.current_file.clone(),
                            None,
                        );
                    }
                    let mut param_tys = Vec::new();
                    for (_, ty) in &m.params {
                        param_tys.push(self.resolve_type(ty.clone()));
                    }
                    let ret_ty = self.resolve_type(m.return_ty.clone());
                    method_map.insert(
                        m.name.clone(),
                        crate::compiler::sema::checker::MethodInfo {
                            type_params: m.type_params.clone(),
                            params: param_tys,
                            ret_ty,
                            is_static: false,
                            is_async: false,
                            is_override: false,
                            is_abstract: true,
                            defined_in_class: decl.name.clone(),
                            access: AccessModifier::Public,
                            span: m.name_span,
                            doc: m.doc.as_ref().map(|d| d.content()),
                        },
                    );
                    self.pop_scope();
                }
                self.pop_scope();

                self.interfaces.insert(
                    decl.name.clone(),
                    crate::compiler::sema::checker::InterfaceInfo {
                        name: decl.name.clone(),
                        type_params: decl.type_params.clone(),
                        fields: field_map,
                        methods: method_map,
                        is_exported,
                        defined_in: self.current_file.clone(),
                        span: decl.span,
                        doc: decl.doc.as_ref().map(|d| d.content()),
                    },
                );
            } else if let Statement::FunctionDeclaration {
                name,
                name_span,
                type_params,
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
                self.push_scope();
                for tp in type_params {
                    self.scope.insert(
                        tp.name.clone(),
                        Type::GenericParam(tp.name.clone()),
                        false,
                        true,
                        false,
                        tp.span,
                        self.current_file.clone(),
                        None,
                    );
                }
                let mut param_tys = Vec::new();
                for (_, ty) in params {
                    param_tys.push(self.resolve_type(ty.clone()));
                }
                let ret_ty = self.resolve_type(return_ty.clone());
                self.pop_scope();
                self.scope.insert(
                    name.clone(),
                    Type::Function(type_params.clone(), param_tys, Box::new(ret_ty)),
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
                let var_ty = if let Some(t) = ty {
                    self.resolve_type(t.clone())
                } else {
                    Type::Error
                };
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

                let mut members = std::collections::HashSet::new();
                for m in &decl.members {
                    members.insert(m.name.clone());
                }
                self.enums.insert(decl.name.clone(), members);

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
                                        Type::Error,
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
