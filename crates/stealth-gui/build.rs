fn main() {
    // the few standard widgets used (text input, scroll views) take the Fluent style, light or dark after the theme
    // (app.slint sets it); the rest is custom
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/app.slint", config).expect("compiling the Slint UI");
}
