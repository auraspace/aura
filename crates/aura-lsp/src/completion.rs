use super::*;

pub(super) fn completion(server: &Server, params: Option<&Value>) -> Value {
    let Some(uri) = uri_param(params) else {
        return json!({"isIncomplete":false,"items":[]});
    };
    let Some(source) = server.document_text(uri) else {
        return json!({"isIncomplete":false,"items":[]});
    };
    let offset = params
        .and_then(|p| p.get("position"))
        .map(|position| position_to_offset(source, position))
        .unwrap_or(source.len());
    let start = word_start(source, offset);
    let prefix = &source[start..offset.min(source.len())];
    let member_completion = start > 0 && source[..start].ends_with('.');
    let mut items = Vec::new();
    if let Some((import_start, import_prefix)) = import_completion_context(source, offset) {
        for package in workspace_packages(server) {
            if package.starts_with(import_prefix) {
                items.push(json!({
                    "label": package,
                    "kind": 9,
                    "detail": "Aura package",
                    "sortText": completion_sort_text(&package, import_prefix, 20),
                    "textEdit": {"range": span_range(source, Span::new(import_start as u32, offset as u32)), "newText": package}
                }));
            }
        }
    }
    if !member_completion {
        for keyword in KEYWORDS {
            if keyword.starts_with(prefix) {
                items.push(json!({
                    "label": keyword,
                    "kind": 14,
                    "sortText": completion_sort_text(keyword, prefix, 40)
                }));
            }
        }
    }
    if member_completion {
        items.extend(semantic_member_completion(
            server, uri, source, start, offset, prefix,
        ));
    }
    for (document_uri, document) in &server.documents {
        let Some(text) = document.get("text").and_then(Value::as_str) else {
            continue;
        };
        let file = if document_uri == uri {
            editor::parse_editor_file(text, offset).map(|(_, file)| file)
        } else {
            parse_file(text).ok()
        };
        let Some(file) = file else { continue };
        if member_completion {
            continue;
        }
        for symbol in declaration_symbols(text, &file) {
            if !symbol.name.starts_with(prefix) {
                continue;
            }
            let documentation = documentation_before(text, symbol.range.start as usize);
            let mut item = json!({
                "label": symbol.name,
                "kind": symbol.kind,
                "detail": symbol.detail,
                "sortText": completion_sort_text(
                    &symbol.name,
                    prefix,
                    if document_uri == uri { 10 } else { 20 },
                ),
                "textEdit": {"range": span_range(source, Span::new(start as u32, offset as u32)), "newText": symbol.name},
                "data": {"uri": document_uri, "bindingId": server.binding_id(document_uri, &symbol)}
            });
            if !documentation.is_empty() {
                item["documentation"] = json!({"kind":"markdown","value":documentation});
            }
            items.push(item);
        }
        if document_uri == uri {
            for symbol in server.local_symbols_for_document(document_uri, text, &file) {
                if !symbol.name.starts_with(prefix) {
                    continue;
                }
                let documentation = documentation_before(text, symbol.range.start as usize);
                let mut item = json!({
                    "label": symbol.name,
                    "kind": symbol.kind,
                    "detail": symbol.detail,
                    "sortText": completion_sort_text(&symbol.name, prefix, 0),
                    "textEdit": {"range": span_range(source, Span::new(start as u32, offset as u32)), "newText": symbol.name},
                    "data": {"uri": document_uri, "bindingId": server.binding_id(document_uri, &symbol)}
                });
                if !documentation.is_empty() {
                    item["documentation"] = json!({"kind":"markdown","value":documentation});
                }
                items.push(item);
            }
        }
    }
    items.sort_by(|left, right| {
        completion_item_sort_key(left).cmp(&completion_item_sort_key(right))
    });
    items.dedup_by(|left, right| {
        left["label"] == right["label"] && left["data"]["bindingId"] == right["data"]["bindingId"]
    });
    json!({"isIncomplete":false,"items":items})
}

fn workspace_packages(server: &Server) -> Vec<String> {
    let mut packages = server
        .documents
        .values()
        .filter_map(|document| document.get("text").and_then(Value::as_str))
        .filter_map(|source| parse_file(source).ok())
        .map(|file| file.package.display())
        .collect::<Vec<_>>();
    packages.sort();
    packages.dedup();
    packages
}

fn semantic_member_completion(
    server: &Server,
    uri: &str,
    source: &str,
    start: usize,
    offset: usize,
    prefix: &str,
) -> Vec<Value> {
    let Some(receiver_span) = receiver_span(source, start) else {
        return Vec::new();
    };
    let id = DocumentId::from(uri);
    let receiver_ty = server
        .expression_type_at(uri, receiver_span)
        .or_else(|| {
            let (_, file) = editor::parse_editor_file(source, offset)?;
            let checked = check_file(&file).ok()?;
            checked
                .expr_tys
                .get(&(receiver_span.start, receiver_span.end))
                .cloned()
        })
        .or_else(|| {
            let (_, file) = editor::parse_editor_file(source, offset)?;
            receiver_field_type_name(source, &file, receiver_span, start as u32)
                .map(|name| receiver_type_from_name(&name))
        })
        .or_else(|| {
            let (_, file) = editor::parse_editor_file(source, offset)?;
            local_receiver_type_from_file(server, source, &file, receiver_span, start as u32)
        })
        .or_else(|| {
            receiver_field_type_name_from_tokens(source, receiver_span)
                .map(|name| receiver_type_from_name(&name))
        });
    let Some(receiver_ty) = receiver_ty.as_ref() else {
        return Vec::new();
    };
    let receiver_name = nominal_type_name(receiver_ty);
    let mut items = builtin_member_completion(receiver_ty, prefix);
    let Ok(analysis) = server.host.snapshot().analyze(&id) else {
        if let Some(receiver_name) = receiver_name {
            items.extend(workspace_member_completion(server, receiver_name, prefix));
            if let Some((fallback_source, _)) = editor::parse_editor_file(source, offset) {
                items.extend(workspace_members_from_source(
                    &fallback_source,
                    receiver_name,
                    prefix,
                ));
            }
        }
        return dedup_member_completion(items);
    };
    for class in &analysis.checked.classes {
        if receiver_name.map(nominal_short_name) != Some(nominal_short_name(&class.name)) {
            continue;
        }
        for field in &class.fields {
            if field.name.starts_with(prefix) {
                let mut item = json!({"label":field.name,"kind":8,"detail":format!("{}: {}", field.name, field.ty.display()), "sortText": completion_sort_text(&field.name, prefix, 20)});
                add_member_documentation(&mut item, source, receiver_name, &field.name);
                items.push(item);
            }
        }
        for method in class.methods.values() {
            if method.name.starts_with(prefix) {
                let mut item = json!({"label":method.name,"kind":2,"detail":format_function_signature(&method.name, &method.params, &method.ret), "sortText": completion_sort_text(&method.name, prefix, 20)});
                add_member_documentation(&mut item, source, receiver_name, &method.name);
                items.push(item);
            }
        }
    }
    for interface in &analysis.checked.interfaces {
        if receiver_name.map(nominal_short_name) != Some(nominal_short_name(&interface.name)) {
            continue;
        }
        for method in interface.methods.values() {
            if method.name.starts_with(prefix) {
                let mut item = json!({"label":method.name,"kind":2,"detail":format_function_signature(&method.name, &method.params, &method.ret), "sortText": completion_sort_text(&method.name, prefix, 20)});
                add_member_documentation(&mut item, source, receiver_name, &method.name);
                items.push(item);
            }
        }
    }
    dedup_member_completion(items)
}

fn local_receiver_type_from_file(
    server: &Server,
    source: &str,
    file: &File,
    receiver_span: Span,
    member_offset: u32,
) -> Option<aura_sema::Ty> {
    let name = source.get(receiver_span.start as usize..receiver_span.end as usize)?;
    for function in &file.functions {
        if function.span.start <= member_offset && member_offset <= function.span.end {
            if let Some(ty) =
                local_type_in_block(server, source, file, &function.body, name, member_offset)
            {
                return Some(ty);
            }
        }
    }
    for function in &file.async_functions {
        if function.span.start <= member_offset && member_offset <= function.span.end {
            if let Some(ty) =
                local_type_in_block(server, source, file, &function.body, name, member_offset)
            {
                return Some(ty);
            }
        }
    }
    for class in &file.classes {
        for method in &class.methods {
            if method.span.start <= member_offset && member_offset <= method.span.end {
                if let Some(ty) =
                    local_type_in_block(server, source, file, &method.body, name, member_offset)
                {
                    return Some(ty);
                }
            }
        }
    }
    None
}

fn local_type_in_block(
    server: &Server,
    source: &str,
    file: &File,
    block: &aura_ast::Block,
    name: &str,
    member_offset: u32,
) -> Option<aura_sema::Ty> {
    for statement in &block.stmts {
        match statement {
            aura_ast::Stmt::Var(variable)
                if variable.name.name == name && variable.span.start <= member_offset =>
            {
                if let Some(ty) = &variable.ty {
                    return Some(receiver_type_from_name(&source_span(source, ty.span)));
                }
                return inferred_expression_type(
                    server,
                    source,
                    file,
                    &variable.init,
                    member_offset,
                );
            }
            aura_ast::Stmt::If(if_stmt) => {
                if let Some(ty) = local_type_in_block(
                    server,
                    source,
                    file,
                    &if_stmt.then_block,
                    name,
                    member_offset,
                ) {
                    return Some(ty);
                }
                if let Some(block) = &if_stmt.else_block {
                    if let Some(ty) =
                        local_type_in_block(server, source, file, block, name, member_offset)
                    {
                        return Some(ty);
                    }
                }
            }
            aura_ast::Stmt::While(while_stmt) => {
                if let Some(ty) =
                    local_type_in_block(server, source, file, &while_stmt.body, name, member_offset)
                {
                    return Some(ty);
                }
            }
            _ => {}
        }
    }
    None
}

fn inferred_expression_type(
    server: &Server,
    source: &str,
    file: &File,
    expr: &aura_ast::Expr,
    member_offset: u32,
) -> Option<aura_sema::Ty> {
    match expr {
        aura_ast::Expr::String(_) => Some(aura_sema::Ty::String),
        aura_ast::Expr::Int(_) => Some(aura_sema::Ty::Int),
        aura_ast::Expr::Bool(_) => Some(aura_sema::Ty::Bool),
        aura_ast::Expr::Null(_) => Some(aura_sema::Ty::Null),
        aura_ast::Expr::Call(call) => {
            let aura_ast::Expr::Field(field) = call.callee.as_ref() else {
                return None;
            };
            let receiver_name =
                receiver_field_type_name(source, file, field.object.span(), member_offset)?;
            if let Some(class) = file.classes.iter().find(|class| {
                nominal_short_name(&class.name.name) == nominal_short_name(&receiver_name)
            }) {
                if let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.name == field.field.name)
                {
                    return method
                        .return_type
                        .as_ref()
                        .map(|ty| receiver_type_from_name(&source_span(source, ty.span)));
                }
            }
            for document in server.documents.values() {
                let Some(other_source) = document.get("text").and_then(Value::as_str) else {
                    continue;
                };
                let Ok(other_file) = parse_file(other_source) else {
                    continue;
                };
                let Some(class) = other_file.classes.iter().find(|class| {
                    nominal_short_name(&class.name.name) == nominal_short_name(&receiver_name)
                }) else {
                    continue;
                };
                let Some(method) = class
                    .methods
                    .iter()
                    .find(|method| method.name.name == field.field.name)
                else {
                    continue;
                };
                return method
                    .return_type
                    .as_ref()
                    .map(|ty| receiver_type_from_name(&source_span(other_source, ty.span)));
            }
            None
        }
        _ => None,
    }
}

fn workspace_member_completion(server: &Server, receiver_name: &str, prefix: &str) -> Vec<Value> {
    let receiver_name = nominal_short_name(receiver_name);
    let mut items = Vec::new();
    for document in server.documents.values() {
        let Some(source) = document.get("text").and_then(Value::as_str) else {
            continue;
        };
        let Ok(file) = parse_file(source) else {
            continue;
        };
        let Some(symbol) = declaration_symbols(source, &file)
            .into_iter()
            .find(|symbol| nominal_short_name(&symbol.name) == receiver_name)
        else {
            continue;
        };
        for member in symbol.children {
            if member.name.starts_with(prefix) {
                let mut item = json!({
                    "label": member.name,
                    "kind": member.kind,
                    "detail": compact_symbol_detail(&member.detail),
                    "sortText": completion_sort_text(&member.name, prefix, 20)
                });
                let documentation = documentation_before(source, member.range.start as usize);
                if !documentation.is_empty() {
                    item["documentation"] = json!({"kind": "markdown", "value": documentation});
                }
                items.push(item);
            }
        }
    }
    items
}
fn builtin_member_completion(ty: &aura_sema::Ty, prefix: &str) -> Vec<Value> {
    if let aura_sema::Ty::Nullable(inner) = ty {
        return builtin_member_completion(inner, prefix);
    }
    if let aura_sema::Ty::ClassApp { name, args } = ty {
        if nominal_short_name(name) == "Array" {
            return array_member_completion(args.first(), prefix);
        }
    }
    if matches!(ty, aura_sema::Ty::Class(name) if nominal_short_name(name) == "Array") {
        return array_member_completion(None, prefix);
    }

    let methods: &[(&str, &[aura_sema::Ty], aura_sema::Ty, &str)] = match ty {
        aura_sema::Ty::String => &[
            (
                "isEmpty",
                &[],
                aura_sema::Ty::Bool,
                "Returns whether the string is empty.",
            ),
            (
                "charAt",
                &[aura_sema::Ty::Int],
                aura_sema::Ty::Int,
                "Returns the UTF-8 byte at an index.",
            ),
            (
                "startsWith",
                &[aura_sema::Ty::String],
                aura_sema::Ty::Bool,
                "Checks whether the string starts with a prefix.",
            ),
            (
                "contains",
                &[aura_sema::Ty::String],
                aura_sema::Ty::Bool,
                "Checks whether the string contains a substring.",
            ),
            (
                "endsWith",
                &[aura_sema::Ty::String],
                aura_sema::Ty::Bool,
                "Checks whether the string ends with a suffix.",
            ),
            ("hash", &[], aura_sema::Ty::Int, "Returns the string hash."),
            (
                "indexOf",
                &[aura_sema::Ty::String],
                aura_sema::Ty::Int,
                "Returns the first byte index of a substring.",
            ),
            (
                "split",
                &[aura_sema::Ty::String],
                aura_sema::Ty::ClassApp {
                    name: "Array".into(),
                    args: vec![aura_sema::Ty::String],
                },
                "Splits the string into an array of strings.",
            ),
            (
                "trim",
                &[],
                aura_sema::Ty::String,
                "Returns the string without surrounding ASCII whitespace.",
            ),
            (
                "trimStart",
                &[],
                aura_sema::Ty::String,
                "Returns the string without leading ASCII whitespace.",
            ),
            (
                "trimEnd",
                &[],
                aura_sema::Ty::String,
                "Returns the string without trailing ASCII whitespace.",
            ),
            (
                "toLower",
                &[],
                aura_sema::Ty::String,
                "Returns an ASCII lowercase copy.",
            ),
            (
                "toUpper",
                &[],
                aura_sema::Ty::String,
                "Returns an ASCII uppercase copy.",
            ),
            (
                "toInt",
                &[],
                aura_sema::Ty::Nullable(Box::new(aura_sema::Ty::Int)),
                "Parses the string as a decimal integer, or returns null.",
            ),
            (
                "substring",
                &[aura_sema::Ty::Int, aura_sema::Ty::Int],
                aura_sema::Ty::String,
                "Returns the substring between byte indices.",
            ),
        ],
        aura_sema::Ty::Int => &[
            (
                "toString",
                &[],
                aura_sema::Ty::String,
                "Formats the integer as a string.",
            ),
            ("hash", &[], aura_sema::Ty::Int, "Returns the integer hash."),
        ],
        _ => return Vec::new(),
    };
    methods
        .iter()
        .filter(|(name, _, _, _)| name.starts_with(prefix))
        .map(|(name, params, ret, documentation)| {
            json!({
                "label": name,
                "kind": 2,
                "detail": format_function_signature(name, params, ret),
                "sortText": completion_sort_text(name, prefix, 20),
                "documentation": {"kind": "markdown", "value": documentation}
            })
        })
        .collect()
}

fn array_member_completion(element: Option<&aura_sema::Ty>, prefix: &str) -> Vec<Value> {
    let element = element
        .cloned()
        .unwrap_or_else(|| aura_sema::Ty::TypeParam("T".into()));
    let array = aura_sema::Ty::ClassApp {
        name: "Array".into(),
        args: vec![element.clone()],
    };
    let methods = [
        (
            "get",
            vec![aura_sema::Ty::Int],
            element.clone(),
            "Returns the element at an index.",
        ),
        (
            "set",
            vec![aura_sema::Ty::Int, element.clone()],
            aura_sema::Ty::Unit,
            "Replaces the element at an index.",
        ),
        (
            "push",
            vec![element.clone()],
            aura_sema::Ty::Unit,
            "Appends an element to the array.",
        ),
        (
            "pop",
            Vec::new(),
            element.clone(),
            "Removes and returns the last element.",
        ),
        (
            "clear",
            Vec::new(),
            aura_sema::Ty::Unit,
            "Removes all elements from the array.",
        ),
        (
            "isEmpty",
            Vec::new(),
            aura_sema::Ty::Bool,
            "Returns whether the array has no elements.",
        ),
        (
            "reserve",
            vec![aura_sema::Ty::Int],
            aura_sema::Ty::Unit,
            "Ensures capacity for at least this many elements.",
        ),
        (
            "clone",
            Vec::new(),
            array,
            "Returns an owning copy of the array.",
        ),
    ];
    let mut items = methods
        .into_iter()
        .filter(|(name, _, _, _)| name.starts_with(prefix))
        .map(|(name, params, ret, documentation)| {
            json!({
                "label": name,
                "kind": 2,
                "detail": format_function_signature(name, &params, &ret),
                "sortText": completion_sort_text(name, prefix, 20),
                "documentation": {"kind": "markdown", "value": documentation}
            })
        })
        .collect::<Vec<_>>();
    if "len".starts_with(prefix) {
        items.push(json!({
            "label": "len",
            "kind": 8,
            "detail": format!("len: {}", aura_sema::Ty::Int.display()),
            "sortText": completion_sort_text("len", prefix, 20),
            "documentation": {"kind": "markdown", "value": "Returns the number of elements."}
        }));
    }
    items
}

fn dedup_member_completion(mut items: Vec<Value>) -> Vec<Value> {
    items.sort_by(|left, right| {
        completion_item_sort_key(left).cmp(&completion_item_sort_key(right))
    });
    items.dedup_by(|left, right| left["label"] == right["label"]);
    items
}

fn completion_sort_text(label: &str, prefix: &str, group: u8) -> String {
    let match_rank = if label == prefix {
        0
    } else if label.starts_with(prefix) {
        1
    } else {
        2
    };
    format!("{group:02}{match_rank}{label}")
}

fn completion_item_sort_key(item: &Value) -> (&str, &str) {
    (
        item["sortText"].as_str().unwrap_or("99"),
        item["label"].as_str().unwrap_or(""),
    )
}

fn workspace_members_from_source(source: &str, receiver_name: &str, prefix: &str) -> Vec<Value> {
    let receiver_name = nominal_short_name(receiver_name);
    let Ok(file) = parse_file(source) else {
        return Vec::new();
    };
    let Some(symbol) = declaration_symbols(source, &file)
        .into_iter()
        .find(|symbol| nominal_short_name(&symbol.name) == receiver_name)
    else {
        return Vec::new();
    };
    symbol
        .children
        .into_iter()
        .filter(|member| member.name.starts_with(prefix))
        .map(|member| {
            let mut item = json!({
                "label": member.name,
                "kind": member.kind,
                "detail": compact_symbol_detail(&member.detail),
                "sortText": completion_sort_text(&member.name, prefix, 20)
            });
            let documentation = documentation_before(source, member.range.start as usize);
            if !documentation.is_empty() {
                item["documentation"] = json!({"kind": "markdown", "value": documentation});
            }
            item
        })
        .collect()
}

fn add_member_documentation(
    item: &mut Value,
    source: &str,
    receiver_name: Option<&str>,
    member_name: &str,
) {
    let Some(receiver_name) = receiver_name.map(nominal_short_name) else {
        return;
    };
    let Ok(file) = parse_file(source) else {
        return;
    };
    let Some(symbol) = declaration_symbols(source, &file)
        .into_iter()
        .find(|symbol| nominal_short_name(&symbol.name) == receiver_name)
    else {
        return;
    };
    let Some(member) = symbol
        .children
        .iter()
        .find(|member| member.name == member_name)
    else {
        return;
    };
    let documentation = documentation_before(source, member.range.start as usize);
    if !documentation.is_empty() {
        item["documentation"] = json!({"kind": "markdown", "value": documentation});
    }
}
fn receiver_type_from_name(name: &str) -> aura_sema::Ty {
    let name = name.trim();
    if let Some(inner) = name.strip_suffix('?') {
        return aura_sema::Ty::Nullable(Box::new(receiver_type_from_name(inner)));
    }
    if let Some(open) = name.find('<') {
        if name.ends_with('>') {
            let base = name[..open].trim();
            let args = split_type_arguments(&name[open + 1..name.len() - 1])
                .into_iter()
                .map(receiver_type_from_name)
                .collect();
            return aura_sema::Ty::ClassApp {
                name: base.to_owned(),
                args,
            };
        }
    }
    match name {
        "Int" => aura_sema::Ty::Int,
        "Bool" => aura_sema::Ty::Bool,
        "String" => aura_sema::Ty::String,
        _ => aura_sema::Ty::Class(name.to_owned()),
    }
}

fn split_type_arguments(arguments: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (index, character) in arguments.char_indices() {
        match character {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                result.push(arguments[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    if start < arguments.len() {
        result.push(arguments[start..].trim());
    }
    result
}

fn receiver_field_type_name_from_tokens(source: &str, receiver_span: Span) -> Option<String> {
    let receiver = source.get(receiver_span.start as usize..receiver_span.end as usize)?;
    let field_name = receiver.strip_prefix("this.")?;
    let tokens = lex(source).ok()?;
    let class_index = tokens.iter().rposition(|token| {
        token.span.start as usize <= receiver_span.start as usize
            && matches!(token.kind, TokenKind::Class)
    })?;
    let open_brace = tokens
        .iter()
        .enumerate()
        .skip(class_index)
        .find(|(_, token)| matches!(token.kind, TokenKind::LBrace))
        .map(|(index, _)| index)?;
    let header = &tokens[class_index..open_brace];
    for index in 0..header.len().saturating_sub(2) {
        let name = &header[index];
        let colon = &header[index + 1];
        let ty = &header[index + 2];
        let TokenKind::Ident(name) = &name.kind else {
            continue;
        };
        if !matches!(colon.kind, TokenKind::Colon) || name != field_name {
            continue;
        }
        let type_start = ty.span.start as usize;
        let mut type_end = ty.span.end as usize;
        let mut angle_depth = 0;
        for token in &header[index + 2..] {
            match token.kind {
                TokenKind::Lt => angle_depth += 1,
                TokenKind::Gt => angle_depth -= 1,
                TokenKind::Comma | TokenKind::RParen if angle_depth == 0 => break,
                _ => type_end = token.span.end as usize,
            }
        }
        return source.get(type_start..type_end).map(str::to_owned);
    }
    None
}
