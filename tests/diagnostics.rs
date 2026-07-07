use xsh::diagnostic::{Diagnostic, DiagnosticRenderer, Label};
use xsh::source::{SourceMap, Span};

fn resolved_fixture_path() -> String {
    std::path::absolute("fixtures/diagnostics/synthetic.xsh")
        .ok()
        .and_then(|p| p.to_str().map(String::from))
        .unwrap_or_else(|| "fixtures/diagnostics/synthetic.xsh".to_string())
}

#[test]
fn synthetic_diagnostic_fixture_output_is_stable() {
    let mut sources = SourceMap::new();
    let id = sources.add_file("fixtures/diagnostics/synthetic.xsh", "let x =\n");
    let span = Span::new(id, 7, 7);
    let diagnostic = Diagnostic::error("expected expression")
        .with_span(span)
        .with_label(Label::primary(span, "initializer required"));

    let rendered = DiagnosticRenderer::new()
        .with_color(false)
        .render(std::slice::from_ref(&diagnostic), &sources);
    let machine = diagnostic.to_machine(&sources);

    let expected = format!(
        "err: expected expression\n  {}:1:8\n  let x =\n         ^ initializer required\n",
        resolved_fixture_path()
    );
    assert_eq!(rendered, expected);
    assert_eq!(machine.span.unwrap().start_line, 1);
    assert_eq!(
        machine.labels[0].message.as_deref(),
        Some("initializer required")
    );
}
