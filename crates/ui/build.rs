fn main() {
    // Estilo Fluent em todos os SOs por enquanto; Cupertino no macOS entra na Fase 5.
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    slint_build::compile_with_config("ui/telas.slint", config).unwrap();
}
