use aura_ast::{Block, ExportedDecl, Expr, Ident, Program, Stmt, TopLevel};
use aura_typeck::TypedProgram;

pub fn dump_hir(source: &str, ast: &Program, typed: &TypedProgram) {
    let mut printer = HirPrinter {
        source,
        typed,
        indent: 0,
    };
    printer.print_program(ast);
}

struct HirPrinter<'a> {
    source: &'a str,
    typed: &'a TypedProgram,
    indent: usize,
}

impl<'a> HirPrinter<'a> {
    fn print_program(&mut self, ast: &Program) {
        for item in &ast.items {
            self.print_top_level(item);
            println!();
        }
    }

    fn print_top_level(&mut self, item: &TopLevel) {
        match item {
            TopLevel::Import(_) => {
                // Imports don't have types in HIR usually, just skip or print briefly
            }
            TopLevel::Export(export) => {
                self.print_indent();
                print!("export ");
                match export.item.as_ref() {
                    Some(ExportedDecl::Function(f)) => self.print_function(f),
                    Some(ExportedDecl::Class(c)) => self.print_class(c),
                    Some(ExportedDecl::Interface(i)) => self.print_interface(i),
                    None => println!("<invalid export>"),
                }
            }
            TopLevel::Function(f) => {
                self.print_function(f);
            }
            TopLevel::Class(c) => {
                self.print_class(c);
            }
            TopLevel::Interface(i) => {
                self.print_interface(i);
            }
            TopLevel::Stmt(s) => self.print_stmt(s),
        }
    }

    fn print_function(&mut self, f: &aura_ast::FunctionDecl) {
        self.print_indent();
        print!("fn ");
        self.print_ident(&f.name);
        print!("(");
        for (i, param) in f.params.iter().enumerate() {
            if i > 0 {
                print!(", ");
            }
            self.print_ident(&param.name);
            print!(": {}", self.source_at(param.ty.span));
        }
        print!(")");
        if let Some(ret) = &f.return_type {
            print!(": {}", self.source_at(ret.span));
        }
        println!(" ");
        self.print_block(&f.body);
    }

    fn print_class(&mut self, c: &aura_ast::ClassDecl) {
        self.print_indent();
        print!("class ");
        self.print_ident(&c.name);
        if let Some(parent) = &c.extends {
            print!(" extends ");
            print!("{}", self.source_at(parent.span));
        }
        if !c.implements.is_empty() {
            print!(" implements ");
            for (i, imp) in c.implements.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("{}", self.source_at(imp.span));
            }
        }
        println!(" {{");
        self.indent += 2;
        for field in &c.fields {
            self.print_indent();
            self.print_ident(&field.name);
            println!(": {};", self.source_at(field.ty.span));
        }
        for method in &c.methods {
            self.print_indent();
            print!("method ");
            self.print_ident(&method.name);
            print!("(");
            for (i, param) in method.params.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                self.print_ident(&param.name);
                print!(": {}", self.source_at(param.ty.span));
            }
            print!(")");
            if let Some(ret) = &method.return_type {
                print!(": {}", self.source_at(ret.span));
            }
            println!(" ");
            self.print_block(&method.body);
        }
        self.indent -= 2;
        self.print_indent();
        println!("}}");
    }

    fn print_interface(&mut self, i: &aura_ast::InterfaceDecl) {
        self.print_indent();
        print!("interface ");
        self.print_ident(&i.name);
        println!(" {{");
        self.indent += 2;
        for method in &i.methods {
            self.print_indent();
            print!("method ");
            self.print_ident(&method.name);
            print!("(");
            for (i, param) in method.params.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                self.print_ident(&param.name);
                print!(": {}", self.source_at(param.ty.span));
            }
            print!(")");
            if let Some(ret) = &method.return_type {
                print!(": {}", self.source_at(ret.span));
            }
            println!(";");
        }
        self.indent -= 2;
        self.print_indent();
        println!("}}");
    }

    fn print_block(&mut self, block: &Block) {
        self.print_indent();
        println!("{{");
        self.indent += 2;
        for stmt in &block.stmts {
            self.print_stmt(stmt);
        }
        self.indent -= 2;
        self.print_indent();
        println!("}}");
    }

    fn print_stmt(&mut self, stmt: &Stmt) {
        self.print_indent();
        match stmt {
            Stmt::Let(l) => {
                print!("let ");
                self.print_ident(&l.name);
                if let Some(ty) = &l.ty {
                    print!(": {}", self.source_at(ty.span));
                }
                if let Some(init) = &l.init {
                    print!(" = ");
                    self.print_expr(init);
                }
                println!(";");
            }
            Stmt::Const(c) => {
                print!("const ");
                self.print_ident(&c.name);
                if let Some(ty) = &c.ty {
                    print!(": {}", self.source_at(ty.span));
                }
                if let Some(init) = &c.init {
                    print!(" = ");
                    self.print_expr(init);
                }
                println!(";");
            }
            Stmt::Return(r) => {
                print!("return");
                if let Some(val) = &r.value {
                    print!(" ");
                    self.print_expr(val);
                }
                println!(";");
            }
            Stmt::Throw(t) => {
                print!("throw ");
                self.print_expr(&t.value);
                println!(";");
            }
            Stmt::Expr(e) => {
                self.print_expr(&e.expr);
                println!(";");
            }
            Stmt::Block(b) => self.print_block(b),
            Stmt::If(i) => {
                print!("if (");
                self.print_expr(&i.cond);
                println!(") ");
                self.print_block(&i.then_block);
                if let Some(else_b) = &i.else_block {
                    self.print_indent();
                    println!("else ");
                    self.print_block(else_b);
                }
            }
            Stmt::While(w) => {
                print!("while (");
                self.print_expr(&w.cond);
                println!(") ");
                self.print_block(&w.body);
            }
            Stmt::Try(t) => {
                print!("try ");
                self.print_block(&t.try_block);
                if let Some(catch) = &t.catch {
                    self.print_indent();
                    print!("catch (");
                    self.print_ident(&catch.binding);
                    if let Some(ty) = &catch.ty {
                        print!(": {}", self.source_at(ty.span));
                    }
                    println!(") ");
                    self.print_block(&catch.block);
                }
                if let Some(finally_block) = &t.finally_block {
                    self.print_indent();
                    println!("finally ");
                    self.print_block(finally_block);
                }
            }
            Stmt::Empty(_) => println!(";"),
        }
    }

    fn print_expr(&mut self, expr: &Expr) {
        let span = match expr {
            Expr::Ident(i) => i.span,
            Expr::This(s) => *s,
            Expr::IntLit(s) => *s,
            Expr::FloatLit(s) => *s,
            Expr::StringLit(s) => *s,
            Expr::BoolLit(_, s) => *s,
            Expr::Unary { span, .. } => *span,
            Expr::Binary { span, .. } => *span,
            Expr::Assign { span, .. } => *span,
            Expr::Call { span, .. } => *span,
            Expr::New { span, .. } => *span,
            Expr::Member { span, .. } => *span,
            Expr::Paren { span, .. } => *span,
        };

        let ty_name = self
            .typed
            .expression_types
            .get(&span)
            .map(|t| t.name())
            .unwrap_or_else(|| std::borrow::Cow::Borrowed("unknown"));

        print!("(");
        match expr {
            Expr::Ident(i) => print!("{}", self.source_at(i.span)),
            Expr::This(_) => print!("this"),
            Expr::IntLit(s) => print!("{}", self.source_at(*s)),
            Expr::FloatLit(s) => print!("{}", self.source_at(*s)),
            Expr::StringLit(s) => print!("{}", self.source_at(*s)),
            Expr::BoolLit(b, _) => print!("{}", b),
            Expr::Unary { op, expr, .. } => {
                print!("{:?}", op);
                self.print_expr(expr);
            }
            Expr::Binary {
                op, left, right, ..
            } => {
                self.print_expr(left);
                print!(" {:?} ", op);
                self.print_expr(right);
            }
            Expr::Assign { target, value, .. } => {
                self.print_expr(target);
                print!(" = ");
                self.print_expr(value);
            }
            Expr::Call { callee, args, .. } => {
                self.print_expr(callee);
                print!("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    self.print_expr(arg);
                }
                print!(")");
            }
            Expr::New { class, args, .. } => {
                print!("new ");
                self.print_ident(class);
                print!("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        print!(", ");
                    }
                    self.print_expr(arg);
                }
                print!(")");
            }
            Expr::Member { object, field, .. } => {
                self.print_expr(object);
                print!(".");
                self.print_ident(field);
            }
            Expr::Paren { expr, .. } => {
                print!("(");
                self.print_expr(expr);
                print!(")");
            }
        }
        print!(": {})", ty_name);
    }

    fn print_ident(&self, ident: &Ident) {
        print!("{}", self.source_at(ident.span));
    }

    fn source_at(&self, span: aura_span::Span) -> &str {
        self.source
            .get(span.start.raw() as usize..span.end.raw() as usize)
            .unwrap_or("???")
    }

    fn print_indent(&self) {
        for _ in 0..self.indent {
            print!(" ");
        }
    }
}
