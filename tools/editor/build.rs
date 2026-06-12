fn main() {
    // "fluent" (not "fluent-dark") so Palette.color-scheme follows the OS light/dark setting — the Auto theme reads it to pick its chrome; a forced *-dark style pins it to dark.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles)
        .with_style("fluent".into());
    slint_build::compile_with_config("ui/editor.slint", config).expect("ui/editor.slint compiles");
}
