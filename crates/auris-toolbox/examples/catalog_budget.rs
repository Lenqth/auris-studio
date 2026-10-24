//! Inspect transmitted tool sizes without invoking any model or editing a project.
fn main() {
    let mut definitions = auris_toolbox::live_agent::definitions();
    for tool in &mut definitions {
        auris_toolbox::compact_parameters(&mut tool.parameters);
        tool.description = auris_toolbox::concise_description(&tool.description);
    }
    let mut sizes = definitions
        .iter()
        .map(|t| {
            (
                t.parameters.to_string().len() + t.description.len(),
                t.name.as_str(),
            )
        })
        .collect::<Vec<_>>();
    sizes.sort_by(|a, b| b.cmp(a));
    println!(
        "{} tools, {} bytes",
        sizes.len(),
        sizes.iter().map(|p| p.0).sum::<usize>()
    );
    for (bytes, name) in sizes {
        println!("{name}: {bytes}");
    }
    if let Some(path) = std::env::args().nth(1) {
        let value=serde_json::Value::Array(definitions.into_iter().map(|t|serde_json::json!({"name":t.name,"description":t.description,"parameters":t.parameters})).collect());
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    }
}
