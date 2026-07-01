# Tutorial Source

`docs-src/CHAPTER-*.md.in` files are the source of generated tutorial chapters
under `docs/CHAPTER-*.md` and `docs-html/`.

## Workflow

1. Edit the chapter template in this directory.
2. Update examples in `examples/` when the text includes executable code.
3. Add new examples to `examples/catalog.json`.
4. Run the formatter-free docs gate in `docs/TEST-MAP.md`.
5. Keep the generated markdown and HTML changes.

Do not patch only `docs/CHAPTER-*.md` for tutorial content; those files are
generated artifacts.

## Include Directives

Chapter templates may include cataloged examples through directives consumed by
`src/docs.rs`. Keep examples small and executable, and prefer examples that also
serve as regression coverage.
