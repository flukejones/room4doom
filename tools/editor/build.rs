fn main() {
    // "fluent" (not "fluent-dark") so std-widgets and Palette.color-scheme
    // follow the OS light/dark setting — the Auto theme reads color-scheme to
    // pick its chrome. A forced *-dark style pins color-scheme to dark.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedFiles)
        .with_style("fluent".into());
    slint_build::compile_with_config("ui/editor.slint", config).expect("ui/editor.slint compiles");
}
