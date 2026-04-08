use aura_ast::Ident;

pub(crate) fn ident_text<'a>(source: &'a str, ident: &Ident) -> Option<&'a str> {
    let start = ident.span.start.raw() as usize;
    let end = ident.span.end.raw() as usize;
    source.get(start..end)
}
