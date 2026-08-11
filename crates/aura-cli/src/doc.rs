use aura_ast::File;

pub fn render(file: &File, package: &str) -> String {
    let mut out = format!("# {package}\n\nGenerated API reference.\n");
    section(
        &mut out,
        "Functions",
        file.functions.iter().map(|fun| fun.name.name.as_str()),
    );
    section(
        &mut out,
        "Async Functions",
        file.async_functions
            .iter()
            .map(|fun| fun.name.name.as_str()),
    );
    section(
        &mut out,
        "Interfaces",
        file.interfaces.iter().map(|item| item.name.name.as_str()),
    );
    section(
        &mut out,
        "Enums",
        file.enums.iter().map(|item| item.name.name.as_str()),
    );
    section(
        &mut out,
        "Classes",
        file.classes.iter().map(|item| item.name.name.as_str()),
    );
    section(
        &mut out,
        "Type Aliases",
        file.type_aliases.iter().map(|item| item.name.name.as_str()),
    );
    out
}

fn section<'a>(out: &mut String, title: &str, names: impl IntoIterator<Item = &'a str>) {
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort_unstable();
    if names.is_empty() {
        return;
    }
    out.push_str(&format!("\n## {title}\n\n"));
    for name in names {
        out.push_str(&format!("- \x60{name}\x60\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::render;
    use aura_analysis::parse_file;

    #[test]
    fn renders_stable_sections_from_ast() {
        let file =
            parse_file("package demo\n\nfun z() {}\nfun a() {}\nasync fun later() {}\n").unwrap();
        let output = render(&file, "demo");
        assert!(output.contains("# demo"));
        assert!(output.find("- \x60a\x60").unwrap() < output.find("- \x60z\x60").unwrap());
        assert!(output.contains("## Async Functions"));
        assert!(output.contains("- \x60later\x60"));
    }
}
