#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiDocs {
    pub summary: String,
    pub contract: String,
    pub curated: bool,
    pub tags: Vec<String>,
    pub navigation: ApiNavigation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiNavigation {
    pub implementation: Vec<String>,
    pub tests: Vec<String>,
    pub showcase: Option<String>,
}
